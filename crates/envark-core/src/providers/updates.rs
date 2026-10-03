use crate::model::Provider;
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
        }
    }
}
