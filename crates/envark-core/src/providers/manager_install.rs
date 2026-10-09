use super::*;
use crate::{filesystem::reject_links, model::Inventory};
use serde::Serialize;
use std::time::Duration;

pub(crate) const MANAGERS: &[(&str, ProviderId)] = &[
    ("fnm", ProviderId::Js),
    ("nvm", ProviderId::Js),
    ("bun", ProviderId::Js),
    ("pnpm", ProviderId::Js),
    ("yarn", ProviderId::Js),
    ("uv", ProviderId::Py),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionView {
    pub name: String,
    pub source: String,
    pub installed: bool,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Installation {
    pub name: String,
    pub version: String,
    pub target: PathBuf,
    pub script: Option<String>,
    pub commands: Vec<CommandSpec>,
    pub profile: Option<PathBuf>,
    pub source: String,
}

fn powershell(ctx: &Context) -> Option<PathBuf> {
    ctx.executable("pwsh")
        .or_else(|| ctx.executable("powershell"))
}

fn root(ctx: &Context, name: &str) -> PathBuf {
    let (variable, fallback) = match name {
        "fnm" => ("FNM_DIR", ctx.data.join("fnm")),
        "nvm" => ("NVM_DIR", ctx.home.join(".nvm")),
        "bun" => ("BUN_INSTALL", ctx.home.join(".bun")),
        "pnpm" => (
            "PNPM_HOME",
            if cfg!(target_os = "macos") {
                ctx.home.join("Library/pnpm")
            } else {
                ctx.data.join("pnpm")
            },
        ),
        "uv" => ("UV_INSTALL_DIR", ctx.home.join(".local/bin")),
        _ => ("", ctx.home.clone()),
    };
    std::env::var_os(variable)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or(fallback)
}

fn binary(root: &Path, name: &str) -> PathBuf {
    match name {
        "nvm" => root.join("nvm.sh"),
        "bun" => root.join(if cfg!(windows) {
            "bin/bun.exe"
        } else {
            "bin/bun"
        }),
        _ => root.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        }),
    }
}

fn npm(ctx: &Context, inventory: &Inventory, args: &[&str]) -> Result<CommandSpec> {
    if let Ok(spec) = ctx.command("npm", args) {
        return Ok(spec);
    }
    for runtime in inventory
        .providers
        .iter()
        .filter(|p| p.id == ProviderId::Js)
        .flat_map(|p| &p.runtimes)
    {
        let path = runtime
            .path
            .join(if cfg!(windows) { "npm.cmd" } else { "bin/npm" });
        if let Some(spec) = super::js_tooling::cli_command(ctx, &path, "npm", args) {
            return Ok(spec);
        }
    }
    Err(Error::Unavailable(
        "Install a Node.js runtime in Envark before installing Yarn.".into(),
    ))
}

pub fn options(ctx: &Context, inventory: &Inventory, provider: ProviderId) -> Vec<OptionView> {
    MANAGERS
        .iter()
        .filter(|(_, id)| *id == provider)
        .map(|(name, _)| {
            let installed = (*name == "nvm" && binary(&root(ctx, name), name).is_file())
                || (*name != "nvm" && ctx.executable(name).is_some())
                || inventory
                    .providers
                    .iter()
                    .filter(|p| p.id == provider)
                    .any(|p| {
                        p.managers.iter().any(|m| m.name == *name)
                            || p.package_managers.iter().any(|m| m.name == *name)
                    });
            let prerequisite = if *name == "nvm" && cfg!(windows) {
                Some("nvm-sh supports macOS and Linux. Install fnm on Windows instead.")
            } else if *name == "fnm" && cfg!(windows) && ctx.executable("winget").is_none() {
                Some("Installing fnm on Windows requires Windows App Installer (winget).")
            } else if *name == "yarn"
                && ctx.executable("corepack").is_none()
                && npm(ctx, inventory, &["--version"]).is_err()
            {
                Some("Install a Node.js runtime in Envark first.")
            } else if cfg!(windows)
                && !["fnm", "yarn", "nvm"].contains(name)
                && powershell(ctx).is_none()
            {
                Some("PowerShell is unavailable on this installation.")
            } else if !cfg!(windows)
                && *name != "yarn"
                && (ctx.executable("bash").is_none() || ctx.executable("curl").is_none())
            {
                Some("The official installer requires Bash and curl.")
            } else if !cfg!(windows)
                && ["fnm", "bun"].contains(name)
                && ctx.executable("unzip").is_none()
            {
                Some("The official installer requires unzip.")
            } else {
                None
            };
            OptionView {
                name: (*name).into(),
                source: if *name == "yarn" {
                    if ctx.executable("corepack").is_some() {
                        "Corepack"
                    } else {
                        "npm (Yarn Classic)"
                    }
                } else if cfg!(windows) && *name == "fnm" {
                    "winget"
                } else {
                    "official-installer"
                }
                .into(),
                installed,
                available: prerequisite.is_none(),
                reason: prerequisite.map(str::to_owned),
            }
        })
        .collect()
}

async fn release(ctx: &Context, name: &str, modern_yarn: bool) -> Result<String> {
    let url = match name {
        "fnm" => "https://api.github.com/repos/Schniz/fnm/releases/latest",
        "nvm" => "https://api.github.com/repos/nvm-sh/nvm/releases/latest",
        "bun" => "https://api.github.com/repos/oven-sh/bun/releases/latest",
        "uv" => "https://api.github.com/repos/astral-sh/uv/releases/latest",
        "pnpm" => "https://registry.npmjs.org/pnpm/latest",
        "yarn" if modern_yarn => "https://registry.npmjs.org/@yarnpkg%2Fcli-dist/latest",
        "yarn" => "https://registry.npmjs.org/yarn/latest",
        _ => return Err(Error::InvalidInput("Unknown manager.".into())),
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("Envark/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    let request = async {
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let version = json["tag_name"]
            .as_str()
            .or_else(|| json["version"].as_str())
            .ok_or_else(|| {
                Error::Unavailable("The official source did not report a release version.".into())
            })?
            .trim_start_matches("bun-")
            .trim_start_matches('v');
        semver::Version::parse(version)
            .map_err(|_| Error::Unavailable("The official release version is invalid.".into()))?;
        Ok(version.to_owned())
    };
    tokio::select! { result = request => result, _ = ctx.cancel.cancelled() => Err(Error::Cancelled) }
}

fn shell_profile(ctx: &Context) -> PathBuf {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".into()
        } else {
            "/bin/bash".into()
        }
    });
    if shell.ends_with("/zsh") {
        ctx.home.join(".zshrc")
    } else {
        ctx.home.join(".bashrc")
    }
}

pub(crate) async fn prepare(
    ctx: &Context,
    inventory: &Inventory,
    provider: ProviderId,
    name: &str,
) -> Result<Installation> {
    let option = options(ctx, inventory, provider)
        .into_iter()
        .find(|o| o.name == name)
        .ok_or_else(|| {
            Error::InvalidInput("This manager does not belong to the selected environment.".into())
        })?;
    if option.installed {
        return Err(Error::Conflict(
            "This manager is already installed. Use its update action.".into(),
        ));
    }
    if let Some(reason) = option.reason {
        return Err(Error::Unavailable(reason));
    }
    let version = release(ctx, name, option.source == "Corepack").await?;
    recipe(ctx, inventory, name, &version).await
}

pub(crate) async fn recipe(
    ctx: &Context,
    inventory: &Inventory,
    name: &str,
    version: &str,
) -> Result<Installation> {
    semver::Version::parse(version)
        .map_err(|_| Error::InvalidInput("Invalid manager version.".into()))?;
    if name == "yarn" {
        if let Some(corepack) = ctx.executable("corepack") {
            let mut tool = basic_tool("yarn", "0.0.0".into(), "corepack", Some(corepack.clone()));
            tool.latest = Some(version.into());
            let install = super::js_tooling::corepack_command(ctx, &tool, true)?;
            let directory = corepack.parent().ok_or_else(|| {
                Error::Unavailable("Corepack has no installation directory.".into())
            })?;
            let target = directory.join(if cfg!(windows) { "yarn.cmd" } else { "yarn" });
            let mut enable = install.clone();
            enable.args.truncate(1);
            enable.args.extend([
                "enable".into(),
                "--install-directory".into(),
                directory.to_string_lossy().into_owned(),
                "yarn".into(),
            ]);
            let result = Installation {
                name: name.into(),
                version: version.into(),
                target,
                script: None,
                commands: vec![install, enable],
                profile: None,
                source: "corepack".into(),
            };
            validate_absent(&result)?;
            return Ok(result);
        }
        let probe = npm(ctx, inventory, &["prefix", "--global"])?;
        let output = ctx.runner.run(&probe, &ctx.cancel).await?;
        let prefix = PathBuf::from(output.stdout.trim());
        reject_links(&prefix)?;
        let target = prefix.join(if cfg!(windows) {
            "node_modules/yarn"
        } else {
            "lib/node_modules/yarn"
        });
        let mut spec = npm(
            ctx,
            inventory,
            &[
                "install",
                "--global",
                &format!("yarn@{version}"),
                "--prefix",
                &prefix.to_string_lossy(),
            ],
        )?;
        spec.cwd = Some(ctx.home.clone());
        spec.timeout = Duration::from_secs(1800);
        let result = Installation {
            name: name.into(),
            version: version.into(),
            target,
            script: None,
            commands: vec![spec],
            profile: None,
            source: "npm".into(),
        };
        validate_absent(&result)?;
        return Ok(result);
    }
    if name == "fnm" && cfg!(windows) {
        let mut spec = ctx.command(
            "winget",
            &[
                "install",
                "--id",
                "Schniz.fnm",
                "--exact",
                "--version",
                version,
                "--source",
                "winget",
                "--scope",
                "user",
                "--silent",
                "--disable-interactivity",
                "--accept-source-agreements",
                "--accept-package-agreements",
            ],
        )?;
        spec.timeout = Duration::from_secs(1800);
        let result = Installation {
            name: name.into(),
            version: version.into(),
            target: ctx
                .home
                .join("AppData/Local/Microsoft/WinGet/Links/fnm.exe"),
            script: None,
            commands: vec![spec],
            profile: None,
            source: "winget".into(),
        };
        validate_absent(&result)?;
        return Ok(result);
    }
    if name == "nvm" && cfg!(windows) {
        return Err(Error::Unavailable(
            "nvm-sh is unavailable on Windows; install fnm instead.".into(),
        ));
    }
    let install_root = root(ctx, name);
    reject_links(&install_root)?;
    let target = binary(&install_root, name);
    let script = match name {
        "fnm" => format!("https://raw.githubusercontent.com/Schniz/fnm/v{version}/.ci/install.sh"),
        "nvm" => format!("https://raw.githubusercontent.com/nvm-sh/nvm/v{version}/install.sh"),
        "bun" => format!(
            "https://raw.githubusercontent.com/oven-sh/bun/bun-v{version}/src/cli/install.{}",
            if cfg!(windows) { "ps1" } else { "sh" }
        ),
        "uv" => format!(
            "https://github.com/astral-sh/uv/releases/download/{version}/uv-installer.{}",
            if cfg!(windows) { "ps1" } else { "sh" }
        ),
        "pnpm" => format!(
            "https://raw.githubusercontent.com/pnpm/get.pnpm.io/master/install.{}",
            if cfg!(windows) { "ps1" } else { "sh" }
        ),
        _ => return Err(Error::InvalidInput("Unknown official installer.".into())),
    };
    let mut command = if cfg!(windows) {
        CommandSpec::new(
            powershell(ctx)
                .ok_or_else(|| Error::Unavailable("PowerShell is unavailable.".into()))?,
            [
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script,
            ],
        )
    } else {
        let mut spec = ctx.command("bash", &["--noprofile", "--norc", &script])?;
        spec.env.insert("BASH_ENV".into(), "/dev/null".into());
        spec
    };
    let path = install_root.to_string_lossy().into_owned();
    let profile = (!cfg!(windows)).then(|| shell_profile(ctx));
    match name {
        "fnm" => command.args.extend([
            "--force-no-brew".into(),
            "--install-dir".into(),
            path,
            "--release".into(),
            format!("v{version}"),
        ]),
        "nvm" => {
            command.env.extend([
                ("NVM_DIR".into(), path),
                ("NVM_INSTALL_VERSION".into(), format!("v{version}")),
                ("NVM_INSTALL_GITHUB_REPO".into(), "nvm-sh/nvm".into()),
                ("NODE_VERSION".into(), String::new()),
                ("METHOD".into(), "script".into()),
                (
                    "PROFILE".into(),
                    profile.as_ref().unwrap().to_string_lossy().into_owned(),
                ),
            ]);
            command.remove_env.push("NVM_SOURCE".into());
        }
        "bun" => {
            command.env.insert("BUN_INSTALL".into(), path);
            if cfg!(windows) {
                command.args.extend([
                    "-Version".into(),
                    version.into(),
                    "-NoRegisterInstallation".into(),
                ]);
            } else {
                command.args.push(format!("bun-v{version}"));
            }
        }
        "uv" => {
            command.env.insert("UV_INSTALL_DIR".into(), path);
        }
        "pnpm" => {
            command.env.extend([
                ("PNPM_HOME".into(), path),
                ("PNPM_VERSION".into(), version.into()),
            ]);
        }
        _ => unreachable!(),
    }
    if !cfg!(windows) && std::env::var_os("SHELL").is_none() {
        command.env.insert(
            "SHELL".into(),
            if cfg!(target_os = "macos") {
                "/bin/zsh"
            } else {
                "/bin/bash"
            }
            .into(),
        );
    }
    command.cwd = Some(ctx.home.clone());
    command.timeout = Duration::from_secs(1800);
    let result = Installation {
        name: name.into(),
        version: version.into(),
        target,
        script: Some(script),
        commands: vec![command],
        profile,
        source: if name == "uv" {
            "uv-self"
        } else if name == "fnm" {
            "fnm-script"
        } else if name == "nvm" {
            "nvm-script"
        } else if name == "pnpm" {
            "pnpm-self"
        } else {
            "bun"
        }
        .into(),
    };
    validate_absent(&result)?;
    Ok(result)
}

pub(crate) fn validate_absent(install: &Installation) -> Result<()> {
    reject_links(&install.target)?;
    let mut occupied = vec![install.target.clone()];
    let directory = install
        .target
        .parent()
        .ok_or_else(|| Error::InvalidInput("Missing installation directory.".into()))?;
    let companions: &[&str] = match install.name.as_str() {
        "uv" if cfg!(windows) => &["uvx.exe"],
        "uv" => &["uvx"],
        "bun" if cfg!(windows) => &["bunx.exe"],
        "bun" => &["bunx"],
        "nvm" => &["nvm-exec", "bash_completion"],
        "yarn" if install.source == "corepack" && cfg!(windows) => {
            &["yarn", "yarn.ps1", "yarnpkg", "yarnpkg.cmd", "yarnpkg.ps1"]
        }
        "yarn" if install.source == "corepack" => &["yarnpkg"],
        _ => &[],
    };
    occupied.extend(companions.iter().map(|name| directory.join(name)));
    if install.name == "pnpm" {
        for directory in [directory.to_path_buf(), directory.join("bin")] {
            for name in if cfg!(windows) {
                vec![
                    "pnpm.exe", "pnpx.exe", "pnpm.cmd", "pnpx.cmd", "pnpm.ps1", "pnpx.ps1",
                ]
            } else {
                vec!["pnpm", "pnpx"]
            } {
                occupied.push(directory.join(name));
            }
        }
        occupied.push(install.target.parent().unwrap().join(if cfg!(windows) {
            "bin/pnpm.exe"
        } else {
            "bin/pnpm"
        }));
    }
    if install.source == "npm" {
        let prefix = install
            .target
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| Error::InvalidInput("Missing npm prefix.".into()))?;
        let bin = if cfg!(windows) {
            prefix.to_path_buf()
        } else {
            prefix.parent().unwrap().join("bin")
        };
        for name in if cfg!(windows) {
            vec![
                "yarn",
                "yarn.cmd",
                "yarn.ps1",
                "yarnpkg",
                "yarnpkg.cmd",
                "yarnpkg.ps1",
            ]
        } else {
            vec!["yarn", "yarnpkg"]
        } {
            occupied.push(bin.join(name));
        }
    }
    for path in &occupied {
        reject_links(path)?;
    }
    if occupied
        .iter()
        .any(|path| std::fs::symlink_metadata(path).is_ok())
    {
        return Err(Error::Conflict(
            "An installation already occupies this path. Refresh before continuing.".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn execute(ctx: &Context, install: Installation) -> Result<(u64, String)> {
    validate_absent(&install)?;
    let temp = tempfile::tempdir()?;
    let mut commands = install.commands.clone();
    if let Some(url) = &install.script {
        let bytes = super::script_installers::download(ctx, url).await?;
        let path = temp.path().join(if cfg!(windows) {
            "install.ps1"
        } else {
            "install.sh"
        });
        std::fs::write(&path, bytes)?;
        let arg = commands[0]
            .args
            .iter_mut()
            .find(|arg| *arg == url)
            .ok_or_else(|| Error::InvalidInput("Missing installer URL.".into()))?;
        *arg = path.to_string_lossy().into_owned();
    }
    run_prepared(ctx, &install, commands).await
}

async fn run_prepared(
    ctx: &Context,
    install: &Installation,
    commands: Vec<CommandSpec>,
) -> Result<(u64, String)> {
    validate_absent(install)?;
    if ctx.cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if install.name == "nvm" {
        std::fs::create_dir_all(install.target.parent().unwrap())?;
    }
    if let Some(profile) = &install.profile {
        reject_links(profile)?;
        if !profile.exists() {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(profile)?;
        }
    }
    for command in commands {
        ctx.runner.run(&command, &ctx.cancel).await?;
    }
    let actual = verify(ctx, install).await?;
    if actual != install.version {
        return Err(Error::Conflict(format!(
            "{} reported version {actual}; the reviewed version {} was not installed.",
            install.name, install.version
        )));
    }
    if install.name == "nvm" {
        let receipt = ctx.data.join("envark/manager-installations/nvm.json");
        reject_links(&receipt)?;
        std::fs::create_dir_all(receipt.parent().unwrap())?;
        std::fs::write(
            receipt,
            serde_json::to_vec(&serde_json::json!({"path":install.target,"method":"script"}))?,
        )?;
    }
    Ok((
        0,
        format!(
            "{} {} installed. The environment was refreshed; installed runtimes and project files were kept.",
            install.name, actual
        ),
    ))
}

pub(crate) async fn verify(ctx: &Context, install: &Installation) -> Result<String> {
    if install.name == "nvm" {
        return super::script_installers::installed_version(
            ctx,
            &basic_tool(
                "nvm",
                String::new(),
                "nvm-script",
                Some(install.target.clone()),
            ),
        )
        .await;
    }
    let path = if install.name == "pnpm" && !install.target.is_file() {
        install.target.parent().unwrap().join(if cfg!(windows) {
            "bin/pnpm.exe"
        } else {
            "bin/pnpm"
        })
    } else {
        install.target.clone()
    };
    let probe = if install.source == "npm" {
        super::js_tooling::cli_command(ctx, &path.join("bin/yarn.js"), "yarn", &["--version"])
            .ok_or_else(|| {
                Error::Unavailable("The installed Yarn command cannot be executed.".into())
            })?
    } else {
        super::js_tooling::cli_command(ctx, &path, &install.name, &["--version"])
            .unwrap_or_else(|| CommandSpec::new(path, ["--version"]))
    };
    let mut probe = probe;
    let isolated = super::js_tooling::isolated_directory()?;
    ctx.apply_read_policy(&mut probe);
    probe.cwd = Some(isolated.path().into());
    let output = ctx.runner.run(&probe, &ctx.cancel).await?;
    super::package_managers::version(&output.stdout)
        .ok_or_else(|| Error::Unavailable("The installed manager did not report a version.".into()))
}

pub(crate) fn script_nvm(ctx: &Context, root: &Path) -> bool {
    let receipt = ctx.data.join("envark/manager-installations/nvm.json");
    read_small(&receipt, 16_384)
        .ok()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok())
        .is_some_and(|json| {
            json["method"] == "script"
                && json["path"]
                    .as_str()
                    .is_some_and(|path| Path::new(path) == root.join("nvm.sh"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(path: &Path) -> Context {
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = path.canonicalize().unwrap();
        ctx.data = ctx.home.join("data");
        ctx.cache = ctx.home.join("cache");
        ctx
    }

    #[test]
    fn installation_rejects_existing_files_and_symlink_targets() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let install = Installation {
            name: "fnm".into(),
            version: "1.0.0".into(),
            target: root.join("fnm"),
            script: None,
            commands: vec![],
            profile: None,
            source: "fnm-script".into(),
        };
        validate_absent(&install).unwrap();
        std::fs::write(&install.target, "existing installation").unwrap();
        assert!(validate_absent(&install).is_err());
        #[cfg(unix)]
        {
            std::fs::remove_file(&install.target).unwrap();
            std::os::unix::fs::symlink(root.join("missing"), &install.target).unwrap();
            assert!(validate_absent(&install).is_err());
        }
    }

    #[tokio::test]
    async fn unknown_managers_cannot_supply_installation_commands() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        assert!(
            prepare(&ctx, &Inventory::default(), ProviderId::Go, "uv")
                .await
                .is_err()
        );
        assert!(
            recipe(&ctx, &Inventory::default(), "untrusted;command", "1.0.0")
                .await
                .is_err()
        );
        assert!(
            recipe(&ctx, &Inventory::default(), "uv", "1.0.0;command")
                .await
                .is_err()
        );
        assert!(options(&ctx, &Inventory::default(), ProviderId::Go).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn manager_install_remove_and_reinstall_preserve_user_data() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "providers::manager_install::tests::lifecycle_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("ENVARK_LIFECYCLE_FIXTURE", &root)
            .env("FNM_DIR", root.join("data/fnm"))
            .env("NVM_DIR", root.join(".nvm"))
            .env("BUN_INSTALL", root.join(".bun"))
            .env("UV_INSTALL_DIR", root.join(".local/bin"))
            .env("PNPM_HOME", root.join("data/pnpm"))
            .env("XDG_CONFIG_HOME", root.join(".config"))
            .env("SHELL", "/bin/bash")
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
    #[ignore = "Runs in an isolated subprocess with fixture-owned manager roots"]
    async fn lifecycle_fixture() {
        use crate::{
            model::Settings,
            operations::{self, ActionRequest, RefreshTarget},
        };
        let temp = PathBuf::from(std::env::var_os("ENVARK_LIFECYCLE_FIXTURE").unwrap());
        let ctx = context(&temp);
        let profile = ctx.home.join(".bashrc");
        std::fs::write(&profile, "# existing configuration\n").unwrap();
        let installer = ctx.home.join("installer.sh");
        std::fs::write(
            &installer,
            r##"#!/bin/bash
set -eu
mkdir -p "$(dirname "$1")"
if [ "$3" = nvm ]; then
  printf 'nvm() { echo %s; }\n' "$2" > "$1"
else
  printf '#!/bin/sh\necho %s\n' "$2" > "$1"
fi
chmod +x "$1"
"##,
        )
        .unwrap();
        for (name, provider) in [
            ("fnm", ProviderId::Js),
            ("nvm", ProviderId::Js),
            ("bun", ProviderId::Js),
            ("pnpm", ProviderId::Js),
            ("uv", ProviderId::Py),
        ] {
            let install = recipe(&ctx, &Inventory::default(), name, "1.2.3")
                .await
                .unwrap();
            assert!(install.script.as_ref().unwrap().starts_with("https://"));
            let data_root = root(&ctx, name);
            let sentinel = data_root.join("retained-runtime/source.txt");
            std::fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
            std::fs::write(&sentinel, "keep runtime and user files").unwrap();
            let command = CommandSpec::new(
                "/bin/bash",
                [
                    installer.to_string_lossy().into_owned(),
                    install.target.to_string_lossy().into_owned(),
                    install.version.clone(),
                    name.into(),
                ],
            );
            run_prepared(&ctx, &install, vec![command.clone()])
                .await
                .unwrap();
            assert!(verify(&ctx, &install).await.unwrap() == "1.2.3");
            assert!(
                run_prepared(&ctx, &install, vec![command.clone()])
                    .await
                    .is_err()
            );
            if name == "uv" {
                let receipt = ctx.home.join(".config/uv/uv-receipt.json");
                std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
                std::fs::write(
                    receipt,
                    serde_json::json!({"install_prefix":data_root}).to_string(),
                )
                .unwrap();
            }
            if name == "nvm" {
                // Retained Git metadata must not turn a subsequent script install into a Git update.
                std::fs::create_dir_all(data_root.join(".git")).unwrap();
                assert!(script_nvm(&ctx, &data_root));
            }
            let mut tool = basic_tool(
                name,
                "1.2.3".into(),
                &install.source,
                Some(install.target.clone()),
            );
            tool.can_remove = true;
            let mut p = Provider::empty(provider);
            p.package_managers.push(tool.clone());
            let inventory = Inventory {
                providers: vec![p],
                ..Default::default()
            };
            let settings = Settings {
                use_trash: false,
                ..Default::default()
            };
            let plan = operations::prepare(
                ActionRequest::RemoveTool {
                    provider,
                    id: tool.id.clone(),
                },
                &inventory,
                &settings,
                &ctx,
            )
            .await
            .unwrap();
            assert_eq!(plan.view.kind, "removeManager");
            assert!(
                matches!(plan.refresh_targets()[0], Some(RefreshTarget::Provider(id)) if id == provider)
            );
            let removal = super::super::manager_remove::prepare(&ctx, &tool)
                .await
                .unwrap();
            super::super::manager_remove::execute(&ctx, removal, false)
                .await
                .unwrap();
            assert!(!install.target.exists());
            assert_eq!(
                std::fs::read_to_string(&sentinel).unwrap(),
                "keep runtime and user files"
            );
            assert_eq!(
                std::fs::read_to_string(&profile).unwrap(),
                "# existing configuration\n"
            );
            run_prepared(&ctx, &install, vec![command]).await.unwrap();
            let removal = super::super::manager_remove::prepare(&ctx, &tool)
                .await
                .unwrap();
            std::fs::write(&install.target, "changed after review").unwrap();
            assert!(
                super::super::manager_remove::execute(&ctx, removal, false)
                    .await
                    .is_err()
            );
            assert!(install.target.exists());
        }
        // A successful exit without an installed binary is not success.
        assert!(
            recipe(&ctx, &Inventory::default(), "fnm", "1.2.3")
                .await
                .is_err()
        );
        let absent = Installation {
            name: "fnm".into(),
            version: "1.2.3".into(),
            target: ctx.home.join("not-installed"),
            script: None,
            commands: vec![],
            profile: None,
            source: "fnm-script".into(),
        };
        assert!(
            run_prepared(
                &ctx,
                &absent,
                vec![CommandSpec::new("/bin/true", [] as [&str; 0])]
            )
            .await
            .is_err()
        );
        let cancelled = Context {
            cancel: CancellationToken::new(),
            ..ctx.clone()
        };
        cancelled.cancel.cancel();
        assert!(matches!(
            run_prepared(&cancelled, &absent, vec![]).await,
            Err(Error::Cancelled)
        ));
        assert!(!absent.target.exists());
    }
}
