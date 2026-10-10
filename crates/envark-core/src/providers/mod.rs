pub(crate) mod cache_cleanup;
pub(crate) mod cache_discovery;
mod javascript;
pub(crate) mod js_tooling;
pub(crate) mod language_lifecycle;
mod languages;
pub mod manager_install;
pub(crate) mod manager_remove;
pub(crate) mod native_lifecycle;
pub mod ollama;
pub(crate) mod ollama_lifecycle;
pub(crate) mod package_managers;
pub mod python;
mod resources;
pub(crate) mod script_installers;
mod shell_managers;
mod tool_commands;
pub use tool_commands::tool_command;
pub mod updates;

use crate::{
    Error, Result,
    filesystem::{id_for, read_small},
    model::{Cache, ConfigFile, Manager, Provider, ProviderId, Tool},
    process::{CommandSpec, Runner},
};
use directories::BaseDirs;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Context {
    pub home: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    pub runner: Runner,
    pub cancel: CancellationToken,
}

impl Context {
    pub fn new(cancel: CancellationToken) -> Result<Self> {
        let dirs = BaseDirs::new()
            .ok_or_else(|| Error::Unavailable("Home directory is unavailable.".into()))?;
        Ok(Self {
            home: dirs.home_dir().into(),
            data: dirs.data_dir().into(),
            cache: dirs.cache_dir().into(),
            runner: Runner::default(),
            cancel,
        })
    }

    pub fn executable(&self, name: &str) -> Option<PathBuf> {
        if let Ok(path) = which::which(name) {
            return Some(path);
        }
        self.executable_candidates(name)
            .into_iter()
            .find(|path| path.is_file())
    }

    fn executable_candidates(&self, name: &str) -> Vec<PathBuf> {
        let file = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        };
        let mut candidates = vec![
            self.home.join(".cargo/bin"),
            self.home.join(".local/bin"),
            self.home.join(".bun/bin"),
            self.home.join(".pyenv/bin"),
            self.home.join(".local/share/fnm"),
            self.home.join(".fnm"),
            self.data.join("fnm"),
            self.home.join("AppData/Local/fnm"),
            self.home.join("AppData/Local/Microsoft/WinGet/Links"),
            self.home.join("AppData/Local/Microsoft/WindowsApps"),
            self.home.join("AppData/Local/Programs/Ollama"),
        ];
        if !cfg!(windows) {
            candidates.extend([
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
            ]);
        }
        let configured_root = match name {
            "cargo" | "rustc" | "rustup" => Some(("CARGO_HOME", "bin")),
            "pyenv" => Some(("PYENV_ROOT", "bin")),
            "bun" => Some(("BUN_INSTALL", "bin")),
            "fnm" => Some(("FNM_DIR", "")),
            "uv" | "uvx" => Some(("UV_INSTALL_DIR", "")),
            "java" | "javac" => Some(("JAVA_HOME", "bin")),
            _ => None,
        };
        if let Some((variable, suffix)) = configured_root
            && let Some(root) = std::env::var_os(variable).filter(|root| !root.is_empty())
        {
            candidates.insert(0, PathBuf::from(root).join(suffix));
        }
        if matches!(name, "python" | "python3") && cfg!(target_os = "macos") {
            candidates.push(PathBuf::from(
                "/Library/Frameworks/Python.framework/Versions/Current/bin",
            ));
        }
        if matches!(name, "node" | "npm") && cfg!(windows) {
            if let Some(root) = std::env::var_os("NVM_SYMLINK") {
                candidates.push(PathBuf::from(root));
            }
            if let Some(root) = std::env::var_os("ProgramFiles") {
                candidates.push(PathBuf::from(root).join("nodejs"));
            }
        }
        if name == "ollama" {
            if cfg!(target_os = "macos") {
                candidates.push(PathBuf::from("/Applications/Ollama.app/Contents/Resources"));
            }
            candidates.push(if cfg!(target_os = "macos") {
                self.home.join("Applications/Ollama.app/Contents/Resources")
            } else if cfg!(windows) {
                self.data.join("envark/ollama/program")
            } else {
                self.data.join("envark/ollama/program/bin")
            });
        }
        if name == "go" {
            if let Some(root) = std::env::var_os("GOROOT").filter(|root| !root.is_empty()) {
                candidates.insert(0, PathBuf::from(root).join("bin"));
            }
            if cfg!(windows) {
                if let Some(root) = std::env::var_os("ProgramFiles") {
                    candidates.push(PathBuf::from(root).join("Go/bin"));
                }
            } else {
                candidates.push(PathBuf::from("/usr/local/go/bin"));
            }
        }
        if name == "pnpm" {
            let mut homes = vec![
                self.data.join("pnpm"),
                self.home.join("Library/pnpm"),
                self.home.join("AppData/Local/pnpm"),
            ];
            if let Some(home) = std::env::var_os("PNPM_HOME") {
                homes.insert(0, home.into());
            }
            for home in homes {
                candidates.push(home.join("bin"));
                candidates.push(home);
            }
        }
        if ["node", "npm", "npx", "pnpm", "yarn", "corepack"].contains(&name) {
            candidates.extend(js_tooling::node_bins(self));
        }
        candidates
            .into_iter()
            .flat_map(|dir| {
                let mut paths = vec![dir.join(&file)];
                if cfg!(windows) && ["npm", "pnpm", "yarn", "corepack"].contains(&name) {
                    paths.push(dir.join(format!("{name}.cmd")));
                }
                paths
            })
            .collect()
    }

    pub fn command(&self, name: &str, args: &[&str]) -> Result<CommandSpec> {
        if !cfg!(windows) && ["nvm", "SDKMAN!"].contains(&name) {
            return shell_managers::command(self, name, args);
        }
        let program = self.executable(name).ok_or_else(|| {
            Error::Unavailable(format!(
                "{name} is not installed or cannot be found in PATH."
            ))
        })?;
        if ["npm", "pnpm", "yarn", "corepack"].contains(&name)
            && let Some(spec) = js_tooling::cli_command(self, &program, name, args)
        {
            return Ok(spec);
        }
        // Use Node directly for npm, avoiding cmd.exe and shell metacharacters on Windows.
        if name == "npm"
            && let Some(node) = self.executable("node")
        {
            let mut roots = vec![
                program.parent().unwrap_or(Path::new(".")).to_path_buf(),
                node.parent().unwrap_or(Path::new(".")).to_path_buf(),
            ];
            if let Ok(real) = std::fs::canonicalize(&program) {
                roots.push(real.parent().unwrap_or(Path::new(".")).to_path_buf());
            }
            for root in roots {
                for candidate in [
                    root.join("node_modules/npm/bin/npm-cli.js"),
                    root.join("../lib/node_modules/npm/bin/npm-cli.js"),
                ] {
                    if candidate.is_file() {
                        let mut spec =
                            CommandSpec::new(node, [candidate.to_string_lossy().into_owned()]);
                        spec.args.extend(args.iter().map(|s| s.to_string()));
                        return Ok(spec);
                    }
                }
            }
        }
        Ok(CommandSpec::new(program, args.iter().copied()))
    }

    pub async fn read(&self, name: &str, args: &[&str]) -> Result<String> {
        let mut spec = self.command(name, args)?;
        self.apply_read_policy(&mut spec);
        let output = self.runner.run(&spec, &self.cancel).await?;
        Ok(if output.stdout.trim().is_empty() {
            output.stderr
        } else {
            output.stdout
        })
    }

    fn apply_read_policy(&self, spec: &mut CommandSpec) {
        spec.cwd = Some(self.home.clone());
        spec.env
            .insert("COREPACK_ENABLE_PROJECT_SPEC".into(), "0".into());
        spec.env
            .insert("COREPACK_ENABLE_AUTO_PIN".into(), "0".into());
        spec.env
            .insert("COREPACK_DEFAULT_TO_LATEST".into(), "0".into());
        spec.env
            .insert("COREPACK_ENABLE_NETWORK".into(), "0".into());
        spec.env
            .insert("UV_PYTHON_DOWNLOADS".into(), "never".into());
        spec.env.insert("GOTOOLCHAIN".into(), "local".into());
        spec.env.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
    }

    pub async fn manager(&self, name: &str, install: bool, default: bool) -> Option<Manager> {
        let path = shell_managers::initialization(self, name).or_else(|| self.executable(name))?;
        if name == "SDKMAN!" {
            self.executable("bash")?;
            let root = path.parent()?.parent()?;
            return Some(Manager {
                name: name.into(),
                version: read_small(&root.join("var/version"), 128)
                    .map(|v| v.trim().into())
                    .unwrap_or_else(|_| "unknown".into()),
                path,
                supports_install: install,
                supports_default: default,
            });
        }
        let version = self
            .read(
                name,
                &[if name == "SDKMAN!" {
                    "version"
                } else {
                    "--version"
                }],
            )
            .await
            .ok()?
            .lines()
            .next()?
            .trim()
            .to_owned();
        Some(Manager {
            name: name.into(),
            version,
            path,
            supports_install: install,
            supports_default: default,
        })
    }

    pub async fn cache_path(&self, name: &str, args: &[&str]) -> Option<PathBuf> {
        let output = self.read(name, args).await.ok()?;
        output
            .lines()
            .rev()
            .map(str::trim)
            .map(PathBuf::from)
            .find(|path| path.is_absolute() && path.is_dir())
    }
}

pub struct Discovery {
    pub providers: Vec<Provider>,
    pub caches: Vec<Cache>,
}

pub async fn discover(context: Context) -> Result<Discovery> {
    discover_selected(context, &ProviderId::ALL).await
}

pub(crate) async fn discover_selected(
    context: Context,
    selected: &[ProviderId],
) -> Result<Discovery> {
    let mut jobs = tokio::task::JoinSet::new();
    for &id in selected {
        let ctx = context.clone();
        jobs.spawn(async move {
            let mut provider = Provider::empty(id);
            let caches = match id {
                ProviderId::Js => javascript::discover(&ctx, &mut provider).await,
                ProviderId::Py | ProviderId::Jvm | ProviderId::Rust | ProviderId::Go => {
                    languages::discover(&ctx, &mut provider).await
                }
                _ => resources::discover(&ctx, &mut provider).await,
            };
            package_managers::resolve(&ctx, &mut provider).await;
            language_lifecycle::discover(&ctx, &mut provider).await;
            ollama_lifecycle::discover(&ctx, &mut provider).await;
            provider.detected = !provider.runtimes.is_empty()
                || !provider.managers.is_empty()
                || !provider.package_managers.is_empty()
                || !provider.tools.is_empty()
                || !provider.assets.is_empty();
            (provider, caches)
        });
    }
    let mut providers = vec![];
    let mut caches = vec![];
    while let Some(result) = jobs.join_next().await {
        let (provider, found) = result.map_err(|e| Error::Unavailable(e.to_string()))?;
        providers.push(provider);
        caches.extend(found);
    }
    if context.cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    providers.sort_by_key(|p| ProviderId::ALL.iter().position(|id| *id == p.id));
    // Shared cache paths are counted only once, even when several tools report them.
    let mut seen = std::collections::HashSet::new();
    caches.retain(|c| seen.insert(std::fs::canonicalize(&c.path).unwrap_or(c.path.clone())));
    cache_discovery::check_capabilities(&context, &mut caches).await?;
    Ok(Discovery { providers, caches })
}

pub(super) fn config(provider: &mut Provider, path: PathBuf, format: &str, editable: bool) {
    if path.is_file() {
        provider.configs.push(ConfigFile {
            id: id_for("config", &path),
            path,
            format: format.into(),
            editable,
            warning: None,
        });
    }
}

pub(super) fn cache(
    provider: ProviderId,
    name: &str,
    path: PathBuf,
    strategy: &str,
    can_clean: bool,
) -> Option<Cache> {
    if !path.is_dir() {
        return None;
    }
    Some(Cache { cleanup_issue: None, id: id_for("cache", &path), provider, name: name.into(), path, size: Default::default(), strategy: strategy.into(), can_clean, warning: match strategy {
        "maven-repository" => "This repository can contain locally published artifacts that cannot be downloaded again. Blanket cache cleanup is disabled to preserve them.",
        "cargo-registry" | "cargo-git" => "Downloaded dependencies are removed while holding Cargo cache locks. Installed tools and configuration are preserved. Dependencies will be downloaded again when needed.",
        "gradle-caches" | "gradle-dists" => "Gradle applies its retention policy to both caches and distributions. Recently used entries are retained; this does not empty every cache.",
        _ => "Cache size is an upper bound; native pruning may retain entries that are still referenced. Offline builds may need to download dependencies again.",
    }.into() })
}

pub(super) fn basic_tool(name: &str, version: String, source: &str, path: Option<PathBuf>) -> Tool {
    let key = path
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("{source}/{name}")));
    Tool {
        id: id_for("tool", &key),
        name: name.into(),
        version,
        latest: None,
        update_status: Default::default(),
        source: source.into(),
        runtime: None,
        path,
        size: None,
        can_update: false,
        can_remove: false,
        note: None,
    }
}

pub(super) fn directories(path: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|t| t.is_dir() && !t.is_symlink())
        })
        .map(|entry| entry.path())
        .collect()
}

pub(super) fn package_manifests(root: &Path) -> Vec<(PathBuf, serde_json::Value)> {
    fn packages(root: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect()
    }
    let mut paths = vec![];
    for path in packages(root) {
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('@'))
        {
            paths.extend(packages(&path));
        } else {
            paths.push(path);
        }
    }
    paths
        .into_iter()
        .filter_map(|path| {
            read_small(&path.join("package.json"), 1_048_576)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .map(|json| (path, json))
        })
        .collect()
}

pub fn valid_identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@._/+:-".contains(&b))
        || value.contains("..")
    {
        return Err(Error::InvalidInput(
            "Invalid package, model, or version identifier.".into(),
        ));
    }
    Ok(())
}

pub fn runtime_command(
    ctx: &Context,
    manager: &str,
    verb: &str,
    version: &str,
) -> Result<CommandSpec> {
    valid_identifier(version)?;
    let mut spec = match (manager, verb) {
        ("fnm", "install") => ctx.command("fnm", &["install", version])?,
        ("fnm", "default") => ctx.command("fnm", &["default", version])?,
        ("fnm", "remove") => ctx.command("fnm", &["uninstall", version])?,
        ("uv", "install") => ctx.command("uv", &["python", "install", version])?,
        ("uv", "default") => ctx.command("uv", &["python", "pin", "--global", version])?,
        ("uv", "remove") => ctx.command("uv", &["python", "uninstall", version])?,
        ("rustup", "install") => ctx.command("rustup", &["toolchain", "install", version])?,
        ("rustup", "default") => ctx.command("rustup", &["default", version])?,
        ("rustup", "remove") => ctx.command("rustup", &["toolchain", "uninstall", version])?,
        ("pyenv", "install") => ctx.command("pyenv", &["install", version])?,
        ("pyenv", "default") => ctx.command("pyenv", &["global", version])?,
        ("pyenv", "remove") => ctx.command("pyenv", &["uninstall", "-f", version])?,
        ("nvm", _) if cfg!(windows) => ctx.command(
            "nvm",
            &[
                match verb {
                    "install" => "install",
                    "default" => "use",
                    "remove" => "uninstall",
                    _ => return Err(Error::InvalidInput("Unknown operation.".into())),
                },
                version,
            ],
        )?,
        ("nvm", "install") => ctx.command("nvm", &["install", version])?,
        ("nvm", "default") => ctx.command("nvm", &["alias", "default", version])?,
        ("nvm", "remove") => ctx.command("nvm", &["uninstall", version])?,
        ("SDKMAN!", "install") => ctx.command("SDKMAN!", &["install", "java", version])?,
        ("SDKMAN!", "default") => ctx.command("SDKMAN!", &["default", "java", version])?,
        ("SDKMAN!", "remove") => ctx.command("SDKMAN!", &["uninstall", "java", version])?,
        _ => {
            return Err(Error::Unavailable(format!(
                "{manager} does not support {verb} on this installation."
            )));
        }
    };
    spec.timeout = std::time::Duration::from_secs(1800);
    Ok(spec)
}

pub fn cache_command(ctx: &Context, cache: &Cache) -> Result<CommandSpec> {
    let mut spec = match cache.strategy.as_str() {
        "pip-purge" => cache_cleanup::pip_command(ctx, &cache.path),
        "gradle-caches" | "gradle-dists" => cache_cleanup::gradle_command(ctx, cache),
        "npm-verify" => ctx.command("npm", &["cache", "verify"]),
        "pnpm-prune" => ctx.command("pnpm", &["store", "prune"]),
        "yarn-clean" => ctx.command("yarn", &["cache", "clean"]),
        "uv-prune" => ctx.command("uv", &["cache", "prune"]),
        "go-build" => ctx.command("go", &["clean", "-cache"]),
        "go-modules" => ctx.command("go", &["clean", "-modcache"]),
        _ => Err(Error::Unavailable(
            "No cleanup strategy is registered for this cache.".into(),
        )),
    }?;
    let path = cache.path.to_string_lossy().into_owned();
    match cache.strategy.as_str() {
        "pip-purge" | "gradle-caches" | "gradle-dists" => {}
        "npm-verify" => spec.args.extend(["--cache".into(), path]),
        "pnpm-prune" | "yarn-clean" => {
            let versioned = cache
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| {
                    n.strip_prefix('v')
                        .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
                });
            let root = if versioned {
                cache.path.parent().unwrap_or(&cache.path)
            } else {
                &cache.path
            };
            spec.args.extend([
                if cache.strategy == "pnpm-prune" {
                    "--store-dir"
                } else {
                    "--cache-folder"
                }
                .into(),
                root.to_string_lossy().into_owned(),
            ]);
        }
        "uv-prune" => spec.args.extend(["--cache-dir".into(), path]),
        "go-build" => {
            spec.env.insert("GOCACHE".into(), path);
        }
        "go-modules" => {
            spec.env.insert("GOMODCACHE".into(), path);
        }
        _ => unreachable!("unsupported strategies were rejected above"),
    }
    spec.cwd = Some(ctx.home.clone());
    spec.env.insert("GOTOOLCHAIN".into(), "local".into());
    spec.timeout = std::time::Duration::from_secs(600);
    Ok(spec)
}

pub fn cache_probe(cache: &Cache, command: &CommandSpec) -> Option<CommandSpec> {
    if matches!(cache.strategy.as_str(), "go-build" | "go-modules") {
        let mut probe = command.clone();
        probe.args = vec![
            "env".into(),
            if cache.strategy == "go-build" {
                "GOCACHE"
            } else {
                "GOMODCACHE"
            }
            .into(),
        ];
        probe.timeout = std::time::Duration::from_secs(20);
        return Some(probe);
    }
    if cache.strategy == "npm-verify" {
        let mut probe = command.clone();
        let offset = probe
            .args
            .windows(2)
            .position(|args| args == ["cache", "verify"])?;
        probe.args.splice(
            offset..offset + 2,
            ["config".into(), "get".into(), "cache".into()],
        );
        probe.timeout = std::time::Duration::from_secs(20);
        return Some(probe);
    }
    let (action, replacement) = match cache.strategy.as_str() {
        "pnpm-prune" => ("prune", "path"),
        "yarn-clean" => ("clean", "dir"),
        "pip-purge" => ("purge", "dir"),
        "uv-prune" => ("prune", "dir"),
        _ => return None,
    };
    let mut probe = command.clone();
    *probe.args.iter_mut().find(|a| *a == action)? = replacement.into();
    probe.timeout = std::time::Duration::from_secs(20);
    Some(probe)
}

#[cfg(test)]
mod read_policy_tests {
    use super::*;
    #[cfg(unix)]
    use crate::model::ProviderId;

    #[test]
    #[ignore = "subprocess fixture launched by read_policy_reaches_child_processes"]
    fn toolchain_probe_fixture() {
        assert_eq!(std::env::var("RUSTUP_AUTO_INSTALL").unwrap(), "0");
        assert_eq!(std::env::var("UV_PYTHON_DOWNLOADS").unwrap(), "never");
        assert_eq!(std::env::var("GOTOOLCHAIN").unwrap(), "local");
    }

    #[tokio::test]
    async fn read_policy_reaches_child_processes() {
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let mut command = CommandSpec::new(
            std::env::current_exe().unwrap(),
            [
                "--ignored",
                "--exact",
                "providers::read_policy_tests::toolchain_probe_fixture",
            ],
        );
        command.env.insert("RUSTUP_AUTO_INSTALL".into(), "1".into());
        ctx.apply_read_policy(&mut command);
        ctx.runner.run(&command, &ctx.cancel).await.unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "isolated environment fixture launched by go_is_detected_without_shell_path"]
    async fn go_probe_fixture() {
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let root = PathBuf::from(std::env::var_os("GOROOT").unwrap());
        assert_eq!(ctx.executable("go"), Some(root.join("bin/go")));
        assert!(
            ctx.executable_candidates("go")
                .contains(&PathBuf::from("/usr/local/go/bin/go"))
        );
        for name in ["cargo", "rustup", "pyenv", "java", "bun"] {
            assert_eq!(
                ctx.executable(name),
                Some(root.join("bin").join(name)),
                "{name}"
            );
        }
        let found = discover_selected(ctx, &[ProviderId::Go]).await.unwrap();
        assert!(found.providers[0].detected);
        assert_eq!(found.providers[0].runtimes[0].version, "1.26.1");
        assert_eq!(found.providers[0].runtimes[0].path, root);
    }

    #[cfg(unix)]
    #[test]
    fn go_is_detected_without_shell_path() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("bin")).unwrap();
        let go = root.path().join("bin/go");
        // The probe is intentionally outside PATH, as in a Finder-launched app.
        let payload = serde_json::json!({"GOROOT":root.path(),"GOVERSION":"go1.26.1"});
        std::fs::write(&go, format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", payload)).unwrap();
        std::fs::set_permissions(&go, std::fs::Permissions::from_mode(0o755)).unwrap();
        for name in ["cargo", "rustup", "pyenv", "java", "bun"] {
            let binary = root.path().join("bin").join(name);
            std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "providers::read_policy_tests::go_probe_fixture",
            ])
            .env("PATH", root.path().join("empty-path"))
            .env("GOROOT", root.path())
            .env("CARGO_HOME", root.path())
            .env("PYENV_ROOT", root.path())
            .env("JAVA_HOME", root.path())
            .env("BUN_INSTALL", root.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
