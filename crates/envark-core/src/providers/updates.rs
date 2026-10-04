use crate::model::{Provider, Tool, UpdateStatus};
use crate::{Error, Result};
use std::cmp::Ordering;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn check(providers: &mut [Provider], cancel: &CancellationToken) {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent(concat!("Envark/", env!("CARGO_PKG_VERSION")))
        .build()
    else {
        return;
    };
    let mut requests = vec![];
    for provider in providers.iter() {
        for tool in provider.tools.iter().chain(&provider.package_managers) {
            let (base, suffix, field) = match tool.source.as_str() {
                "npm" | "pnpm" | "yarn" => ("https://registry.npmjs.org", "/latest", "version"),
                "uv" | "pipx" => ("https://pypi.org/pypi", "/json", "info.version"),
                "cargo" => (
                    "https://crates.io/api/v1/crates",
                    "",
                    "crate.max_stable_version",
                ),
                _ => continue,
            };
            if super::valid_identifier(&tool.name).is_err() {
                continue;
            }
            let Ok(mut url) = reqwest::Url::parse(base) else {
                continue;
            };
            if let Ok(mut segments) = url.path_segments_mut() {
                segments.push(&tool.name);
            }
            let url = format!("{}{suffix}", url.as_str().trim_end_matches('/'));
            requests.push((tool.id.clone(), url, field));
        }
    }
    let mut updates = std::collections::HashMap::new();
    for batch in requests.chunks(4) {
        if cancel.is_cancelled() {
            break;
        }
        let mut jobs = tokio::task::JoinSet::new();
        for (id, url, field) in batch {
            let client = client.clone();
            let id = id.clone();
            let url = url.clone();
            let field = *field;
            let cancel = cancel.clone();
            jobs.spawn(async move {
                let request = async {
                    let mut response =
                        client.get(url).send().await.ok()?.error_for_status().ok()?;
                    if response
                        .content_length()
                        .is_some_and(|length| length > 2_097_152)
                    {
                        return None;
                    }
                    let mut bytes = Vec::new();
                    while let Some(chunk) = response.chunk().await.ok()? {
                        if bytes.len().saturating_add(chunk.len()) > 2_097_152 {
                            return None;
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    let data = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?;
                    let value = field.split('.').fold(&data, |value, key| &value[key]);
                    Some((id, value.as_str()?.to_owned()))
                };
                tokio::select! { result = request => result, _ = cancel.cancelled() => None }
            });
        }
        while let Some(result) = jobs.join_next().await {
            if let Ok(Some((id, version))) = result {
                updates.insert(id, version);
            }
        }
    }
    for provider in providers {
        for tool in provider
            .tools
            .iter_mut()
            .chain(&mut provider.package_managers)
        {
            tool.latest = updates.remove(&tool.id);
            tool.update_status = classify(tool);
        }
    }
}

pub fn classify(tool: &Tool) -> UpdateStatus {
    let Some(latest) = &tool.latest else {
        return UpdateStatus::Unknown;
    };
    let (order, major) = match tool.source.as_str() {
        "uv" | "pipx" => {
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
        "npm" | "pnpm" | "yarn" | "cargo" | "go" => {
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
    if !classify(tool).is_upgrade() {
        return Err(Error::Conflict("No verified newer version is available. Enable update checks and refresh; downgrades require the owning tool.".into()));
    }
    Ok(())
}

pub async fn verify_installed(ctx: &super::Context, tool: &Tool) -> Result<()> {
    let version = match tool.source.as_str() {
        "npm" | "pnpm" => {
            let path = tool
                .path
                .as_ref()
                .ok_or_else(|| Error::Conflict("Missing tool installation path.".into()))?;
            let json = crate::filesystem::read_small(&path.join("package.json"), 1_048_576)?;
            let json: serde_json::Value = serde_json::from_str(&json)?;
            json["version"].as_str().map(str::to_owned)
        }
        "uv" | "cargo" => {
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
        "pipx" => {
            let output = ctx.read("pipx", &["list", "--json"]).await?;
            let json: serde_json::Value = serde_json::from_str(&output)?;
            json["venvs"][&tool.name]["metadata"]["main_package"]["package_version"]
                .as_str()
                .map(str::to_owned)
        }
        _ => None,
    };
    if version.as_ref() != Some(&tool.version) {
        return Err(Error::Conflict(
            "The installed tool version changed after review. Refresh before updating.".into(),
        ));
    }
    require_upgrade(tool)
}

#[cfg(test)]
mod tests {
    use super::*;

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
