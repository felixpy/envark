use super::*;

fn manifest_name(root: &Path) -> Option<String> {
    let json: serde_json::Value =
        serde_json::from_str(&read_small(&root.join("package.json"), 1_048_576).ok()?).ok()?;
    json["name"].as_str().map(str::to_owned)
}

// Resolve Unix symlinks and Windows command shims without executing a shell.
pub(crate) fn package_root(program: &Path, expected: &str) -> Option<PathBuf> {
    if let Ok(real) = std::fs::canonicalize(program)
        && let Some(root) = real
            .ancestors()
            .find(|root| manifest_name(root).as_deref() == Some(expected))
    {
        return Some(root.into());
    }
    let parent = program.parent()?;
    let wrapper = read_small(program, 65_536).ok()?.replace('\\', "/");
    for root in [
        parent.join("node_modules").join(expected),
        parent.join("../lib/node_modules").join(expected),
    ] {
        if wrapper.contains(&format!("node_modules/{expected}/"))
            && manifest_name(&root).as_deref() == Some(expected)
        {
            return std::fs::canonicalize(root).ok();
        }
    }
    None
}

pub(crate) fn corepack_root(program: &Path, name: &str) -> Option<PathBuf> {
    if !["pnpm", "yarn", "corepack"].contains(&name) {
        return None;
    }
    let root = package_root(program, "corepack")?;
    root.join("dist/corepack.js").is_file().then_some(root)
}

fn node_command(ctx: &Context, root: &Path, cli: PathBuf, args: &[&str]) -> Option<CommandSpec> {
    if !cli.is_file() {
        return None;
    }
    let modules = root.parent()?;
    let prefix = if cfg!(windows) {
        modules.parent()?
    } else {
        modules.parent()?.parent()?
    };
    let owned_node = prefix.join(if cfg!(windows) {
        "node.exe"
    } else {
        "bin/node"
    });
    let node = if owned_node.is_file() {
        owned_node
    } else {
        ctx.executable("node")?
    };
    let mut spec = CommandSpec::new(&node, [cli.to_string_lossy().into_owned()]);
    spec.args.extend(args.iter().map(|s| (*s).to_owned()));
    let mut paths = vec![node.parent()?.to_path_buf()];
    paths.extend(
        std::env::var_os("PATH")
            .as_deref()
            .map(std::env::split_paths)
            .into_iter()
            .flatten(),
    );
    spec.env.insert(
        "PATH".into(),
        std::env::join_paths(paths)
            .ok()?
            .to_string_lossy()
            .into_owned(),
    );
    Some(spec)
}

pub(crate) fn cli_command(
    ctx: &Context,
    program: &Path,
    name: &str,
    args: &[&str],
) -> Option<CommandSpec> {
    if let Some(root) = corepack_root(program, name) {
        return node_command(ctx, &root, root.join(format!("dist/{name}.js")), args);
    }
    let root = package_root(program, name)?;
    let files: &[&str] = match name {
        "npm" => &["bin/npm-cli.js"],
        "pnpm" => &["bin/pnpm.cjs", "bin/pnpm.js"],
        "yarn" => &["bin/yarn.js"],
        _ => return None,
    };
    files
        .iter()
        .find_map(|file| node_command(ctx, &root, root.join(file), args))
}

pub(crate) fn corepack_command(ctx: &Context, tool: &Tool, update: bool) -> Result<CommandSpec> {
    let root = tool
        .path
        .as_deref()
        .and_then(|path| corepack_root(path, &tool.name))
        .ok_or_else(|| {
            Error::Conflict("The Corepack shim changed. Check versions again.".into())
        })?;
    let args = if update {
        let latest = tool
            .latest
            .as_deref()
            .ok_or_else(|| Error::Conflict("Check updates first.".into()))?;
        let target = format!("{}@{latest}", tool.name);
        valid_identifier(&target)?;
        let legacy = read_small(&root.join("package.json"), 1_048_576)
            .ok()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
            .and_then(|json| {
                json["version"]
                    .as_str()
                    .and_then(|v| semver::Version::parse(v).ok())
            })
            .is_some_and(|v| v < semver::Version::new(0, 20, 0));
        if legacy {
            vec!["prepare".into(), target, "--activate".into()]
        } else {
            vec!["install".into(), "--global".into(), target]
        }
    } else {
        vec![tool.name.clone(), "--version".into()]
    };
    let mut spec = node_command(
        ctx,
        &root,
        root.join("dist/corepack.js"),
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .ok_or_else(|| Error::Unavailable("The Node runtime for Corepack is unavailable.".into()))?;
    spec.env
        .insert("COREPACK_ENABLE_PROJECT_SPEC".into(), "0".into());
    spec.env
        .insert("COREPACK_ENABLE_AUTO_PIN".into(), "0".into());
    spec.env
        .insert("COREPACK_DEFAULT_TO_LATEST".into(), "0".into());
    spec.env
        .insert("COREPACK_ENABLE_DOWNLOAD_PROMPT".into(), "0".into());
    if !update {
        spec.env
            .insert("COREPACK_ENABLE_NETWORK".into(), "0".into());
    }
    Ok(spec)
}

pub(crate) fn pnpm_home(ctx: &Context, program: &Path) -> Option<PathBuf> {
    let real = std::fs::canonicalize(program).ok()?;
    let mut homes = vec![
        ctx.data.join("pnpm"),
        ctx.home.join("Library/pnpm"),
        ctx.home.join("AppData/Local/pnpm"),
    ];
    if let Some(home) = std::env::var_os("PNPM_HOME") {
        homes.insert(0, home.into());
    }
    homes.into_iter().find(|home| {
        let Ok(root) = std::fs::canonicalize(home) else {
            return false;
        };
        if !real.starts_with(&root) {
            return false;
        }
        let parent = program
            .parent()
            .and_then(|parent| std::fs::canonicalize(parent).ok());
        parent
            .as_ref()
            .is_some_and(|parent| parent == &root || parent == &root.join("bin"))
    })
}

// Isolate the global pnpm update from packageManager pins and workspace files,
// even when the user's home directory is itself a project.
pub(crate) fn isolated_directory() -> Result<tempfile::TempDir> {
    let temp = tempfile::tempdir()?;
    std::fs::write(temp.path().join("package.json"), r#"{"private":true}"#)?;
    std::fs::write(temp.path().join("pnpm-workspace.yaml"), "packages: []\n")?;
    Ok(temp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(if cfg!(windows) {
            "node_modules/corepack"
        } else {
            "lib/node_modules/corepack"
        });
        std::fs::create_dir_all(root.join("dist")).unwrap();
        std::fs::write(root.join("package.json"), r#"{"name":"corepack"}"#).unwrap();
        for name in ["corepack", "pnpm", "yarn"] {
            std::fs::write(root.join(format!("dist/{name}.js")), "fixture").unwrap();
        }
        let node = temp.path().join(if cfg!(windows) {
            "node.exe"
        } else {
            "bin/node"
        });
        std::fs::create_dir_all(node.parent().unwrap()).unwrap();
        std::fs::write(&node, "fixture").unwrap();
        (temp, root, node)
    }

    #[test]
    fn corepack_updates_use_the_owning_node_and_cannot_edit_project_pins() {
        let (_temp, root, node) = fixture_root();
        let ctx = Context::new(CancellationToken::new()).unwrap();
        for name in ["pnpm", "yarn"] {
            let mut tool = basic_tool(
                name,
                "3.0.0".into(),
                "corepack",
                Some(root.join(format!("dist/{name}.js"))),
            );
            tool.latest = Some("4.0.0".into());
            let spec = corepack_command(&ctx, &tool, true).unwrap();
            assert_eq!(spec.program, node);
            assert_eq!(
                &spec.args[1..],
                &["install", "--global", &format!("{name}@4.0.0")]
            );
            assert_eq!(spec.env["COREPACK_ENABLE_PROJECT_SPEC"], "0");
            assert_eq!(spec.env["COREPACK_ENABLE_AUTO_PIN"], "0");
            assert_eq!(
                corepack_command(&ctx, &tool, false).unwrap().env["COREPACK_ENABLE_NETWORK"],
                "0"
            );
        }
    }

    #[test]
    fn windows_wrappers_are_resolved_without_cmd_exe() {
        let (temp, root, node) = fixture_root();
        let parent = if cfg!(windows) {
            temp.path().to_path_buf()
        } else {
            temp.path().join("bin")
        };
        let wrapper = parent.join("pnpm.cmd");
        std::fs::write(
            &wrapper,
            if cfg!(windows) {
                "@node %~dp0\\node_modules\\corepack\\dist\\pnpm.js %*"
            } else {
                "node ../lib/node_modules/corepack/dist/pnpm.js"
            },
        )
        .unwrap();
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let spec = cli_command(&ctx, &wrapper, "pnpm", &["--version"]).unwrap();
        assert_eq!(spec.program, node);
        assert_eq!(
            std::fs::canonicalize(&spec.args[0]).unwrap(),
            std::fs::canonicalize(root.join("dist/pnpm.js")).unwrap()
        );
    }

    #[test]
    fn pnpm_updates_have_an_isolated_project_boundary() {
        let scratch = isolated_directory().unwrap();
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(scratch.path().join("package.json")).unwrap(),
        )
        .unwrap();
        assert!(manifest.get("packageManager").is_none());
        assert!(scratch.path().join("pnpm-workspace.yaml").is_file());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn corepack_pnpm_and_yarn_updates_complete_without_editing_project_pins() {
        use crate::{
            model::{Inventory, Settings, silent_progress},
            operations::{self, ActionRequest},
        };
        use std::os::unix::fs::PermissionsExt;
        let (temp, root, node) = fixture_root();
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = temp.path().canonicalize().unwrap();
        let project = ctx.home.join("package.json");
        let pin = r#"{"packageManager":"yarn@1.22.0"}"#;
        std::fs::write(&project, pin).unwrap();
        std::fs::write(
            &node,
            r#"#!/bin/sh
[ "$COREPACK_ENABLE_PROJECT_SPEC" = 0 ] && [ "$COREPACK_ENABLE_AUTO_PIN" = 0 ] || exit 7
root="${0%/*}/.."
case "$2" in
install) [ "$3" = --global ] || exit 8; printf '%s\n' "${4#*@}" > "$root/version";;
prepare) [ "$4" = --activate ] || exit 8; printf '%s\n' "${3#*@}" > "$root/version";;
pnpm|yarn) [ "$COREPACK_ENABLE_NETWORK" = 0 ] || exit 9; cat "$root/version";;
*) exit 10;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o755)).unwrap();
        for (name, corepack_version) in [
            ("pnpm", "0.34.0"),
            ("yarn", "0.34.0"),
            ("pnpm", "0.17.0"),
            ("yarn", "0.17.0"),
        ] {
            std::fs::write(ctx.home.join("version"), "3.0.0").unwrap();
            std::fs::write(
                root.join("package.json"),
                format!(r#"{{"name":"corepack","version":"{corepack_version}"}}"#),
            )
            .unwrap();
            let mut provider = Provider::empty(ProviderId::Js);
            provider.package_managers.push(basic_tool(
                name,
                "3.0.0".into(),
                "PATH",
                Some(root.join(format!("dist/{name}.js"))),
            ));
            super::super::package_managers::resolve(&ctx, &mut provider).await;
            let tool = &mut provider.package_managers[0];
            assert_eq!(tool.source, "corepack");
            tool.latest = Some("4.0.0".into());
            let id = tool.id.clone();
            let inventory = Inventory {
                providers: vec![provider],
                ..Default::default()
            };
            let settings = Settings::default();
            let plan = operations::prepare(
                ActionRequest::UpdateTool {
                    provider: ProviderId::Js,
                    id,
                },
                &inventory,
                &settings,
                &ctx,
            )
            .await
            .unwrap();
            let result = operations::execute(
                plan,
                settings,
                ctx.clone(),
                silent_progress(),
                "update".into(),
                false,
            )
            .await
            .unwrap();
            assert_eq!(result.items[0].status, "success", "{:?}", result.items);
            assert_eq!(std::fs::read_to_string(&project).unwrap(), pin);
            assert_eq!(
                std::fs::read_to_string(ctx.home.join("version"))
                    .unwrap()
                    .trim(),
                "4.0.0"
            );
        }
    }
}
