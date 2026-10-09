use super::*;

pub(crate) fn handles(tool: &Tool) -> bool {
    matches!(tool.source.as_str(), "fnm-script" | "nvm-script")
}

pub(crate) fn installation_root(ctx: &Context, name: &str, binary: &Path) -> Option<PathBuf> {
    if cfg!(windows) {
        return None;
    }
    let real = std::fs::canonicalize(binary).ok()?;
    let roots = match name {
        "fnm" => vec![
            std::env::var_os("FNM_DIR").map(PathBuf::from),
            Some(ctx.data.join("fnm")),
            Some(ctx.home.join(".local/share/fnm")),
            Some(ctx.home.join(".fnm")),
            Some(ctx.home.join("Library/Application Support/fnm")),
        ],
        "nvm" => vec![
            std::env::var_os("NVM_DIR").map(PathBuf::from),
            Some(ctx.home.join(".nvm")),
            std::env::var_os("XDG_CONFIG_HOME").map(|path| PathBuf::from(path).join("nvm")),
        ],
        _ => return None,
    };
    roots
        .into_iter()
        .flatten()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .find(|root| real == root.join(if name == "nvm" { "nvm.sh" } else { "fnm" }))
}

pub(crate) fn url(tool: &Tool) -> Result<String> {
    let version = tool
        .latest
        .as_deref()
        .ok_or_else(|| Error::Conflict("Check manager updates first.".into()))?;
    let version = semver::Version::parse(version)
        .map_err(|_| Error::InvalidInput("Invalid installer release version.".into()))?;
    Ok(match tool.source.as_str() {
        "fnm-script" => {
            format!("https://raw.githubusercontent.com/Schniz/fnm/v{version}/.ci/install.sh")
        }
        "nvm-script" => {
            format!("https://raw.githubusercontent.com/nvm-sh/nvm/v{version}/install.sh")
        }
        _ => return Err(Error::InvalidInput("Unknown official installer.".into())),
    })
}

pub(crate) fn command(ctx: &Context, tool: &Tool) -> Result<CommandSpec> {
    let root = tool
        .path
        .as_deref()
        .and_then(|path| installation_root(ctx, &tool.name, path))
        .ok_or_else(|| {
            Error::Conflict("The manager installation changed. Check versions again.".into())
        })?;
    let bash = ctx
        .executable("bash")
        .ok_or_else(|| Error::Unavailable("Bash is unavailable.".into()))?;
    let mut command = CommandSpec::new(bash, ["--noprofile", "--norc", &url(tool)?]);
    command.env.insert("BASH_ENV".into(), "/dev/null".into());
    command.cwd = Some(ctx.home.clone());
    command.timeout = std::time::Duration::from_secs(1800);
    if tool.name == "fnm" {
        for name in ["curl", "unzip"] {
            if ctx.executable(name).is_none() {
                return Err(Error::Unavailable(format!(
                    "The official fnm installer requires {name}."
                )));
            }
        }
        command.args.extend([
            "--skip-shell".into(),
            "--force-no-brew".into(),
            "--install-dir".into(),
            root.to_string_lossy().into_owned(),
            "--release".into(),
            format!("v{}", tool.latest.as_deref().unwrap()),
        ]);
    } else {
        command
            .env
            .insert("NVM_DIR".into(), root.to_string_lossy().into_owned());
        command.env.insert("PROFILE".into(), "/dev/null".into());
        command.env.insert(
            "NVM_INSTALL_VERSION".into(),
            format!("v{}", tool.latest.as_deref().unwrap()),
        );
        command
            .env
            .insert("NVM_INSTALL_GITHUB_REPO".into(), "nvm-sh/nvm".into());
        command.env.insert("NODE_VERSION".into(), String::new());
        command.env.insert(
            "METHOD".into(),
            if root.join(".git").is_dir() {
                "git"
            } else {
                "script"
            }
            .into(),
        );
        command.remove_env.push("NVM_SOURCE".into());
    }
    Ok(command)
}

pub(crate) async fn validate(ctx: &Context, tool: &Tool) -> Result<()> {
    let root = tool
        .path
        .as_deref()
        .and_then(|path| installation_root(ctx, &tool.name, path))
        .ok_or_else(|| Error::Conflict("The manager installation changed.".into()))?;
    if tool.name == "nvm" && root.join(".git").exists() {
        let mut probe = ctx.command("git", &["config", "--get", "remote.origin.url"])?;
        probe.cwd = Some(root.clone());
        let output = ctx.runner.run(&probe, &ctx.cancel).await?;
        if ![
            "https://github.com/nvm-sh/nvm.git",
            "https://github.com/nvm-sh/nvm",
            "git@github.com:nvm-sh/nvm.git",
        ]
        .contains(&output.stdout.trim())
        {
            return Err(Error::Conflict(
                "This nvm checkout has a custom origin; the official installer cannot update it."
                    .into(),
            ));
        }
        probe.args = vec![
            "status".into(),
            "--porcelain".into(),
            "--untracked-files=all".into(),
        ];
        if !ctx
            .runner
            .run(&probe, &ctx.cancel)
            .await?
            .stdout
            .trim()
            .is_empty()
        {
            return Err(Error::Conflict("The nvm installation has locally modified manager files. Updating would overwrite them.".into()));
        }
    }
    Ok(())
}

pub(crate) async fn installed_version(ctx: &Context, tool: &Tool) -> Result<String> {
    let output = if tool.name == "nvm" {
        ctx.read("nvm", &["--version"]).await?
    } else {
        let command = CommandSpec::new(
            tool.path
                .as_ref()
                .ok_or_else(|| Error::Conflict("Missing manager path.".into()))?,
            ["--version"],
        );
        ctx.runner.run(&command, &ctx.cancel).await?.stdout
    };
    super::package_managers::version(&output)
        .ok_or_else(|| Error::Conflict("Cannot read the installed manager version.".into()))
}

pub(crate) async fn run(
    ctx: &Context,
    tool: &Tool,
    command: CommandSpec,
) -> Result<crate::process::Output> {
    let url = url(tool)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| Error::Unavailable(error.to_string()))?;
    let fetch = async {
        let mut response = client
            .get(&url)
            .send()
            .await
            .map_err(|error| Error::Unavailable(error.to_string()))?;
        if !response.status().is_success() {
            return Err(Error::Unavailable(format!(
                "Official installer download failed: HTTP {}",
                response.status()
            )));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| Error::Unavailable(error.to_string()))?
        {
            if bytes.len() + chunk.len() > 1_048_576 {
                return Err(Error::Unavailable(
                    "The official installer exceeds the size limit.".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !bytes.starts_with(b"#!/") {
            return Err(Error::Unavailable(
                "The download is not an installer script.".into(),
            ));
        }
        Ok(bytes)
    };
    let bytes = tokio::select! { result = fetch => result?, _ = ctx.cancel.cancelled() => return Err(Error::Cancelled) };
    run_downloaded(ctx, tool, command, &bytes).await
}

async fn run_downloaded(
    ctx: &Context,
    tool: &Tool,
    mut command: CommandSpec,
    bytes: &[u8],
) -> Result<crate::process::Output> {
    // Download fully before execution; keep the private file alive for the child.
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("install.sh");
    std::fs::write(&path, bytes)?;
    // Revalidate after the download as well, before any installation is modified.
    validate(ctx, tool).await?;
    if installed_version(ctx, tool).await? != tool.version {
        return Err(Error::Conflict(
            "The manager version changed during download.".into(),
        ));
    }
    let current = self::command(ctx, tool)?;
    if current.program != command.program
        || current.args != command.args
        || current.env != command.env
    {
        return Err(Error::Conflict(
            "The manager installation changed during download.".into(),
        ));
    }
    command.args[2] = path.to_string_lossy().into_owned();
    ctx.runner.run(&command, &ctx.cancel).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_urls_are_pinned_to_official_release_tags() {
        for (name, expected) in [
            ("fnm", "Schniz/fnm/v1.2.3/.ci/install.sh"),
            ("nvm", "nvm-sh/nvm/v1.2.3/install.sh"),
        ] {
            let mut tool = basic_tool(name, "1.0.0".into(), &format!("{name}-script"), None);
            tool.latest = Some("1.2.3".into());
            assert_eq!(
                url(&tool).unwrap(),
                format!("https://raw.githubusercontent.com/{expected}")
            );
            for invalid in ["../../other", "1.0.0;echo", "latest"] {
                tool.latest = Some(invalid.into());
                assert!(url(&tool).is_err());
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn official_installer_adapters_keep_runtimes_aliases_and_profiles() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "providers::script_installers::tests::official_installer_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("ENVARK_INSTALLER_FIXTURE", &root)
            .env("FNM_DIR", root.join("data/fnm"))
            .env("NVM_DIR", root.join(".nvm"))
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
    #[ignore = "Run in a subprocess with isolated manager directories"]
    async fn official_installer_fixture() {
        use std::os::unix::fs::PermissionsExt;
        let root = PathBuf::from(std::env::var_os("ENVARK_INSTALLER_FIXTURE").unwrap());
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = root.clone();
        ctx.data = root.join("data");
        for name in ["fnm", "nvm"] {
            let install = root.join(if name == "fnm" { "data/fnm" } else { ".nvm" });
            std::fs::create_dir_all(install.join("versions/node")).unwrap();
            std::fs::create_dir_all(install.join("aliases")).unwrap();
            std::fs::write(install.join("versions/node/keep"), "runtime").unwrap();
            std::fs::write(install.join("aliases/default"), "v24.0.0").unwrap();
            std::fs::write(root.join(".zshrc"), "unchanged profile").unwrap();
            std::fs::write(install.join("version"), "1.0.0").unwrap();
            let path = install.join(if name == "fnm" { "fnm" } else { "nvm.sh" });
            let content = if name == "fnm" {
                "#!/bin/sh\nread -r version < \"${0%/*}/version\"; printf '%s\\n' \"$version\"\n"
            } else {
                "nvm() { read -r version < \"$NVM_DIR/version\"; printf '%s\\n' \"$version\"; }\n"
            };
            std::fs::write(&path, content).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            let mut tool = basic_tool(name, "1.0.0".into(), &format!("{name}-script"), Some(path));
            tool.latest = Some("2.0.0".into());
            let spec = command(&ctx, &tool).unwrap();
            assert_eq!(spec.env["BASH_ENV"], "/dev/null");
            let script = if name == "fnm" {
                b"#!/bin/bash\n[ \"$1\" = --skip-shell ] && [ \"$2\" = --force-no-brew ] && [ \"$3\" = --install-dir ] && [ \"$5\" = --release ] && [ \"$6\" = v2.0.0 ] || exit 8\nprintf '2.0.0\\n' > \"$4/version\"\n".as_slice()
            } else {
                b"#!/bin/bash\n[ \"$PROFILE\" = /dev/null ] && [ \"$NVM_INSTALL_VERSION\" = v2.0.0 ] && [ \"$NVM_INSTALL_GITHUB_REPO\" = nvm-sh/nvm ] && [ -z \"$NODE_VERSION\" ] && [ -z \"$NVM_SOURCE\" ] || exit 8\nprintf '2.0.0\\n' > \"$NVM_DIR/version\"\n".as_slice()
            };
            run_downloaded(&ctx, &tool, spec, script).await.unwrap();
            assert_eq!(installed_version(&ctx, &tool).await.unwrap(), "2.0.0");
            assert_eq!(
                std::fs::read_to_string(install.join("versions/node/keep")).unwrap(),
                "runtime"
            );
            assert_eq!(
                std::fs::read_to_string(install.join("aliases/default")).unwrap(),
                "v24.0.0"
            );
            assert_eq!(
                std::fs::read_to_string(root.join(".zshrc")).unwrap(),
                "unchanged profile"
            );
            // The same reviewed installer cannot run again after an external update.
            assert!(
                run_downloaded(&ctx, &tool, command(&ctx, &tool).unwrap(), script)
                    .await
                    .is_err()
            );
        }

        let install = root.join(".nvm");
        std::fs::write(install.join(".gitignore"), "versions/\naliases/\nversion\n").unwrap();
        for args in [
            vec!["init"],
            vec![
                "remote",
                "add",
                "origin",
                "https://github.com/nvm-sh/nvm.git",
            ],
            vec!["add", "nvm.sh", ".gitignore"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
        ] {
            let mut spec = ctx.command("git", &args).unwrap();
            spec.cwd = Some(install.clone());
            ctx.runner.run(&spec, &ctx.cancel).await.unwrap();
        }
        let tool = basic_tool(
            "nvm",
            "2.0.0".into(),
            "nvm-script",
            Some(install.join("nvm.sh")),
        );
        validate(&ctx, &tool).await.unwrap();
        std::fs::write(install.join("local-manager-extension"), "keep local work").unwrap();
        assert!(validate(&ctx, &tool).await.is_err());
        std::fs::remove_file(install.join("local-manager-extension")).unwrap();
        std::fs::write(install.join("nvm.sh"), "local modification").unwrap();
        assert!(validate(&ctx, &tool).await.is_err());
    }
}
