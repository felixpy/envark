use super::*;
use crate::filesystem::{contained_directory, reject_links};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

pub(crate) fn direct(cache: &Cache) -> bool {
    matches!(
        cache.strategy.as_str(),
        "cargo-registry"
            | "cargo-git"
            | "gradle-caches-files"
            | "gradle-dists-files"
            | "pnpm-store-files"
    )
}

pub(super) fn pnpm_roots(ctx: &Context) -> Vec<PathBuf> {
    let mut roots = vec![
        ctx.data.join("pnpm/store"),
        ctx.home.join("Library/pnpm/store"),
        ctx.home.join(".local/share/pnpm/store"),
        ctx.home.join(".pnpm-store"),
        ctx.home.join("AppData/Local/pnpm/store"),
    ];
    if let Some(path) = super::cache_discovery::configured_path(ctx, "store-dir") {
        roots.push(path);
    }
    roots.sort();
    roots.dedup();
    roots
}

pub(super) fn store_version(name: &str) -> bool {
    name.strip_prefix('v')
        .is_some_and(|version| !version.is_empty() && version.bytes().all(|c| c.is_ascii_digit()))
}

// Inspect only layout markers, never recursively traverse a store during discovery.
// A directory's name alone is not proof that it contains disposable dependencies.
fn store_layout(path: &Path, versions: bool) -> Result<bool> {
    let mut recognized = false;
    for (index, entry) in std::fs::read_dir(path)?.enumerate() {
        if index >= 128 {
            return Ok(false);
        }
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Ok(false);
        }
        if kind.is_dir() && ["files", "index"].contains(&name.as_ref()) {
            recognized = true;
        } else if versions && kind.is_dir() && store_version(&name) {
            if store_layout(&entry.path(), false)? {
                recognized = true;
            } else if std::fs::read_dir(entry.path())?.next().is_some() {
                return Ok(false);
            }
        } else if !(kind.is_file()
            && ["store.json", "server.json", "lock", ".lock"].contains(&name.as_ref()))
        {
            return Ok(false);
        }
    }
    Ok(recognized)
}

pub(crate) fn root(ctx: &Context, cache: &Cache) -> Result<PathBuf> {
    if cache.strategy.starts_with("cargo-") {
        return super::cache_cleanup::cargo_root(ctx, cache);
    }
    reject_links(&cache.path)?;
    if cache.strategy.starts_with("gradle-") {
        let root = super::cache_cleanup::gradle_home(ctx);
        let expected = root.join(match cache.strategy.as_str() {
            "gradle-caches-files" => "caches",
            "gradle-dists-files" => "wrapper/dists",
            _ => {
                return Err(Error::InvalidInput(
                    "Unknown direct Gradle cleanup strategy.".into(),
                ));
            }
        });
        reject_links(&expected)?;
        if std::fs::canonicalize(expected)? != contained_directory(&root, &cache.path)? {
            return Err(Error::Conflict(
                "The Gradle cache directory changed.".into(),
            ));
        }
        return Ok(std::fs::canonicalize(root)?);
    }
    if cache.strategy == "pnpm-store-files" {
        let real = std::fs::canonicalize(&cache.path)?;
        for candidate in pnpm_roots(ctx) {
            if reject_links(&candidate).is_err() {
                continue;
            }
            let Ok(candidate) = std::fs::canonicalize(candidate) else {
                continue;
            };
            if (real == candidate
                || (real.parent() == Some(candidate.as_path())
                    && real
                        .file_name()
                        .is_some_and(|n| store_version(&n.to_string_lossy()))))
                && store_layout(&real, true)?
            {
                let parent = real
                    .parent()
                    .ok_or_else(|| Error::unsafe_path(&real, "a store must have a parent"))?;
                contained_directory(parent, &real)?;
                if [&ctx.home, &ctx.data, &ctx.cache]
                    .iter()
                    .any(|path| path.canonicalize().is_ok_and(|p| p == real))
                {
                    return Err(Error::unsafe_path(&real, "shared roots cannot be removed"));
                }
                return Ok(parent.to_owned());
            }
        }
        return Err(Error::unsafe_path(
            &cache.path,
            "unrecognized pnpm store layout; keep files whose ownership is unconfirmed",
        ));
    }
    Err(Error::InvalidInput(
        "Unknown direct cache cleanup strategy.".into(),
    ))
}

fn uses_cache(strategy: &str, name: &str, args: &[String]) -> bool {
    if strategy.starts_with("gradle-") {
        ["gradle", "gradle.exe", "gradle.cmd"].contains(&name)
            || args
                .iter()
                .any(|arg| arg.starts_with("org.gradle.") || arg.ends_with("gradle-wrapper.jar"))
            || (name.starts_with("java") && args.is_empty())
    } else {
        let is_pnpm = |arg: &str| {
            arg.rsplit(['/', '\\']).next().is_some_and(|name| {
                [
                    "pnpm", "pnpm.exe", "pnpm.cmd", "pnpm.ps1", "pnpm.cjs", "pnpm.js",
                ]
                .contains(&name)
            })
        };
        let direct = ["pnpm", "pnpm.exe", "pnpm.cmd"].contains(&name);
        let script = args.iter().position(|arg| is_pnpm(arg));
        if !direct && script.is_none() {
            return false;
        }
        let command = script.map_or(args, |index| &args[index + 1..]);
        // Discovery probes run concurrently with eligibility checks. Queries do
        // not use package contents and must not block cleanup of an old store.
        !(command.len() == 1 && ["--version", "-v"].contains(&command[0].as_str())
            || command.first().is_some_and(|arg| arg == "store")
                && command.get(1).is_some_and(|arg| arg == "path"))
    }
}

fn gradle_home_identity(path: &Path) -> Option<PathBuf> {
    // Resolve existing ancestors even after a snapshot's Gradle home has been
    // deleted. Missing leaves are not evidence that it uses every other home.
    // Relative, inaccessible and dangling-link paths remain unconfirmed.
    if !path.is_absolute() {
        return None;
    }
    let mut ancestor = path;
    let mut missing = vec![];
    loop {
        match ancestor.canonicalize() {
            Ok(mut resolved) => {
                for name in missing.into_iter().rev() {
                    resolved.push(name);
                }
                return Some(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if ancestor.symlink_metadata().is_ok()
                    || path
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                {
                    return None;
                }
                missing.push(ancestor.file_name()?.to_owned());
                ancestor = ancestor.parent()?;
            }
            Err(_) => return None,
        }
    }
}

pub(crate) fn check_idle(cache: &Cache) -> Result<()> {
    if cache.strategy.starts_with("cargo-") {
        return Ok(());
    }
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    for process in system.processes().values() {
        let args: Vec<_> = process
            .cmd()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        if cache.strategy.starts_with("gradle-") {
            let explicit_home = args
                .windows(2)
                .find_map(|pair| {
                    ["-g", "--gradle-user-home"]
                        .contains(&pair[0].as_str())
                        .then(|| PathBuf::from(&pair[1]))
                })
                .or_else(|| {
                    args.iter()
                        .find_map(|arg| arg.strip_prefix("--gradle-user-home=").map(PathBuf::from))
                });
            let cache_home = if cache.strategy == "gradle-caches-files" {
                cache.path.parent()
            } else {
                cache.path.parent().and_then(Path::parent)
            };
            if let Some(home) = explicit_home.and_then(|path| gradle_home_identity(&path))
                && cache_home
                    .and_then(gradle_home_identity)
                    .is_some_and(|cache_home| cache_home != home)
            {
                continue;
            }
        }
        if uses_cache(&cache.strategy, &process.name().to_string_lossy(), &args) {
            return Err(Error::Conflict(format!(
                "{} is using this cache. Retry cleanup when its build, download, or daemon finishes.",
                if cache.strategy.starts_with("gradle-") {
                    "Gradle"
                } else {
                    "pnpm"
                }
            )));
        }
    }
    Ok(())
}

pub(crate) async fn check(ctx: &Context, cache: &Cache) -> Result<()> {
    if ctx.cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    root(ctx, cache)?;
    if cache.strategy == "pnpm-store-files"
        && let Some(current) = ctx
            .cache_path("pnpm", &["store", "path"])
            .await
            .and_then(|path| path.canonicalize().ok())
        && (current == cache.path || current.starts_with(&cache.path))
    {
        return Err(Error::Conflict(
            "This is the current pnpm store. Refresh caches to use native store pruning.".into(),
        ));
    }
    let cache = cache.clone();
    tokio::task::spawn_blocking(move || check_idle(&cache))
        .await
        .map_err(|error| Error::Unavailable(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleted_gradle_homes_keep_their_identity_without_resolving_uncertain_paths() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let home = root.join("deleted/gradle-home");
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(gradle_home_identity(&home), Some(home.clone()));
        std::fs::remove_dir_all(root.join("deleted")).unwrap();
        assert_eq!(gradle_home_identity(&home), Some(home.clone()));
        assert!(gradle_home_identity(Path::new("relative/gradle-home")).is_none());
        assert!(gradle_home_identity(&root.join("missing/../gradle-home")).is_none());

        #[cfg(unix)]
        {
            let alias = root.join("alias");
            std::os::unix::fs::symlink(&root, &alias).unwrap();
            assert_eq!(
                gradle_home_identity(&alias.join("deleted/gradle-home")),
                Some(home)
            );
            let dangling = root.join("dangling");
            std::os::unix::fs::symlink(root.join("missing"), &dangling).unwrap();
            assert!(gradle_home_identity(&dangling).is_none());
            assert!(gradle_home_identity(&dangling.join("gradle-home")).is_none());
        }
    }

    #[test]
    fn active_gradle_and_pnpm_processes_block_direct_removal() {
        for name in [
            "org.gradle.launcher.daemon.bootstrap.GradleDaemon",
            "org.gradle.wrapper.GradleWrapperMain",
            "org.gradle.launcher.GradleMain",
        ] {
            assert!(uses_cache("gradle-caches-files", "java", &[name.into()]));
        }
        assert!(uses_cache("gradle-dists-files", "java", &[]));
        assert!(!uses_cache(
            "gradle-caches-files",
            "java",
            &["-jar".into(), "language-server.jar".into()]
        ));
        assert!(uses_cache(
            "pnpm-store-files",
            "node",
            &["/bin/pnpm.cjs".into(), "install".into()]
        ));
        assert!(!uses_cache("pnpm-store-files", "node", &["app.js".into()]));
        assert!(uses_cache(
            "pnpm-store-files",
            "pnpm.exe",
            &["install".into()]
        ));
        assert!(uses_cache(
            "pnpm-store-files",
            "node.exe",
            &[r"C:\pnpm\pnpm.cjs".into()]
        ));
    }

    #[test]
    fn read_only_pnpm_probes_do_not_occupy_dependency_stores() {
        for (name, argv) in [
            ("pnpm", vec!["pnpm", "store", "path"]),
            (
                "pnpm.exe",
                vec!["store", "path", "--store-dir", r"C:\cache"],
            ),
            ("node", vec!["node", "/tools/pnpm.cjs", "store", "path"]),
            (
                "node.exe",
                vec![r"C:\node.exe", r"C:\tools\pnpm.cjs", "--version"],
            ),
            ("sh", vec!["/bin/sh", "/tools/pnpm", "--version"]),
            ("pnpm", vec!["/bin/sh", "/tools/pnpm", "--version"]),
        ] {
            let args = argv.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(
                !uses_cache("pnpm-store-files", name, &args),
                "{name}: {args:?}"
            );
        }
        for command in [
            vec!["store", "prune"],
            vec!["install"],
            vec!["add", "package"],
            vec!["--filter", "workspace", "install"],
        ] {
            let args = std::iter::once("/tools/pnpm.cjs")
                .chain(command)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(uses_cache("pnpm-store-files", "node", &args), "{args:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn live_pnpm_queries_do_not_block_cleanup_but_installs_do() {
        use std::io::BufRead;
        use std::os::unix::fs::PermissionsExt;
        let _pnpm = super::super::CACHE_PROCESS_TEST_LOCK.lock().unwrap();
        struct ChildGuard(std::process::Child);
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap();
        let binary = path.join("pnpm");
        std::fs::write(&binary, "#!/bin/sh\nprintf 'ready\\n'\nread input\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cache = cache(ProviderId::Js, "pnpm", path, "pnpm-store-files", true).unwrap();
        for (args, blocked) in [
            (vec!["--version"], false),
            (vec!["store", "path"], false),
            (vec!["install"], true),
        ] {
            let mut child = ChildGuard(
                std::process::Command::new(&binary)
                    .args(args)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
            let mut line = String::new();
            std::io::BufReader::new(child.0.stdout.take().unwrap())
                .read_line(&mut line)
                .unwrap();
            assert_eq!(line.trim(), "ready");
            let result = check_idle(&cache);
            assert_eq!(result.is_err(), blocked, "{result:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn live_gradle_process_is_detected_without_a_jdk() {
        let _processes = super::super::CACHE_PROCESS_TEST_LOCK.lock().unwrap();
        use std::io::BufRead;
        struct ChildGuard(std::process::Child);
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let cache_path = temp.path().canonicalize().unwrap().join("caches");
        std::fs::create_dir_all(&cache_path).unwrap();
        let mut child = ChildGuard(
            std::process::Command::new("/bin/sh")
                .args([
                    "-c",
                    "printf 'ready\\n'; read input",
                    "org.gradle.launcher.daemon.bootstrap.GradleDaemon",
                    "--gradle-user-home",
                    cache_path.parent().unwrap().to_str().unwrap(),
                ])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut line = String::new();
        std::io::BufReader::new(child.0.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line.trim(), "ready");
        let cache = cache(
            ProviderId::Jvm,
            "Gradle",
            cache_path,
            "gradle-caches-files",
            true,
        )
        .unwrap();
        assert!(
            matches!(check_idle(&cache), Err(Error::Conflict(message)) if message.contains("using this cache"))
        );
        let other = tempfile::tempdir().unwrap();
        let mut unrelated = cache.clone();
        unrelated.path = other.path().canonicalize().unwrap().join("caches");
        check_idle(&unrelated).unwrap();

        // A process snapshot can outlive a temporary Gradle home. Losing that
        // directory must not make the process occupy every other Gradle cache.
        std::fs::remove_dir(&cache.path).unwrap();
        std::fs::remove_dir(cache.path.parent().unwrap()).unwrap();
        check_idle(&unrelated).unwrap();
        assert!(check_idle(&cache).is_err());
    }

    #[test]
    fn legacy_stores_must_have_cache_markers_and_no_unrelated_top_level_files() {
        let temp = tempfile::tempdir().unwrap();
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = temp.path().canonicalize().unwrap();
        let path = ctx.home.join(".pnpm-store");
        std::fs::create_dir_all(path.join("v3/files")).unwrap();
        let cache = cache(
            ProviderId::Js,
            "pnpm",
            path.clone(),
            "pnpm-store-files",
            true,
        )
        .unwrap();
        assert!(root(&ctx, &cache).is_ok());
        std::fs::create_dir_all(path.join("v10/user-project")).unwrap();
        std::fs::write(path.join("v10/user-project/source.rs"), "keep").unwrap();
        assert!(root(&ctx, &cache).is_err());
        std::fs::remove_dir_all(path.join("v10")).unwrap();
        std::fs::write(path.join("user-notes.txt"), "keep").unwrap();
        assert!(root(&ctx, &cache).is_err());
        std::fs::remove_file(path.join("user-notes.txt")).unwrap();
        std::fs::remove_dir_all(path.join("v3")).unwrap();
        assert!(root(&ctx, &cache).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&ctx.home, path.join("v3")).unwrap();
            assert!(root(&ctx, &cache).is_err());
        }
    }
}
