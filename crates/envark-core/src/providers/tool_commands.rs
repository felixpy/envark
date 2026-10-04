use super::*;

fn npm_name(name: &str) -> Result<()> {
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
    let path = tool
        .path
        .as_ref()
        .ok_or_else(|| Error::Unavailable("Missing package installation path.".into()))?;
    let modules = if tool.name.starts_with('@') {
        path.parent().and_then(Path::parent)
    } else {
        path.parent()
    }
    .filter(|p| p.file_name().is_some_and(|n| n == "node_modules"))
    .ok_or_else(|| Error::InvalidInput("Missing global node_modules directory.".into()))?;
    let prefix = if cfg!(windows) {
        modules.parent()
    } else {
        modules.parent().and_then(Path::parent)
    }
    .ok_or_else(|| Error::InvalidInput("Missing npm installation prefix.".into()))?;
    let node = prefix.join(if cfg!(windows) {
        "node.exe"
    } else {
        "bin/node"
    });
    let cli = modules.join("npm/bin/npm-cli.js");
    let target = if remove {
        tool.name.clone()
    } else {
        tool.latest
            .as_ref()
            .map(|version| format!("{}@{version}", tool.name))
            .unwrap_or(tool.name.clone())
    };
    valid_identifier(&target)?;
    let args = [
        if remove { "uninstall" } else { "install" },
        "--global",
        &target,
    ];
    let mut spec = if node.is_file() && cli.is_file() {
        let mut command = CommandSpec::new(&node, [cli.to_string_lossy().into_owned()]);
        command.args.extend(args.map(str::to_owned));
        // Lifecycle scripts must see the same runtime as npm itself.
        let mut paths = vec![node.parent().unwrap().to_path_buf()];
        paths.extend(
            std::env::var_os("PATH")
                .as_deref()
                .map(std::env::split_paths)
                .into_iter()
                .flatten(),
        );
        command.env.insert(
            "PATH".into(),
            std::env::join_paths(paths)
                .map_err(|e| Error::InvalidInput(e.to_string()))?
                .to_string_lossy()
                .into_owned(),
        );
        command
    } else {
        ctx.command("npm", &args)?
    };
    spec.args
        .extend(["--prefix".into(), prefix.to_string_lossy().into_owned()]);
    Ok(spec)
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
            let name = if remove {
                tool.name.clone()
            } else {
                tool.latest
                    .as_ref()
                    .map(|v| format!("{}@{v}", tool.name))
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
        "pipx" => ctx.command(
            "pipx",
            &[if remove { "uninstall" } else { "upgrade" }, &tool.name],
        )?,
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
            let mut command = ctx.command("go", &["install", &format!("{}@latest", tool.name)])?;
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
    if tool.source == "cargo" && !remove {
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
