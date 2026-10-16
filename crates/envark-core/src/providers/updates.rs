use crate::model::{Provider, Tool, UpdateIssue, UpdateStatus};
use crate::{Error, Result};
use std::cmp::Ordering;
use tokio_util::sync::CancellationToken;

const REGISTRY_LIMIT: usize = 2 * 1024 * 1024;
// PyPI's JSON endpoint contains historical files as well as the current version.
// Projects with platform wheels (uv, ruff, etc.) legitimately exceed 2 MiB.
const PYPI_LIMIT: usize = 16 * 1024 * 1024;

fn registry(tool: &Tool) -> Option<(String, &'static str, usize)> {
    let (base, suffix, field, limit) = match tool.source.as_str() {
        "npm" | "pnpm" | "yarn" | "bun" | "corepack" | "pnpm-self" => (
            "https://registry.npmjs.org",
            "/latest",
            "version",
            REGISTRY_LIMIT,
        ),
        "uv" | "pipx" | "uv-self" => ("https://pypi.org/pypi", "/json", "info.version", PYPI_LIMIT),
        "cargo" => (
            "https://crates.io/api/v1/crates",
            "",
            "crate.max_stable_version",
            REGISTRY_LIMIT,
        ),
        _ => return None,
    };
    super::valid_identifier(&tool.name).ok()?;
    let mut url = reqwest::Url::parse(base).ok()?;
    let name = if tool.source == "corepack"
        && tool.name == "yarn"
        && semver::Version::parse(&tool.version).is_ok_and(|v| v.major >= 2)
    {
        "@yarnpkg/cli-dist"
    } else {
        &tool.name
    };
    url.path_segments_mut().ok()?.push(name);
    Some((
        format!("{}{suffix}", url.as_str().trim_end_matches('/')),
        field,
        limit,
    ))
}

async fn registry_version(
    client: &reqwest::Client,
    url: &str,
    field: &str,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<String> {
    let bytes = super::release_versions::bytes(client, url, limit, cancel).await?;
    let data: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| Error::Unavailable(format!("Invalid metadata from {url}: {error}")))?;
    let value = field.split('.').fold(&data, |value, key| &value[key]);
    value
        .as_str()
        .filter(|version| !version.trim().is_empty())
        .map(|version| version.trim_start_matches('v').to_owned())
        .ok_or_else(|| Error::Unavailable(format!("Metadata from {url} omitted {field}.")))
}

fn apply_result(tool: &mut Tool, result: Result<String>) {
    match result {
        Ok(version) => {
            tool.latest = Some(version);
            tool.update_status = classify(tool);
            tool.update_issue =
                (tool.update_status == UpdateStatus::Unknown).then(|| UpdateIssue {
                    reason: "invalidVersion".into(),
                    detail: "The installed and published versions could not be compared.".into(),
                });
        }
        Err(error) => {
            // Keep the last successful lookup visible; never relabel it as fresh.
            tool.update_source = None;
            tool.update_issue = Some(UpdateIssue {
                reason: "queryFailed".into(),
                detail: error.to_string(),
            });
        }
    }
}

fn apply_results(
    providers: &mut [Provider],
    results: &std::collections::HashMap<String, std::result::Result<String, String>>,
) {
    for tool in providers
        .iter_mut()
        .flat_map(|p| p.tools.iter_mut().chain(&mut p.package_managers))
    {
        if let Some(result) = results.get(&tool.id) {
            apply_result(tool, result.clone().map_err(Error::Unavailable));
        }
    }
}

pub async fn check(providers: &mut [Provider], cancel: &CancellationToken) {
    let client = match super::release_versions::client() {
        Ok(client) => client,
        Err(error) => {
            for tool in providers
                .iter_mut()
                .flat_map(|p| p.tools.iter_mut().chain(&mut p.package_managers))
            {
                apply_result(tool, Err(Error::Unavailable(error.to_string())));
            }
            return;
        }
    };
    let ctx = match super::Context::new(cancel.clone()) {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let mut requests = vec![];
    let mut requested = std::collections::HashSet::new();
    for tool in providers
        .iter_mut()
        .flat_map(|p| p.tools.iter_mut().chain(&mut p.package_managers))
    {
        if tool.name == "cargo" && tool.source == "rustup" {
            tool.update_issue = Some(UpdateIssue {
                reason: "runtimeManaged".into(),
                detail: "Cargo is supplied by the selected Rust toolchain and updates with it."
                    .into(),
            });
        } else if super::native_lifecycle::owns_tool(tool)
            || super::package_sources::handles(tool)
            || registry(tool).is_some()
            || matches!(
                tool.source.as_str(),
                "fnm-script" | "nvm-script" | "homebrew"
            )
            || (tool.source == "winget" && tool.name == "fnm")
        {
            if requested.insert(tool.id.clone()) {
                requests.push(tool.clone());
            }
        } else {
            tool.update_issue = Some(UpdateIssue {
                reason: "unsupported".into(),
                detail: tool.note.clone().unwrap_or_else(|| {
                    "No verified update source is registered for this installation.".into()
                }),
            });
        }
    }
    let mut results = std::collections::HashMap::new();
    let mut sources = std::collections::HashMap::new();
    for batch in requests.chunks(4) {
        if cancel.is_cancelled() {
            return;
        }
        let mut jobs = tokio::task::JoinSet::new();
        for tool in batch {
            let ctx = ctx.clone();
            let client = client.clone();
            let tool = tool.clone();
            jobs.spawn(async move {
                let mut source = None;
                let result = if super::native_lifecycle::owns_tool(&tool) {
                    super::native_lifecycle::latest(&ctx, &tool).await
                } else if super::package_sources::handles(&tool) {
                    match super::package_sources::query(&ctx, &tool).await {
                        Ok((version, resolved)) => {
                            source = Some(resolved);
                            Ok(version)
                        }
                        Err(error) => Err(error),
                    }
                } else if matches!(tool.source.as_str(), "fnm-script" | "nvm-script" | "winget") {
                    super::release_versions::github_latest(
                        if tool.name == "fnm" {
                            "Schniz/fnm"
                        } else {
                            "nvm-sh/nvm"
                        },
                        &ctx.cancel,
                    )
                    .await
                } else if tool.source == "homebrew" {
                    async {
                        let path = tool.path.as_deref().ok_or_else(|| {
                            Error::Unavailable("Homebrew tool path is missing.".into())
                        })?;
                        let formula = super::package_managers::brew_formula(&ctx, path)
                            .await
                            .ok_or_else(|| {
                                Error::Conflict("Homebrew installation ownership changed.".into())
                            })?;
                        let text = ctx.read("brew", &["info", "--json=v2", &formula]).await?;
                        let json: serde_json::Value = serde_json::from_str(&text)?;
                        if json["formulae"][0]["pinned"].as_bool() == Some(true) {
                            return Err(Error::Unavailable(
                                "The Homebrew formula is pinned.".into(),
                            ));
                        }
                        json["formulae"][0]["versions"]["stable"]
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| {
                                Error::Unavailable(
                                    "Homebrew did not report a stable version.".into(),
                                )
                            })
                    }
                    .await
                } else {
                    let (url, field, limit) = registry(&tool).expect("registered update source");
                    registry_version(&client, &url, field, limit, &ctx.cancel).await
                };
                (tool.id, result, source)
            });
        }
        while let Some(result) = jobs.join_next().await {
            if let Ok((id, result, source)) = result {
                if let Some(source) = source {
                    sources.insert(id.clone(), source);
                }
                results.insert(id, result.map_err(|error| error.to_string()));
            }
        }
    }
    if cancel.is_cancelled() {
        return;
    }
    apply_results(providers, &results);
    for tool in providers
        .iter_mut()
        .flat_map(|p| p.tools.iter_mut().chain(&mut p.package_managers))
    {
        if let Some(source) = sources.remove(&tool.id) {
            tool.update_source = Some(source);
        }
    }
}

pub fn classify(tool: &Tool) -> UpdateStatus {
    let Some(latest) = &tool.latest else {
        return UpdateStatus::Unknown;
    };
    let source = if super::native_lifecycle::owns_tool(tool) {
        "npm"
    } else {
        &tool.source
    };
    let (order, major) = match source {
        "uv" | "pipx" | "uv-self" | "homebrew" => {
            let (Ok(installed), Ok(available)) = (
                tool.version.parse::<pep440_rs::Version>(),
                latest.parse::<pep440_rs::Version>(),
            ) else {
                return UpdateStatus::Unknown;
            };
            (
                available.cmp(&installed),
                available.epoch() > installed.epoch()
                    || available.release()[0] > installed.release()[0],
            )
        }
        "npm" | "pnpm" | "yarn" | "cargo" | "go" | "bun" | "corepack" | "pnpm-self"
        | "fnm-script" | "nvm-script" | "winget" => {
            let (Ok(installed), Ok(available)) = (
                semver::Version::parse(tool.version.trim_start_matches('v')),
                semver::Version::parse(latest.trim_start_matches('v')),
            ) else {
                return UpdateStatus::Unknown;
            };
            (
                available.cmp_precedence(&installed),
                available.major > installed.major,
            )
        }
        _ => return UpdateStatus::Unknown,
    };
    match order {
        Ordering::Less => UpdateStatus::Ahead,
        Ordering::Equal => UpdateStatus::Latest,
        Ordering::Greater if major => UpdateStatus::Major,
        Ordering::Greater => UpdateStatus::Minor,
    }
}

pub fn require_upgrade(tool: &Tool) -> Result<()> {
    if let Some(issue) = &tool.update_issue {
        return Err(Error::Conflict(format!(
            "Resolve the version lookup before updating: {}",
            issue.detail
        )));
    }
    if !classify(tool).is_upgrade() {
        return Err(Error::Conflict("No verified newer version is available. Enable update checks and refresh; downgrades require the owning tool.".into()));
    }
    Ok(())
}

pub async fn verify_installed(ctx: &super::Context, tool: &Tool) -> Result<()> {
    super::package_sources::validate(ctx, tool)?;
    if super::package_managers::handles(tool) {
        super::package_managers::validate_owner(ctx, tool).await?;
    }
    if installed_version(ctx, tool).await? != tool.version {
        return Err(Error::Conflict(
            "The installed tool version changed after review. Refresh before updating.".into(),
        ));
    }
    require_upgrade(tool)
}

pub(crate) async fn installed_version(ctx: &super::Context, tool: &Tool) -> Result<String> {
    if super::package_managers::handles(tool) {
        return super::package_managers::installed_version(ctx, tool).await;
    }
    let version = match tool.source.as_str() {
        "pnpm" if tool.installation.is_some() => {
            super::package_sources::pnpm_version(ctx, tool).await?
        }
        "npm" | "pnpm" | "yarn" => {
            let path = tool
                .path
                .as_ref()
                .ok_or_else(|| Error::Conflict("Missing tool installation path.".into()))?;
            let json = crate::filesystem::read_small(&path.join("package.json"), 1_048_576)?;
            let json: serde_json::Value = serde_json::from_str(&json)?;
            if json["name"]
                .as_str()
                .is_some_and(|name| name != super::package_sources::package(tool))
            {
                return Err(Error::Conflict(
                    "The installed package identity changed.".into(),
                ));
            }
            json["version"].as_str().map(str::to_owned)
        }
        "uv" => {
            let (program, args): (&str, &[&str]) = if tool.source == "uv" {
                ("uv", &["tool", "list"])
            } else {
                ("cargo", &["install", "--list"])
            };
            let output = ctx.read(program, args).await?;
            output
                .lines()
                .filter(|line| !line.starts_with([' ', '-']))
                .find_map(|line| {
                    let mut fields = line.split_whitespace();
                    if fields.next()? != tool.name {
                        return None;
                    }
                    Some(
                        fields
                            .next()?
                            .trim_start_matches('v')
                            .trim_end_matches(':')
                            .to_owned(),
                    )
                })
        }
        "cargo" | "go" => super::package_sources::installed_version(ctx, tool).await?,
        "pipx" => {
            let output = ctx.read("pipx", &["list", "--json"]).await?;
            let json: serde_json::Value = serde_json::from_str(&output)?;
            json["venvs"][&tool.name]["metadata"]["main_package"]["package_version"]
                .as_str()
                .map(str::to_owned)
        }
        _ => None,
    };
    version
        .ok_or_else(|| Error::Conflict("The installed tool version could not be verified.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_lookups_preserve_prior_versions_and_recovery_clears_the_error() {
        let mut tool = super::super::basic_tool("uv", "0.10.10".into(), "uv-self", None);
        apply_result(&mut tool, Ok("0.12.0".into()));
        assert_eq!(tool.update_status, UpdateStatus::Minor);
        tool.update_source = Some(crate::model::PackageUpdateSource {
            label: "https://old.invalid/".into(),
            fingerprint: "old".into(),
        });
        apply_result(
            &mut tool,
            Err(Error::Unavailable(
                "HTTP 403 from the official source".into(),
            )),
        );
        assert_eq!(tool.latest.as_deref(), Some("0.12.0"));
        assert_eq!(tool.update_issue.as_ref().unwrap().reason, "queryFailed");
        assert!(tool.update_source.is_none());
        assert!(require_upgrade(&tool).is_err());
        apply_result(&mut tool, Ok("0.13.0".into()));
        assert!(tool.update_issue.is_none());
        require_upgrade(&tool).unwrap();
    }

    #[test]
    fn a_shared_mise_installation_updates_both_go_and_java_views() {
        let shared = super::super::basic_tool("mise", "2026.9.8".into(), "mise-self", None);
        let mut providers: Vec<_> = [crate::model::ProviderId::Go, crate::model::ProviderId::Jvm]
            .into_iter()
            .map(|id| {
                let mut p = Provider::empty(id);
                p.package_managers.push(shared.clone());
                p
            })
            .collect();
        let mut results =
            std::collections::HashMap::from([(shared.id.clone(), Ok("2026.10.8".into()))]);
        apply_results(&mut providers, &results);
        assert!(
            providers
                .iter()
                .all(|p| p.package_managers[0].latest.as_deref() == Some("2026.10.8"))
        );
        results.insert(shared.id, Err("HTTP 403".into()));
        apply_results(&mut providers, &results);
        assert!(
            providers.iter().all(
                |p| p.package_managers[0].update_issue.as_ref().unwrap().reason == "queryFailed"
            )
        );
    }

    #[tokio::test]
    async fn registry_metadata_handles_large_pypi_projects_and_reports_http_failures() {
        use std::io::{Read, Write};
        let body = serde_json::json!({"info":{"version":"0.13.0"},"releases":{"history":"x".repeat(5 * 1024 * 1024)}}).to_string();
        let uv = super::super::basic_tool("uv", "0.10.10".into(), "uv-self", None);
        let (_, field, limit) = registry(&uv).unwrap();
        assert!(body.len() > REGISTRY_LIMIT && body.len() < limit);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/metadata", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (status, body) in [("200 OK", body), ("403 Forbidden", "denied".into())] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = [0u8; 4096];
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                    assert!(request.len() <= 8192);
                }
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let cancel = CancellationToken::new();
        assert_eq!(
            registry_version(&client, &url, field, limit, &cancel)
                .await
                .unwrap(),
            "0.13.0"
        );
        let error = registry_version(&client, &url, field, limit, &cancel)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("403"), "{error}");
        server.join().unwrap();
        let cargo = super::super::basic_tool("cargo", "1.98.1".into(), "rustup", None);
        assert!(registry(&cargo).is_none());
    }

    #[tokio::test]
    async fn cargo_and_unregistered_tools_do_not_pretend_to_have_network_failures() {
        let mut provider = Provider::empty(crate::model::ProviderId::Rust);
        provider.package_managers = vec![
            super::super::basic_tool("cargo", "1.98.1".into(), "rustup", None),
            super::super::basic_tool("custom", "1.0.0".into(), "external", None),
        ];
        check(
            std::slice::from_mut(&mut provider),
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(
            provider.package_managers[0]
                .update_issue
                .as_ref()
                .unwrap()
                .reason,
            "runtimeManaged"
        );
        assert_eq!(
            provider.package_managers[1]
                .update_issue
                .as_ref()
                .unwrap()
                .reason,
            "unsupported"
        );
    }

    #[tokio::test]
    #[ignore = "Explicit read-only verification against official public release sources"]
    async fn live_official_release_sources() {
        let cancel = CancellationToken::new();
        let client = super::super::release_versions::client().unwrap();
        let uv = registry(&super::super::basic_tool(
            "uv",
            "0.10.10".into(),
            "uv-self",
            None,
        ))
        .unwrap();
        println!(
            "uv: {}",
            registry_version(&client, &uv.0, uv.1, uv.2, &cancel)
                .await
                .unwrap()
        );
        for repo in [
            "jdx/mise",
            "Schniz/fnm",
            "nvm-sh/nvm",
            "oven-sh/bun",
            "ollama/ollama",
        ] {
            println!(
                "{repo}: {}",
                super::super::release_versions::github_latest(repo, &cancel)
                    .await
                    .unwrap()
            );
        }
        let bytes = super::super::release_versions::bytes(
            &client,
            "https://static.rust-lang.org/rustup/release-stable.toml",
            REGISTRY_LIMIT,
            &cancel,
        )
        .await
        .unwrap();
        let version = super::super::language_lifecycle::rustup_release_version(
            std::str::from_utf8(&bytes).unwrap(),
        )
        .unwrap();
        println!("rustup: {version}");
    }

    #[test]
    fn rustup_official_document_is_parsed_as_a_document_not_a_single_toml_value() {
        let manifest = "schema-version = '1'\nversion = '1.29.1'\n";
        assert!(manifest.parse::<toml::Value>().is_err());
        assert_eq!(
            super::super::language_lifecycle::rustup_release_version(manifest).unwrap(),
            "1.29.1"
        );
        for malformed in [
            "version = []",
            "schema-version = '1'",
            "version = 'not-a-version'",
        ] {
            assert!(super::super::language_lifecycle::rustup_release_version(malformed).is_err());
        }
    }

    #[tokio::test]
    async fn an_external_upgrade_invalidates_the_reviewed_update() {
        let root = tempfile::tempdir().unwrap();
        let mut tool =
            super::super::basic_tool("fixture", "2.3.0".into(), "npm", Some(root.path().into()));
        tool.latest = Some("2.4.0".into());
        let ctx = super::super::Context::new(CancellationToken::new()).unwrap();
        std::fs::write(root.path().join("package.json"), r#"{"version":"2.5.0"}"#).unwrap();
        assert!(verify_installed(&ctx, &tool).await.is_err());
        std::fs::write(root.path().join("package.json"), r#"{"version":"2.3.0"}"#).unwrap();
        verify_installed(&ctx, &tool).await.unwrap();
    }

    #[test]
    fn version_ordering_respects_each_package_ecosystem() {
        for (source, installed, latest, expected) in [
            ("bun", "1.3.0", "1.4.0", UpdateStatus::Minor),
            ("uv-self", "0.10.0", "0.11.0", UpdateStatus::Minor),
            ("homebrew", "8.10", "8.11", UpdateStatus::Minor),
            ("npm", "2.3.0", "2.2.0", UpdateStatus::Ahead),
            ("npm", "10.0.0", "9.0.0", UpdateStatus::Ahead),
            (
                "cargo",
                "2.0.0+build.2",
                "2.0.0+build.1",
                UpdateStatus::Latest,
            ),
            ("npm", "2.0.0-beta.2", "2.0.0-beta.11", UpdateStatus::Minor),
            ("npm", "2.0.0", "2.0.0-rc.1", UpdateStatus::Ahead),
            ("npm", "2.0.0", "3.0.0", UpdateStatus::Major),
            ("uv", "2.0rc1", "2.0", UpdateStatus::Minor),
            ("pipx", "2.0.post1", "2.0", UpdateStatus::Ahead),
            ("uv", "1!1.0", "9.0", UpdateStatus::Ahead),
            ("uv", "2.0+local", "2.0", UpdateStatus::Ahead),
            ("pipx", "2.0", "2.0.0", UpdateStatus::Latest),
            ("npm", "git-main", "2.0.0", UpdateStatus::Unknown),
        ] {
            let mut tool = super::super::basic_tool("fixture", installed.into(), source, None);
            tool.latest = Some(latest.into());
            assert_eq!(
                classify(&tool),
                expected,
                "{source}: {installed} -> {latest}"
            );
            assert_eq!(require_upgrade(&tool).is_ok(), expected.is_upgrade());
        }
    }
}
