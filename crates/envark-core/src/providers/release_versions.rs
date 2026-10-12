//! Read-only official release metadata shared by update and installer adapters.
use super::*;
use std::time::Duration;

pub(crate) fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("Envark/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(network_error)
}

fn network_error(error: reqwest::Error) -> Error {
    use std::error::Error as _;
    let mut detail = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        detail.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    Error::Unavailable(detail)
}

pub(crate) async fn bytes(
    client: &reqwest::Client,
    url: &str,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    let request = async {
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?;
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(Error::Unavailable(format!(
                "Metadata from {url} exceeds {limit} bytes."
            )));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(network_error)? {
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err(Error::Unavailable(format!(
                    "Metadata from {url} exceeds {limit} bytes."
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    };
    tokio::select! { value = request => value, _ = cancel.cancelled() => Err(Error::Cancelled) }
}

pub(crate) fn github_tag(repo: &str, url: &reqwest::Url) -> Result<String> {
    let prefix = format!("/{repo}/releases/tag/");
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return Err(Error::Unavailable(
            "Official release redirected outside GitHub.".into(),
        ));
    }
    let tag = url
        .path()
        .strip_prefix(&prefix)
        .filter(|tag| !tag.contains('/'))
        .ok_or_else(|| {
            Error::Unavailable("Official release did not resolve to a release tag.".into())
        })?;
    let version = semver::Version::parse(tag.trim_start_matches("bun-").trim_start_matches('v'))
        .map_err(|_| Error::Unavailable("Official release tag is not a version.".into()))?;
    if !version.pre.is_empty() {
        return Err(Error::Unavailable(
            "Official latest release is a prerelease.".into(),
        ));
    }
    Ok(version.to_string())
}

pub(crate) async fn github_latest(repo: &str, cancel: &CancellationToken) -> Result<String> {
    let client = client()?;
    let request = async {
        // GitHub's official latest redirect needs no API token or API rate-limit quota.
        let response = client
            .get(format!("https://github.com/{repo}/releases/latest"))
            .send()
            .await
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?;
        github_tag(repo, response.url())
    };
    let first = tokio::select! { value = request => value, _ = cancel.cancelled() => return Err(Error::Cancelled) };
    match first {
        Ok(version) => Ok(version),
        Err(first) => {
            let fallback = async {
                let url = format!("https://api.github.com/repos/{repo}/releases/latest");
                let bytes = bytes(&client, &url, 2 * 1024 * 1024, cancel).await?;
                let json: serde_json::Value = serde_json::from_slice(&bytes)?;
                if json["draft"] == true || json["prerelease"] == true {
                    return Err(Error::Unavailable(
                        "Official latest release is not stable.".into(),
                    ));
                }
                let tag = json["tag_name"]
                    .as_str()
                    .ok_or_else(|| Error::Unavailable("GitHub release omitted its tag.".into()))?;
                if tag.contains(['/', '?', '#', '%']) {
                    return Err(Error::Unavailable("GitHub release tag is invalid.".into()));
                }
                let url = format!("https://github.com/{repo}/releases/tag/{tag}")
                    .parse()
                    .map_err(|_| Error::Unavailable("GitHub release tag is invalid.".into()))?;
                github_tag(repo, &url)
            }
            .await;
            fallback.map_err(|second| {
                if matches!(second, Error::Cancelled) {
                    return second;
                }
                Error::Unavailable(format!(
                    "Official latest redirect: {first}; GitHub API fallback: {second}"
                ))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_latest_redirects_validate_owner_and_stable_version() {
        for (repo, tag, version) in [
            ("jdx/mise", "v2026.10.8", "2026.10.8"),
            ("astral-sh/uv", "0.13.0", "0.13.0"),
            ("oven-sh/bun", "bun-v1.3.0", "1.3.0"),
            ("Schniz/fnm", "v1.38.1", "1.38.1"),
        ] {
            let url = format!("https://github.com/{repo}/releases/tag/{tag}")
                .parse()
                .unwrap();
            assert_eq!(github_tag(repo, &url).unwrap(), version);
        }
        for url in [
            "https://example.com/jdx/mise/releases/tag/v2026.10.8",
            "https://github.com/other/mise/releases/tag/v2026.10.8",
            "https://github.com/jdx/mise/releases/latest",
            "https://github.com/jdx/mise/releases/tag/v2026.10.8-rc.1",
        ] {
            assert!(github_tag("jdx/mise", &url.parse().unwrap()).is_err());
        }
    }
}
