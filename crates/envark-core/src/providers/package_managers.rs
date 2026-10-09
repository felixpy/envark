use super::*;

pub(crate) fn version(output: &str) -> Option<String> {
    output.split_whitespace().find_map(|word| {
        let value = word.trim_matches(['(', ')']).trim_start_matches('v');
        (semver::Version::parse(value).is_ok() || value.parse::<pep440_rs::Version>().is_ok())
            .then(|| value.to_owned())
    })
}

fn same_file(a: &Path, b: &Path) -> bool {
    std::fs::canonicalize(a)
        .ok()
        .zip(std::fs::canonicalize(b).ok())
        .is_some_and(|(a, b)| a == b)
}

fn standalone(ctx: &Context, tool: &Tool) -> bool {
    let Some(path) = &tool.path else { return false };
    if tool.name == "bun" {
        let root = std::env::var_os("BUN_INSTALL")
            .map(PathBuf::from)
            .unwrap_or_else(|| ctx.home.join(".bun"));
        return std::fs::canonicalize(path).ok().is_some_and(|real| {
            std::fs::canonicalize(root.join("bin")).ok().as_deref() == real.parent()
        }) && same_file(
            path,
            &root.join(if cfg!(windows) {
                "bin/bun.exe"
            } else {
                "bin/bun"
            }),
        );
    }
    if tool.name == "uv" {
        let mut receipts = vec![
            ctx.home.join(".config/uv/uv-receipt.json"),
            ctx.data.join("uv/uv-receipt.json"),
        ];
        if let Some(root) = std::env::var_os("XDG_CONFIG_HOME") {
            receipts.insert(0, PathBuf::from(root).join("uv/uv-receipt.json"));
        }
        return receipts.iter().any(|receipt| {
            let Some(json) = read_small(receipt, 1_048_576)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            else {
                return false;
            };
            let Some(prefix) = json["install_prefix"].as_str() else {
                return false;
            };
            same_file(
                path,
                &Path::new(prefix).join(if cfg!(windows) { "uv.exe" } else { "uv" }),
            )
        });
    }
    false
}

// A Cellar-looking path alone does not prove ownership. Match the active
// Homebrew installation and its installation receipt before offering updates.
fn brew_receipt(path: &Path) -> Option<(PathBuf, String)> {
    let real = std::fs::canonicalize(path).ok()?;
    let keg = real
        .ancestors()
        .find(|ancestor| ancestor.join("INSTALL_RECEIPT.json").is_file())?;
    let formula = keg.parent()?;
    let cellar = formula.parent()?;
    let json: serde_json::Value =
        serde_json::from_str(&read_small(&keg.join("INSTALL_RECEIPT.json"), 1_048_576).ok()?)
            .ok()?;
    let tap = json["source"]["tap"].as_str()?;
    let name = formula.file_name()?.to_str()?;
    let full = format!("{tap}/{name}");
    if full.split('/').count() != 3 || valid_identifier(&full).is_err() {
        return None;
    }
    Some((cellar.into(), full))
}

pub(crate) async fn brew_formula(ctx: &Context, path: &Path) -> Option<String> {
    let (cellar, formula) = brew_receipt(path)?;
    let output = ctx.read("brew", &["--cellar"]).await.ok()?;
    same_file(&cellar, Path::new(output.trim())).then_some(formula)
}

pub(crate) async fn resolve(ctx: &Context, provider: &mut Provider) {
    for manager in &mut provider.package_managers {
        if let Some(value) = version(&manager.version) {
            manager.version = value;
        }
        // Already-owned package records retain their package directory, which
        // is needed to target the correct global prefix during updates.
        if manager.source != "PATH" {
            continue;
        }
        let Some(binary) = manager.path.clone() else {
            continue;
        };
        if let Some(package) = provider.tools.iter().find(|package| {
            package.name == manager.name
                && package.can_update
                && package.path.as_ref().is_some_and(|root| {
                    std::fs::canonicalize(&binary)
                        .ok()
                        .zip(std::fs::canonicalize(root).ok())
                        .is_some_and(|(binary, root)| binary.starts_with(root))
                })
        }) {
            let id = manager.id.clone();
            *manager = package.clone();
            manager.id = id;
            manager.can_remove = false;
            continue;
        }
        if brew_formula(ctx, &binary).await.is_some_and(|formula| {
            formula.rsplit('/').next()
                == Some(if manager.name == "mvn" {
                    "maven"
                } else {
                    &manager.name
                })
        }) {
            manager.source = "homebrew".into();
            manager.can_update = true;
            manager.note = None;
        } else if standalone(ctx, manager) {
            manager.source = if manager.name == "bun" {
                "bun"
            } else {
                "uv-self"
            }
            .into();
            manager.can_update = true;
            manager.note = None;
        }
    }
    // Include package managers installed in inactive runtimes as well. Their
    // package directory keeps updates bound to that runtime's global prefix.
    for package in &provider.tools {
        if [
            "npm", "pnpm", "yarn", "corepack", "bun", "uv", "pipx", "poetry",
        ]
        .contains(&package.name.as_str())
            && package.can_update
            && !provider.package_managers.iter().any(|manager| {
                manager.name == package.name
                    && manager.path == package.path
                    && manager.source == package.source
            })
        {
            let mut manager = package.clone();
            manager.can_remove = false;
            provider.package_managers.push(manager);
        }
    }
    // The same installation belongs on the package-manager tab only.
    provider.tools.retain(|tool| {
        !provider.package_managers.iter().any(|manager| {
            tool.name == manager.name && tool.path == manager.path && tool.source == manager.source
        })
    });
}

pub(crate) fn handles(tool: &Tool) -> bool {
    matches!(tool.source.as_str(), "bun" | "uv-self" | "homebrew")
}

pub(crate) async fn installed_version(ctx: &Context, tool: &Tool) -> Result<String> {
    let path = tool
        .path
        .as_ref()
        .ok_or_else(|| Error::Conflict("The package manager path is unavailable.".into()))?;
    let mut probe = CommandSpec::new(path, ["--version"]);
    probe.cwd = Some(ctx.home.clone());
    let output = ctx.runner.run(&probe, &ctx.cancel).await?;
    version(&output.stdout)
        .ok_or_else(|| Error::Conflict("Cannot read the installed package manager version.".into()))
}

pub(crate) async fn validate_owner(ctx: &Context, tool: &Tool) -> Result<()> {
    let owned = match tool.source.as_str() {
        "homebrew" => match &tool.path {
            Some(path) => brew_formula(ctx, path).await.is_some(),
            None => false,
        },
        "bun" | "uv-self" => standalone(ctx, tool),
        _ => false,
    };
    if !owned {
        return Err(Error::Conflict(
            "The package manager installation changed. Check updates again before continuing."
                .into(),
        ));
    }
    Ok(())
}

pub(crate) fn command(ctx: &Context, tool: &Tool) -> Result<CommandSpec> {
    super::updates::require_upgrade(tool)?;
    if matches!(tool.source.as_str(), "bun" | "uv-self") && !standalone(ctx, tool) {
        return Err(Error::Conflict(
            "The standalone package manager installation changed.".into(),
        ));
    }
    let binary = tool
        .path
        .as_ref()
        .ok_or_else(|| Error::Conflict("Missing package manager path.".into()))?;
    let mut spec = match tool.source.as_str() {
        "bun" => CommandSpec::new(binary, ["upgrade"]),
        "uv-self" => CommandSpec::new(
            binary,
            [
                "self",
                "update",
                tool.latest.as_deref().expect("verified upgrade"),
            ],
        ),
        "homebrew" => {
            let (_, formula) = brew_receipt(binary)
                .ok_or_else(|| Error::Conflict("Homebrew ownership changed.".into()))?;
            let mut spec = ctx.command("brew", &["upgrade", &formula])?;
            // Updating one tool must not prune unrelated installed versions.
            spec.env
                .insert("HOMEBREW_NO_INSTALL_CLEANUP".into(), "1".into());
            spec.env
                .insert("HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK".into(), "1".into());
            spec
        }
        _ => {
            return Err(Error::Unavailable(
                "Unsupported package manager update.".into(),
            ));
        }
    };
    spec.cwd = Some(ctx.home.clone());
    spec.timeout = std::time::Duration::from_secs(1800);
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::model::{Inventory, Settings, silent_progress};
    #[cfg(unix)]
    use crate::operations::{self, ActionRequest};

    #[test]
    fn versions_are_normalized_for_registry_comparisons() {
        for (output, expected) in [
            ("1.3.13", "1.3.13"),
            ("uv 0.10.1 (build)", "0.10.1"),
            ("Poetry (version 2.1.0)", "2.1.0"),
            ("Apache Maven 3.9.9", "3.9.9"),
        ] {
            assert_eq!(version(output).as_deref(), Some(expected));
        }
        assert!(version("unknown").is_none());
    }

    #[tokio::test]
    async fn inactive_runtime_package_managers_keep_their_owning_prefix() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let mut provider = Provider::empty(ProviderId::Js);
        for runtime in ["node-20", "node-24"] {
            let mut tool = basic_tool(
                "npm",
                "10.0.0".into(),
                "npm",
                Some(root.path().join(runtime).join("lib/node_modules/npm")),
            );
            tool.can_update = true;
            tool.runtime = Some(runtime.into());
            provider.tools.push(tool);
        }
        resolve(&ctx, &mut provider).await;
        assert!(provider.tools.is_empty());
        assert_eq!(provider.package_managers.len(), 2);
        assert!(
            provider
                .package_managers
                .iter()
                .all(|tool| tool.can_update && !tool.can_remove)
        );
        assert_ne!(
            provider.package_managers[0].path,
            provider.package_managers[1].path
        );
        resolve(&ctx, &mut provider).await;
        assert_eq!(provider.package_managers.len(), 2);
    }

    #[cfg(unix)]
    fn script(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn package_manager_updates_complete_inside_the_client() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "providers::package_managers::tests::package_manager_update_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("ENVARK_MANAGER_FIXTURE", &root)
            .env("PATH", root.join("bin"))
            .env("BUN_INSTALL", root.join(".bun"))
            .env("XDG_CONFIG_HOME", root.join(".config"))
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
    #[ignore = "Run in an isolated subprocess with fixture-owned executable paths"]
    async fn package_manager_update_fixture() {
        let root = PathBuf::from(std::env::var_os("ENVARK_MANAGER_FIXTURE").unwrap());
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = root.clone();
        ctx.data = root.join("data");
        for (name, relative, source, id) in [
            ("bun", ".bun/bin/bun", "bun", ProviderId::Js),
            ("uv", ".local/bin/uv", "uv-self", ProviderId::Py),
        ] {
            let binary = root.join(relative);
            script(
                &binary,
                "#!/bin/sh\ncase \"$1\" in\n--version) read -r version < \"$0.version\"; printf '%s\\n' \"$version\";;\nupgrade|self) printf '2.0.0\\n' > \"$0.version\";;\n*) exit 9;;\nesac\n",
            );
            std::fs::write(binary.with_extension("version"), "1.0.0\n").unwrap();
            if name == "uv" {
                std::fs::create_dir_all(root.join(".config/uv")).unwrap();
                std::fs::write(
                    root.join(".config/uv/uv-receipt.json"),
                    serde_json::json!({"install_prefix": binary.parent().unwrap()}).to_string(),
                )
                .unwrap();
            }
            let mut provider = Provider::empty(id);
            provider.package_managers.push(basic_tool(
                name,
                "1.0.0".into(),
                "PATH",
                Some(binary.clone()),
            ));
            resolve(&ctx, &mut provider).await;
            let tool = &mut provider.package_managers[0];
            assert_eq!(tool.source, source);
            assert!(tool.can_update);
            tool.latest = Some("2.0.0".into());
            let request = ActionRequest::UpdateTool {
                provider: id,
                id: tool.id.clone(),
            };
            let inventory = Inventory {
                providers: vec![provider],
                ..Default::default()
            };
            let settings = Settings::default();
            let plan = operations::prepare(request.clone(), &inventory, &settings, &ctx)
                .await
                .unwrap();
            let result = operations::execute(
                plan,
                settings.clone(),
                ctx.clone(),
                silent_progress(),
                "update".into(),
                false,
            )
            .await
            .unwrap();
            assert_eq!(result.items[0].status, "success", "{:?}", result.items[0]);
            assert!(
                result.items[0]
                    .message
                    .contains("updated from 1.0.0 to 2.0.0")
            );
            // An external change between review and execution must not be overwritten.
            let plan = operations::prepare(request.clone(), &inventory, &settings, &ctx)
                .await
                .unwrap();
            let result = operations::execute(
                plan,
                settings.clone(),
                ctx.clone(),
                silent_progress(),
                "changed".into(),
                false,
            )
            .await
            .unwrap();
            assert_eq!(result.items[0].status, "failed");
            assert!(result.items[0].message.contains("changed after review"));
            // A successful process exit alone is not proof of an update.
            std::fs::write(binary.with_extension("version"), "1.0.0\n").unwrap();
            script(
                &binary,
                "#!/bin/sh\nif [ \"$1\" = --version ]; then printf '1.0.0\\n'; fi\n",
            );
            let plan = operations::prepare(request, &inventory, &settings, &ctx)
                .await
                .unwrap();
            let result = operations::execute(
                plan,
                settings,
                ctx.clone(),
                silent_progress(),
                "noop".into(),
                false,
            )
            .await
            .unwrap();
            assert_eq!(result.items[0].status, "failed");
            assert!(result.items[0].message.contains("still at version"));
        }
        // Homebrew ownership must select brew, including for Bun and uv.
        let cellar = root.join("Cellar");
        script(
            &root.join("bin/brew"),
            r#"#!/bin/sh
case "$1" in
--cellar) printf '%s/Cellar\n' "$ENVARK_MANAGER_FIXTURE";;
info) printf '{"formulae":[{"versions":{"stable":"2.0.0"},"pinned":false}]}\n';;
upgrade) formula="${2##*/}"; printf '2.0.0\n' > "$ENVARK_MANAGER_FIXTURE/Cellar/$formula/1.0.0/version";;
*) exit 9;;
esac
"#,
        );
        for name in ["bun", "uv", "pnpm", "poetry", "mvn", "gradle"] {
            let formula = if name == "mvn" { "maven" } else { name };
            let keg = cellar.join(formula).join("1.0.0");
            let binary = keg.join("bin").join(name);
            script(
                &binary,
                "#!/bin/sh\nread -r version < \"${0%/*}/../version\"; printf '%s\\n' \"$version\"\n",
            );
            std::fs::write(keg.join("version"), "1.0.0\n").unwrap();
            std::fs::write(
                keg.join("INSTALL_RECEIPT.json"),
                r#"{"source":{"tap":"homebrew/core"}}"#,
            )
            .unwrap();
            let mut provider = Provider::empty(ProviderId::Js);
            provider
                .package_managers
                .push(basic_tool(name, "1.0.0".into(), "PATH", Some(binary)));
            resolve(&ctx, &mut provider).await;
            let tool = &mut provider.package_managers[0];
            assert_eq!(tool.source, "homebrew");
            tool.latest = Some("2.0.0".into());
            let command = command(&ctx, tool).unwrap();
            assert_eq!(
                command.args,
                ["upgrade", &format!("homebrew/core/{formula}")]
            );
            assert_eq!(command.program, root.join("bin/brew"));
            validate_owner(&ctx, tool).await.unwrap();
            let id = tool.id.clone();
            super::super::updates::check(std::slice::from_mut(&mut provider), &ctx.cancel).await;
            assert_eq!(
                provider.package_managers[0].latest.as_deref(),
                Some("2.0.0")
            );
            let inventory = Inventory {
                providers: vec![provider.clone()],
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
                "brew-update".into(),
                false,
            )
            .await
            .unwrap();
            assert_eq!(result.items[0].status, "success", "{:?}", result.items[0]);
            std::fs::remove_file(keg.join("INSTALL_RECEIPT.json")).unwrap();
            assert!(
                validate_owner(&ctx, &provider.package_managers[0])
                    .await
                    .is_err()
            );
        }
        // A Bun symlink to another installer must not be treated as standalone.
        let binary = root.join(".bun/bin/bun");
        std::fs::remove_file(&binary).unwrap();
        let external = root.join("outside/bun");
        script(&external, "#!/bin/sh\nprintf '1.0.0\\n'\n");
        std::os::unix::fs::symlink(&external, &binary).unwrap();
        assert!(!standalone(
            &ctx,
            &basic_tool("bun", "1.0.0".into(), "PATH", Some(binary))
        ));
    }
}
