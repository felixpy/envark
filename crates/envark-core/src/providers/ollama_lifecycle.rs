//! Ollama program ownership is separate from model storage and service ownership.
//! Official archives are installed in a dedicated user directory without sudo.
use super::*;
use crate::{
    filesystem::{measure, reject_links},
    model::{Inventory, Settings},
    operations::{ActionRequest, PlanItem},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

#[path = "ollama_service.rs"]
mod service;

const SOURCE: &str = "ollama-official";
const BREW: &str = "ollama-homebrew";
const RELEASE: &str = "https://api.github.com/repos/ollama/ollama/releases/latest";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Receipt {
    root: PathBuf,
    binary: PathBuf,
    version: String,
}

#[derive(Debug, Clone)]
struct Artifact {
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct Plan {
    action: String,
    tool: Tool,
    root: PathBuf,
    fingerprint: Option<String>,
    artifact: Option<Artifact>,
    command: Option<CommandSpec>,
    version: String,
    service_snapshot: Option<String>,
}

fn receipt_path(ctx: &Context) -> PathBuf {
    ctx.data.join("envark/ollama/installation.json")
}

fn install_root(ctx: &Context) -> PathBuf {
    if cfg!(target_os = "macos") {
        ctx.home.join("Applications/Ollama.app")
    } else {
        ctx.data.join("envark/ollama/program")
    }
}

fn binary(root: &Path) -> PathBuf {
    root.join(if cfg!(target_os = "macos") {
        "Contents/Resources/ollama"
    } else if cfg!(windows) {
        "ollama.exe"
    } else {
        "bin/ollama"
    })
}

fn receipt(ctx: &Context) -> Option<Receipt> {
    let path = receipt_path(ctx);
    reject_links(&path).ok()?;
    let record: Receipt = serde_json::from_str(&read_small(&path, 16384).ok()?).ok()?;
    (record.root == install_root(ctx)
        && record.binary == binary(&record.root)
        && reject_links(&record.root).is_ok()
        && reject_links(&record.binary).is_ok()
        && record.binary.is_file())
    .then_some(record)
}

pub(crate) fn owns_tool(tool: &Tool) -> bool {
    tool.name == "ollama" && [SOURCE, BREW].contains(&tool.source.as_str())
}

fn find<'a>(inventory: &'a Inventory, id: &str) -> Option<&'a Tool> {
    inventory
        .providers
        .iter()
        .filter(|p| p.id == ProviderId::Ollama)
        .flat_map(|p| p.tools.iter().chain(&p.package_managers))
        .find(|t| t.id == id && owns_tool(t))
}

pub(crate) fn handles(request: &ActionRequest, inventory: &Inventory) -> bool {
    match request {
        ActionRequest::InstallManager {
            provider: ProviderId::Ollama,
            manager,
        } => manager == "ollama",
        ActionRequest::UpdateTool {
            provider: ProviderId::Ollama,
            id,
        }
        | ActionRequest::RemoveTool {
            provider: ProviderId::Ollama,
            id,
        } => find(inventory, id).is_some(),
        ActionRequest::ServiceAction {
            provider: ProviderId::Ollama,
            ..
        } => true,
        _ => false,
    }
}

fn prerequisite(ctx: &Context) -> Option<String> {
    if !["x86_64", "aarch64"].contains(&std::env::consts::ARCH) {
        return Some("Official Ollama archives require x86_64 or ARM64.".into());
    }
    if cfg!(target_os = "macos") {
        if !Path::new("/usr/bin/ditto").is_file() || !Path::new("/usr/bin/codesign").is_file() {
            return Some("The official macOS application requires ditto and codesign.".into());
        }
    } else if cfg!(windows) {
        if ctx
            .executable("powershell")
            .or_else(|| ctx.executable("pwsh"))
            .is_none()
        {
            return Some(
                "PowerShell is required to extract the official standalone archive.".into(),
            );
        }
    } else if ctx.executable("tar").is_none() || ctx.executable("zstd").is_none() {
        return Some("Install tar and zstd to extract Ollama's official Linux archive. No administrator permission is needed for the Ollama installation itself.".into());
    }
    None
}

pub(crate) fn options(
    ctx: &Context,
    inventory: &Inventory,
    provider: ProviderId,
) -> Vec<manager_install::OptionView> {
    if provider != ProviderId::Ollama {
        return vec![];
    }
    let installed = receipt(ctx).is_some()
        || ctx.executable("ollama").is_some()
        || inventory
            .providers
            .iter()
            .filter(|p| p.id == provider)
            .any(|p| !p.runtimes.is_empty() || p.tools.iter().any(|t| t.name == "ollama"));
    let reason = prerequisite(ctx);
    vec![manager_install::OptionView {
        name: "ollama".into(),
        source: "Official Ollama user installation".into(),
        installed,
        available: reason.is_none(),
        reason,
    }]
}

async fn json(ctx: &Context, url: &str) -> Result<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("Envark/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    let request = async {
        client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Unavailable(e.to_string()))?
            .json()
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))
    };
    tokio::select! { value = request => value, _ = ctx.cancel.cancelled() => Err(Error::Cancelled) }
}

fn release_version(value: &serde_json::Value) -> Result<String> {
    let version = value["tag_name"]
        .as_str()
        .unwrap_or_default()
        .trim_start_matches('v');
    semver::Version::parse(version)
        .map_err(|_| Error::Unavailable("Ollama did not report a valid release version.".into()))?;
    Ok(version.into())
}

fn asset_name() -> String {
    let arch = if std::env::consts::ARCH == "aarch64" {
        "arm64"
    } else {
        "amd64"
    };
    if cfg!(target_os = "macos") {
        "Ollama-darwin.zip".into()
    } else if cfg!(windows) {
        format!("ollama-windows-{arch}.zip")
    } else {
        format!("ollama-linux-{arch}.tar.zst")
    }
}

fn artifact(value: &serde_json::Value, name: &str) -> Result<Artifact> {
    let asset = value["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|a| a["name"] == name))
        .ok_or_else(|| {
            Error::Unavailable(format!("The official release has no {name} archive."))
        })?;
    let version = release_version(value)?;
    let url = format!("https://github.com/ollama/ollama/releases/download/v{version}/{name}");
    let digest = asset["digest"]
        .as_str()
        .unwrap_or_default()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let size = asset["size"].as_u64().unwrap_or(0);
    if asset["browser_download_url"] != url
        || digest.len() != 64
        || !digest.bytes().all(|c| c.is_ascii_hexdigit())
        || size == 0
        || size > 12 * 1024 * 1024 * 1024
    {
        return Err(Error::Unavailable("The official archive lacks a valid SHA-256 digest, size, or download URL. Installation was not started.".into()));
    }
    Ok(Artifact {
        url,
        sha256: digest.to_ascii_lowercase(),
        size,
    })
}

pub(crate) async fn latest(ctx: &Context, tool: &Tool) -> Result<String> {
    if tool.source == BREW {
        let text = ctx
            .read("brew", &["info", "--json=v2", "--formula", "ollama"])
            .await?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let version = value["formulae"][0]["versions"]["stable"]
            .as_str()
            .unwrap_or_default();
        semver::Version::parse(version)
            .map_err(|_| Error::Unavailable("Homebrew did not report an Ollama version.".into()))?;
        Ok(version.into())
    } else if owns_tool(tool) {
        release_version(&json(ctx, RELEASE).await?)
    } else {
        Err(Error::Unavailable(
            "Ollama program ownership is unverified.".into(),
        ))
    }
}

async fn client_version(ctx: &Context, path: &Path) -> Result<String> {
    let mut command = CommandSpec::new(path, ["--version"]);
    // --version otherwise prefers the running server, which might be an unrelated installation.
    command
        .env
        .insert("OLLAMA_HOST".into(), "http://127.0.0.1:0".into());
    command.env.insert("NO_PROXY".into(), "*".into());
    let output = ctx.runner.run(&command, &ctx.cancel).await?;
    let text = format!("{}\n{}", output.stdout, output.stderr);
    text.lines()
        .rev()
        .find_map(package_managers::version)
        .ok_or_else(|| {
            Error::Unavailable("The Ollama client did not report its installed version.".into())
        })
}

async fn verify_update(ctx: &Context, tool: &Tool) -> Result<()> {
    super::updates::require_upgrade(tool)?;
    let path = tool
        .path
        .as_ref()
        .ok_or_else(|| Error::Conflict("Ollama executable disappeared.".into()))?;
    if client_version(ctx, path).await? != tool.version {
        return Err(Error::Conflict(
            "Ollama's installed version changed. Check updates again before reviewing this action."
                .into(),
        ));
    }
    Ok(())
}

pub(crate) async fn discover(ctx: &Context, provider: &mut Provider) {
    if provider.id != ProviderId::Ollama {
        return;
    }
    let mut found = None;
    if let Some(record) = receipt(ctx) {
        found = Some((record.binary, SOURCE));
    } else if let Some(path) = ctx.executable("ollama") {
        let source = if package_managers::brew_formula(ctx, &path).await.as_deref()
            == Some("homebrew/core/ollama")
        {
            BREW
        } else {
            "PATH"
        };
        found = Some((path, source));
    }
    if let Some((path, source)) = found {
        let version = client_version(ctx, &path)
            .await
            .unwrap_or_else(|_| "unknown".into());
        let mut tool = basic_tool("ollama", version, source, Some(path));
        tool.can_update = owns_tool(&tool);
        tool.can_remove = owns_tool(&tool);
        tool.note = Some(if owns_tool(&tool) {
            "Program actions preserve models and configuration. Service start and stop are separate actions."
        } else {
            "This Ollama program has no verified Envark receipt or Homebrew formula receipt. Its installation cannot be changed safely in this view."
        }.into());
        provider.tools.retain(|t| t.name != "ollama");
        provider.tools.push(tool);
        provider.runtimes.clear();
    }
    let owned = service::owned(ctx).is_some();
    if let Some(status) = &mut provider.service {
        status.owned = owned;
    }
}

async fn validate_owner(ctx: &Context, tool: &Tool) -> Result<PathBuf> {
    let path = tool
        .path
        .as_ref()
        .ok_or_else(|| Error::Conflict("Ollama program path is missing.".into()))?;
    if tool.source == SOURCE {
        let record = receipt(ctx).ok_or_else(|| {
            Error::Conflict(
                "Ollama installation receipt changed. Refresh before continuing.".into(),
            )
        })?;
        if record.binary != *path {
            return Err(Error::Conflict("Ollama installation path changed.".into()));
        }
        Ok(record.root)
    } else if tool.source == BREW
        && package_managers::brew_formula(ctx, path).await.as_deref()
            == Some("homebrew/core/ollama")
    {
        let real = path.canonicalize()?;
        real.ancestors()
            .find(|p| p.join("INSTALL_RECEIPT.json").is_file())
            .map(Path::to_path_buf)
            .ok_or_else(|| Error::Conflict("Homebrew receipt changed.".into()))
    } else {
        Err(Error::Conflict("Ollama installation owner changed.".into()))
    }
}

fn fingerprint(ctx: &Context, root: &Path) -> Result<String> {
    let measured = measure(root, &ctx.cancel)?;
    if !measured.complete {
        return Err(Error::Unavailable(
            "The complete Ollama program directory could not be inspected.".into(),
        ));
    }
    measured
        .fingerprint
        .ok_or_else(|| Error::Unavailable("Missing Ollama directory fingerprint.".into()))
}

fn preserve_data(ctx: &Context, root: &Path) -> Result<()> {
    // Never remove a custom models directory placed inside the program tree.
    let mut data = vec![ctx.home.join(".ollama")];
    if let Some(path) = std::env::var_os("OLLAMA_MODELS") {
        data.push(path.into());
    }
    for path in data {
        let path = path.canonicalize().unwrap_or(path);
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if path.starts_with(&root) || root.starts_with(&path) {
            return Err(Error::Conflict("Ollama models or configuration overlap the program directory. Move the data outside it before changing the program.".into()));
        }
    }
    Ok(())
}

pub(crate) async fn prepare(
    ctx: &Context,
    request: ActionRequest,
    inventory: &Inventory,
    _settings: &Settings,
) -> Result<Plan> {
    let (action, tool) = match request {
        ActionRequest::InstallManager {
            provider: ProviderId::Ollama,
            manager,
        } if manager == "ollama" => {
            let option = options(ctx, inventory, ProviderId::Ollama).remove(0);
            if option.installed {
                return Err(Error::Conflict(
                    "Ollama is already installed. Refresh and use its program update action."
                        .into(),
                ));
            }
            if let Some(reason) = option.reason {
                return Err(Error::Unavailable(reason));
            }
            (
                "install",
                basic_tool(
                    "ollama",
                    String::new(),
                    SOURCE,
                    Some(binary(&install_root(ctx))),
                ),
            )
        }
        ActionRequest::UpdateTool {
            provider: ProviderId::Ollama,
            id,
        } => (
            "update",
            find(inventory, &id)
                .cloned()
                .ok_or_else(|| Error::Conflict("Ollama program changed.".into()))?,
        ),
        ActionRequest::RemoveTool {
            provider: ProviderId::Ollama,
            id,
        } => (
            "remove",
            find(inventory, &id)
                .cloned()
                .ok_or_else(|| Error::Conflict("Ollama program changed.".into()))?,
        ),
        ActionRequest::ServiceAction {
            provider: ProviderId::Ollama,
            action,
        } if ["start", "stop"].contains(&action.as_str()) => {
            if action == "stop" {
                let (path, snapshot) = service::review(ctx)?;
                return Ok(Plan {
                    action,
                    root: path.clone(),
                    tool: basic_tool("ollama", String::new(), SOURCE, Some(path)),
                    version: String::new(),
                    fingerprint: None,
                    artifact: None,
                    command: None,
                    service_snapshot: Some(snapshot),
                });
            }
            let tool = inventory.providers.iter().filter(|p| p.id == ProviderId::Ollama).flat_map(|p| &p.tools).find(|t| owns_tool(t)).cloned()
                .ok_or_else(|| Error::Unavailable("Install Ollama in Envark or refresh a verified Homebrew installation before managing its service.".into()))?;
            let root = validate_owner(ctx, &tool).await?;
            let path = tool
                .path
                .as_ref()
                .ok_or_else(|| Error::Conflict("Missing Ollama executable.".into()))?
                .canonicalize()?;
            return Ok(Plan {
                service_snapshot: None,
                action,
                fingerprint: Some(fingerprint(ctx, &path)?),
                version: tool.version.clone(),
                tool,
                root,
                artifact: None,
                command: None,
            });
        }
        _ => {
            return Err(Error::InvalidInput(
                "Unsupported Ollama program action.".into(),
            ));
        }
    };
    if action == "update" {
        verify_update(ctx, &tool).await?;
    }
    let root = if action == "install" {
        install_root(ctx)
    } else {
        validate_owner(ctx, &tool).await?
    };
    reject_links(&root)?;
    preserve_data(ctx, &root)?;
    let fingerprint = if action == "install" {
        if root.exists() {
            return Err(Error::Conflict(
                "The Ollama install directory is occupied and has no verified receipt.".into(),
            ));
        }
        None
    } else {
        Some(fingerprint(ctx, &root)?)
    };
    if action != "install" {
        service::require_stopped(ctx, &root).await?;
    }
    let (version, artifact) = if action == "remove" {
        (tool.version.clone(), None)
    } else if tool.source == BREW {
        (latest(ctx, &tool).await?, None)
    } else {
        let release = json(ctx, RELEASE).await?;
        (
            release_version(&release)?,
            Some(artifact(&release, &asset_name())?),
        )
    };
    if action == "update" && tool.latest.as_deref() != Some(&version) {
        return Err(Error::Conflict(
            "The available Ollama version changed. Check updates before reviewing again.".into(),
        ));
    }
    let command = if tool.source == BREW {
        let mut spec = ctx.command(
            "brew",
            &[
                if action == "remove" {
                    "uninstall"
                } else {
                    "upgrade"
                },
                "--formula",
                "homebrew/core/ollama",
            ],
        )?;
        spec.env
            .insert("HOMEBREW_NO_AUTO_UPDATE".into(), "1".into());
        spec.env
            .insert("HOMEBREW_NO_INSTALL_CLEANUP".into(), "1".into());
        spec.timeout = Duration::from_secs(3600);
        Some(spec)
    } else {
        None
    };
    Ok(Plan {
        action: action.into(),
        service_snapshot: None,
        tool,
        root,
        fingerprint,
        artifact,
        command,
        version,
    })
}

impl Plan {
    pub(crate) fn kind(&self) -> &'static str {
        match self.action.as_str() {
            "install" => "installManager",
            "remove" => "removeManager",
            "start" | "stop" => "serviceAction",
            _ => "updateTool",
        }
    }
    pub(crate) fn providers(&self) -> Vec<ProviderId> {
        vec![ProviderId::Ollama]
    }
    pub(crate) fn item(&self) -> PlanItem {
        if ["start", "stop"].contains(&self.action.as_str()) {
            return PlanItem {
                title: format!("{} Ollama service", self.action),
                path: self.tool.path.clone(),
                command: if self.action == "start" {
                    self.tool
                        .path
                        .as_ref()
                        .map(|path| CommandSpec::new(path, ["serve"]).display())
                } else {
                    Some("Stop the reviewed Envark-owned Ollama process".into())
                },
                bytes: 0,
                restore: Some(
                    if self.action == "start" {
                        "Use Stop service to stop this Envark-owned process."
                    } else {
                        "Use Start service to start the installed Ollama program again."
                    }
                    .into(),
                ),
            };
        }
        PlanItem { title: format!("{} Ollama {}", self.action, self.version), path: Some(self.root.clone()),
            command: self.command.as_ref().map(CommandSpec::display).or_else(|| self.artifact.as_ref().map(|a| format!("Download {} (SHA-256 {}), verify and install in {}", a.url, a.sha256, self.root.display()))),
            bytes: 0, restore: Some("Reinstall the Ollama program. Models and configuration remain in their existing location.".into()) }
    }
    pub(crate) fn warnings(&self) -> Vec<String> {
        if self.action == "start" {
            return vec!["Starts the reviewed Ollama executable at http://127.0.0.1:11434. An existing listener will not be replaced. Program files, models, and configuration are preserved.".into()];
        }
        if self.action == "stop" {
            return vec!["Stops only the reviewed Envark-owned process. Active requests may be interrupted. Program files, models, and configuration are preserved.".into()];
        }
        let mut warnings = vec!["Models and configuration are preserved. Program installation does not start the Ollama service; use Start service separately.".into()];
        if cfg!(target_os = "linux") {
            warnings.push("The official base archive includes its bundled backends. Optional AMD ROCm packages and GPU drivers are not installed by this action.".into());
        }
        if cfg!(windows) {
            warnings.push("Installs the official standalone CLI and libraries in your account; Windows tray UI and startup registration are not installed.".into());
        }
        warnings
    }
    async fn validate_service_start(&self, ctx: &Context) -> Result<()> {
        if validate_owner(ctx, &self.tool).await? != self.root {
            return Err(Error::Conflict(
                "The reviewed Ollama installation owner changed.".into(),
            ));
        }
        let path = self
            .tool
            .path
            .as_ref()
            .ok_or_else(|| Error::Conflict("Missing reviewed Ollama executable.".into()))?
            .canonicalize()?;
        if self.fingerprint.as_deref() != Some(&fingerprint(ctx, &path)?) {
            return Err(Error::Conflict(
                "The reviewed Ollama executable changed. Review Start service again.".into(),
            ));
        }
        Ok(())
    }
    pub(crate) async fn execute(self, ctx: &Context, use_trash: bool) -> Result<(u64, String)> {
        if ctx.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if self.action == "stop" {
            return service::stop(
                ctx,
                self.service_snapshot
                    .as_deref()
                    .ok_or_else(|| Error::Conflict("Missing reviewed service identity.".into()))?,
            )
            .await;
        }
        if self.action == "start" {
            self.validate_service_start(ctx).await?;
            return service::start(ctx, self.tool.path.as_ref().unwrap()).await;
        }
        reject_links(&self.root)?;
        preserve_data(ctx, &self.root)?;
        if let Some(expected) = &self.fingerprint {
            if validate_owner(ctx, &self.tool).await? != self.root
                || fingerprint(ctx, &self.root)? != *expected
            {
                return Err(Error::Conflict(
                    "The reviewed Ollama installation changed. Refresh before continuing.".into(),
                ));
            }
        } else if self.root.exists() {
            return Err(Error::Conflict(
                "The Ollama install directory became occupied.".into(),
            ));
        }
        if self.action == "update" {
            verify_update(ctx, &self.tool).await?;
        }
        service::require_stopped(ctx, &self.root).await?;
        if let Some(command) = &self.command {
            if self.action == "update" && latest(ctx, &self.tool).await? != self.version {
                return Err(Error::Conflict(
                    "Homebrew's Ollama version changed after review.".into(),
                ));
            }
            ctx.runner.run(command, &ctx.cancel).await?;
        } else if self.action == "remove" {
            if use_trash {
                trash::delete(&self.root).map_err(|e| Error::Unavailable(e.to_string()))?;
            } else {
                std::fs::remove_dir_all(&self.root)?;
            }
            let path = receipt_path(ctx);
            reject_links(&path)?;
            std::fs::remove_file(path)?;
        } else {
            install(ctx, &self).await?;
        }
        if self.action == "remove" {
            if self.root.exists() {
                return Err(Error::Conflict(
                    "Ollama's program directory remains after removal.".into(),
                ));
            }
            return Ok((0, "Ollama program removed. Models and configuration were preserved; no service was stopped by the removal action.".into()));
        }
        let path = if self.tool.source == BREW {
            ctx.executable("ollama").ok_or_else(|| {
                Error::Unavailable("The updated Ollama executable was not found.".into())
            })?
        } else {
            binary(&self.root)
        };
        let actual = client_version(ctx, &path).await?;
        if actual != self.version {
            return Err(Error::Conflict(format!(
                "Ollama reports {actual}, but the reviewed version was {}.",
                self.version
            )));
        }
        Ok((
            0,
            format!(
                "Ollama {actual} installed. Models and configuration were preserved. Use Start service to serve models."
            ),
        ))
    }
}

async fn download(ctx: &Context, artifact: &Artifact, path: &Path) -> Result<()> {
    use std::io::Write;
    let request = async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3600))
            .build()
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let mut response = client
            .get(&artifact.url)
            .send()
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?
        {
            size += chunk.len() as u64;
            if size > artifact.size {
                return Err(Error::Conflict(
                    "Ollama archive exceeds the reviewed size.".into(),
                ));
            }
            hasher.update(&chunk);
            file.write_all(&chunk)?;
        }
        if size != artifact.size || hex::encode(hasher.finalize()) != artifact.sha256 {
            return Err(Error::Conflict(
                "Ollama archive SHA-256 or size does not match the reviewed release.".into(),
            ));
        }
        Ok(())
    };
    tokio::select! { value = request => value, _ = ctx.cancel.cancelled() => Err(Error::Cancelled) }
}

async fn install(ctx: &Context, plan: &Plan) -> Result<()> {
    let parent = plan
        .root
        .parent()
        .ok_or_else(|| Error::InvalidInput("Missing program parent.".into()))?;
    reject_links(parent)?;
    std::fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".envark-ollama-")
        .tempdir_in(parent)?;
    let archive = stage.path().join(asset_name());
    download(ctx, plan.artifact.as_ref().unwrap(), &archive).await?;
    let extracted = stage.path().join("extracted");
    std::fs::create_dir(&extracted)?;
    let a = archive.to_string_lossy();
    let dest = extracted.to_string_lossy();
    let mut command = if cfg!(target_os = "macos") {
        CommandSpec::new("/usr/bin/ditto", ["-xk", &a, &dest])
    } else if cfg!(windows) {
        let mut spec = CommandSpec::new(
            ctx.executable("powershell")
                .or_else(|| ctx.executable("pwsh"))
                .unwrap(),
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Expand-Archive -LiteralPath $env:ENVARK_ARCHIVE -DestinationPath $env:ENVARK_DESTINATION -ErrorAction Stop",
            ],
        );
        spec.env.insert("ENVARK_ARCHIVE".into(), a.into_owned());
        spec.env
            .insert("ENVARK_DESTINATION".into(), dest.into_owned());
        spec
    } else {
        ctx.command(
            "tar",
            &[
                "--zstd",
                "-xf",
                &a,
                "-C",
                &dest,
                "--no-same-owner",
                "--no-same-permissions",
            ],
        )?
    };
    command.timeout = Duration::from_secs(1800);
    ctx.runner.run(&command, &ctx.cancel).await?;
    let staged_root = if cfg!(target_os = "macos") {
        extracted.join("Ollama.app")
    } else {
        extracted
    };
    // Official archives can contain library symlinks, but none may escape the staged program.
    let canonical = staged_root.canonicalize()?;
    for entry in walkdir::WalkDir::new(&staged_root).follow_links(false) {
        let entry = entry.map_err(|e| Error::Unavailable(e.to_string()))?;
        if entry.file_type().is_symlink() && !entry.path().canonicalize()?.starts_with(&canonical) {
            return Err(Error::unsafe_path(
                entry.path(),
                "archive link escapes the program",
            ));
        }
    }
    if cfg!(target_os = "macos") {
        ctx.runner
            .run(
                &CommandSpec::new(
                    "/usr/bin/codesign",
                    [
                        "--verify",
                        "--deep",
                        "--strict",
                        &staged_root.to_string_lossy(),
                    ],
                ),
                &ctx.cancel,
            )
            .await?;
    }
    let backup = stage.path().join("previous");
    let result = activate(ctx, plan, &staged_root, &backup).await;
    if let Err(error) = &result
        && backup.exists()
    {
        let retained = stage.keep();
        return Err(Error::Conflict(format!(
            "Ollama installation failed; the previous program was retained at {}. {}",
            retained.join("previous").display(),
            error
        )));
    }
    result
}

async fn activate(ctx: &Context, plan: &Plan, staged_root: &Path, backup: &Path) -> Result<()> {
    let actual = client_version(ctx, &binary(staged_root)).await?;
    if actual != plan.version {
        return Err(Error::Conflict(
            "The extracted Ollama client does not match the reviewed version.".into(),
        ));
    }
    // Recheck after the long download, before replacing the reviewed installation.
    if let Some(expected) = &plan.fingerprint {
        if fingerprint(ctx, &plan.root)? != *expected {
            return Err(Error::Conflict(
                "Ollama changed while its update downloaded.".into(),
            ));
        }
    } else if plan.root.exists() {
        return Err(Error::Conflict(
            "Ollama's installation path became occupied.".into(),
        ));
    }
    service::require_stopped(ctx, &plan.root).await?;
    reject_links(&plan.root)?;
    let record = Receipt {
        root: plan.root.clone(),
        binary: binary(&plan.root),
        version: plan.version.clone(),
    };
    let path = receipt_path(ctx);
    reject_links(&path)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let mut pending = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    use std::io::Write;
    pending.write_all(&serde_json::to_vec(&record)?)?;
    if ctx.cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if plan.root.exists() {
        std::fs::rename(&plan.root, backup)?;
    }
    if let Err(error) = std::fs::rename(staged_root, &plan.root) {
        if backup.exists() {
            let _ = std::fs::rename(backup, &plan.root);
        }
        return Err(error.into());
    }
    if let Err(error) = pending.persist(&path) {
        std::fs::rename(&plan.root, staged_root)?;
        if backup.exists() {
            std::fs::rename(backup, &plan.root)?;
        }
        return Err(Error::Io(error.error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(root: &Path) -> Context {
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = root.canonicalize().unwrap();
        ctx.data = ctx.home.join("data");
        ctx.cache = ctx.home.join("cache");
        ctx
    }

    #[cfg(unix)]
    fn fixture(root: &Path, version: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = binary(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!("#!/bin/sh\nprintf 'Warning: client version is {version}\\n' >&2\n"),
        )
        .unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn program_update_rejects_downgrades_and_changed_installed_versions() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let root = install_root(&ctx);
        fixture(&root, "1.0.0");
        let mut tool = basic_tool("ollama", "1.0.0".into(), SOURCE, Some(binary(&root)));
        tool.latest = Some("0.9.0".into());
        assert!(verify_update(&ctx, &tool).await.is_err());
        tool.latest = Some("1.1.0".into());
        verify_update(&ctx, &tool).await.unwrap();
        fixture(&root, "1.2.0");
        assert!(verify_update(&ctx, &tool).await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn service_start_reviews_only_the_owned_executable() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let root = install_root(&ctx);
        fixture(&root, "1.0.0");
        let path = receipt_path(&ctx);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            serde_json::to_vec(&Receipt {
                root: root.clone(),
                binary: binary(&root),
                version: "1.0.0".into(),
            })
            .unwrap(),
        )
        .unwrap();
        let mut provider = Provider::empty(ProviderId::Ollama);
        provider.tools.push(basic_tool(
            "ollama",
            "1.0.0".into(),
            SOURCE,
            Some(binary(&root)),
        ));
        let inventory = Inventory {
            providers: vec![provider],
            ..Default::default()
        };
        let plan = prepare(
            &ctx,
            ActionRequest::ServiceAction {
                provider: ProviderId::Ollama,
                action: "start".into(),
            },
            &inventory,
            &Settings::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            plan.fingerprint,
            Some(fingerprint(&ctx, &binary(&root)).unwrap())
        );
        assert!(
            plan.warnings()
                .iter()
                .all(|warning| !warning.contains("ROCm")
                    && !warning.contains("tray")
                    && !warning.contains("install"))
        );
        std::fs::create_dir_all(root.join("lib/nested")).unwrap();
        std::fs::write(
            root.join("lib/nested/changed"),
            "not part of executable review",
        )
        .unwrap();
        plan.validate_service_start(&ctx).await.unwrap();
        fixture(&root, "2.0.0");
        assert!(plan.validate_service_start(&ctx).await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn install_update_remove_and_reinstall_preserve_models_and_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let root = install_root(&ctx);
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        let models = ctx.home.join(".ollama/models/sentinel");
        std::fs::create_dir_all(models.parent().unwrap()).unwrap();
        std::fs::write(&models, "keep models").unwrap();
        let config = ctx.home.join(".ollama/config.json");
        std::fs::write(&config, "keep configuration").unwrap();
        let tool = basic_tool("ollama", "1.0.0".into(), SOURCE, Some(binary(&root)));
        let plan = Plan {
            action: "install".into(),
            tool,
            root: root.clone(),
            fingerprint: None,
            artifact: None,
            command: None,
            version: "1.0.0".into(),
            service_snapshot: None,
        };
        let staged = temp.path().join("staged");
        let backup = temp.path().join("backup");
        fixture(&staged, "1.0.0");
        activate(&ctx, &plan, &staged, &backup).await.unwrap();
        assert_eq!(client_version(&ctx, &binary(&root)).await.unwrap(), "1.0.0");
        assert!(receipt(&ctx).is_some());
        let mut update = plan.clone();
        update.action = "update".into();
        update.fingerprint = Some(fingerprint(&ctx, &root).unwrap());
        update.version = "1.1.0".into();
        fixture(&staged, "1.1.0");
        activate(&ctx, &update, &staged, &backup).await.unwrap();
        assert_eq!(client_version(&ctx, &binary(&root)).await.unwrap(), "1.1.0");
        let mut remove = update;
        remove.action = "remove".into();
        remove.fingerprint = Some(fingerprint(&ctx, &root).unwrap());
        let stale = remove.clone();
        std::fs::write(root.join("changed-after-review"), "changed").unwrap();
        assert!(stale.execute(&ctx, false).await.is_err());
        remove.fingerprint = Some(fingerprint(&ctx, &root).unwrap());
        remove.execute(&ctx, false).await.unwrap();
        assert!(!root.exists());
        assert!(receipt(&ctx).is_none());
        assert_eq!(std::fs::read_to_string(&models).unwrap(), "keep models");
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            "keep configuration"
        );
        fixture(&staged, "1.0.0");
        activate(&ctx, &plan, &staged, &backup).await.unwrap();
        assert!(receipt(&ctx).is_some());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_postcondition_and_cancelled_install_do_not_publish_program() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let root = install_root(&ctx);
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        let plan = Plan {
            action: "install".into(),
            tool: basic_tool("ollama", "1.0.0".into(), SOURCE, Some(binary(&root))),
            root: root.clone(),
            fingerprint: None,
            artifact: None,
            command: None,
            version: "1.0.0".into(),
            service_snapshot: None,
        };
        let staged = temp.path().join("staged");
        let backup = temp.path().join("backup");
        fixture(&staged, "0.9.0");
        assert!(activate(&ctx, &plan, &staged, &backup).await.is_err());
        assert!(!root.exists());
        fixture(&staged, "1.0.0");
        ctx.cancel.cancel();
        assert!(activate(&ctx, &plan, &staged, &backup).await.is_err());
        assert!(!root.exists());
    }
    #[test]
    fn release_assets_require_official_url_and_sha256() {
        let name = "ollama-linux-amd64.tar.zst";
        let mut release = serde_json::json!({"tag_name":"v1.2.3","assets":[{"name":name,"size":123,"digest":format!("sha256:{}", "a".repeat(64)),"browser_download_url":format!("https://github.com/ollama/ollama/releases/download/v1.2.3/{name}")}]});
        assert!(artifact(&release, name).is_ok());
        release["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(artifact(&release, name).is_err());
        release["assets"][0]["digest"] = format!("sha256:{}", "a".repeat(64)).into();
        release["assets"][0]["browser_download_url"] = "https://unrelated.example/archive".into();
        assert!(artifact(&release, name).is_err());
    }
    #[test]
    fn receipt_never_adopts_an_unrelated_program() {
        let temp = tempfile::tempdir().unwrap();
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = temp.path().canonicalize().unwrap();
        ctx.data = ctx.home.join("data");
        let path = receipt_path(&ctx);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let other = ctx.home.join("other");
        std::fs::write(&other, "unrelated").unwrap();
        std::fs::write(
            path,
            serde_json::to_vec(&Receipt {
                root: ctx.home.clone(),
                binary: other,
                version: "1.0.0".into(),
            })
            .unwrap(),
        )
        .unwrap();
        assert!(receipt(&ctx).is_none());
    }
}
