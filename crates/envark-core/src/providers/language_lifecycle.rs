//! Native language manager lifecycle. Manager removal deliberately keeps SDKs and user data.
//! Sources: rust-lang.github.io/rustup/installation; sdkman.io/install;
//! mise.jdx.dev/installing-mise.html and mise.jdx.dev/cli/{install,use,uninstall,ls}.html.
use super::*;
use crate::{
    filesystem::reject_links,
    model::{Inventory, Runtime, Settings},
    operations::{ActionRequest, PlanItem},
};
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime};

const NAMES: &[&str] = &["rustup", "SDKMAN!", "mise"];

fn configured(variable: &str, fallback: PathBuf) -> PathBuf {
    std::env::var_os(variable)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or(fallback)
}
fn cargo_home(ctx: &Context) -> PathBuf {
    configured("CARGO_HOME", ctx.home.join(".cargo"))
}
fn sdk_home(ctx: &Context) -> PathBuf {
    configured("SDKMAN_DIR", ctx.home.join(".sdkman"))
}
fn mise_data(ctx: &Context) -> PathBuf {
    configured(
        "MISE_DATA_DIR",
        if cfg!(windows) {
            configured("LOCALAPPDATA", ctx.home.join("AppData/Local")).join("mise")
        } else {
            configured("XDG_DATA_HOME", ctx.home.join(".local/share")).join("mise")
        },
    )
}
fn file_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    }
}
fn target(ctx: &Context, name: &str) -> PathBuf {
    match name {
        "rustup" => cargo_home(ctx).join("bin").join(file_name(name)),
        "SDKMAN!" => sdk_home(ctx).join("bin/sdkman-init.sh"),
        _ if cfg!(windows) => ctx
            .home
            .join("AppData/Local/Microsoft/WinGet/Links/mise.exe"),
        _ => ctx.home.join(".local/bin/mise"),
    }
}
fn provider_name(provider: ProviderId) -> Result<&'static str> {
    match provider {
        ProviderId::Go => Ok("go"),
        ProviderId::Jvm => Ok("java"),
        _ => Err(Error::InvalidInput("mise manages Go and Java here.".into())),
    }
}
fn valid_pair(provider: ProviderId, name: &str) -> bool {
    matches!(
        (provider, name),
        (ProviderId::Rust, "rustup")
            | (ProviderId::Jvm, "SDKMAN!")
            | (ProviderId::Jvm | ProviderId::Go, "mise")
    )
}
fn manager_path(ctx: &Context, name: &str) -> Option<PathBuf> {
    if name == "SDKMAN!" {
        let p = target(ctx, name);
        p.is_file().then_some(p)
    } else {
        ctx.executable(name).or_else(|| {
            let p = target(ctx, name);
            p.is_file().then_some(p)
        })
    }
}

pub(crate) fn options(
    ctx: &Context,
    inventory: &Inventory,
    provider: ProviderId,
) -> Vec<manager_install::OptionView> {
    NAMES
        .iter()
        .filter(|name| valid_pair(provider, name))
        .map(|name| {
            let reason = if *name == "SDKMAN!" && cfg!(windows) {
                Some("SDKMAN! requires Unix Bash; install mise to manage Java on Windows.".into())
            } else if *name == "mise" && cfg!(windows) && ctx.executable("winget").is_none() {
                Some("Install Windows App Installer (winget) to install mise.".into())
            } else if !cfg!(windows) && ["bash", "curl"].iter().any(|p| ctx.executable(p).is_none())
            {
                Some("The official installer requires Bash and curl.".into())
            } else if *name == "SDKMAN!"
                && ["zip", "unzip"].iter().any(|p| ctx.executable(p).is_none())
            {
                Some("The official SDKMAN! installer requires zip and unzip.".into())
            } else {
                None
            };
            manager_install::OptionView {
                name: (*name).into(),
                source: if *name == "mise" && cfg!(windows) {
                    "winget"
                } else {
                    "official-installer"
                }
                .into(),
                installed: manager_path(ctx, name).is_some()
                    || inventory
                        .providers
                        .iter()
                        .any(|p| p.managers.iter().any(|m| m.name == *name)),
                available: reason.is_none(),
                reason,
            }
        })
        .collect()
}

pub(crate) fn owns_tool(tool: &Tool) -> bool {
    NAMES.contains(&tool.name.as_str())
        && matches!(
            tool.source.as_str(),
            "rustup-self" | "sdkman-self" | "mise-self" | "mise-winget" | "homebrew"
        )
}
fn find_tool<'a>(inventory: &'a Inventory, provider: ProviderId, id: &str) -> Option<&'a Tool> {
    inventory
        .providers
        .iter()
        .find(|p| p.id == provider)?
        .package_managers
        .iter()
        .chain(&inventory.providers.iter().find(|p| p.id == provider)?.tools)
        .find(|t| t.id == id)
}
pub(crate) fn handles(request: &ActionRequest, inventory: &Inventory) -> bool {
    match request {
        ActionRequest::InstallManager { provider, manager } => valid_pair(*provider, manager),
        ActionRequest::UpdateTool { provider, id } | ActionRequest::RemoveTool { provider, id } => {
            find_tool(inventory, *provider, id).is_some_and(owns_tool)
        }
        ActionRequest::InstallRuntime {
            provider, manager, ..
        } => manager == "mise" && provider_name(*provider).is_ok(),
        ActionRequest::SetDefault { provider, id }
        | ActionRequest::RemoveRuntime { provider, id } => inventory
            .providers
            .iter()
            .find(|p| p.id == *provider)
            .is_some_and(|p| {
                p.runtimes
                    .iter()
                    .any(|r| r.id == *id && r.manager == "mise")
            }),
        _ => false,
    }
}

async fn fetch(ctx: &Context, url: &str) -> Result<Vec<u8>> {
    let request = async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("Envark/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?
        {
            if bytes.len() + chunk.len() > 64 * 1024 * 1024 {
                return Err(Error::Unavailable(
                    "Official download exceeds 64 MiB.".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    };
    tokio::select! { result=request => result, _=ctx.cancel.cancelled()=>Err(Error::Cancelled) }
}
fn normalized(text: &str) -> Result<String> {
    let clean = text
        .trim()
        .trim_start_matches('v')
        .trim_end_matches("-stable");
    semver::Version::parse(clean)
        .map(|v| v.to_string())
        .map_err(|_| Error::Unavailable(format!("Invalid manager release version: {clean}")))
}
async fn official_latest(ctx: &Context, name: &str) -> Result<String> {
    let url = match name {
        "rustup" => "https://static.rust-lang.org/rustup/release-stable.toml",
        "SDKMAN!" => "https://api.sdkman.io/2/broker/version/sdkman/script/stable",
        _ => "https://api.github.com/repos/jdx/mise/releases/latest",
    };
    let bytes = fetch(ctx, url).await?;
    let text = String::from_utf8(bytes).map_err(|e| Error::Unavailable(e.to_string()))?;
    let version = match name {
        "rustup" => text
            .parse::<toml::Value>()
            .map_err(|e| Error::Unavailable(e.to_string()))?
            .get("version")
            .and_then(toml::Value::as_str)
            .map(str::to_owned),
        "SDKMAN!" => Some(text),
        _ => serde_json::from_str::<serde_json::Value>(&text)?
            .get("tag_name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    }
    .ok_or_else(|| Error::Unavailable("Official source did not report a release.".into()))?;
    normalized(&version)
}
pub(crate) async fn latest(ctx: &Context, tool: &Tool) -> Result<String> {
    validate_owner(ctx, tool).await?;
    if tool.source == "homebrew" {
        let formula = package_managers::brew_formula(ctx, tool.path.as_deref().unwrap())
            .await
            .ok_or_else(|| Error::Conflict("Homebrew ownership changed.".into()))?;
        let output = ctx.read("brew", &["info", "--json=v2", &formula]).await?;
        let json: serde_json::Value = serde_json::from_str(&output)?;
        if json["formulae"][0]["pinned"].as_bool() == Some(true) {
            return Err(Error::Unavailable(
                "This Homebrew formula is pinned.".into(),
            ));
        }
        return normalized(
            json["formulae"][0]["versions"]["stable"]
                .as_str()
                .ok_or_else(|| {
                    Error::Unavailable("Homebrew did not report a stable release.".into())
                })?,
        );
    }
    if tool.source == "mise-winget" {
        return winget_latest(ctx).await;
    }
    official_latest(ctx, &tool.name).await
}
async fn winget_latest(ctx: &Context) -> Result<String> {
    let text = ctx
        .read(
            "winget",
            &[
                "show",
                "--id",
                "jdx.mise",
                "--exact",
                "--source",
                "winget",
                "--versions",
                "--disable-interactivity",
            ],
        )
        .await?;
    text.lines()
        .filter_map(|l| semver::Version::parse(l.trim()).ok())
        .max()
        .map(|v| v.to_string())
        .ok_or_else(|| Error::Unavailable("winget did not report mise release versions.".into()))
}

fn winget_owned(ctx: &Context, path: &Path) -> bool {
    if !cfg!(windows) {
        return false;
    }
    path.canonicalize()
        .ok()
        .and_then(|p| {
            ctx.home
                .join("AppData/Local/Microsoft/WinGet/Packages")
                .canonicalize()
                .ok()
                .and_then(|base| p.strip_prefix(base).ok().map(Path::to_path_buf))
        })
        .is_some_and(|p| {
            p.components()
                .next()
                .is_some_and(|v| v.as_os_str().to_string_lossy().starts_with("jdx.mise_"))
                && p.file_name().is_some_and(|n| n == "mise.exe")
        })
}
async fn owner(ctx: &Context, name: &str, path: &Path) -> Option<String> {
    if let Some(formula) = package_managers::brew_formula(ctx, path).await {
        if formula
            .rsplit('/')
            .next()
            .is_some_and(|v| v == name || (name == "rustup" && v == "rustup-init"))
        {
            return Some("homebrew".into());
        }
        return None;
    }
    if name == "mise" && winget_owned(ctx, path) {
        return Some("mise-winget".into());
    }
    // Do not adopt arbitrary PATH binaries or links into another package manager's tree.
    let expected = target(ctx, name);
    if path != expected || reject_links(&expected).is_err() || !expected.is_file() {
        return None;
    }
    Some(
        match name {
            "rustup" => "rustup-self",
            "SDKMAN!" => "sdkman-self",
            _ => "mise-self",
        }
        .into(),
    )
}
async fn validate_owner(ctx: &Context, tool: &Tool) -> Result<()> {
    if tool.source == "sdkman-self" {
        for name in ["bin", "src", "libexec", "var", "tmp"] {
            let directory = sdk_home(ctx).join(name);
            reject_links(&directory)?;
            if directory.is_dir() {
                for entry in walkdir::WalkDir::new(&directory).follow_links(false) {
                    let entry = entry.map_err(|e| Error::Unavailable(e.to_string()))?;
                    reject_links(entry.path())?;
                }
            }
        }
    }
    let path = tool
        .path
        .as_deref()
        .ok_or_else(|| Error::Conflict("Manager path is missing.".into()))?;
    if owner(ctx, &tool.name, path).await.as_deref() != Some(&tool.source) {
        return Err(Error::Conflict(
            "Manager installation ownership changed after review.".into(),
        ));
    }
    Ok(())
}
async fn installed_version(ctx: &Context, name: &str, path: &Path) -> Result<String> {
    if name == "SDKMAN!" {
        return normalized(&read_small(
            &path.parent().unwrap().parent().unwrap().join("var/version"),
            128,
        )?);
    }
    let mut command = CommandSpec::new(path, ["--version"]);
    command.cwd = Some(ctx.home.clone());
    let output = ctx.runner.run(&command, &ctx.cancel).await?;
    package_managers::version(&output.stdout)
        .ok_or_else(|| Error::Unavailable(format!("Cannot read {name} version.")))
}

fn mise_command(ctx: &Context, path: &Path, args: &[&str], scratch: &Path) -> CommandSpec {
    let mut spec = CommandSpec::new(path, args.iter().copied());
    spec.cwd = Some(scratch.into());
    spec.timeout = Duration::from_secs(1800);
    // A fresh cwd and ceiling prevent local config, hooks and task files being loaded.
    spec.env.insert(
        "MISE_CEILING_PATHS".into(),
        scratch.to_string_lossy().into_owned(),
    );
    spec.env.insert(
        "MISE_DATA_DIR".into(),
        mise_data(ctx).to_string_lossy().into_owned(),
    );
    spec.env.insert("MISE_YES".into(), "1".into());
    spec.env.insert("MISE_SELF_UPDATE_AUTO".into(), "0".into());
    spec.env.insert(
        "MISE_IDIOMATIC_VERSION_FILE_ENABLE_TOOLS".into(),
        String::new(),
    );
    spec.remove_env.extend(
        [
            "MISE_ENV",
            "MISE_DEFAULT_CONFIG_FILENAME",
            "MISE_OVERRIDE_CONFIG_FILENAMES",
            "MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES",
        ]
        .map(str::to_owned),
    );
    spec
}
async fn mise_runtimes(ctx: &Context, path: &Path, provider: ProviderId) -> Result<Vec<Runtime>> {
    let scratch = tempfile::tempdir()?;
    let name = provider_name(provider)?;
    let output = ctx
        .runner
        .run(
            &mise_command(
                ctx,
                path,
                &["ls", name, "--installed", "--json"],
                scratch.path(),
            ),
            &ctx.cancel,
        )
        .await?;
    let json: serde_json::Value = serde_json::from_str(&output.stdout)?;
    let records = json
        .as_array()
        .or_else(|| json.get(name).and_then(serde_json::Value::as_array))
        .ok_or_else(|| {
            Error::Unavailable("mise returned an invalid installed-runtime list.".into())
        })?;
    let mut result = vec![];
    for record in records {
        let Some(version) = record["version"].as_str() else {
            continue;
        };
        let Some(path) = record["install_path"].as_str().map(PathBuf::from) else {
            continue;
        };
        // External links and shared installations are visible, but never mutable.
        let managed = path.starts_with(mise_data(ctx).join("installs").join(name))
            && reject_links(&path).is_ok()
            && path.is_dir();
        result.push(Runtime {
            id: id_for("runtime", &path),
            version: version.into(),
            selector: Some(version.into()),
            manager: "mise".into(),
            path,
            active: record["active"].as_bool().unwrap_or(false),
            active_known: record["active"].is_boolean(),
            managed,
            size: None,
            note: (!managed).then(|| {
                "Linked or external mise installation; manage it with its original owner.".into()
            }),
        });
    }
    Ok(result)
}
pub(crate) async fn discover(ctx: &Context, provider: &mut Provider) {
    let names: &[&str] = match provider.id {
        ProviderId::Rust => &["rustup"],
        ProviderId::Jvm => &["SDKMAN!", "mise"],
        ProviderId::Go => &["mise"],
        _ => return,
    };
    for name in names {
        let Some(path) = manager_path(ctx, name) else {
            continue;
        };
        let version = match installed_version(ctx, name, &path).await {
            Ok(v) => v,
            Err(e) => {
                provider.issues.push(e.to_string());
                continue;
            }
        };
        let source = owner(ctx, name, &path).await;
        let mut tool = basic_tool(
            name,
            version.clone(),
            source.as_deref().unwrap_or("external"),
            Some(path.clone()),
        );
        tool.can_update = source.is_some();
        tool.can_remove = source.is_some();
        if source.is_none() {
            tool.note=Some("Installation ownership is unverified; package-manager updates remain with its owner.".into());
        }
        provider.package_managers.retain(|t| t.name != *name);
        provider.package_managers.push(tool);
        if *name == "mise" {
            provider.managers.retain(|m| m.name != "mise");
            provider.managers.push(Manager {
                name: "mise".into(),
                version,
                path: path.clone(),
                supports_install: source.is_some(),
                supports_default: source.is_some(),
            });
            match mise_runtimes(ctx, &path, provider.id).await {
                Ok(runtimes) => {
                    provider.runtimes.retain(|r| {
                        r.manager != "mise" && !runtimes.iter().any(|m| m.path == r.path)
                    });
                    provider.runtimes.extend(runtimes);
                }
                Err(e) => provider.issues.push(e.to_string()),
            }
        }
        provider.detected = true;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSnapshot {
    path: PathBuf,
    real: PathBuf,
    length: u64,
    modified: SystemTime,
    hash: String,
}
fn snapshot(path: &Path) -> Result<FileSnapshot> {
    let real = path.canonicalize()?;
    let metadata = std::fs::metadata(&real)?;
    if !metadata.is_file() {
        return Err(Error::unsafe_path(path, "expected a manager file"));
    }
    // Also verify content: same-length replacements must invalidate a reviewed plan.
    let mut file = std::fs::File::open(&real)?;
    let mut hash = Sha256::new();
    use std::io::Read;
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(FileSnapshot {
        path: path.into(),
        real,
        length: metadata.len(),
        modified: metadata.modified()?,
        hash: hex::encode(hash.finalize()),
    })
}
fn removal_files(tool: &Tool) -> Result<Vec<FileSnapshot>> {
    let path = tool
        .path
        .as_deref()
        .ok_or_else(|| Error::Conflict("Missing manager path.".into()))?;
    reject_links(path)?;
    let mut paths = vec![path.to_path_buf()];
    if tool.name == "rustup" {
        let primary = snapshot(path)?;
        // rustup installs its proxies as hardlinks/copies. Only identical bytes prove
        // that a familiar filename is a rustup proxy rather than a Cargo package.
        for name in [
            "cargo",
            "cargo-clippy",
            "cargo-fmt",
            "cargo-miri",
            "clippy-driver",
            "rls",
            "rust-analyzer",
            "rust-gdb",
            "rust-gdbgui",
            "rust-lldb",
            "rustc",
            "rustdoc",
            "rustfmt",
        ] {
            let proxy = path.parent().unwrap().join(file_name(name));
            if proxy.is_file()
                && reject_links(&proxy).is_ok()
                && snapshot(&proxy)?.hash == primary.hash
            {
                paths.push(proxy);
            }
        }
    } else if tool.name == "SDKMAN!" {
        let root = path.parent().unwrap().parent().unwrap();
        paths.clear();
        for name in ["bin", "src", "libexec"] {
            let directory = root.join(name);
            if !directory.exists() {
                continue;
            }
            reject_links(&directory)?;
            for entry in walkdir::WalkDir::new(&directory).follow_links(false) {
                let entry = entry.map_err(|e| Error::Unavailable(e.to_string()))?;
                reject_links(entry.path())?;
                if entry.file_type().is_file() {
                    paths.push(entry.path().into());
                }
            }
        }
    }
    paths.sort();
    paths.iter().map(|p| snapshot(p)).collect()
}
#[derive(Debug, Clone)]
enum Operation {
    Install {
        name: String,
        version: String,
        target: PathBuf,
        payload: Option<Vec<u8>>,
    },
    Update {
        tool: Tool,
        version: String,
    },
    Remove {
        tool: Tool,
        files: Vec<FileSnapshot>,
    },
    Runtime {
        tool: Tool,
        verb: String,
        version: String,
        runtime: Option<Box<Runtime>>,
    },
}
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    provider: ProviderId,
    operation: Operation,
    expected: Option<FileSnapshot>,
    command: Option<CommandSpec>,
}
fn runtime_selector(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 100
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_' | b'+'))
    {
        return Err(Error::InvalidInput("Enter a Go version (for example 1.24.3) or Java distribution version (for example temurin-21.0.7+6; use a mise-supported selector).".into()));
    }
    Ok(())
}
async fn manager_tool(ctx: &Context, inventory: &Inventory, provider: ProviderId) -> Result<Tool> {
    if let Some(tool) = inventory
        .providers
        .iter()
        .find(|p| p.id == provider)
        .and_then(|p| {
            p.package_managers
                .iter()
                .find(|t| t.name == "mise" && owns_tool(t))
        })
    {
        validate_owner(ctx, tool).await?;
        return Ok(tool.clone());
    }
    Err(Error::Unavailable(
        "Install a verified mise manager in Envark first.".into(),
    ))
}
fn winget(ctx: &Context, verb: &str, version: Option<&str>) -> Result<CommandSpec> {
    let mut args = vec![
        verb,
        "--id",
        "jdx.mise",
        "--exact",
        "--source",
        "winget",
        "--silent",
        "--disable-interactivity",
    ];
    if let Some(version) = version {
        args.extend([
            "--version",
            version,
            "--accept-source-agreements",
            "--accept-package-agreements",
        ]);
    }
    let mut command = ctx.command("winget", &args)?;
    command.timeout = Duration::from_secs(1800);
    Ok(command)
}
async fn update_command(ctx: &Context, tool: &Tool, version: &str) -> Result<CommandSpec> {
    let path = tool.path.as_deref().unwrap();
    let mut command = match tool.source.as_str() {
        "homebrew" => {
            let formula = package_managers::brew_formula(ctx, path)
                .await
                .ok_or_else(|| Error::Conflict("Homebrew ownership changed.".into()))?;
            let mut spec = ctx.command("brew", &["upgrade", &formula])?;
            spec.env
                .insert("HOMEBREW_NO_INSTALL_CLEANUP".into(), "1".into());
            spec.env
                .insert("HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK".into(), "1".into());
            spec
        }
        "mise-winget" => winget(ctx, "upgrade", Some(version))?,
        "rustup-self" => {
            let mut spec = CommandSpec::new(path, ["self", "update"]);
            spec.env.insert(
                "RUSTUP_UPDATE_ROOT".into(),
                "https://static.rust-lang.org/rustup".into(),
            );
            spec
        }
        "mise-self" => CommandSpec::new(path, ["self-update", "--yes", "--no-plugins", version]),
        "sdkman-self" => {
            let mut spec=ctx.command("bash",&["--noprofile","--norc","-c","source \"$1\" || exit; sdkman_beta_channel=false; SDKMAN_CANDIDATES_API=https://api.sdkman.io/2; sdkman_auto_answer=true; sdk selfupdate","envark-sdkman",&path.to_string_lossy()])?;
            spec.env.insert(
                "SDKMAN_DIR".into(),
                sdk_home(ctx).to_string_lossy().into_owned(),
            );
            spec.env.insert("BASH_ENV".into(), "/dev/null".into());
            spec
        }
        _ => return Err(Error::Unavailable("Unsupported manager owner.".into())),
    };
    command.timeout = Duration::from_secs(1800);
    command.cwd = Some(ctx.home.clone());
    Ok(command)
}
pub(crate) async fn prepare(
    ctx: &Context,
    request: ActionRequest,
    inventory: &Inventory,
    _settings: &Settings,
) -> Result<Plan> {
    match request {
        ActionRequest::InstallManager { provider, manager } if valid_pair(provider, &manager) => {
            let option = options(ctx, inventory, provider)
                .into_iter()
                .find(|o| o.name == manager)
                .unwrap();
            if option.installed {
                return Err(Error::Conflict(format!(
                    "{manager} is already installed. Refresh inventory or use its update action."
                )));
            }
            if !option.available {
                return Err(Error::Unavailable(option.reason.unwrap_or_default()));
            }
            let target = target(ctx, &manager);
            reject_links(&target)?;
            if std::fs::symlink_metadata(&target).is_ok() {
                return Err(Error::Conflict(
                    "The manager target is already occupied.".into(),
                ));
            }
            let version = if manager == "mise" && cfg!(windows) {
                winget_latest(ctx).await?
            } else {
                official_latest(ctx, &manager).await?
            };
            let payload = if manager == "mise" && cfg!(windows) {
                None
            } else {
                let url = match manager.as_str() {
                    "rustup" if cfg!(windows) => format!(
                        "https://static.rust-lang.org/rustup/archive/{version}/{}/rustup-init.exe",
                        if cfg!(target_arch = "aarch64") {
                            "aarch64-pc-windows-msvc"
                        } else {
                            "x86_64-pc-windows-msvc"
                        }
                    ),
                    "rustup" => "https://sh.rustup.rs".into(),
                    "SDKMAN!" => "https://get.sdkman.io?rcupdate=false".into(),
                    _ => "https://mise.run".into(),
                };
                Some(fetch(ctx, &url).await?)
            };
            let command = if manager == "mise" && cfg!(windows) {
                Some(winget(ctx, "install", Some(&version))?)
            } else {
                None
            };
            Ok(Plan {
                provider,
                operation: Operation::Install {
                    name: manager,
                    version,
                    target,
                    payload,
                },
                expected: None,
                command,
            })
        }
        ActionRequest::UpdateTool { provider, id } => {
            prepare_tool(ctx, inventory, provider, &id, false).await
        }
        ActionRequest::RemoveTool { provider, id } => {
            prepare_tool(ctx, inventory, provider, &id, true).await
        }
        ActionRequest::InstallRuntime {
            provider,
            manager,
            version,
        } if manager == "mise" => {
            provider_name(provider)?;
            runtime_selector(&version)?;
            let tool = manager_tool(ctx, inventory, provider).await?;
            let scratch = tempfile::tempdir()?;
            let output = ctx
                .runner
                .run(
                    &mise_command(
                        ctx,
                        tool.path.as_deref().unwrap(),
                        &[
                            "latest",
                            &format!("{}@{}", provider_name(provider)?, version),
                        ],
                        scratch.path(),
                    ),
                    &ctx.cancel,
                )
                .await?;
            let version = output.stdout.trim().to_owned();
            runtime_selector(&version)?;
            let expected = Some(snapshot(tool.path.as_deref().unwrap())?);
            Ok(Plan {
                provider,
                operation: Operation::Runtime {
                    tool,
                    verb: "install".into(),
                    version,
                    runtime: None,
                },
                expected,
                command: None,
            })
        }
        ActionRequest::SetDefault { provider, id } => {
            prepare_runtime(ctx, inventory, provider, &id, false).await
        }
        ActionRequest::RemoveRuntime { provider, id } => {
            prepare_runtime(ctx, inventory, provider, &id, true).await
        }
        _ => Err(Error::InvalidInput(
            "Unsupported language lifecycle action.".into(),
        )),
    }
}
async fn prepare_tool(
    ctx: &Context,
    inventory: &Inventory,
    provider: ProviderId,
    id: &str,
    remove: bool,
) -> Result<Plan> {
    let tool = find_tool(inventory, provider, id)
        .filter(|t| owns_tool(t))
        .ok_or_else(|| Error::Conflict("Manager no longer exists.".into()))?
        .clone();
    validate_owner(ctx, &tool).await?;
    if installed_version(ctx, &tool.name, tool.path.as_deref().unwrap()).await? != tool.version {
        return Err(Error::Conflict(
            "The manager version changed after review.".into(),
        ));
    }
    let expected = Some(snapshot(tool.path.as_deref().unwrap())?);
    if remove {
        let command = match tool.source.as_str() {
            "homebrew" => {
                let formula = package_managers::brew_formula(ctx, tool.path.as_deref().unwrap())
                    .await
                    .unwrap();
                Some(ctx.command("brew", &["uninstall", &formula])?)
            }
            "mise-winget" => Some(winget(ctx, "uninstall", None)?),
            _ => None,
        };
        let files = if command.is_none() {
            removal_files(&tool)?
        } else {
            vec![]
        };
        Ok(Plan {
            provider,
            operation: Operation::Remove { tool, files },
            expected,
            command,
        })
    } else {
        let version = tool
            .latest
            .clone()
            .ok_or_else(|| Error::Conflict("Check updates before reviewing this update.".into()))?;
        if semver::Version::parse(&version).ok() <= semver::Version::parse(&tool.version).ok() {
            return Err(Error::Conflict(
                "No newer manager version was reviewed.".into(),
            ));
        }
        if latest(ctx, &tool).await? != version {
            return Err(Error::Conflict(
                "The available manager release changed. Check updates again.".into(),
            ));
        }
        let command = Some(update_command(ctx, &tool, &version).await?);
        Ok(Plan {
            provider,
            operation: Operation::Update { tool, version },
            expected,
            command,
        })
    }
}
async fn prepare_runtime(
    ctx: &Context,
    inventory: &Inventory,
    provider: ProviderId,
    id: &str,
    remove: bool,
) -> Result<Plan> {
    let runtime = inventory
        .providers
        .iter()
        .find(|p| p.id == provider)
        .and_then(|p| {
            p.runtimes
                .iter()
                .find(|r| r.id == id && r.manager == "mise")
        })
        .ok_or_else(|| Error::Conflict("mise runtime no longer exists.".into()))?
        .clone();
    if !runtime.managed || (remove && (runtime.active || !runtime.active_known)) {
        return Err(Error::Unavailable(
            "Active, unverified, or linked runtimes cannot be removed.".into(),
        ));
    }
    reject_links(&runtime.path)?;
    let tool = manager_tool(ctx, inventory, provider).await?;
    let expected = Some(snapshot(tool.path.as_deref().unwrap())?);
    let version = runtime
        .selector
        .clone()
        .unwrap_or_else(|| runtime.version.clone());
    runtime_selector(&version)?;
    Ok(Plan {
        provider,
        operation: Operation::Runtime {
            tool,
            verb: if remove { "uninstall" } else { "default" }.into(),
            version,
            runtime: Some(Box::new(runtime)),
        },
        expected,
        command: None,
    })
}
impl Plan {
    pub(crate) fn kind(&self) -> &'static str {
        match &self.operation {
            Operation::Install { .. } => "installManager",
            Operation::Update { .. } => "updateTool",
            Operation::Remove { .. } => "removeManager",
            Operation::Runtime { verb, .. } => match verb.as_str() {
                "install" => "installRuntime",
                "default" => "setDefault",
                _ => "removeRuntime",
            },
        }
    }
    pub(crate) fn providers(&self) -> Vec<ProviderId> {
        let shared = match &self.operation {
            Operation::Install { name, .. } => name == "mise",
            Operation::Update { tool, .. } | Operation::Remove { tool, .. } => tool.name == "mise",
            _ => false,
        };
        if shared {
            vec![ProviderId::Go, ProviderId::Jvm]
        } else {
            vec![self.provider]
        }
    }
    pub(crate) fn warnings(&self) -> Vec<String> {
        match &self.operation {
            Operation::Install{name,..}=>vec![format!("Downloads {name} from its official release source. Shell activation may require restarting or configuring your shell; Envark can use the manager immediately.")],
            Operation::Remove{..}=>vec!["Only manager files are removed. Installed runtimes, Cargo packages, candidates, caches, project files and configuration are kept. Reinstall this manager to manage them again.".into()],
            Operation::Runtime{verb,..} if verb=="default"=>vec!["Changes mise's global default. Existing terminals retain their inherited environment; project configuration is preserved.".into()],
            Operation::Runtime{verb,..} if verb=="uninstall"=>vec!["mise permanently removes the selected runtime, including files installed inside it. Configuration and project files are preserved.".into()],
            _=>vec![],
        }
    }
    pub(crate) fn item(&self) -> PlanItem {
        let (title, path, command, bytes) = match &self.operation {
            Operation::Install {
                name,
                version,
                target,
                ..
            } => (
                format!("Install {name} {version}"),
                Some(target.clone()),
                Some(format!("Official {name} installer (version {version})")),
                0,
            ),
            Operation::Update { tool, version } => (
                format!("Update {} {} → {version}", tool.name, tool.version),
                tool.path.clone(),
                self.command.as_ref().map(CommandSpec::display),
                0,
            ),
            Operation::Remove { tool, files } => (
                format!("Remove {} (keep runtimes)", tool.name),
                tool.path.clone(),
                self.command.as_ref().map(CommandSpec::display),
                files.iter().map(|f| f.length).sum(),
            ),
            Operation::Runtime {
                verb,
                version,
                runtime,
                ..
            } => (
                format!(
                    "{} {} {version} with mise",
                    if verb == "default" {
                        "Set default"
                    } else if verb == "install" {
                        "Install"
                    } else {
                        "Remove"
                    },
                    provider_name(self.provider).unwrap_or("runtime")
                ),
                runtime.as_ref().map(|r| r.path.clone()),
                Some(format!(
                    "mise {} {}@{}",
                    if verb == "default" {
                        "use --global --pin"
                    } else {
                        verb
                    },
                    provider_name(self.provider).unwrap_or("runtime"),
                    version
                )),
                0,
            ),
        };
        PlanItem {
            title,
            path,
            command,
            bytes,
            restore: None,
        }
    }
    pub(crate) async fn execute(self, ctx: &Context, use_trash: bool) -> Result<(u64, String)> {
        if ctx.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if let Some(expected) = &self.expected
            && snapshot(&expected.path)? != *expected
        {
            return Err(Error::Conflict(
                "Manager files changed after review.".into(),
            ));
        }
        match self.operation {
            Operation::Install {
                name,
                version,
                target,
                payload,
            } => {
                if manager_path(ctx, &name).is_some() || std::fs::symlink_metadata(&target).is_ok()
                {
                    return Err(Error::Conflict(
                        "A manager appeared after review. Refresh inventory.".into(),
                    ));
                }
                reject_links(&target)?;
                if let Some(command) = self.command {
                    ctx.runner.run(&command, &ctx.cancel).await?;
                } else {
                    install(ctx, &name, &version, &target, payload.as_deref().unwrap()).await?;
                }
                let path = manager_path(ctx, &name).unwrap_or(target);
                let actual = installed_version(ctx, &name, &path).await?;
                if actual != version {
                    return Err(Error::Conflict(format!(
                        "Installer finished, but {name} reports {actual}; reviewed {version}. Refresh inventory."
                    )));
                }
                Ok((
                    0,
                    format!(
                        "Installed {name} {actual}. Existing runtimes and configuration were kept."
                    ),
                ))
            }
            Operation::Update { tool, version } => {
                validate_owner(ctx, &tool).await?;
                // Native unpinned updaters must still target the reviewed official release.
                if latest(ctx, &tool).await? != version {
                    return Err(Error::Conflict(
                        "The available release changed after review. Check updates again.".into(),
                    ));
                }
                let actual =
                    installed_version(ctx, &tool.name, tool.path.as_deref().unwrap()).await?;
                if actual != tool.version {
                    return Err(Error::Conflict(
                        "The manager version changed after review.".into(),
                    ));
                }
                let mut command = self.command.unwrap();
                let scratch = tempfile::tempdir()?;
                if tool.name == "mise" && tool.source == "mise-self" {
                    command = mise_command(
                        ctx,
                        tool.path.as_deref().unwrap(),
                        &["self-update", "--yes", "--no-plugins", &version],
                        scratch.path(),
                    );
                }
                ctx.runner.run(&command, &ctx.cancel).await?;
                let actual =
                    installed_version(ctx, &tool.name, tool.path.as_deref().unwrap()).await?;
                if actual != version {
                    return Err(Error::Conflict(format!(
                        "{} is at version {actual}, but the reviewed target was {version}. Refresh inventory.",
                        tool.name
                    )));
                }
                Ok((
                    0,
                    format!("{} updated from {} to {actual}.", tool.name, tool.version),
                ))
            }
            Operation::Remove { tool, files } => {
                validate_owner(ctx, &tool).await?;
                if let Some(mut command) = self.command {
                    command.timeout = Duration::from_secs(1800);
                    ctx.runner.run(&command, &ctx.cancel).await?;
                    if tool.path.as_ref().is_some_and(|p| p.exists()) {
                        return Err(Error::Conflict(
                            "Native removal finished, but the manager is still present.".into(),
                        ));
                    }
                } else {
                    if removal_files(&tool)? != files {
                        return Err(Error::Conflict(
                            "Manager removal files changed after review.".into(),
                        ));
                    }
                    let cancel = ctx.cancel.clone();
                    let removing = files.clone();
                    tokio::task::spawn_blocking(move || -> Result<()> {
                        for entry in &removing {
                            reject_links(&entry.path)?;
                            if snapshot(&entry.path)? != *entry {
                                return Err(Error::Conflict(
                                    "Manager files changed before removal.".into(),
                                ));
                            }
                        }
                        if cancel.is_cancelled() {
                            return Err(Error::Cancelled);
                        }
                        if use_trash {
                            trash::delete_all(removing.iter().map(|e| &e.path))
                                .map_err(|e| Error::Unavailable(e.to_string()))?;
                        } else {
                            for entry in removing {
                                std::fs::remove_file(entry.path)?;
                            }
                        }
                        Ok(())
                    })
                    .await
                    .map_err(|e| Error::Unavailable(e.to_string()))??;
                }
                Ok((
                    files.iter().map(|f| f.length).sum(),
                    format!(
                        "{} removed. Runtimes, installed packages and configuration were kept.",
                        tool.name
                    ),
                ))
            }
            Operation::Runtime {
                tool,
                verb,
                version,
                runtime,
            } => {
                validate_owner(ctx, &tool).await?;
                let path = tool.path.as_deref().unwrap();
                let before = mise_runtimes(ctx, path, self.provider).await?;
                if let Some(reviewed) = &runtime {
                    reject_links(&reviewed.path)?;
                    let current = before.iter().find(|r| r.id == reviewed.id).ok_or_else(|| {
                        Error::Conflict("Runtime disappeared after review.".into())
                    })?;
                    if !current.managed
                        || current.path != reviewed.path
                        || current.version != reviewed.version
                        || (verb == "uninstall" && (current.active || !current.active_known))
                    {
                        return Err(Error::Conflict(
                            "Runtime ownership or default changed after review.".into(),
                        ));
                    }
                }
                let scratch = tempfile::tempdir()?;
                let name = provider_name(self.provider)?;
                let resolved = version;
                let selector = format!("{name}@{resolved}");
                let args = if verb == "default" {
                    vec!["use", "--global", "--pin", &selector]
                } else {
                    vec![verb.as_str(), &selector]
                };
                ctx.runner
                    .run(&mise_command(ctx, path, &args, scratch.path()), &ctx.cancel)
                    .await?;
                let after = mise_runtimes(ctx, path, self.provider).await?;
                let found = after.iter().find(|r| r.version == resolved && r.managed);
                let confirmed = match verb.as_str() {
                    "install" => found.is_some(),
                    "default" => found.is_some_and(|r| r.active && r.active_known),
                    _ => found.is_none() && runtime.as_ref().is_none_or(|r| !r.path.exists()),
                };
                if !confirmed {
                    return Err(Error::Conflict(
                        "mise completed without the reviewed runtime state. Refresh inventory."
                            .into(),
                    ));
                }
                Ok((0, format!("mise {verb} completed for {name} {resolved}.")))
            }
        }
    }
}
async fn install(
    ctx: &Context,
    name: &str,
    version: &str,
    target: &Path,
    payload: &[u8],
) -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let installer = scratch.path().join(if cfg!(windows) && name == "rustup" {
        "rustup-init.exe"
    } else {
        "install.sh"
    });
    std::fs::write(&installer, payload)?;
    let staged = scratch.path().join("sdkman");
    let mut command = if cfg!(windows) && name == "rustup" {
        CommandSpec::new(
            &installer,
            ["-y", "--no-modify-path", "--default-toolchain", "none"],
        )
    } else {
        let mut command = ctx.command("bash", &[&installer.to_string_lossy()])?;
        if name == "rustup" {
            command.args.extend(
                ["-y", "--no-modify-path", "--default-toolchain", "none"].map(str::to_owned),
            );
        }
        command.env.insert("BASH_ENV".into(), "/dev/null".into());
        command
    };
    command.cwd = Some(scratch.path().into());
    command.timeout = Duration::from_secs(1800);
    match name {
        "rustup" => {
            reject_links(&cargo_home(ctx))?;
            let rustup_home = configured("RUSTUP_HOME", ctx.home.join(".rustup"));
            reject_links(&rustup_home)?;
            command.env.insert(
                "CARGO_HOME".into(),
                cargo_home(ctx).to_string_lossy().into_owned(),
            );
            command.env.insert(
                "RUSTUP_HOME".into(),
                rustup_home.to_string_lossy().into_owned(),
            );
            command.env.insert("RUSTUP_VERSION".into(), version.into());
            command.env.insert(
                "RUSTUP_UPDATE_ROOT".into(),
                "https://static.rust-lang.org/rustup".into(),
            );
            command.env.insert(
                "RUSTUP_DIST_SERVER".into(),
                "https://static.rust-lang.org".into(),
            );
        }
        "SDKMAN!" => {
            command
                .env
                .insert("SDKMAN_DIR".into(), staged.to_string_lossy().into_owned());
            // The official rcupdate=false installer leaves shell profiles unchanged.
        }
        _ => {
            command
                .env
                .insert("MISE_VERSION".into(), format!("v{version}"));
            command.env.insert(
                "MISE_INSTALL_PATH".into(),
                target.to_string_lossy().into_owned(),
            );
            command.env.insert("MISE_INSTALL_HELP".into(), "0".into());
        }
    }
    ctx.runner.run(&command, &ctx.cancel).await?;
    if name == "SDKMAN!" {
        let actual = normalized(&read_small(&staged.join("var/version"), 128)?)?;
        if actual != version {
            return Err(Error::Conflict(
                "SDKMAN! release changed while its installer ran. Review installation again."
                    .into(),
            ));
        }
        deploy_sdkman(ctx, &staged, &sdk_home(ctx))?;
    }
    Ok(())
}
fn deploy_sdkman(ctx: &Context, staged: &Path, destination: &Path) -> Result<()> {
    reject_links(destination)?;
    for entry in walkdir::WalkDir::new(staged).follow_links(false) {
        if ctx.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let entry = entry.map_err(|e| Error::Unavailable(e.to_string()))?;
        reject_links(entry.path())?;
        let relative = entry
            .path()
            .strip_prefix(staged)
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let target = destination.join(relative);
        reject_links(&target)?;
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        let component = relative
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();
        let manager_file = matches!(component.as_str(), "bin" | "src" | "libexec")
            || [
                Path::new("var/version"),
                Path::new("var/version_native"),
                Path::new("var/candidates"),
            ]
            .contains(&relative);
        // Preserve existing candidates, archives, user config and state on reinstall.
        if manager_file || !target.exists() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn script(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn inventory(provider: ProviderId, tool: Tool, runtimes: Vec<Runtime>) -> Inventory {
        let mut p = Provider::empty(provider);
        p.package_managers.push(tool);
        p.runtimes = runtimes;
        Inventory {
            providers: vec![p],
            ..Default::default()
        }
    }
    #[test]
    fn language_lifecycle_runs_in_isolated_home() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "providers::language_lifecycle::tests::isolated_lifecycle_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("ENVARK_LANGUAGE_FIXTURE", &root)
            .env("PATH", root.join("bin"))
            .env("CARGO_HOME", root.join("cargo"))
            .env("RUSTUP_HOME", root.join("rustup"))
            .env("SDKMAN_DIR", root.join("sdkman"))
            .env("MISE_DATA_DIR", root.join("mise"))
            .env_remove("MISE_CONFIG_DIR")
            .env_remove("MISE_GLOBAL_CONFIG_FILE")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[tokio::test]
    #[ignore = "Run by language_lifecycle_runs_in_isolated_home in a subprocess"]
    async fn isolated_lifecycle_fixture() {
        let root = PathBuf::from(std::env::var_os("ENVARK_LANGUAGE_FIXTURE").unwrap());
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = root.clone();
        ctx.data = root.join("data");
        ctx.cache = root.join("cache");
        let settings = Settings::default();
        // Removing rustup must preserve toolchains, cargo-installed packages and config.
        let rustup = target(&ctx, "rustup");
        script(&rustup, "#!/bin/sh\nprintf 'rustup 1.28.2 (fixture)\\n'\n");
        let cargo = rustup.parent().unwrap().join("cargo");
        std::fs::hard_link(&rustup, &cargo).unwrap();
        let package = rustup.parent().unwrap().join("ripgrep");
        script(&package, "#!/bin/sh\nprintf 'cargo-installed package\\n'\n");
        let metadata = cargo_home(&ctx).join(".crates2.json");
        std::fs::write(&metadata, "keep package metadata").unwrap();
        let rust_config = root.join("rustup/settings.toml");
        std::fs::create_dir_all(root.join("rustup/toolchains/stable/bin")).unwrap();
        std::fs::write(&rust_config, "default_toolchain = 'stable'").unwrap();
        let mut tool = basic_tool(
            "rustup",
            "1.28.2".into(),
            "rustup-self",
            Some(rustup.clone()),
        );
        tool.can_remove = true;
        let inv = inventory(ProviderId::Rust, tool.clone(), vec![]);
        let request = ActionRequest::RemoveTool {
            provider: ProviderId::Rust,
            id: tool.id.clone(),
        };
        let stale = prepare(&ctx, request.clone(), &inv, &settings)
            .await
            .unwrap();
        script(&rustup, "#!/bin/sh\nprintf 'rustup 1.28.3 (fixture)\\n'\n");
        assert!(
            stale
                .execute(&ctx, false)
                .await
                .unwrap_err()
                .to_string()
                .contains("changed after review")
        );
        assert!(package.exists());
        assert!(rustup.exists());
        script(&rustup, "#!/bin/sh\nprintf 'rustup 1.28.2 (fixture)\\n'\n");
        let plan = prepare(&ctx, request, &inv, &settings).await.unwrap();
        assert_eq!(plan.kind(), "removeManager");
        plan.execute(&ctx, false).await.unwrap();
        assert!(!rustup.exists());
        assert!(!cargo.exists());
        assert!(package.exists());
        assert!(metadata.exists());
        assert!(rust_config.exists());
        assert!(root.join("rustup/toolchains/stable/bin").exists());
        // Reinstalling SDKMAN over retained candidates preserves user config byte-for-byte.
        let sdk = sdk_home(&ctx);
        std::fs::create_dir_all(sdk.join("candidates/java/21/bin")).unwrap();
        std::fs::create_dir_all(sdk.join("etc")).unwrap();
        std::fs::write(sdk.join("etc/config"), "sdkman_auto_answer=false\n").unwrap();
        let stage = root.join("stage");
        script(
            &stage.join("bin/sdkman-init.sh"),
            "# official fixture init\n",
        );
        script(&stage.join("src/sdkman-main.sh"), "# manager\n");
        script(&stage.join("libexec/help"), "# native\n");
        std::fs::create_dir_all(stage.join("etc")).unwrap();
        std::fs::create_dir_all(stage.join("var")).unwrap();
        std::fs::write(stage.join("etc/config"), "replacement must not overwrite").unwrap();
        std::fs::write(stage.join("var/version"), "5.20.0").unwrap();
        deploy_sdkman(&ctx, &stage, &sdk).unwrap();
        assert_eq!(
            std::fs::read_to_string(sdk.join("etc/config")).unwrap(),
            "sdkman_auto_answer=false\n"
        );
        let tool = basic_tool(
            "SDKMAN!",
            "5.20.0".into(),
            "sdkman-self",
            Some(sdk.join("bin/sdkman-init.sh")),
        );
        let inv = inventory(ProviderId::Jvm, tool.clone(), vec![]);
        let plan = prepare(
            &ctx,
            ActionRequest::RemoveTool {
                provider: ProviderId::Jvm,
                id: tool.id.clone(),
            },
            &inv,
            &settings,
        )
        .await
        .unwrap();
        plan.execute(&ctx, false).await.unwrap();
        assert!(!sdk.join("bin/sdkman-init.sh").exists());
        assert!(!sdk.join("src/sdkman-main.sh").exists());
        assert!(sdk.join("candidates/java/21/bin").exists());
        assert!(sdk.join("etc/config").exists());
        deploy_sdkman(&ctx, &stage, &sdk).unwrap();
        assert!(sdk.join("bin/sdkman-init.sh").exists());
        // A symlink at a standard location must not be adopted as standalone.
        let mise = target(&ctx, "mise");
        std::fs::create_dir_all(mise.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&package, &mise).unwrap();
        assert!(owner(&ctx, "mise", &mise).await.is_none());
        std::fs::remove_file(&mise).unwrap();
        script(
            &mise,
            r#"#!/bin/sh
case "$1" in
 --version) printf 'mise 2026.10.1\n';;
 latest) printf '1.25.0\n';;
 ls)
  if [ -d "$MISE_DATA_DIR/installs/go/1.25.0" ]; then
   active=false; [ -f "$MISE_DATA_DIR/active" ] && active=true
   printf '[{"version":"1.25.0","install_path":"%s/installs/go/1.25.0","active":%s}]\n' "$MISE_DATA_DIR" "$active"
  else printf '[]\n'; fi;;
 install) /bin/mkdir -p "$MISE_DATA_DIR/installs/go/1.25.0";;
 use) [ "$2" = --global ] || exit 9; printf 'selected\n' > "$MISE_DATA_DIR/active";;
 uninstall) /bin/rmdir "$MISE_DATA_DIR/installs/go/1.25.0";;
 *) exit 8;;
esac
"#,
        );
        let tool = basic_tool("mise", "2026.10.1".into(), "mise-self", Some(mise.clone()));
        let mut inv = inventory(ProviderId::Go, tool.clone(), vec![]);
        let plan = prepare(
            &ctx,
            ActionRequest::InstallRuntime {
                provider: ProviderId::Go,
                manager: "mise".into(),
                version: "latest".into(),
            },
            &inv,
            &settings,
        )
        .await
        .unwrap();
        assert!(plan.item().title.contains("1.25.0"));
        plan.execute(&ctx, false).await.unwrap();
        let runtimes = mise_runtimes(&ctx, &mise, ProviderId::Go).await.unwrap();
        assert_eq!(runtimes.len(), 1);
        assert!(runtimes[0].managed);
        let id = runtimes[0].id.clone();
        inv.providers[0].runtimes = runtimes;
        prepare(
            &ctx,
            ActionRequest::SetDefault {
                provider: ProviderId::Go,
                id: id.clone(),
            },
            &inv,
            &settings,
        )
        .await
        .unwrap()
        .execute(&ctx, false)
        .await
        .unwrap();
        // A newly active runtime must invalidate an already reviewed removal.
        let error = prepare(
            &ctx,
            ActionRequest::RemoveRuntime {
                provider: ProviderId::Go,
                id: id.clone(),
            },
            &inv,
            &settings,
        )
        .await
        .unwrap()
        .execute(&ctx, false)
        .await
        .unwrap_err();
        assert!(error.to_string().contains("default changed"));
        std::fs::remove_file(mise_data(&ctx).join("active")).unwrap();
        prepare(
            &ctx,
            ActionRequest::RemoveRuntime {
                provider: ProviderId::Go,
                id,
            },
            &inv,
            &settings,
        )
        .await
        .unwrap()
        .execute(&ctx, false)
        .await
        .unwrap();
        assert!(!mise_data(&ctx).join("installs/go/1.25.0").exists());
        let command = mise_command(
            &ctx,
            &mise,
            &["install", "go@1.25.0"],
            &root.join("isolated"),
        );
        assert_eq!(command.cwd, Some(root.join("isolated")));
        assert_eq!(
            command.env["MISE_CEILING_PATHS"],
            root.join("isolated").to_string_lossy()
        );
        assert!(!command.args.iter().any(|a| a == "--force"));
        // Homebrew ownership routes updates to Homebrew, and a no-op updater cannot succeed.
        let keg = root.join("Cellar/mise/2026.10.1");
        let brewed = keg.join("bin/mise");
        script(
            &brewed,
            "#!/bin/sh\nread -r version < \"${0%/*}/../version\"; printf '%s\\n' \"$version\"\n",
        );
        std::fs::write(keg.join("version"), "2026.10.1\n").unwrap();
        std::fs::write(
            keg.join("INSTALL_RECEIPT.json"),
            r#"{"source":{"tap":"homebrew/core"}}"#,
        )
        .unwrap();
        script(
            &root.join("bin/brew"),
            r#"#!/bin/sh
case "$1" in
 --cellar) printf '%s/Cellar\n' "$ENVARK_LANGUAGE_FIXTURE";;
 info) printf '{"formulae":[{"versions":{"stable":"2026.10.2"},"pinned":false}]}\n';;
 upgrade) [ -f "$ENVARK_LANGUAGE_FIXTURE/brew-noop" ] || printf '2026.10.2\n' > "$ENVARK_LANGUAGE_FIXTURE/Cellar/mise/2026.10.1/version";;
 *) exit 9;;
esac
"#,
        );
        assert_eq!(
            owner(&ctx, "mise", &brewed).await.as_deref(),
            Some("homebrew")
        );
        let mut brewed_tool = basic_tool("mise", "2026.10.1".into(), "homebrew", Some(brewed));
        brewed_tool.latest = Some("2026.10.2".into());
        let brewed_inv = inventory(ProviderId::Go, brewed_tool.clone(), vec![]);
        let request = ActionRequest::UpdateTool {
            provider: ProviderId::Go,
            id: brewed_tool.id.clone(),
        };
        let plan = prepare(&ctx, request.clone(), &brewed_inv, &settings)
            .await
            .unwrap();
        assert_eq!(
            plan.command.as_ref().unwrap().args,
            ["upgrade", "homebrew/core/mise"]
        );
        assert_eq!(plan.providers(), [ProviderId::Go, ProviderId::Jvm]);
        plan.execute(&ctx, false).await.unwrap();
        std::fs::write(keg.join("version"), "2026.10.1\n").unwrap();
        std::fs::write(root.join("brew-noop"), "").unwrap();
        let plan = prepare(&ctx, request, &brewed_inv, &settings)
            .await
            .unwrap();
        assert!(
            plan.execute(&ctx, false)
                .await
                .unwrap_err()
                .to_string()
                .contains("reviewed target")
        );
        // A zero exit status without the SDK state is not a successful install.
        script(
            &mise,
            "#!/bin/sh\ncase \"$1\" in\n--version) printf '2026.10.1\\n';; latest) printf '1.25.0\\n';; ls) printf '[]\\n';; esac\n",
        );
        let plan = prepare(
            &ctx,
            ActionRequest::InstallRuntime {
                provider: ProviderId::Go,
                manager: "mise".into(),
                version: "1.25.0".into(),
            },
            &inv,
            &settings,
        )
        .await
        .unwrap();
        assert!(
            plan.execute(&ctx, false)
                .await
                .unwrap_err()
                .to_string()
                .contains("without the reviewed runtime state")
        );
    }
    #[test]
    fn selectors_and_versions_are_strict() {
        for version in ["1.25.0", "temurin-21.0.7+6", "latest"] {
            assert!(runtime_selector(version).is_ok());
        }
        for version in [
            "--force",
            "../system",
            "java@latest",
            "1; rm -rf /",
            "$(touch x)",
            "",
        ] {
            assert!(runtime_selector(version).is_err());
        }
        assert_eq!(normalized("5.20.0-stable").unwrap(), "5.20.0");
        assert_eq!(normalized("v2026.10.1").unwrap(), "2026.10.1");
    }
}
