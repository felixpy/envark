use super::*;

pub(super) fn npm_name(name: &str) -> Result<()> {
    let parts: Vec<_> = name.strip_prefix('@').unwrap_or(name).split('/').collect();
    if parts.len() != if name.starts_with('@') { 2 } else { 1 }
        || parts.iter().any(|p| {
            p.is_empty()
                || p.starts_with(['.', '-'])
                || !p
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
    {
        return Err(Error::InvalidInput("Invalid npm package name.".into()));
    }
    Ok(())
}

fn npm_command(ctx: &Context, tool: &Tool, remove: bool) -> Result<CommandSpec> {
    npm_name(&tool.name)?;
    let entry = super::package_sources::entry(tool);
    npm_name(entry)?;
    let target = if remove {
        entry.into()
    } else {
        let version = tool
            .latest
            .as_deref()
            .ok_or_else(|| Error::Conflict("Check updates first.".into()))?;
        if entry != tool.name {
            format!("{entry}@npm:{}@{version}", tool.name)
        } else {
            format!("{}@{version}", tool.name)
        }
    };
    valid_identifier(&target)?;
    let args = [
        if remove { "uninstall" } else { "install" },
        "--global",
        &target,
    ];
    super::package_sources::npm_spec(ctx, tool, &args)
}

pub fn tool_command(ctx: &Context, tool: &Tool, remove: bool) -> Result<CommandSpec> {
    valid_identifier(&tool.name)?;
    if !remove {
        super::updates::require_upgrade(tool)?;
    }
    let mut spec = match tool.source.as_str() {
        "npm" => npm_command(ctx, tool, remove)?,
        "pnpm" => {
            npm_name(&tool.name)?;
            let entry = super::package_sources::entry(tool);
            let name = if remove {
                entry.into()
            } else {
                tool.latest
                    .as_ref()
                    .map(|v| {
                        if entry != tool.name {
                            format!("{entry}@npm:{}@{v}", tool.name)
                        } else {
                            format!("{}@{v}", tool.name)
                        }
                    })
                    .unwrap_or(tool.name.clone())
            };
            valid_identifier(&name)?;
            ctx.command(
                "pnpm",
                &[
                    if remove { "uninstall" } else { "install" },
                    "--global",
                    &name,
                ],
            )?
        }
        "uv" => ctx.command(
            "uv",
            &[
                "tool",
                if remove { "uninstall" } else { "upgrade" },
                &tool.name,
            ],
        )?,
        "yarn" => {
            let entry = super::package_sources::entry(tool);
            npm_name(entry)?;
            let target = if remove {
                entry.into()
            } else {
                let version = tool
                    .latest
                    .as_deref()
                    .ok_or_else(|| Error::Conflict("Check updates first.".into()))?;
                if entry != tool.name {
                    format!("{entry}@npm:{}@{version}", tool.name)
                } else {
                    format!("{}@{version}", tool.name)
                }
            };
            valid_identifier(&target)?;
            let mut command = ctx.command(
                "yarn",
                &[
                    "global",
                    if remove { "remove" } else { "add" },
                    &target,
                    "--non-interactive",
                ],
            )?;
            if let Some(root) = tool.installation.as_ref().and_then(|i| i.root.as_ref()) {
                command.args.extend([
                    "--global-folder".into(),
                    root.to_string_lossy().into_owned(),
                ]);
            }
            command
        }
        "pipx" => {
            let mut command = ctx.command(
                "pipx",
                &[if remove { "uninstall" } else { "upgrade" }, &tool.name],
            )?;
            if !remove {
                let args = super::package_sources::pipx_args(tool)?;
                if !args.is_empty() {
                    command.args.extend([
                        "--pip-args".into(),
                        args.iter()
                            .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
                            .collect::<Vec<_>>()
                            .join(" "),
                    ]);
                }
            }
            command
        }
        "cargo" if tool.installation.is_some() => {
            super::package_sources::cargo_command(ctx, tool, remove)?
        }
        "cargo" => ctx.command(
            "cargo",
            &[if remove { "uninstall" } else { "install" }, &tool.name],
        )?,
        "go" if !remove => {
            let bin = tool
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .ok_or_else(|| Error::Unavailable("Missing Go binary directory.".into()))?;
            let version = tool
                .latest
                .as_deref()
                .ok_or_else(|| Error::Conflict("Check updates first.".into()))?;
            let version = if version.starts_with('v') {
                version.into()
            } else {
                format!("v{version}")
            };
            let mut command =
                ctx.command("go", &["install", &format!("{}@{version}", tool.name)])?;
            command
                .env
                .insert("GOBIN".into(), bin.to_string_lossy().into_owned());
            command.env.insert("GOTOOLCHAIN".into(), "local".into());
            command
        }
        _ => {
            return Err(Error::Unavailable(
                "The owner of this installation does not expose a supported operation.".into(),
            ));
        }
    };
    spec.cwd = Some(ctx.home.clone());
    if tool.source == "cargo" && !remove && tool.installation.is_none() {
        spec.args.extend([
            "--version".into(),
            tool.latest.clone().expect("verified upgrade"),
        ]);
    }
    spec.timeout = std::time::Duration::from_secs(1800);
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_package_names_cannot_be_urls_or_flags() {
        for name in [
            "https://example.com/package",
            "--help",
            "../other",
            "@scope/a/extra",
            "a&b",
        ] {
            assert!(npm_name(name).is_err());
        }
        for name in ["typescript", "@scope/tool", "some-tool.js"] {
            assert!(npm_name(name).is_ok());
        }
    }

    #[test]
    fn npm_updates_use_the_runtime_that_owns_the_global_prefix() {
        let root = tempfile::tempdir().unwrap();
        let modules = root.path().join(if cfg!(windows) {
            "node_modules"
        } else {
            "lib/node_modules"
        });
        let node = root.path().join(if cfg!(windows) {
            "node.exe"
        } else {
            "bin/node"
        });
        std::fs::create_dir_all(modules.join("npm/bin")).unwrap();
        std::fs::create_dir_all(node.parent().unwrap()).unwrap();
        std::fs::write(&node, "fixture").unwrap();
        std::fs::write(modules.join("npm/bin/npm-cli.js"), "fixture").unwrap();
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let mut tool = basic_tool(
            "@scope/tool",
            "1.0.0".into(),
            "npm",
            Some(modules.join("@scope/tool")),
        );
        tool.latest = Some("2.0.0".into());
        let spec = npm_command(&ctx, &tool, false).unwrap();
        assert_eq!(spec.program, node);
        assert!(spec.args.contains(&"@scope/tool@2.0.0".into()));
        assert_eq!(spec.args.last().unwrap(), &root.path().to_string_lossy());
    }
}
