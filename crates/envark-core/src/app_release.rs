use crate::{Error, Result};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const ENDPOINT: &str = "https://api.github.com/repos/felixpy/envark/releases/latest";
const MAX_RESPONSE_BYTES: usize = 262_144;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRelease {
    pub version: String,
    pub available: bool,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn parse_release(body: &[u8], current: &str) -> Result<AppRelease> {
    let release: Release = serde_json::from_slice(body)?;
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| Error::Unavailable("The release has an invalid version.".into()))?;
    let current = Version::parse(current)
        .map_err(|_| Error::InvalidInput("Invalid application version.".into()))?;
    if release.draft || release.prerelease || !version.pre.is_empty() {
        return Err(Error::Unavailable("No stable release was returned.".into()));
    }
    Ok(AppRelease {
        available: version > current,
        version: version.to_string(),
    })
}

/// Explicit, anonymous checks only. No inventory, paths, or environment details are sent.
pub async fn check(current: &str) -> Result<AppRelease> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("Envark/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    let mut response = client
        .get(ENDPOINT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(Error::Unavailable("Release response is too large.".into()));
        }
        body.extend_from_slice(&chunk);
    }
    parse_release(&body, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions_semantically() {
        let body = br#"{"tag_name":"v0.10.0","draft":false,"prerelease":false}"#;
        assert!(parse_release(body, "0.9.0").unwrap().available);
        assert!(!parse_release(body, "0.10.0").unwrap().available);
        assert!(!parse_release(body, "0.11.0").unwrap().available);
    }

    #[test]
    fn rejects_drafts_prereleases_and_malformed_versions() {
        for body in [
            r#"{"tag_name":"v1.0.0","draft":true,"prerelease":false}"#,
            r#"{"tag_name":"v1.0.0","draft":false,"prerelease":true}"#,
            r#"{"tag_name":"v1.0.0-beta.1","draft":false,"prerelease":false}"#,
            r#"{"tag_name":"latest","draft":false,"prerelease":false}"#,
        ] {
            assert!(parse_release(body.as_bytes(), "0.1.0").is_err());
        }
    }
}
