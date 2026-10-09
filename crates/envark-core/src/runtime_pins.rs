//! Match known project pins to a runtime without scanning unrelated directories.
use crate::model::{Project, ProviderId, Runtime};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDependent {
    pub path: PathBuf,
    pub pins: Vec<String>,
}

// Plain versions are exact (not semver's implicit caret). Partial versions match
// their numeric components. Unresolved aliases are not evidence of a dependency.
fn matches(pin: &str, version: &str) -> bool {
    let pin = pin.trim().trim_start_matches('v');
    let version = version.trim().trim_start_matches('v');
    if pin.is_empty() {
        return false;
    }
    if pin == version {
        return true;
    }
    let parts: Vec<_> = pin.split('.').collect();
    if parts.len() <= 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
    {
        return version.split('.').take(parts.len()).eq(parts);
    }
    false
}

fn matches_runtime(provider: ProviderId, pin: &str, runtime: &Runtime) -> bool {
    if matches(pin, &runtime.version) || runtime.selector.as_deref() == Some(pin) {
        return true;
    }
    if provider == ProviderId::Jvm {
        // SDKMAN identifiers include the distribution; a plain numeric Java pin
        // still applies to the same compiler version from that distribution.
        if let Some((version, _vendor)) = runtime.version.split_once('-') {
            return matches(pin, version);
        }
    }
    // rustup displays a compiler version separately from the channel/host selector.
    provider == ProviderId::Rust
        && runtime.selector.as_deref().is_some_and(|selector| {
            selector.strip_prefix(pin).is_some_and(|suffix| {
                [
                    "-x86_64-",
                    "-aarch64-",
                    "-i686-",
                    "-arm-",
                    "-armv7-",
                    "-riscv64gc-",
                    "-powerpc64le-",
                    "-s390x-",
                ]
                .iter()
                .any(|host| suffix.starts_with(host))
            })
        })
}

fn values(provider: ProviderId, file: &str, content: &str) -> Vec<String> {
    let direct = matches!(
        (provider, file),
        (ProviderId::Js, ".nvmrc" | ".node-version")
            | (ProviderId::Py, ".python-version")
            | (ProviderId::Jvm, ".java-version")
            | (ProviderId::Rust, "rust-toolchain")
    );
    if direct {
        return content
            .lines()
            .filter_map(|line| {
                line.split('#')
                    .next()?
                    .split_whitespace()
                    .next()
                    .map(str::to_owned)
            })
            .collect();
    }
    if file == ".tool-versions" {
        let tool = match provider {
            ProviderId::Js => "nodejs",
            ProviderId::Py => "python",
            ProviderId::Jvm => "java",
            ProviderId::Rust => "rust",
            ProviderId::Go => "golang",
            _ => return vec![],
        };
        return content
            .lines()
            .flat_map(|line| {
                let mut words = line.split('#').next().unwrap_or("").split_whitespace();
                if words.next() == Some(tool) {
                    words.map(str::to_owned).collect()
                } else {
                    vec![]
                }
            })
            .collect();
    }
    if provider == ProviderId::Jvm && file == ".sdkmanrc" {
        return content
            .lines()
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("java=")
                    .map(|v| v.trim().to_owned())
            })
            .collect();
    }
    if provider == ProviderId::Rust && file == "rust-toolchain.toml" {
        return toml::from_str::<toml::Value>(content)
            .ok()
            .and_then(|v| {
                v.get("toolchain")?
                    .get("channel")?
                    .as_str()
                    .map(str::to_owned)
            })
            .into_iter()
            .collect();
    }
    if provider == ProviderId::Go && file == "go.mod" {
        // The `go` directive is a minimum language version, not a runtime pin.
        return content
            .lines()
            .filter_map(|line| line.trim().strip_prefix("toolchain go").map(str::to_owned))
            .collect();
    }
    vec![]
}

pub(crate) fn dependents(
    projects: &[Project],
    provider: ProviderId,
    runtime: &Runtime,
) -> Vec<RuntimeDependent> {
    projects
        .iter()
        .filter_map(|project| {
            let pins: Vec<_> = project
                .pins
                .iter()
                .flat_map(|(file, content)| {
                    values(provider, file, content)
                        .into_iter()
                        .filter(|pin| matches_runtime(provider, pin, runtime))
                        .map(move |pin| format!("{file}: {pin}"))
                })
                .collect();
            (!pins.is_empty()).then(|| RuntimeDependent {
                path: project.path.clone(),
                pins,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_partial_and_unrelated_pins_do_not_become_blanket_warnings() {
        for pin in ["16", "16.20", "16.20.2", "v16.20.2"] {
            assert!(matches(pin, "16.20.2"), "{pin}");
        }
        for pin in ["", "1", "20", "16.20.1", "16.2", "lts/*", "system", "node"] {
            assert!(!matches(pin, "16.20.2"), "{pin}");
        }
        assert!(values(ProviderId::Js, ".python-version", "16.20.2").is_empty());
        assert!(values(ProviderId::Js, "go.mod", "go 16.20.2").is_empty());
        assert_eq!(
            values(
                ProviderId::Js,
                ".tool-versions",
                "python 3.12\nnodejs 20 16.20.2 # comment"
            ),
            vec!["20", "16.20.2"]
        );
        assert_eq!(
            values(ProviderId::Js, ".nvmrc", "v16.20.2 # comment"),
            vec!["v16.20.2"]
        );
        assert_eq!(
            values(
                ProviderId::Rust,
                "rust-toolchain.toml",
                "[toolchain]\nchannel = '1.85.0'"
            ),
            vec!["1.85.0"]
        );
        assert!(values(ProviderId::Go, "go.mod", "go 1.24.0").is_empty());
    }
    #[test]
    fn each_ecosystem_matches_only_its_own_version_and_rust_uses_channels() {
        let project = Project {
            id: "mixed".into(),
            name: "mixed".into(),
            path: "/projects/mixed".into(),
            providers: vec![
                ProviderId::Js,
                ProviderId::Py,
                ProviderId::Jvm,
                ProviderId::Rust,
                ProviderId::Go,
            ],
            last_active: None,
            activity_complete: true,
            branch: None,
            protected: false,
            artifacts: vec![],
            repository: None,
            is_worktree: false,
            pins: [
                (".node-version".into(), "20".into()),
                (".python-version".into(), "3.12.1".into()),
                (".sdkmanrc".into(), "java=21.0.2-tem".into()),
                (
                    "rust-toolchain.toml".into(),
                    "[toolchain]\nchannel='stable'".into(),
                ),
                (
                    "go.mod".into(),
                    "module example\ngo 1.24\ntoolchain go1.24.3".into(),
                ),
            ]
            .into(),
        };
        for (provider, version, file) in [
            (ProviderId::Js, "20.1.0", ".node-version"),
            (ProviderId::Py, "3.12.1", ".python-version"),
            (ProviderId::Jvm, "21.0.2-tem", ".sdkmanrc"),
            (ProviderId::Rust, "1.90.0", "rust-toolchain.toml"),
            (ProviderId::Go, "1.24.3", "go.mod"),
        ] {
            let mut runtime = Runtime {
                id: "runtime".into(),
                version: version.into(),
                manager: "manager".into(),
                path: "/runtime".into(),
                selector: (provider == ProviderId::Rust)
                    .then(|| "stable-aarch64-apple-darwin".into()),
                active: false,
                active_known: true,
                managed: true,
                size: None,
                note: None,
            };
            let found = dependents(std::slice::from_ref(&project), provider, &runtime);
            assert_eq!(found.len(), 1, "{provider:?}");
            assert_eq!(found[0].pins.len(), 1);
            assert!(found[0].pins[0].starts_with(file));
            runtime.version = "16.20.2".into();
            runtime.selector = None;
            assert!(dependents(std::slice::from_ref(&project), provider, &runtime).is_empty());
        }
    }
}
