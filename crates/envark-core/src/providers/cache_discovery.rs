use super::*;

fn configured_path(ctx: &Context, key: &str) -> Option<PathBuf> {
    let rc = std::env::var_os("NPM_CONFIG_USERCONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".npmrc"));
    let content = read_small(&rc, 65_536).ok()?;
    let value = content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with(['#', ';']) {
                return None;
            }
            let (name, value) = line.split_once('=')?;
            (name.trim().eq_ignore_ascii_case(key)).then(|| value.trim().trim_matches(['\'', '"']))
        })
        .next_back()?;
    let value = value.replace("${HOME}", &ctx.home.to_string_lossy());
    let path = if let Some(suffix) = value.strip_prefix("~/") {
        ctx.home.join(suffix)
    } else {
        PathBuf::from(value)
    };
    path.is_absolute().then_some(path)
}

fn add(
    ctx: &Context,
    found: &mut Vec<Cache>,
    id: ProviderId,
    name: &str,
    path: PathBuf,
    strategy: &str,
    clean: bool,
) {
    let Ok(real) = std::fs::canonicalize(&path) else {
        return;
    };
    // A misconfigured cache must not turn a cache refresh into a home/root scan.
    if real.parent().is_none()
        || [&ctx.home, &ctx.data, &ctx.cache]
            .iter()
            .any(|root| std::fs::canonicalize(root).is_ok_and(|root| root == real))
    {
        return;
    }
    if found.iter().any(|cache| cache.path == real) {
        return;
    }
    if let Some(cache) = cache(id, name, real, strategy, clean) {
        found.push(cache);
    }
}

pub(super) async fn javascript(ctx: &Context) -> Vec<Cache> {
    let mut found = vec![];
    for (name, args, strategy, defaults) in [
        (
            "npm",
            vec!["config", "get", "cache"],
            "npm-verify",
            vec![
                ctx.home.join(".npm"),
                ctx.home.join("AppData/Local/npm-cache"),
                ctx.home.join("AppData/Roaming/npm-cache"),
            ],
        ),
        (
            "pnpm",
            vec!["store", "path"],
            "pnpm-prune",
            vec![
                ctx.data.join("pnpm/store"),
                ctx.home.join("Library/pnpm/store"),
                ctx.home.join(".local/share/pnpm/store"),
                ctx.home.join(".pnpm-store"),
                ctx.home.join("AppData/Local/pnpm/store"),
            ],
        ),
        (
            "yarn",
            vec!["cache", "dir"],
            "yarn-clean",
            vec![
                ctx.cache.join("yarn"),
                ctx.cache.join("Yarn"),
                ctx.home.join("AppData/Local/Yarn/Cache"),
            ],
        ),
    ] {
        let resolved = ctx.cache_path(name, &args).await;
        let configured = configured_path(ctx, if name == "pnpm" { "store-dir" } else { "cache" });
        if let Some(path) = resolved {
            add(ctx, &mut found, ProviderId::Js, name, path, strategy, true);
        }
        let mut fallbacks = defaults;
        if name != "yarn"
            && let Some(path) = configured
        {
            fallbacks.insert(0, path);
        }
        for path in fallbacks {
            let real = std::fs::canonicalize(&path).ok();
            // Do not count a reported versioned store and its configured parent twice.
            if real.as_ref().is_some_and(|root| {
                found.iter().any(|cache| {
                    cache.name == name
                        && (cache.path.starts_with(root) || root.starts_with(&cache.path))
                })
            }) {
                continue;
            }
            add(ctx, &mut found, ProviderId::Js, name, path, strategy, false);
        }
    }
    for cache in found.iter_mut().filter(|cache| !cache.can_clean) {
        cache.warning = format!(
            "This {} cache is present, but the current tool has not confirmed its location. Select a compatible Node environment, then refresh caches to enable native cleanup.",
            cache.name
        );
    }
    found
}

pub(super) async fn language(ctx: &Context, id: ProviderId) -> Vec<Cache> {
    match id {
        ProviderId::Py => {
            let mut found = vec![];
            if let Some(path) = ctx.cache_path("uv", &["cache", "dir"]).await {
                add(ctx, &mut found, id, "uv", path, "uv-prune", true);
            }
            if let Some(path) = cache_cleanup::pip_path(ctx).await {
                add(ctx, &mut found, id, "pip", path, "pip-purge", true);
            }
            found
        }
        ProviderId::Rust | ProviderId::Jvm => {
            let paths = if id == ProviderId::Rust {
                let root = cache_cleanup::cargo_home(ctx);
                vec![
                    (
                        "Cargo registry",
                        root.join("registry"),
                        "cargo-registry",
                        true,
                    ),
                    ("Cargo Git checkouts", root.join("git"), "cargo-git", true),
                ]
            } else {
                let root = cache_cleanup::gradle_home(ctx);
                vec![
                    (
                        "Maven repository",
                        ctx.home.join(".m2/repository"),
                        "maven-repository",
                        false,
                    ),
                    ("Gradle caches", root.join("caches"), "gradle-caches", true),
                    (
                        "Gradle distributions",
                        root.join("wrapper/dists"),
                        "gradle-dists",
                        true,
                    ),
                ]
            };
            let mut found = vec![];
            for (name, path, strategy, clean) in paths {
                add(ctx, &mut found, id, name, path, strategy, clean);
            }
            found
        }
        ProviderId::Go => {
            let env = ctx
                .read("go", &["env", "-json", "GOCACHE", "GOMODCACHE"])
                .await
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .unwrap_or_default();
            go_paths(ctx, &env)
        }
        _ => vec![],
    }
}

pub(super) fn go_paths(ctx: &Context, env: &serde_json::Value) -> Vec<Cache> {
    let mut found = vec![];
    for (name, key, strategy) in [
        ("Go build", "GOCACHE", "go-build"),
        ("Go modules", "GOMODCACHE", "go-modules"),
    ] {
        if let Some(path) = env[key].as_str() {
            add(
                ctx,
                &mut found,
                ProviderId::Go,
                name,
                PathBuf::from(path),
                strategy,
                true,
            );
        }
    }
    found
}

pub(crate) async fn discover(ctx: Context) -> Result<Vec<Cache>> {
    let mut jobs = tokio::task::JoinSet::new();
    for id in [
        ProviderId::Js,
        ProviderId::Py,
        ProviderId::Rust,
        ProviderId::Jvm,
        ProviderId::Go,
    ] {
        let ctx = ctx.clone();
        jobs.spawn(async move {
            if id == ProviderId::Js {
                javascript(&ctx).await
            } else {
                language(&ctx, id).await
            }
        });
    }
    let mut found = vec![];
    while let Some(result) = jobs.join_next().await {
        found.extend(result.map_err(|error| Error::Unavailable(error.to_string()))?);
    }
    if ctx.cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.dedup_by(|a, b| a.path == b.path);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_configuration_cannot_expand_measurement_to_home_or_shared_roots() {
        let root = tempfile::tempdir().unwrap();
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = std::fs::canonicalize(root.path()).unwrap();
        ctx.cache = ctx.home.join("caches");
        ctx.data = ctx.home.join("data");
        std::fs::create_dir_all(&ctx.cache).unwrap();
        std::fs::create_dir_all(&ctx.data).unwrap();
        let cache = ctx.cache.join("go-build");
        std::fs::create_dir_all(&cache).unwrap();
        let found = go_paths(
            &ctx,
            &serde_json::json!({ "GOCACHE": ctx.home, "GOMODCACHE": cache }),
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, cache);
        assert!(
            go_paths(
                &ctx,
                &serde_json::json!({ "GOCACHE": ctx.cache, "GOMODCACHE": ctx.data })
            )
            .is_empty()
        );
    }
}
