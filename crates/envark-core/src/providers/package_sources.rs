//! Native source resolution shared by package queries and mutations.
use super::*;
use crate::model::{PackageInstallation, PackageUpdateSource};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

mod node;
mod python;
mod rust_go;
pub(crate) use node::pnpm_version;

static SESSION_KEY: LazyLock<String> = LazyLock::new(|| uuid::Uuid::new_v4().to_string());

pub(crate) fn handles(tool: &Tool) -> bool {
    matches!(
        tool.source.as_str(),
        "npm" | "pnpm" | "yarn" | "corepack" | "pnpm-self" | "uv" | "pipx" | "cargo" | "go"
    )
}

pub(crate) fn entry(tool: &Tool) -> &str {
    tool.installation.as_ref().map_or(&tool.name, |i| &i.entry)
}

pub(crate) fn package(tool: &Tool) -> &str {
    tool.installation
        .as_ref()
        .map_or(&tool.name, |i| &i.package)
}

pub(crate) fn sanitized_url(value: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(value) else {
        return value.into();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

pub(crate) fn fingerprint(ctx: &Context, tool: &Tool) -> String {
    // The random process-local salt prevents persistent hashes from disclosing
    // low-entropy credentials and invalidates candidates restored after restart.
    let mut hash = Sha256::new();
    hash.update(SESSION_KEY.as_bytes());
    hash.update(tool.source.as_bytes());
    hash.update(package(tool).as_bytes());
    hash.update(entry(tool).as_bytes());
    let mut paths = vec![ctx.home.join(".netrc"), ctx.home.join("_netrc")];
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(windows) {
                ctx.home.join("AppData/Roaming")
            } else {
                ctx.home.join(".config")
            }
        });
    if tool.source == "uv" {
        paths.extend(python::configuration_paths(ctx));
    }
    match tool.source.as_str() {
        "npm" | "pnpm" | "yarn" | "corepack" | "pnpm-self" => paths.extend([
            ctx.home.join(".npmrc"),
            ctx.home.join(".yarnrc"),
            ctx.home.join(".yarnrc.yml"),
            config.join("pnpm/rc"),
            config.join("pnpm/config.yaml"),
            PathBuf::from("/etc/npmrc"),
            PathBuf::from("/usr/local/etc/npmrc"),
            PathBuf::from("/opt/homebrew/etc/npmrc"),
        ]),
        "uv" | "pipx" => paths.extend([
            config.join("uv/uv.toml"),
            PathBuf::from("/etc/uv/uv.toml"),
            ctx.home.join("AppData/Local/uv/uv.toml"),
            ctx.home.join("AppData/Roaming/uv/uv.toml"),
            ctx.data.join("uv/credentials/credentials.toml"),
            config.join("pip/pip.conf"),
            ctx.home.join(".pip/pip.conf"),
            ctx.home.join("pip/pip.ini"),
            ctx.home.join("AppData/Roaming/pip/pip.ini"),
            ctx.home.join("Library/Application Support/pip/pip.conf"),
            PathBuf::from("/etc/pip.conf"),
        ]),
        "cargo" => {
            for directory in ctx.home.ancestors() {
                paths.extend([
                    directory.join(".cargo/config"),
                    directory.join(".cargo/config.toml"),
                ]);
            }
            paths.extend([
                ctx.home.join(".cargo/.crates2.json"),
                ctx.home.join(".cargo/.crates.toml"),
            ]);
        }
        "go" => paths.extend([
            config.join("go/env"),
            ctx.home.join("Library/Application Support/go/env"),
        ]),
        _ => {}
    }
    let mut environment: Vec<_> = std::env::vars_os().collect();
    environment.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in environment {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if name.starts_with("NPM_CONFIG_")
            || name.starts_with("YARN_")
            || name.starts_with("PNPM_")
            || name.starts_with("UV_")
            || name.starts_with("PIP_")
            || name.starts_with("PIPX_")
            || name.starts_with("CARGO_")
            || matches!(
                name.as_str(),
                "GOPROXY"
                    | "GOPRIVATE"
                    | "GONOPROXY"
                    | "GONOSUMDB"
                    | "GOSUMDB"
                    | "GOENV"
                    | "GOBIN"
                    | "GOPATH"
                    | "PATH"
                    | "HTTP_PROXY"
                    | "HTTPS_PROXY"
                    | "SSL_CERT_FILE"
                    | "XDG_CONFIG_HOME"
                    | "XDG_CONFIG_DIRS"
                    | "APPDATA"
                    | "PROGRAMDATA"
            )
        {
            hash.update(key.as_encoded_bytes());
            hash.update([0]);
            hash.update(value.as_encoded_bytes());
            hash.update([0]);
            if matches!(
                name.as_str(),
                "NPM_CONFIG_USERCONFIG"
                    | "NPM_CONFIG_GLOBALCONFIG"
                    | "PIP_CONFIG_FILE"
                    | "UV_CONFIG_FILE"
                    | "GOENV"
            ) {
                paths.push(value.into());
            } else if name == "CARGO_HOME" {
                paths.extend([
                    PathBuf::from(&value).join("config.toml"),
                    PathBuf::from(&value).join("config"),
                    PathBuf::from(&value).join(".crates2.json"),
                ]);
            } else if name == "XDG_CONFIG_HOME" {
                paths.extend([
                    PathBuf::from(&value).join("uv/uv.toml"),
                    PathBuf::from(&value).join("pip/pip.conf"),
                    PathBuf::from(&value).join("pnpm/rc"),
                    PathBuf::from(&value).join("pnpm/config.yaml"),
                    PathBuf::from(&value).join("go/env"),
                ]);
            }
        }
    }
    if let Some(path) = &tool.path {
        hash.update(path.as_os_str().as_encoded_bytes());
        paths.push(if path.is_dir() {
            path.join("package.json")
        } else {
            path.clone()
        });
    }
    let program = match tool.source.as_str() {
        "pnpm-self" => "pnpm",
        "corepack" => "npm",
        value => value,
    };
    if let Some(program) = ctx.executable(program) {
        hash.update(program.as_os_str().as_encoded_bytes());
        if let Ok(real) = std::fs::canonicalize(&program) {
            hash.update(real.as_os_str().as_encoded_bytes());
        }
        if let Ok(metadata) = std::fs::metadata(&program) {
            hash.update(metadata.len().to_le_bytes());
            hash.update(format!("{:?}", metadata.modified()).as_bytes());
        }
    }
    if let Some(installation) = &tool.installation {
        hash.update(serde_json::to_vec(installation).unwrap_or_default());
        if let Some(root) = &installation.root {
            for name in [
                ".npmrc",
                "etc/npmrc",
                "bin/node",
                "node.exe",
                "lib/node_modules/npm/bin/npm-cli.js",
                "node_modules/npm/bin/npm-cli.js",
                "package.json",
                "pnpm-lock.yaml",
                "node_modules/.modules.yaml",
                "yarn.lock",
                "uv-receipt.toml",
                "pipx_metadata.json",
                ".crates2.json",
            ] {
                paths.push(root.join(name));
            }
        }
    }
    paths.sort();
    paths.dedup();
    for path in paths {
        hash.update(path.as_os_str().as_encoded_bytes());
        if let Ok(metadata) = std::fs::metadata(&path) {
            hash.update(metadata.len().to_le_bytes());
            hash.update(format!("{:?}", metadata.modified()).as_bytes());
        }
        if let Ok(bytes) = read_small(&path, 32 * 1024 * 1024) {
            hash.update(bytes.as_bytes());
            for reference in bytes
                .split("${")
                .skip(1)
                .filter_map(|part| part.split('}').next())
            {
                if let Some(value) = std::env::var_os(reference) {
                    hash.update(reference.as_bytes());
                    hash.update(value.as_encoded_bytes());
                }
            }
        }
    }
    hex::encode(hash.finalize())
}

pub(crate) async fn read(ctx: &Context, mut spec: CommandSpec) -> Result<String> {
    spec.cwd = Some(ctx.home.clone());
    spec.env
        .insert("COREPACK_ENABLE_PROJECT_SPEC".into(), "0".into());
    spec.env
        .insert("COREPACK_ENABLE_AUTO_PIN".into(), "0".into());
    spec.env
        .insert("UV_PYTHON_DOWNLOADS".into(), "never".into());
    spec.env.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
    spec.env.insert("GOTOOLCHAIN".into(), "local".into());
    spec.env.insert("LC_ALL".into(), "C".into());
    let output = ctx.runner.run(&spec, &ctx.cancel).await?;
    Ok(output.stdout)
}

pub(crate) async fn query(ctx: &Context, tool: &Tool) -> Result<(String, PackageUpdateSource)> {
    let before = fingerprint(ctx, tool);
    let (version, label) = match tool.source.as_str() {
        "npm" | "pnpm" | "yarn" | "corepack" | "pnpm-self" => node::query(ctx, tool).await?,
        "uv" | "pipx" => python::query(ctx, tool).await?,
        "cargo" | "go" => rust_go::query(ctx, tool).await?,
        _ => {
            return Err(Error::Unavailable(
                "No native package source is registered.".into(),
            ));
        }
    };
    if fingerprint(ctx, tool) != before {
        return Err(Error::Conflict(
            "The package source configuration changed during the lookup. Check updates again."
                .into(),
        ));
    }
    Ok((
        version,
        PackageUpdateSource {
            label,
            fingerprint: before,
        },
    ))
}

pub(crate) fn validate(ctx: &Context, tool: &Tool) -> Result<()> {
    if let Some(source) = &tool.update_source
        && source.fingerprint != fingerprint(ctx, tool)
    {
        return Err(Error::Conflict(
            "The package source or installation changed. Check updates again before updating."
                .into(),
        ));
    }
    Ok(())
}

pub(crate) use node::{discover_node_evidence, discover_pnpm, npm_spec};
pub(crate) use python::discover_python;
pub(crate) use python::pipx_args;
pub(crate) use rust_go::discover_cargo;
pub(crate) use rust_go::{cargo_command, installed_version};

pub(crate) async fn hydrate(ctx: &Context, provider: &mut Provider) {
    for tool in provider
        .tools
        .iter_mut()
        .chain(&mut provider.package_managers)
    {
        if tool.installation.is_some() || !matches!(tool.source.as_str(), "npm" | "pnpm" | "yarn") {
            continue;
        }
        if let Some(path) = tool.path.clone() {
            let root = path.parent().and_then(|parent| {
                if parent
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('@'))
                {
                    parent.parent()
                } else {
                    Some(parent)
                }
            });
            if let Some(root) =
                root.filter(|root| root.file_name().is_some_and(|name| name == "node_modules"))
            {
                discover_node_evidence(tool, root);
            }
        }
    }
    if provider
        .tools
        .iter()
        .any(|tool| matches!(tool.source.as_str(), "uv" | "pipx") && tool.installation.is_none())
    {
        discover_python(ctx, provider).await;
    }
    if provider
        .tools
        .iter()
        .any(|tool| tool.source == "cargo" && tool.installation.is_none())
    {
        discover_cargo(ctx, provider);
    }
    for tool in &mut provider.tools {
        if tool.source != "go" || tool.installation.is_some() {
            continue;
        }
        if let Some(path) = &tool.path
            && let Ok(spec) = ctx.command("go", &["version", "-m", &path.to_string_lossy()])
            && let Ok(output) = read(ctx, spec).await
            && let Some(module) = output
                .lines()
                .find_map(|line| line.trim().strip_prefix("mod\t"))
                .and_then(|line| line.split_whitespace().next())
        {
            tool.installation = Some(PackageInstallation {
                entry: tool.name.clone(),
                package: tool.name.clone(),
                module: Some(module.into()),
                evidence: Some("Go binary metadata".into()),
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_changes_invalidate_a_reviewed_candidate_without_exposing_credentials() {
        let home = tempfile::tempdir().unwrap();
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = home.path().into();
        let mut tool = basic_tool("@scope/tool", "1.0.0".into(), "npm", None);
        std::fs::write(home.path().join(".npmrc"), "@scope:registry=https://private.invalid/npm/\n//private.invalid/:_authToken=fixture-one").unwrap();
        tool.update_source = Some(PackageUpdateSource {
            label: "https://private.invalid/npm/".into(),
            fingerprint: fingerprint(&ctx, &tool),
        });
        validate(&ctx, &tool).unwrap();
        std::fs::write(
            home.path().join(".npmrc"),
            "@scope:registry=https://other.invalid/npm/",
        )
        .unwrap();
        assert!(validate(&ctx, &tool).is_err());
        assert!(
            !serde_json::to_string(&tool)
                .unwrap()
                .contains("fixture-one")
        );
        assert_eq!(
            sanitized_url("https://user:secret@host.invalid/npm/?token=secret#part"),
            "https://host.invalid/npm/"
        );
    }

    #[tokio::test]
    #[ignore = "isolated native registry fixture; requires ENVARK_SOURCE_TOOL and ENVARK_SOURCE_EXPECTED"]
    async fn native_private_registry_fixture() {
        let path = std::env::var("ENVARK_SOURCE_TOOL").expect("fixture tool");
        let mut tool: Tool = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let (version, source) = query(&ctx, &tool).await.unwrap();
        assert_eq!(
            version.trim_start_matches('v'),
            std::env::var("ENVARK_SOURCE_EXPECTED").unwrap()
        );
        assert!(!source.label.contains("fixture-only"));
        tool.latest = Some(version);
        tool.update_source = Some(source);
        validate(&ctx, &tool).unwrap();
        if std::env::var_os("ENVARK_SOURCE_MUTATE").is_some() {
            tool.update_status = super::super::updates::classify(&tool);
            let id = match tool.source.as_str() {
                "uv" | "pipx" => crate::model::ProviderId::Py,
                "cargo" => crate::model::ProviderId::Rust,
                "go" => crate::model::ProviderId::Go,
                _ => crate::model::ProviderId::Js,
            };
            let mut provider = Provider::empty(id);
            provider.tools.push(tool.clone());
            let inventory = crate::model::Inventory {
                providers: vec![provider],
                ..Default::default()
            };
            let settings = crate::model::Settings::default();
            let plan = crate::operations::prepare(
                crate::operations::ActionRequest::UpdateTool {
                    provider: id,
                    id: tool.id.clone(),
                },
                &inventory,
                &settings,
                &ctx,
            )
            .await
            .unwrap();
            let result = crate::operations::execute(
                plan,
                settings,
                ctx.clone(),
                crate::model::silent_progress(),
                "source-fixture".into(),
                false,
            )
            .await
            .unwrap();
            assert!(!result.items.is_empty());
            assert!(
                result.items.iter().all(|item| item.status == "success"),
                "{result:?}"
            );
            let installed = super::super::updates::installed_version(&ctx, &tool)
                .await
                .unwrap();
            assert_eq!(Some(installed.as_str()), tool.latest.as_deref());
        }
    }
}
