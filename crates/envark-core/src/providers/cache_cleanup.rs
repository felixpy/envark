use super::*;
use crate::filesystem::{contained_directory, reject_links};

pub(crate) fn cargo_home(ctx: &Context) -> PathBuf {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".cargo"))
}

pub(crate) fn gradle_home(ctx: &Context) -> PathBuf {
    std::env::var_os("GRADLE_USER_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".gradle"))
}

pub(crate) fn cargo_root(ctx: &Context, cache: &Cache) -> Result<PathBuf> {
    let root = cargo_home(ctx);
    let expected = match cache.strategy.as_str() {
        "cargo-registry" => root.join("registry"),
        "cargo-git" => root.join("git"),
        _ => return Err(Error::InvalidInput("Unknown Cargo cache.".into())),
    };
    let path = contained_directory(&root, &cache.path)?;
    if std::fs::canonicalize(expected)? != path {
        return Err(Error::Conflict("The Cargo cache directory changed.".into()));
    }
    Ok(std::fs::canonicalize(root)?)
}

pub(crate) fn lock_cargo(root: &Path) -> Result<Vec<std::fs::File>> {
    reject_links(root)?;
    let mut guards = vec![];
    // Match Cargo's MutateExclusive protocol: mutate lock, then download lock.
    for name in [".package-cache-mutate", ".package-cache"] {
        let path = root.join(name);
        reject_links(&path)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        file.try_lock().map_err(|_| Error::Conflict("Cargo is using this cache. Retry cleanup when the current build or download finishes.".into()))?;
        guards.push(file);
    }
    Ok(guards)
}

fn pip_base(ctx: &Context, action: &str) -> Result<CommandSpec> {
    let mut command = if ctx.executable("pip").is_some() {
        ctx.command("pip", &["cache", action])?
    } else if ctx.executable("pip3").is_some() {
        ctx.command("pip3", &["cache", action])?
    } else {
        ctx.command(
            if ctx.executable("python3").is_some() {
                "python3"
            } else {
                "python"
            },
            &["-m", "pip", "cache", action],
        )?
    };
    command
        .env
        .insert("PIP_DISABLE_PIP_VERSION_CHECK".into(), "1".into());
    Ok(command)
}

pub(crate) async fn pip_path(ctx: &Context) -> Option<PathBuf> {
    let mut probe = pip_base(ctx, "dir").ok()?;
    ctx.apply_read_policy(&mut probe);
    let output = ctx.runner.run(&probe, &ctx.cancel).await.ok()?;
    let path = PathBuf::from(output.stdout.trim());
    (path.is_absolute() && path.is_dir()).then_some(path)
}

pub(crate) fn pip_command(ctx: &Context, path: &Path) -> Result<CommandSpec> {
    let mut command = pip_base(ctx, "purge")?;
    command
        .env
        .insert("PIP_CACHE_DIR".into(), path.to_string_lossy().into_owned());
    Ok(command)
}

pub(crate) async fn validate_cargo_checkouts(ctx: &Context, cache: &Cache) -> Result<()> {
    if cache.strategy != "cargo-git" {
        return Ok(());
    }
    for repository in directories(&cache.path.join("checkouts")) {
        for checkout in directories(&repository) {
            reject_links(&checkout)?;
            let mut spec = ctx.command(
                "git",
                &["status", "--porcelain", "-z", "--untracked-files=all"],
            )?;
            ctx.apply_read_policy(&mut spec);
            spec.cwd = Some(checkout.clone());
            let output = ctx.runner.run(&spec, &ctx.cancel).await?;
            // Cargo creates this empty completion marker outside Git. It is not
            // user work; every other untracked or modified entry is protected.
            let marker = std::fs::symlink_metadata(checkout.join(".cargo-ok"))
                .is_ok_and(|meta| meta.is_file() && meta.len() == 0);
            if output
                .stdout
                .split('\0')
                .filter(|entry| !entry.is_empty())
                .any(|entry| entry != "?? .cargo-ok" || !marker)
            {
                return Err(Error::Conflict(format!(
                    "{} has local changes. This cached checkout is kept to preserve your work.",
                    checkout.display()
                )));
            }
        }
    }
    Ok(())
}

fn launchers(installation: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(installation.join("lib"))
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    (name.starts_with("gradle-launcher-")
                        || name.starts_with("gradle-gradle-cli-main-"))
                        && name.ends_with(".jar")
                })
        })
        .collect()
}

pub(crate) fn gradle_command(ctx: &Context, cache: &Cache) -> Result<CommandSpec> {
    let home = gradle_home(ctx);
    let expected = home.join(if cache.strategy == "gradle-dists" {
        "wrapper/dists"
    } else {
        "caches"
    });
    if std::fs::canonicalize(&expected)? != contained_directory(&home, &cache.path)? {
        return Err(Error::Conflict(
            "The Gradle cache directory changed.".into(),
        ));
    }
    let mut candidates = vec![];
    if let Some(binary) = ctx
        .executable("gradle")
        .and_then(|path| std::fs::canonicalize(path).ok())
        && let Some(root) = binary.parent().and_then(Path::parent)
    {
        candidates.extend(launchers(root));
    }
    for distribution in directories(&home.join("wrapper/dists")) {
        for hash in directories(&distribution) {
            for installation in directories(&hash) {
                candidates.extend(launchers(&installation));
            }
        }
    }
    let mut candidates: Vec<_> = candidates
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let version = name
                .strip_prefix("gradle-launcher-")
                .or_else(|| name.strip_prefix("gradle-gradle-cli-main-"))?
                .strip_suffix(".jar")?;
            let version = version.parse::<pep440_rs::Version>().ok()?;
            Some((version, path))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let (version, launcher) = candidates.first().ok_or_else(|| {
        Error::Unavailable(
            "No installed Gradle distribution is available for native cache cleanup.".into(),
        )
    })?;
    if version < &"8.0".parse::<pep440_rs::Version>().unwrap() {
        return Err(Error::Unavailable(
            "Native on-demand cache cleanup requires Gradle 8 or later.".into(),
        ));
    }
    let mut spec = ctx.command(
        "java",
        &[
            "-cp",
            &launcher.to_string_lossy(),
            "org.gradle.launcher.GradleMain",
            "--no-daemon",
            "--offline",
            "--no-watch-fs",
            "--gradle-user-home",
            &home.to_string_lossy(),
            "help",
        ],
    )?;
    spec.env.insert(
        "GRADLE_USER_HOME".into(),
        home.to_string_lossy().into_owned(),
    );
    spec.cwd = Some(ctx.home.clone());
    spec.timeout = std::time::Duration::from_secs(600);
    Ok(spec)
}

pub(crate) async fn run_gradle(
    ctx: &Context,
    cache: &Cache,
    mut reviewed: CommandSpec,
) -> Result<crate::process::Output> {
    let current = super::cache_command(ctx, cache)?;
    if current.program != reviewed.program
        || current.args != reviewed.args
        || current.env != reviewed.env
    {
        return Err(Error::Conflict(
            "The Gradle installation changed after review.".into(),
        ));
    }
    let temp = tempfile::tempdir()?;
    std::fs::write(
        temp.path().join("settings.gradle"),
        "rootProject.name = 'envark-cache-cleanup'\n",
    )?;
    std::fs::write(temp.path().join("build.gradle"), "")?;
    let init = temp.path().join("cleanup.init.gradle");
    std::fs::write(
        &init,
        r#"import org.gradle.util.GradleVersion
if (GradleVersion.current() >= GradleVersion.version('8.0')) {
    beforeSettings { settings ->
        settings.caches {
            cleanup = org.gradle.api.cache.Cleanup.ALWAYS
        }
    }
}
"#,
    )?;
    reviewed.args.extend([
        "--project-dir".into(),
        temp.path().to_string_lossy().into_owned(),
        "--init-script".into(),
        init.to_string_lossy().into_owned(),
    ]);
    reviewed.cwd = Some(temp.path().into());
    ctx.runner.run(&reviewed, &ctx.cancel).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_cleanup_requires_both_cache_locks() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap();
        let first = lock_cargo(&path).unwrap();
        assert!(lock_cargo(&path).is_err());
        drop(first);
        lock_cargo(&path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cargo_lock_files_cannot_escape_through_dangling_links() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap();
        let target = path.join("outside");
        std::os::unix::fs::symlink(&target, path.join(".package-cache-mutate")).unwrap();
        assert!(lock_cargo(&path).is_err());
        assert!(!target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn cache_cleanup_completes_without_scanning_projects() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "providers::cache_cleanup::tests::cache_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("ENVARK_CACHE_FIXTURE", &path)
            .env("CARGO_HOME", path.join(".cargo"))
            .env("GRADLE_USER_HOME", path.join(".gradle"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    path.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "Isolated cache homes and executable paths are supplied by the parent test"]
    async fn cache_fixture() {
        use crate::{
            model::{Inventory, Settings, silent_progress},
            operations::{self, ActionRequest},
        };
        use std::os::unix::fs::PermissionsExt;
        let root = PathBuf::from(std::env::var_os("ENVARK_CACHE_FIXTURE").unwrap());
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = root.clone();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let cargo = cargo_home(&ctx);
        std::fs::create_dir_all(cargo.join("registry/cache")).unwrap();
        std::fs::create_dir_all(cargo.join("bin")).unwrap();
        std::fs::write(cargo.join("bin/keep"), "installed executable").unwrap();
        std::fs::write(cargo.join("config.toml"), "configuration").unwrap();
        std::fs::write(cargo.join("registry/cache/download"), "payload").unwrap();
        let registry = cache(
            ProviderId::Rust,
            "Cargo registry",
            cargo.join("registry"),
            "cargo-registry",
            true,
        )
        .unwrap();
        let mut inventory = Inventory {
            caches: vec![registry.clone()],
            ..Default::default()
        };
        let settings = Settings {
            use_trash: false,
            ..Default::default()
        };
        let request = ActionRequest::CleanCaches {
            ids: vec![registry.id.clone()],
        };
        let trash_plan =
            operations::prepare(request.clone(), &inventory, &Settings::default(), &ctx)
                .await
                .unwrap();
        assert!(trash_plan.view.use_trash);

        let plan = operations::prepare(request.clone(), &inventory, &settings, &ctx)
            .await
            .unwrap();
        let locks = lock_cargo(&cargo).unwrap();
        let result = operations::execute(
            plan,
            settings.clone(),
            ctx.clone(),
            silent_progress(),
            "locked".into(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(result.items[0].status, "failed");
        assert!(cargo.join("registry/cache/download").is_file());
        drop(locks);
        let plan = operations::prepare(request, &inventory, &settings, &ctx)
            .await
            .unwrap();
        let targets = plan.refresh_targets();
        let result = operations::execute(
            plan,
            settings.clone(),
            ctx.clone(),
            silent_progress(),
            "clean".into(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(result.items[0].status, "success", "{:?}", result.items);
        assert_eq!(result.removed_bytes, 7);
        assert!(cargo.join("bin/keep").is_file());
        assert!(cargo.join("config.toml").is_file());
        assert!(!cargo.join("registry").exists());
        let (updated, _) = crate::operation_refresh::refresh(
            inventory.clone(),
            targets,
            &result,
            ctx.clone(),
            silent_progress(),
            "refresh".into(),
        )
        .await
        .unwrap();
        assert_eq!(updated.caches[0].size.bytes, 0);

        // Refuse modified cached Git checkouts, and never follow escaped roots.
        let checkout = cargo.join("git/checkouts/repo/hash");
        crate::git::init(&checkout);
        std::fs::write(checkout.join("local-work"), "keep").unwrap();
        let git_cache = cache(
            ProviderId::Rust,
            "Cargo Git",
            cargo.join("git"),
            "cargo-git",
            true,
        )
        .unwrap();
        assert!(validate_cargo_checkouts(&ctx, &git_cache).await.is_err());
        std::fs::remove_file(checkout.join("local-work")).unwrap();
        std::fs::write(checkout.join(".cargo-ok"), "").unwrap();
        validate_cargo_checkouts(&ctx, &git_cache).await.unwrap();
        std::fs::write(checkout.join(".cargo-ok"), "local user data").unwrap();
        assert!(validate_cargo_checkouts(&ctx, &git_cache).await.is_err());

        std::os::unix::fs::symlink(&root, cargo.join("registry")).unwrap();
        assert!(cargo_root(&ctx, &registry).is_err());

        // Native pip must resolve and purge precisely the reviewed cache.
        let pip_path = root.join("custom-pip-cache");
        std::fs::create_dir_all(&pip_path).unwrap();
        std::fs::write(pip_path.join("wheel"), "cached wheel").unwrap();
        let pip = root.join("bin/pip");
        std::fs::write(&pip, r#"#!/bin/sh
[ "$1" = cache ] || exit 7
case "$2" in
dir) printf '%s\n' "${PIP_CACHE_DIR:-$ENVARK_CACHE_FIXTURE/custom-pip-cache}";;
purge) [ "$PIP_CACHE_DIR" = "$ENVARK_CACHE_FIXTURE/custom-pip-cache" ] || exit 8; rm "$PIP_CACHE_DIR/wheel";;
*) exit 9;;
esac
"#).unwrap();
        std::fs::set_permissions(&pip, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(super::pip_path(&ctx).await, Some(pip_path.clone()));
        let pip_cache = cache(ProviderId::Py, "pip", pip_path.clone(), "pip-purge", true).unwrap();
        inventory.caches = vec![pip_cache.clone()];
        let plan = operations::prepare(
            ActionRequest::CleanCaches {
                ids: vec![pip_cache.id],
            },
            &inventory,
            &Settings::default(),
            &ctx,
        )
        .await
        .unwrap();
        assert!(!plan.view.use_trash);
        let result = operations::execute(
            plan,
            settings.clone(),
            ctx.clone(),
            silent_progress(),
            "pip".into(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(result.items[0].status, "success", "{:?}", result.items);
        assert_eq!(result.removed_bytes, 12);
        assert!(!pip_path.join("wheel").exists());

        // JS discovery and cleanup agree on the versioned store and owning tool.
        let node = root.join("bin/node");
        std::fs::write(&node, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(node, std::fs::Permissions::from_mode(0o755)).unwrap();
        for (name, strategy, suffix, prefix, query, action, flag) in [
            (
                "npm",
                "npm-verify",
                "npm",
                "cache",
                "config",
                "verify",
                "--cache",
            ),
            (
                "pnpm",
                "pnpm-prune",
                "pnpm/v10",
                "store",
                "store",
                "prune",
                "--store-dir",
            ),
            (
                "yarn",
                "yarn-clean",
                "yarn/v6",
                "cache",
                "cache",
                "clean",
                "--cache-folder",
            ),
        ] {
            let path = root.join("js-cache").join(suffix);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("entry"), "cache").unwrap();
            let native_root = if name == "npm" {
                &path
            } else {
                path.parent().unwrap()
            };
            let script = format!(
                r#"#!/bin/sh
if [ "$1" = '{prefix}' ] && [ "$2" = '{action}' ]; then
    [ "$3" = '{flag}' ] && [ "$4" = '{native_root}' ] || exit 12
    rm '{path}/entry'
elif [ "$1" = '{query}' ]; then
    printf '%s\n' '{path}'
else
    exit 13
fi
"#,
                path = path.display(),
                native_root = native_root.display()
            );
            let binary = root.join("bin").join(name);
            std::fs::write(&binary, script).unwrap();
            std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755)).unwrap();
            let cache = cache(ProviderId::Js, name, path.clone(), strategy, true).unwrap();
            inventory.caches = vec![cache.clone()];
            let plan = operations::prepare(
                ActionRequest::CleanCaches {
                    ids: vec![cache.id],
                },
                &inventory,
                &Settings::default(),
                &ctx,
            )
            .await
            .unwrap();
            let result = operations::execute(
                plan,
                Settings::default(),
                ctx.clone(),
                silent_progress(),
                name.into(),
                false,
            )
            .await
            .unwrap();
            assert_eq!(
                result.items[0].status, "success",
                "{name}: {:?}",
                result.items
            );
            assert_eq!(result.removed_bytes, 5);
            assert!(!path.join("entry").exists());
        }

        // One Gradle operation covers both cache rows and uses an isolated project.
        let gradle = gradle_home(&ctx);
        let lib = gradle.join("wrapper/dists/gradle-8.14-bin/hash/gradle-8.14/lib");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("gradle-launcher-8.14.jar"), "fixture").unwrap();
        std::fs::create_dir_all(gradle.join("caches/expired")).unwrap();
        std::fs::write(gradle.join("caches/expired/file"), "payload").unwrap();
        let java = root.join("bin/java");
        std::fs::write(
            &java,
            r#"#!/bin/sh
case "$PWD" in "$ENVARK_CACHE_FIXTURE"*) exit 10;; esac
[ -f settings.gradle ] && [ -f cleanup.init.gradle ] || exit 11
rm -r "$GRADLE_USER_HOME/caches/expired"
printf 'Native cleanup completed\n'
"#,
        )
        .unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        inventory.caches = vec![
            cache(
                ProviderId::Jvm,
                "Gradle caches",
                gradle.join("caches"),
                "gradle-caches",
                true,
            )
            .unwrap(),
            cache(
                ProviderId::Jvm,
                "Gradle distributions",
                gradle.join("wrapper/dists"),
                "gradle-dists",
                true,
            )
            .unwrap(),
        ];
        let plan = operations::prepare(
            ActionRequest::CleanCaches {
                ids: inventory.caches.iter().map(|c| c.id.clone()).collect(),
            },
            &inventory,
            &settings,
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(plan.view.items.len(), 1);
        assert!(plan.view.items[0].title.contains("Gradle distributions"));
        let targets = plan.refresh_targets();
        let result = operations::execute(
            plan,
            settings,
            ctx.clone(),
            silent_progress(),
            "gradle".into(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(result.items[0].status, "success", "{:?}", result.items);
        assert_eq!(result.removed_bytes, 7);
        let (updated, _) = crate::operation_refresh::refresh(
            inventory,
            targets,
            &result,
            ctx.clone(),
            silent_progress(),
            "refresh".into(),
        )
        .await
        .unwrap();
        assert_eq!(updated.caches[0].size.bytes, 0);
        assert_eq!(updated.caches[1].size.bytes, 7);

        // Exercise the public engine path with the default Trash preference.
        // Preparing native cleanup must not turn that preference into a false policy conflict.
        std::fs::create_dir_all(gradle.join("caches/expired")).unwrap();
        std::fs::write(gradle.join("caches/expired/file"), "payload").unwrap();
        let engine = crate::engine::Engine::new(root.join("engine-state")).unwrap();
        {
            let mut state = engine.state.write().await;
            state.inventory = updated;
            state.settings.roots = vec![root.join("must-not-scan")];
            state.settings.use_trash = true;
        }
        let ids = engine
            .snapshot()
            .await
            .inventory
            .caches
            .iter()
            .map(|c| c.id.clone())
            .collect();
        let review = engine
            .plan(
                ActionRequest::CleanCaches { ids },
                "review".into(),
                silent_progress(),
            )
            .await
            .unwrap();
        let result = engine
            .execute(&review.id, "execute".into(), silent_progress(), false)
            .await
            .unwrap();
        assert_eq!(result.items[0].status, "success", "{:?}", result.items);
        assert!(!gradle.join("caches/expired").exists());

        // A real preference change still requires another review.
        let review = engine
            .plan(
                ActionRequest::CleanCaches {
                    ids: engine
                        .snapshot()
                        .await
                        .inventory
                        .caches
                        .iter()
                        .map(|c| c.id.clone())
                        .collect(),
                },
                "changed-review".into(),
                silent_progress(),
            )
            .await
            .unwrap();
        engine.state.write().await.settings.use_trash = false;
        assert!(
            matches!(engine.execute(&review.id, "changed-execute".into(), silent_progress(), false).await, Err(Error::Conflict(message)) if message.contains("cleanup policy changed"))
        );

        // Cache refresh calls only cache probes and retains the unrelated inventory.
        for name in ["node", "npm", "pnpm", "yarn", "uv", "pip", "go"] {
            let cache = root.join(format!("cache-{name}"));
            std::fs::create_dir_all(&cache).unwrap();
            std::fs::write(cache.join("entry"), "cache").unwrap();
            let output = if name == "go" {
                serde_json::json!({"GOCACHE": cache, "GOMODCACHE": cache}).to_string()
            } else {
                cache.display().to_string()
            };
            let binary = root.join("bin").join(name);
            std::fs::write(&binary, format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", output)).unwrap();
            std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        ctx.data = root.join("data");
        ctx.cache = root.join("cache");
        let before = engine.snapshot().await;
        let events = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let sink = events.clone();
        engine
            .refresh_caches_inner(
                ctx.clone(),
                "caches-only",
                std::sync::Arc::new(move |event| sink.lock().unwrap().push(event.stage)),
            )
            .await
            .unwrap();
        let after = engine.snapshot().await;
        assert_eq!(
            serde_json::to_value(before.inventory.projects).unwrap(),
            serde_json::to_value(after.inventory.projects).unwrap()
        );
        assert_eq!(
            serde_json::to_value(before.inventory.providers).unwrap(),
            serde_json::to_value(after.inventory.providers).unwrap()
        );
        assert_eq!(before.inventory.scanned_at, after.inventory.scanned_at);
        assert!(!events.lock().unwrap().iter().any(|stage| {
            ["discover", "measure-projects", "updates", "environments"].contains(&stage.as_str())
        }));
        assert!(
            after
                .inventory
                .caches
                .iter()
                .any(|cache| cache.name == "npm" && cache.size.bytes == 5),
            "{:#?}",
            after.inventory.caches
        );
        assert!(
            after
                .inventory
                .caches
                .iter()
                .any(|cache| cache.name == "pnpm" && cache.size.bytes == 5)
        );
    }
}
