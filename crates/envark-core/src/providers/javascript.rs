use super::*;
use crate::{filesystem::id_for, model::Runtime};

pub async fn discover(ctx: &Context, provider: &mut Provider) -> Vec<Cache> {
    let mut caches = vec![];
    let active_path = ctx
        .executable("node")
        .and_then(|p| std::fs::canonicalize(p).ok());
    let active_version = ctx
        .read("node", &["--version"])
        .await
        .ok()
        .map(|s| s.trim().trim_start_matches('v').to_owned());
    for manager in ["fnm", "nvm"] {
        if let Some(found) = ctx.manager(manager, true, true).await {
            provider.managers.push(found);
        }
    }
    let fnm_root = ctx
        .read("fnm", &["env", "--json"])
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["FNM_DIR"].as_str().map(PathBuf::from))
        .or_else(|| std::env::var_os("FNM_DIR").map(PathBuf::from))
        .unwrap_or_else(|| {
            if cfg!(windows) {
                ctx.home.join("AppData/Local/fnm")
            } else {
                ctx.data.join("fnm")
            }
        });
    let nvm_root = std::env::var_os("NVM_HOME")
        .or_else(|| std::env::var_os("NVM_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".nvm"));
    for (manager, root) in [
        ("fnm", fnm_root.join("node-versions")),
        (
            "nvm",
            if cfg!(windows) {
                nvm_root.clone()
            } else {
                nvm_root.join("versions/node")
            },
        ),
    ] {
        for path in directories(&root) {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if !name.starts_with('v') {
                continue;
            }
            let installation = if manager == "fnm" {
                path.join("installation")
            } else {
                path
            };
            let binary = installation.join(if cfg!(windows) {
                "node.exe"
            } else {
                "bin/node"
            });
            if !binary.is_file() {
                continue;
            }
            let active = active_path.as_ref().is_some_and(|p| {
                p == &binary || std::fs::canonicalize(&binary).is_ok_and(|b| b == *p)
            });
            let version = name.trim_start_matches('v').to_owned();
            provider.runtimes.push(Runtime {
                selector: None,
                active_known: true,
                id: id_for("runtime", &installation),
                version: version.clone(),
                manager: manager.into(),
                path: installation.clone(),
                active,
                managed: provider.managers.iter().any(|m| m.name == manager),
                size: None,
                note: None,
            });
            let modules = installation.join(if cfg!(windows) {
                "node_modules"
            } else {
                "lib/node_modules"
            });
            add_packages(provider, &modules, "npm", Some(format!("Node {version}")));
        }
    }
    if let (Some(path), Some(version)) = (active_path, active_version)
        && !provider.runtimes.iter().any(|r| r.active)
    {
        let root = path.parent().unwrap_or(&path).to_path_buf();
        provider.runtimes.push(Runtime { selector: None, active_known: true, id: id_for("runtime", &path), version, manager: "PATH".into(), path: root, active: true, managed: false, size: None, note: Some("The installation owner is unknown; manage this runtime with its original installer.".into()) });
    }
    for manager in ["npm", "pnpm", "yarn", "bun", "corepack"] {
        if let Ok(version) = ctx.read(manager, &["--version"]).await {
            provider.package_managers.push(basic_tool(
                manager,
                version.lines().next().unwrap_or_default().trim().to_owned(),
                "PATH",
                ctx.executable(manager),
            ));
        }
    }
    if let Some(root) = ctx.cache_path("npm", &["root", "--global"]).await {
        add_packages(
            provider,
            &root,
            "npm",
            provider
                .runtimes
                .iter()
                .find(|r| r.active)
                .map(|r| format!("Node {}", r.version)),
        );
    }
    if let Some(root) = ctx.cache_path("pnpm", &["root", "--global"]).await {
        add_packages(provider, &root, "pnpm", None);
    }
    if let Some(root) = ctx.cache_path("yarn", &["global", "dir"]).await {
        add_packages(provider, &root.join("node_modules"), "yarn", None);
    }
    for (manager, args, strategy) in [
        ("npm", vec!["config", "get", "cache"], "npm-verify"),
        ("pnpm", vec!["store", "path"], "pnpm-prune"),
        ("yarn", vec!["cache", "dir"], "yarn-clean"),
    ] {
        if let Some(path) = ctx.cache_path(manager, &args).await
            && let Some(found) = cache(ProviderId::Js, manager, path, strategy, true)
        {
            caches.push(found);
        }
    }
    config(provider, ctx.home.join(".npmrc"), "ini", true);
    config(provider, ctx.home.join(".yarnrc.yml"), "yaml", true);
    caches
}

fn add_packages(provider: &mut Provider, root: &Path, source: &str, runtime: Option<String>) {
    // Version managers and npm can report the same prefix through different
    // aliases (including Windows verbatim paths). Resolve the root once while
    // preserving package symlinks and the owning runtime for safe operations.
    let Ok(root) = std::fs::canonicalize(root) else {
        return;
    };
    for (path, manifest) in package_manifests(&root) {
        let Some(name) = manifest["name"].as_str() else {
            continue;
        };
        if provider
            .tools
            .iter()
            .any(|t| t.path.as_ref() == Some(&path))
        {
            continue;
        }
        let mut tool = basic_tool(
            name,
            manifest["version"].as_str().unwrap_or("unknown").into(),
            source,
            Some(path),
        );
        tool.runtime = runtime.clone();
        let linked = tool
            .path
            .as_ref()
            .and_then(|p| std::fs::symlink_metadata(p).ok())
            .is_some_and(|m| m.file_type().is_symlink());
        tool.can_update = ["npm", "pnpm"].contains(&source) && !(source == "npm" && linked);
        tool.can_remove = ["npm", "pnpm"].contains(&source)
            && !["npm", "pnpm", "corepack", "yarn"].contains(&name);
        tool.note = manifest["description"].as_str().map(str::to_owned);
        provider.tools.push(tool);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(root: &Path) {
        let path = root.join("@test/cli");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("package.json"),
            r#"{"name":"@test/cli","version":"1.0.0"}"#,
        )
        .unwrap();
    }

    #[test]
    fn repeated_global_prefix_aliases_are_deduplicated_without_merging_other_runtimes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("node-24/node_modules");
        package(&root);
        let mut provider = Provider::empty(ProviderId::Js);
        add_packages(&mut provider, &root, "npm", Some("Node 24".into()));
        add_packages(
            &mut provider,
            &root.join("../node_modules"),
            "npm",
            Some("Node 24".into()),
        );
        add_packages(
            &mut provider,
            &std::fs::canonicalize(&root).unwrap(),
            "npm",
            Some("Node 24".into()),
        );
        assert_eq!(provider.tools.len(), 1);
        assert_eq!(provider.tools[0].runtime.as_deref(), Some("Node 24"));
        assert!(provider.tools[0].can_remove);
        let other = temp.path().join("node-22/node_modules");
        package(&other);
        add_packages(&mut provider, &other, "npm", Some("Node 22".into()));
        assert_eq!(provider.tools.len(), 2);
        assert_ne!(provider.tools[0].id, provider.tools[1].id);
    }

    #[cfg(unix)]
    #[test]
    fn alias_roots_do_not_hide_linked_package_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("node_modules");
        std::fs::create_dir(&root).unwrap();
        package(temp.path());
        std::os::unix::fs::symlink(temp.path().join("@test/cli"), root.join("linked-cli")).unwrap();
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        let mut provider = Provider::empty(ProviderId::Js);
        add_packages(&mut provider, &alias, "npm", Some("Node 24".into()));
        add_packages(&mut provider, &root, "npm", Some("Node 24".into()));
        assert_eq!(provider.tools.len(), 1);
        assert!(!provider.tools[0].can_update);
        assert!(
            std::fs::symlink_metadata(provider.tools[0].path.as_ref().unwrap())
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
