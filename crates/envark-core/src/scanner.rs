use crate::{
    Error, Result,
    filesystem::{id_for, is_link, measure, modified, read_small, reject_links},
    model::{Artifact, Progress, ProgressSink, Project, ProviderId, Settings},
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    time::{Instant, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

#[derive(Debug, Default)]
pub struct ScanResult {
    pub projects: Vec<Project>,
    pub issues: Vec<String>,
    pub visited: u64,
    pub elapsed_ms: u128,
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
                path: target,
                kind: kind.into(),
                size: Default::default(),
                restore: restore.into(),
            })
        })
        .collect()
}

pub fn is_project_artifact(project: &Project, artifact: &Artifact) -> bool {
    let providers = providers_at(&project.path);
    artifacts_at(&project.path, &providers)
        .iter()
        .any(|a| a.path == artifact.path && a.id == artifact.id)
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

fn branch_at(path: &Path) -> Option<String> {
    // Git worktree .git files are not followed outside the selected scan root.
    read_small(&path.join(".git/HEAD"), 1024).ok().map(|s| {
        s.trim()
            .strip_prefix("ref: refs/heads/")
            .unwrap_or(s.trim())
            .to_owned()
    })
}

pub fn scan(
    settings: &Settings,
    cancel: &CancellationToken,
    progress: ProgressSink,
    job_id: &str,
) -> Result<ScanResult> {
    validate_settings(settings)?;
    let started = Instant::now();
    let ignored = exclusions(settings)?;
    let mut roots = settings
        .roots
        .iter()
        .map(fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
    roots.sort();
    roots.dedup();
    let all_roots = roots.clone();
    roots.retain(|root| {
        !all_roots
            .iter()
            .any(|other| other != root && root.starts_with(other))
    });
    let protected: Vec<_> = settings
        .protected_projects
        .iter()
        .filter_map(|p| fs::canonicalize(p).ok())
        .collect();
    let mut result = ScanResult::default();
    let mut index: HashMap<PathBuf, usize> = HashMap::new();
    let mut artifacts = std::collections::HashSet::new();
    let mut last_report = Instant::now();

    for root in roots {
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
                if artifacts.contains(path)
                    || matches!(
                        entry.file_name().to_str(),
                        Some(".git" | ".svn" | ".hg" | "node_modules")
                    )
                {
                    iter.skip_current_dir();
                    continue;
                }
                let providers = providers_at(path);
                if !providers.is_empty() {
                    let found = artifacts_at(path, &providers);
                    artifacts.extend(found.iter().map(|a| a.path.clone()));
                    index.insert(path.to_path_buf(), result.projects.len());
                    result.projects.push(Project {
                        id: id_for("project", path),
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        path: path.into(),
                        providers,
                        last_active: modified(&path.join(".git/logs/HEAD")),
                        activity_complete: true,
                        branch: branch_at(path),
                        pins: pins_at(path),
                        protected: protected.iter().any(|p| path.starts_with(p)),
                        artifacts: found,
                    });
                }
            } else if meta.is_file()
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
                    message: format!("{} projects", result.projects.len()),
                });
                last_report = Instant::now();
            }
        }
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    let issues: Vec<String> = pool.install(|| {
        result
            .projects
            .par_iter_mut()
            .flat_map(|project| {
                let mut errors = vec![];
                for artifact in &mut project.artifacts {
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
    result.projects.sort_by_key(|project| project.last_active);
    result.elapsed_ms = started.elapsed().as_millis();
    progress(Progress {
        job_id: job_id.into(),
        stage: "complete".into(),
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
        root
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
}
