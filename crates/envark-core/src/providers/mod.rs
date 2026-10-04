mod javascript;
mod languages;
pub mod ollama;
mod resources;
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
            self.data.join("fnm"),
            self.home.join("AppData/Local/fnm"),
            self.home.join("AppData/Local/Programs/Ollama"),
        ];
        if !cfg!(windows) {
            candidates.extend([
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
            ]);
        }
        candidates
            .into_iter()
            .map(|dir| dir.join(&file))
            .find(|path| path.is_file())
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
        spec.cwd = Some(self.home.clone());
        spec.env
            .insert("UV_PYTHON_DOWNLOADS".into(), "never".into());
        spec.env.insert("GOTOOLCHAIN".into(), "local".into());
        let output = self.runner.run(&spec, &self.cancel).await?;
        Ok(if output.stdout.trim().is_empty() {
            output.stderr
        } else {
            output.stdout
        })
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
    let mut jobs = tokio::task::JoinSet::new();
    for id in ProviderId::ALL {
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
    Some(Cache { id: id_for("cache", &path), provider, name: name.into(), path, size: Default::default(), strategy: strategy.into(), can_clean, warning: "Cache size is an upper bound; native pruning may retain entries that are still referenced. Offline builds may need to download dependencies again.".into() })
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
        "npm-verify" => ctx.command("npm", &["cache", "verify"]),
        "pnpm-prune" => ctx.command("pnpm", &["store", "prune"]),
        "yarn-clean" => ctx.command("yarn", &["cache", "clean"]),
        "uv-prune" => ctx.command("uv", &["cache", "prune"]),
        "go-build" => ctx.command("go", &["clean", "-cache"]),
        "go-modules" => ctx.command("go", &["clean", "-modcache"]),
        _ => Err(Error::Unavailable(
            "This cache must be managed by its owning tool.".into(),
        )),
    }?;
    let path = cache.path.to_string_lossy().into_owned();
    match cache.strategy.as_str() {
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
    let (action, replacement) = match cache.strategy.as_str() {
        "pnpm-prune" => ("prune", "path"),
        "yarn-clean" => ("clean", "dir"),
        _ => return None,
    };
    let mut probe = command.clone();
    *probe.args.iter_mut().find(|a| *a == action)? = replacement.into();
    probe.timeout = std::time::Duration::from_secs(20);
    Some(probe)
}
