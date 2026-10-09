use super::*;
use crate::filesystem::{is_link, reject_links};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    path: PathBuf,
    bytes: u64,
    modified: std::time::SystemTime,
    link: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct Removal {
    pub tool: Tool,
    pub command: Option<CommandSpec>,
    entries: Vec<Entry>,
    resolved_path: PathBuf,
}

pub(crate) fn supported(tool: &Tool) -> bool {
    super::manager_install::MANAGERS
        .iter()
        .any(|(name, _)| *name == tool.name)
        && matches!(
            tool.source.as_str(),
            "npm"
                | "homebrew"
                | "corepack"
                | "bun"
                | "uv-self"
                | "fnm-script"
                | "nvm-script"
                | "pnpm-self"
                | "winget"
        )
}

fn entry(path: &Path) -> Result<Entry> {
    reject_links(
        path.parent()
            .ok_or_else(|| Error::unsafe_path(path, "missing parent"))?,
    )?;
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() && !is_link(&meta) {
        return Err(Error::unsafe_path(
            path,
            "only manager files may be removed",
        ));
    }
    Ok(Entry {
        path: path.into(),
        bytes: if is_link(&meta) { 0 } else { meta.len() },
        modified: meta.modified()?,
        link: if is_link(&meta) {
            Some(std::fs::read_link(path)?)
        } else {
            None
        },
    })
}

pub(crate) fn winget_owned(ctx: &Context, tool: &Tool) -> bool {
    tool.name == "fnm"
        && cfg!(windows)
        && tool
            .path
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
            .is_some_and(|path| {
                let base = ctx.home.join("AppData/Local/Microsoft/WinGet/Packages");
                base.canonicalize()
                    .ok()
                    .and_then(|base| path.strip_prefix(base).ok().map(Path::to_path_buf))
                    .is_some_and(|relative| {
                        relative.components().next().is_some_and(|part| {
                            part.as_os_str()
                                .to_string_lossy()
                                .starts_with("Schniz.fnm_")
                        }) && path.file_name().is_some_and(|file| file == "fnm.exe")
                    })
            })
}

pub(crate) fn winget_command(ctx: &Context, tool: &Tool, remove: bool) -> Result<CommandSpec> {
    if !winget_owned(ctx, tool) {
        return Err(Error::Conflict("The winget installation changed.".into()));
    }
    let mut args = vec![
        if remove { "uninstall" } else { "upgrade" },
        "--id",
        "Schniz.fnm",
        "--exact",
        "--source",
        "winget",
        "--silent",
        "--disable-interactivity",
    ];
    if !remove {
        args.extend([
            "--version",
            tool.latest
                .as_deref()
                .ok_or_else(|| Error::Conflict("Check updates first.".into()))?,
            "--accept-source-agreements",
            "--accept-package-agreements",
        ]);
    }
    let mut spec = ctx.command("winget", &args)?;
    spec.timeout = std::time::Duration::from_secs(1800);
    Ok(spec)
}

pub(crate) async fn prepare(ctx: &Context, tool: &Tool) -> Result<Removal> {
    if !supported(tool) {
        return Err(Error::Unavailable(
            "The manager installation owner is not verified.".into(),
        ));
    }
    if tool.source == "npm" {
        let path = tool
            .path
            .as_ref()
            .ok_or_else(|| Error::Conflict("Missing npm package path.".into()))?;
        let manifest: serde_json::Value =
            serde_json::from_str(&read_small(&path.join("package.json"), 1_048_576)?)?;
        if manifest["name"].as_str() != Some(&tool.name)
            || manifest["version"].as_str() != Some(&tool.version)
        {
            return Err(Error::Conflict(
                "The npm manager installation changed after review.".into(),
            ));
        }
    } else if tool.source == "winget" {
        if !winget_owned(ctx, tool) {
            return Err(Error::Conflict("The winget installation changed.".into()));
        }
    } else {
        super::package_managers::validate_owner(ctx, tool).await?;
    }
    let path = tool
        .path
        .as_ref()
        .ok_or_else(|| Error::Conflict("Missing manager path.".into()))?;
    let mut command = match tool.source.as_str() {
        "npm" => Some(super::tool_command(ctx, tool, true)?),
        "homebrew" => {
            let formula = super::package_managers::brew_formula(ctx, path)
                .await
                .ok_or_else(|| Error::Conflict("Homebrew ownership changed.".into()))?;
            Some(ctx.command("brew", &["uninstall", &formula])?)
        }
        "corepack" => {
            let mut spec = super::js_tooling::corepack_command(ctx, tool, false)?;
            spec.args.truncate(1);
            spec.args.extend([
                "disable".into(),
                "--install-directory".into(),
                path.parent().unwrap().to_string_lossy().into_owned(),
                tool.name.clone(),
            ]);
            Some(spec)
        }
        "winget" => Some(winget_command(ctx, tool, true)?),
        _ => None,
    };
    if let Some(command) = &mut command {
        command.timeout = std::time::Duration::from_secs(1800);
    }
    let mut entries = vec![];
    if command.is_none() {
        let primary = if tool.source == "pnpm-self" {
            path.clone()
        } else {
            path.canonicalize()?
        };
        let directory = primary
            .parent()
            .ok_or_else(|| Error::Conflict("Missing manager directory.".into()))?;
        let mut paths = vec![primary.clone()];
        let companions: &[&str] = match tool.name.as_str() {
            "uv" if cfg!(windows) => &["uvx.exe"],
            "uv" => &["uvx"],
            "bun" if cfg!(windows) => &["bunx.exe"],
            "bun" => &["bunx"],
            "nvm" => &["nvm-exec", "bash_completion"],
            "pnpm" if cfg!(windows) => {
                &["pnpx.exe", "pnpm.cmd", "pnpx.cmd", "pnpm.ps1", "pnpx.ps1"]
            }
            "pnpm" => &["pnpx"],
            _ => &[],
        };
        for name in companions {
            let candidate = directory.join(name);
            if candidate != primary && std::fs::symlink_metadata(&candidate).is_ok() {
                paths.push(candidate);
            }
        }
        entries = paths
            .iter()
            .map(|path| entry(path))
            .collect::<Result<_>>()?;
    }
    Ok(Removal {
        tool: tool.clone(),
        command,
        entries,
        resolved_path: path.canonicalize()?,
    })
}

impl Removal {
    pub(crate) fn bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.bytes).sum()
    }
}

pub(crate) async fn execute(
    ctx: &Context,
    removal: Removal,
    use_trash: bool,
) -> Result<(u64, String)> {
    let current = prepare(ctx, &removal.tool).await?;
    let same_command = match (&current.command, &removal.command) {
        (Some(a), Some(b)) => a.program == b.program && a.args == b.args && a.env == b.env,
        (None, None) => true,
        _ => false,
    };
    if !same_command
        || current.entries != removal.entries
        || current.resolved_path != removal.resolved_path
    {
        return Err(Error::Conflict(
            "The manager files changed after review. Review removal again.".into(),
        ));
    }
    if removal.tool.source != "npm" {
        let actual = super::package_managers::installed_version(ctx, &removal.tool).await?;
        if actual != removal.tool.version {
            return Err(Error::Conflict(
                "The manager version changed after review.".into(),
            ));
        }
    }
    if let Some(command) = &removal.command {
        ctx.runner.run(command, &ctx.cancel).await?;
        if removal.tool.path.as_ref().is_some_and(|path| path.exists()) {
            return Err(Error::Conflict(
                "The uninstall command finished, but the reviewed installation is still present."
                    .into(),
            ));
        }
    } else {
        let entries = removal.entries.clone();
        let cancel = ctx.cancel.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            for expected in &entries {
                if entry(&expected.path)? != *expected {
                    return Err(Error::Conflict(
                        "A manager file changed before removal.".into(),
                    ));
                }
            }
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if use_trash {
                trash::delete_all(entries.iter().map(|entry| &entry.path))
                    .map_err(|e| Error::Unavailable(e.to_string()))?;
            } else {
                for expected in entries {
                    std::fs::remove_file(expected.path)?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))??;
    }
    Ok((
        removal.bytes(),
        format!(
            "{} removed. Runtime installations, project files, caches, and configuration were kept. Reinstall the manager in Envark to manage them again.",
            removal.tool.name
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn npm_manager_removal_does_not_require_a_newer_release_and_retains_its_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let prefix = temp.path().canonicalize().unwrap();
        let modules = prefix.join(if cfg!(windows) {
            "node_modules"
        } else {
            "lib/node_modules"
        });
        let node = prefix.join(if cfg!(windows) {
            "node.exe"
        } else {
            "bin/node"
        });
        std::fs::create_dir_all(node.parent().unwrap()).unwrap();
        std::fs::write(&node, "fixture").unwrap();
        std::fs::create_dir_all(modules.join("npm/bin")).unwrap();
        std::fs::write(modules.join("npm/bin/npm-cli.js"), "fixture").unwrap();
        let ctx = Context::new(CancellationToken::new()).unwrap();
        for name in ["pnpm", "yarn", "bun", "uv"] {
            let path = modules.join(name);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(
                path.join("package.json"),
                serde_json::json!({"name":name,"version":"1.0.0"}).to_string(),
            )
            .unwrap();
            let tool = basic_tool(name, "1.0.0".into(), "npm", Some(path.clone()));
            let removal = prepare(&ctx, &tool).await.unwrap();
            let command = removal.command.unwrap();
            assert_eq!(command.program, node);
            assert_eq!(
                &command.args[1..],
                &[
                    "uninstall",
                    "--global",
                    name,
                    "--prefix",
                    &prefix.to_string_lossy()
                ]
            );
            std::fs::write(
                path.join("package.json"),
                r#"{"name":"different","version":"1.0.0"}"#,
            )
            .unwrap();
            assert!(prepare(&ctx, &tool).await.is_err());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_uninstall_cannot_report_success_when_manager_remains() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let prefix = temp.path().canonicalize().unwrap();
        let modules = prefix.join("lib/node_modules");
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        std::fs::create_dir_all(modules.join("npm/bin")).unwrap();
        std::fs::create_dir_all(modules.join("yarn")).unwrap();
        std::fs::write(
            modules.join("yarn/package.json"),
            r#"{"name":"yarn","version":"1.0.0"}"#,
        )
        .unwrap();
        std::fs::write(modules.join("npm/bin/npm-cli.js"), "fixture").unwrap();
        let node = prefix.join("bin/node");
        std::fs::write(&node, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let tool = basic_tool("yarn", "1.0.0".into(), "npm", Some(modules.join("yarn")));
        let removal = prepare(&ctx, &tool).await.unwrap();
        assert!(execute(&ctx, removal, false).await.is_err());
        assert!(modules.join("yarn/package.json").exists());
    }
}
