use crate::{
    Error, Result,
    artifact_policy::ownership_issue,
    filesystem::{id_for, is_link, measure, modified, read_small, reject_links},
    git,
    model::{Artifact, Progress, ProgressSink, Project, ProviderId, Settings, Worktree},
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::{Instant, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

#[derive(Debug, Default, Clone)]
pub struct ScanResult {
    pub projects: Vec<Project>,
    pub worktrees: Vec<Worktree>,
    pub issues: Vec<String>,
    pub visited: u64,
    pub elapsed_ms: u128,
    pub cached_roots: u64,
}

pub fn validate_settings(settings: &Settings) -> Result<()> {
    if !["zh-CN", "zh-TW", "en"].contains(&settings.language.as_str())
        || !["light", "dark", "system"].contains(&settings.theme.as_str())
        || !(1..=3650).contains(&settings.idle_days)
    {
        return Err(Error::InvalidInput(
            "Invalid language, theme, or inactivity threshold.".into(),
        ));
    }
    if settings.roots.len() > 64 || settings.excludes.len() > 256 {
        return Err(Error::InvalidInput(
            "Too many scan roots or exclusions.".into(),
        ));
    }
    exclusions(settings)?;
    for root in &settings.roots {
        reject_links(root)?;
        if !root.is_dir() {
            return Err(Error::InvalidInput(format!(
                "{} is not an accessible directory.",
                root.display()
            )));
        }
    }
    Ok(())
}

fn exclusions(settings: &Settings) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for value in &settings.excludes {
        builder.add(Glob::new(value).map_err(|e| Error::InvalidInput(e.to_string()))?);
    }
    builder
        .build()
        .map_err(|e| Error::InvalidInput(e.to_string()))
}

pub(crate) fn canonical_roots(settings: &Settings) -> Result<Vec<PathBuf>> {
    let mut roots = settings
        .roots
        .iter()
        .map(fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
    roots.sort();
    roots.dedup();
    let all = roots.clone();
    roots.retain(|root| {
        !all.iter()
            .any(|other| root != other && root.starts_with(other))
    });
    Ok(roots)
}

fn configured_scope(path: &Path, settings: &Settings) -> Result<Option<PathBuf>> {
    let ignored = exclusions(settings)?;
    for root in canonical_roots(settings)? {
        if path.starts_with(&root) {
            let excluded = path.ancestors().take_while(|p| *p != root).any(|p| {
                p.file_name().is_some_and(|name| ignored.is_match(name))
                    || ignored.is_match(p.strip_prefix(&root).unwrap_or(p))
            });
            return Ok((!excluded).then_some(root));
        }
    }
    Ok(None)
}

fn scope_root(path: &Path, settings: &Settings) -> Result<Option<PathBuf>> {
    // Explicit exclusions also apply to worktrees nested in a configured root.
    if canonical_roots(settings)?
        .iter()
        .any(|root| path.starts_with(root))
    {
        return configured_scope(path, settings);
    }
    for ancestor in path.ancestors() {
        if ancestor.join(".git").exists() {
            let Some(checkout) = git::checkout(ancestor)? else {
                return Ok(None);
            };
            if checkout.is_worktree
                && configured_scope(&checkout.repository.path, settings)?.is_some()
            {
                let ignored = exclusions(settings)?;
                let excluded = path
                    .ancestors()
                    .take_while(|p| p.starts_with(ancestor))
                    .any(|p| {
                        p.file_name().is_some_and(|name| ignored.is_match(name))
                            || ignored.is_match(p.strip_prefix(ancestor).unwrap_or(p))
                    });
                return Ok((!excluded).then(|| ancestor.to_path_buf()));
            }
            break;
        }
    }
    Ok(None)
}

pub(crate) fn project_in_scope(path: &Path, settings: &Settings) -> Result<bool> {
    Ok(scope_root(path, settings)?.is_some())
}

pub(crate) fn artifact_in_scope(
    path: &Path,
    settings: &Settings,
    cancel: &CancellationToken,
) -> Result<()> {
    reject_links(path)?;
    let canonical = fs::canonicalize(path)?;
    if !project_in_scope(&canonical, settings)? {
        return Err(Error::unsafe_path(
            path,
            "artifact is outside the scan scope or excluded",
        ));
    }
    for protected in &settings.protected_projects {
        let protected = fs::canonicalize(protected)?;
        if canonical.starts_with(&protected) || protected.starts_with(&canonical) {
            return Err(Error::unsafe_path(
                path,
                "artifact overlaps a protected path",
            ));
        }
    }
    let root = scope_root(&canonical, settings)?
        .ok_or_else(|| Error::unsafe_path(path, "artifact is outside the scan roots"))?;
    let ignored = exclusions(settings)?;
    let mut entries = WalkDir::new(&canonical)
        .follow_links(false)
        .max_open(16)
        .into_iter();
    while let Some(entry) = entries.next() {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let entry = entry.map_err(|error| Error::UnsafePath(error.to_string()))?;
        if ignored.is_match(entry.file_name())
            || ignored.is_match(entry.path().strip_prefix(&root).unwrap_or(entry.path()))
        {
            return Err(Error::unsafe_path(
                entry.path(),
                "an excluded entry prevents cleaning its parent directory",
            ));
        }
        if is_link(&fs::symlink_metadata(entry.path())?) && entry.file_type().is_dir() {
            entries.skip_current_dir();
        }
    }
    Ok(())
}

fn providers_at(path: &Path) -> Vec<ProviderId> {
    let mut providers = vec![];
    for (provider, manifests) in [
        (ProviderId::Js, &["package.json"][..]),
        (
            ProviderId::Py,
            &["pyproject.toml", "requirements.txt", "setup.py", "Pipfile"][..],
        ),
        (
            ProviderId::Jvm,
            &["pom.xml", "build.gradle", "build.gradle.kts"][..],
        ),
        (ProviderId::Rust, &["Cargo.toml"][..]),
        (ProviderId::Go, &["go.mod", "go.work"][..]),
    ] {
        if manifests.iter().any(|name| path.join(name).is_file()) {
            providers.push(provider);
        }
    }
    providers
}

fn artifacts_at(path: &Path, providers: &[ProviderId]) -> Vec<Artifact> {
    let mut specs = vec![];
    if providers.contains(&ProviderId::Js) {
        specs.extend([
            (
                "node_modules",
                "dependencies",
                "Install dependencies using the project's lockfile.",
            ),
            (".next", "build", "Run the Next.js build again."),
            (".nuxt", "build", "Run the Nuxt build again."),
            (".svelte-kit", "build", "Run the SvelteKit build again."),
        ]);
    }
    if providers.contains(&ProviderId::Py) {
        if path.join(".venv/pyvenv.cfg").is_file() {
            specs.push((
                ".venv",
                "environment",
                "Recreate the virtual environment and sync the project's dependencies.",
            ));
        }
        specs.extend([
            (".pytest_cache", "cache", "Run pytest again."),
            (".ruff_cache", "cache", "Run Ruff again."),
            (".mypy_cache", "cache", "Run mypy again."),
            (
                "__pycache__",
                "cache",
                "Python recreates bytecode when needed.",
            ),
        ]);
    }
    if providers.contains(&ProviderId::Rust) {
        specs.push(("target", "build", "Run cargo build again."));
    }
    if providers.contains(&ProviderId::Jvm) {
        if path.join("pom.xml").is_file() {
            specs.push(("target", "build", "Run the Maven build again."));
        }
        if path.join("build.gradle").is_file() || path.join("build.gradle.kts").is_file() {
            specs.extend([
                ("build", "build", "Run the Gradle build again."),
                (".gradle", "cache", "Run Gradle again."),
            ]);
        }
    }
    specs
        .into_iter()
        .filter_map(|(name, kind, restore)| {
            let target = path.join(name);
            let meta = fs::symlink_metadata(&target).ok()?;
            if !meta.is_dir() || is_link(&meta) {
                return None;
            }
            Some(Artifact {
                id: id_for("artifact", &target),
                name: name.into(),
                path: target.clone(),
                kind: kind.into(),
                size: Default::default(),
                restore: restore.into(),
                can_clean: ownership_issue(&target, name).is_none(),
                cleanup_issue: ownership_issue(&target, name),
            })
        })
        .collect()
}

pub fn is_project_artifact(project: &Project, artifact: &Artifact) -> bool {
    let Some(parent) = artifact.path.parent() else {
        return false;
    };
    if !parent.starts_with(&project.path)
        || reject_links(parent).is_err()
        || !git::checkout(&project.path).is_ok_and(|checkout| checkout.is_some())
        || parent
            .ancestors()
            .take_while(|p| *p != project.path)
            .any(|p| p.join(".git").exists())
    {
        return false;
    }
    let providers = providers_at(parent);
    artifacts_at(parent, &providers)
        .iter()
        .any(|a| a.path == artifact.path && a.id == artifact.id && a.can_clean)
}

fn pins_at(path: &Path) -> BTreeMap<String, String> {
    let mut pins = BTreeMap::new();
    for name in [
        ".nvmrc",
        ".node-version",
        ".python-version",
        ".java-version",
        ".tool-versions",
        ".sdkmanrc",
        "rust-toolchain",
        "rust-toolchain.toml",
        "go.mod",
    ] {
        if let Ok(content) = read_small(&path.join(name), 16_384) {
            pins.insert(name.into(), content.trim().to_owned());
        }
    }
    pins
}

pub fn scan(
    settings: &Settings,
    cancel: &CancellationToken,
    progress: ProgressSink,
    job_id: &str,
) -> Result<ScanResult> {
    scan_roots(
        settings,
        canonical_roots(settings)?,
        cancel,
        progress,
        job_id,
    )
}

pub(crate) fn scan_roots(
    settings: &Settings,
    roots: Vec<PathBuf>,
    cancel: &CancellationToken,
    progress: ProgressSink,
    job_id: &str,
) -> Result<ScanResult> {
    validate_settings(settings)?;
    let started = Instant::now();
    let ignored = exclusions(settings)?;
    let mut roots: VecDeque<_> = roots.into();
    let protected: Vec<_> = settings
        .protected_projects
        .iter()
        .filter_map(|p| fs::canonicalize(p).ok())
        .collect();
    let mut result = ScanResult::default();
    let mut index: HashMap<PathBuf, usize> = HashMap::new();
    let mut artifacts = std::collections::HashSet::new();
    let mut repositories = std::collections::HashSet::new();
    let mut last_report = Instant::now();
    progress(Progress {
        job_id: job_id.into(),
        stage: "discover".into(),
        completed: 0,
        total: None,
        message: "Scanning Git repositories".into(),
    });

    while let Some(root) = roots.pop_front() {
        if index.contains_key(&root) {
            continue;
        }
        let mut iter = WalkDir::new(&root)
            .follow_links(false)
            .max_open(16)
            .into_iter();
        while let Some(entry) = iter.next() {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    if let Some(path) = e.path()
                        && let Some(owner) = path.ancestors().find_map(|p| index.get(p).copied())
                    {
                        result.projects[owner].activity_complete = false;
                    }
                    if result.issues.len() < 100 {
                        result.issues.push(e.to_string());
                    }
                    continue;
                }
            };
            result.visited += 1;
            let path = entry.path();
            let meta = match fs::symlink_metadata(path) {
                Ok(meta) => meta,
                Err(e) => {
                    if result.issues.len() < 100 {
                        result.issues.push(format!("{}: {e}", path.display()));
                    }
                    continue;
                }
            };
            if is_link(&meta) {
                if meta.is_dir() {
                    iter.skip_current_dir();
                }
                continue;
            }
            if entry.depth() > 0
                && (ignored.is_match(entry.file_name())
                    || ignored.is_match(path.strip_prefix(&root).unwrap_or(path)))
            {
                if meta.is_dir() {
                    iter.skip_current_dir();
                }
                continue;
            }
            if meta.is_dir() {
                if index.contains_key(path) {
                    iter.skip_current_dir();
                    continue;
                }
                if artifacts.contains(path)
                    || matches!(
                        entry.file_name().to_str(),
                        Some(".git" | ".svn" | ".hg" | "node_modules")
                    )
                {
                    iter.skip_current_dir();
                    continue;
                }
                let checkout = match git::checkout(path) {
                    Ok(value) => value,
                    Err(error) => {
                        if result.issues.len() < 100 {
                            result.issues.push(format!("{}: {error}", path.display()));
                        }
                        iter.skip_current_dir();
                        continue;
                    }
                };
                if let Some(checkout) = checkout {
                    if repositories.insert(checkout.repository.id.clone()) {
                        match git::worktrees(&checkout, cancel) {
                            Ok(worktrees) => {
                                if configured_scope(&checkout.repository.path, settings)?.is_some()
                                {
                                    for worktree in &worktrees.entries {
                                        if worktree.issue.is_none()
                                            && project_in_scope(&worktree.path, settings)?
                                        {
                                            roots.push_back(worktree.path.clone());
                                        }
                                    }
                                }
                                result.worktrees.extend(worktrees.entries);
                                result.issues.extend(worktrees.issues);
                            }
                            Err(Error::Cancelled) => return Err(Error::Cancelled),
                            Err(error) => result.issues.push(error.to_string()),
                        }
                    }
                    index.insert(path.to_path_buf(), result.projects.len());
                    result.projects.push(Project {
                        id: id_for("project", path),
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        path: path.into(),
                        providers: vec![],
                        last_active: modified(&checkout.git_dir.join("logs/HEAD")),
                        activity_complete: true,
                        branch: checkout.branch,
                        pins: pins_at(path),
                        protected: protected.iter().any(|p| path.starts_with(p)),
                        artifacts: vec![],
                        repository: Some(checkout.repository),
                        is_worktree: checkout.is_worktree,
                    });
                }
                if let Some(owner) = path.ancestors().find_map(|p| index.get(p).copied()) {
                    let providers = providers_at(path);
                    let found = artifacts_at(path, &providers);
                    artifacts.extend(found.iter().filter(|a| a.can_clean).map(|a| a.path.clone()));
                    let project = &mut result.projects[owner];
                    for provider in providers {
                        if !project.providers.contains(&provider) {
                            project.providers.push(provider);
                        }
                    }
                    project.artifacts.extend(found);
                }
            } else if meta.is_file()
                && entry.file_name() != ".git"
                && let Some(owner) = path.parent().and_then(|p| {
                    p.ancestors()
                        .find_map(|ancestor| index.get(ancestor).copied())
                })
            {
                let project = &mut result.projects[owner];
                match meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|t| t.as_secs())
                {
                    Some(time) => {
                        project.last_active = Some(project.last_active.unwrap_or(0).max(time))
                    }
                    None => project.activity_complete = false,
                }
            }
            if last_report.elapsed().as_millis() >= 150 {
                progress(Progress {
                    job_id: job_id.into(),
                    stage: "discover".into(),
                    completed: result.visited,
                    total: None,
                    message: format!("Scanning projects: {} paths checked", result.visited),
                });
                last_report = Instant::now();
            }
        }
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    progress(Progress {
        job_id: job_id.into(),
        stage: "measure-projects".into(),
        completed: 0,
        total: Some(result.projects.len() as u64),
        message: "Measuring project artifacts".into(),
    });
    let issues: Vec<String> = pool.install(|| {
        result
            .projects
            .par_iter_mut()
            .flat_map(|project| {
                let mut errors = vec![];
                for artifact in &mut project.artifacts {
                    if let Err(error) = artifact_in_scope(&artifact.path, settings, cancel) {
                        artifact.can_clean = false;
                        artifact.cleanup_issue = Some(error.to_string());
                    }
                    match measure(&artifact.path, cancel) {
                        Ok(size) => artifact.size = size,
                        Err(e) => errors.push(format!("{}: {e}", artifact.path.display())),
                    }
                }
                errors
            })
            .collect()
    });
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    result.issues.extend(issues);
    // Measure the entire checkout, including source and ignored files. Only a
    // successfully scanned, in-scope worktree may become a measurement root.
    progress(Progress {
        job_id: job_id.into(),
        stage: "measure-worktrees".into(),
        completed: 0,
        total: Some(result.worktrees.len() as u64),
        message: "Measuring linked worktrees".into(),
    });
    let issues: Vec<String> = pool.install(|| {
        result
            .worktrees
            .par_iter_mut()
            .filter_map(|worktree| {
                if worktree.issue.is_some() || !result.projects.iter().any(|p| p.id == worktree.id)
                {
                    return None;
                }
                match measure(&worktree.path, cancel) {
                    Ok(size) => {
                        worktree.size = Some(size);
                        None
                    }
                    Err(error) => Some(format!("{}: {error}", worktree.path.display())),
                }
            })
            .collect()
    });
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    result.issues.extend(issues);
    result.projects.sort_by_key(|project| project.last_active);
    result.elapsed_ms = started.elapsed().as_millis();
    progress(Progress {
        job_id: job_id.into(),
        stage: "projects-complete".into(),
        completed: result.projects.len() as u64,
        total: Some(result.projects.len() as u64),
        message: format!("{} entries in {} ms", result.visited, result.elapsed_ms),
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::silent_progress;

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "frontend/node_modules/a",
            "backend/target/debug",
            "excluded",
        ] {
            fs::create_dir_all(root.path().join(name)).unwrap();
        }
        for (name, content) in [
            ("frontend/package.json", "{}"),
            ("frontend/index.ts", "source"),
            ("frontend/node_modules/a/package.json", "{}"),
            ("frontend/node_modules/a/lib.js", "generated"),
            ("backend/Cargo.toml", "[package]"),
            ("backend/target/debug/program", "binary"),
            ("excluded/go.mod", "module x"),
        ] {
            fs::write(root.path().join(name), content).unwrap();
        }
        git::init(&root.path().join("frontend"));
        git::init(&root.path().join("backend"));
        git::init(&root.path().join("excluded"));
        root
    }

    #[test]
    fn reports_project_phases_without_completing_the_entire_refresh() {
        let root = fixture();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = events.clone();
        let result = scan(
            &Settings {
                roots: vec![fs::canonicalize(root.path()).unwrap()],
                ..Settings::default()
            },
            &CancellationToken::new(),
            std::sync::Arc::new(move |event| captured.lock().unwrap().push(event)),
            "test",
        )
        .unwrap();
        let events = events.lock().unwrap();
        assert_eq!(events.first().unwrap().stage, "discover");
        assert!(events.iter().any(|event| event.stage == "measure-projects"));
        assert!(
            events
                .iter()
                .any(|event| event.stage == "measure-worktrees")
        );
        let last = events.last().unwrap();
        assert_eq!(last.stage, "projects-complete");
        assert_eq!(last.completed, result.projects.len() as u64);
        assert!(!events.iter().any(|event| event.stage == "complete"));
    }

    #[test]
    fn finds_projects_without_entering_dependencies_and_deduplicates_roots() {
        let root = fixture();
        let settings = Settings {
            roots: vec![
                fs::canonicalize(root.path()).unwrap(),
                fs::canonicalize(root.path().join("frontend")).unwrap(),
            ],
            excludes: vec!["excluded".into()],
            ..Settings::default()
        };
        let result = scan(
            &settings,
            &CancellationToken::new(),
            silent_progress(),
            "test",
        )
        .unwrap();
        assert_eq!(result.projects.len(), 2);
        let frontend = result
            .projects
            .iter()
            .find(|p| p.name == "frontend")
            .unwrap();
        assert_eq!(frontend.artifacts[0].size.bytes, 11);
        assert_eq!(frontend.artifacts[0].size.files, 2);
        assert!(frontend.activity_complete);
    }

    #[test]
    fn cancellation_never_returns_a_complete_snapshot() {
        let root = fixture();
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(matches!(
            scan(
                &Settings {
                    roots: vec![fs::canonicalize(root.path()).unwrap()],
                    ..Settings::default()
                },
                &cancel,
                silent_progress(),
                "test"
            ),
            Err(Error::Cancelled)
        ));
    }

    #[test]
    fn an_unmarked_virtual_environment_is_not_cleanup_eligible() {
        let root = fixture();
        fs::write(root.path().join("pyproject.toml"), "[project]").unwrap();
        fs::create_dir(root.path().join(".venv")).unwrap();
        assert!(artifacts_at(root.path(), &[ProviderId::Py]).is_empty());
    }

    #[test]
    fn monorepos_aggregate_artifacts_but_nested_repositories_remain_independent() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("workspace");
        for folder in [
            "apps/web/node_modules",
            "services/api",
            "vendor/library",
            "loose",
        ] {
            fs::create_dir_all(repo.join(folder)).unwrap();
        }
        git::init(&repo);
        git::init(&repo.join("vendor/library"));
        for (file, content) in [
            ("apps/web/package.json", "{}"),
            ("apps/web/node_modules/.package-lock.json", "{}"),
            ("services/api/pyproject.toml", "[project]"),
            ("vendor/library/go.mod", "module library"),
        ] {
            fs::write(repo.join(file), content).unwrap();
        }
        fs::write(root.path().join("package.json"), "{}").unwrap();
        let settings = Settings {
            roots: vec![fs::canonicalize(root.path()).unwrap()],
            ..Default::default()
        };
        let scanned = scan(
            &settings,
            &CancellationToken::new(),
            silent_progress(),
            "test",
        )
        .unwrap();
        assert_eq!(scanned.projects.len(), 2);
        let workspace = scanned
            .projects
            .iter()
            .find(|p| p.name == "workspace")
            .unwrap();
        assert!(workspace.providers.contains(&ProviderId::Js));
        assert!(workspace.providers.contains(&ProviderId::Py));
        assert!(!workspace.providers.contains(&ProviderId::Go));
        assert_eq!(workspace.artifacts.len(), 1);
        assert!(is_project_artifact(workspace, &workspace.artifacts[0]));
        // A newly introduced repository boundary invalidates a previously planned artifact.
        git::init(&repo.join("apps/web"));
        assert!(!is_project_artifact(workspace, &workspace.artifacts[0]));
    }

    #[test]
    fn a_main_repository_in_scope_includes_its_registered_worktrees() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("main repo");
        let linked = root.path().join("feature checkout");
        fs::create_dir(&repo).unwrap();
        git::init(&repo);
        for args in [
            vec!["commit", "--quiet", "--allow-empty", "-m", "fixture"],
            vec![
                "worktree",
                "add",
                "--quiet",
                "-b",
                "feature",
                linked.to_str().unwrap(),
            ],
            vec!["worktree", "lock", linked.to_str().unwrap()],
        ] {
            let output = std::process::Command::new("git")
                .current_dir(&repo)
                .args([
                    "-c",
                    "core.hooksPath=",
                    "-c",
                    "commit.gpgSign=false",
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let mut settings = Settings {
            roots: vec![fs::canonicalize(&repo).unwrap()],
            ..Default::default()
        };
        let scanned = scan(
            &settings,
            &CancellationToken::new(),
            silent_progress(),
            "test",
        )
        .unwrap();
        assert_eq!(scanned.projects.len(), 2);
        assert_eq!(scanned.worktrees.len(), 1);
        let worktree = &scanned.worktrees[0];
        assert_eq!(worktree.branch.as_deref(), Some("feature"));
        assert!(worktree.locked);
        assert!(worktree.issue.is_none());
        assert!(
            scanned
                .projects
                .iter()
                .all(|p| p.repository.as_ref().unwrap().id == worktree.repository.id)
        );
        assert!(project_in_scope(&fs::canonicalize(&linked).unwrap(), &settings).unwrap());
        let mut removed_scope = settings.clone();
        removed_scope.roots.clear();
        assert!(!project_in_scope(&fs::canonicalize(&linked).unwrap(), &removed_scope).unwrap());
        settings.roots.push(fs::canonicalize(linked).unwrap());
        let scanned = scan(
            &settings,
            &CancellationToken::new(),
            silent_progress(),
            "test",
        )
        .unwrap();
        assert_eq!(scanned.projects.len(), 2);
        let linked = scanned.projects.iter().find(|p| p.is_worktree).unwrap();
        assert_eq!(linked.id, worktree.id);
        assert_eq!(linked.branch.as_deref(), Some("feature"));
    }

    #[test]
    fn worktree_discovery_keeps_healthy_entries_and_honors_exclusions() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("main");
        let linked = root.path().join("feature");
        fs::create_dir(&repo).unwrap();
        git::init(&repo);
        git::add_worktree(&repo, &linked);
        fs::create_dir(repo.join(".git/worktrees/broken")).unwrap();
        let mut settings = Settings {
            roots: vec![fs::canonicalize(&repo).unwrap()],
            ..Default::default()
        };
        let run = |settings: &Settings| {
            scan(
                settings,
                &CancellationToken::new(),
                silent_progress(),
                "test",
            )
            .unwrap()
        };
        let scanned = run(&settings);
        assert_eq!(scanned.projects.len(), 2);
        assert_eq!(scanned.worktrees.len(), 1);
        assert!(!scanned.issues.is_empty());

        settings.excludes.push("feature".into());
        let scanned = run(&settings);
        assert_eq!(scanned.projects.len(), 1);
        assert!(!scanned.projects[0].is_worktree);
        assert_eq!(scanned.worktrees.len(), 1);
        assert!(!project_in_scope(&fs::canonicalize(linked).unwrap(), &settings).unwrap());
    }

    #[test]
    fn invalid_gitfiles_are_reported_without_scanning_their_target() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(".git"), "gitdir: missing-metadata").unwrap();
        fs::write(root.path().join("package.json"), "{}").unwrap();
        let scanned = scan(
            &Settings {
                roots: vec![fs::canonicalize(root.path()).unwrap()],
                ..Default::default()
            },
            &CancellationToken::new(),
            silent_progress(),
            "test",
        )
        .unwrap();
        assert!(scanned.projects.is_empty());
        assert!(!scanned.issues.is_empty());
    }
}
