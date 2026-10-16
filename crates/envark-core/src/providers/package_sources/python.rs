use super::*;
use std::io::Write;

fn interpreter(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    })
}

pub(crate) async fn discover_python(ctx: &Context, provider: &mut Provider) {
    if let Ok(output) = ctx.read("uv", &["tool", "dir"]).await {
        let root = PathBuf::from(output.trim());
        for tool in &mut provider.tools {
            if tool.source != "uv" {
                continue;
            }
            let environment = root.join(&tool.name);
            if !environment.join("uv-receipt.toml").is_file() {
                continue;
            }
            let mut origin = None;
            if let Ok(text) = read_small(&environment.join("uv-receipt.toml"), 1_048_576)
                && let Ok(receipt) = toml::from_str::<toml::Value>(&text)
            {
                origin = receipt
                    .get("tool")
                    .and_then(|t| t.get("options"))
                    .and_then(|t| t.get("index-url"))
                    .and_then(toml::Value::as_str)
                    .map(sanitized_url);
                if origin.is_none() {
                    origin = receipt
                        .get("tool")
                        .and_then(|t| t.get("options"))
                        .and_then(|t| t.get("index"))
                        .and_then(toml::Value::as_array)
                        .and_then(|indexes| indexes.iter().find_map(|i| i.get("url")?.as_str()))
                        .map(sanitized_url);
                }
            }
            tool.installation = Some(PackageInstallation {
                entry: tool.name.clone(),
                package: tool.name.clone(),
                root: Some(environment),
                origin,
                evidence: Some("uv receipt".into()),
                ..Default::default()
            });
        }
    }
    if let Ok(output) = ctx.read("pipx", &["list", "--json"]).await
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&output)
        && let Some(venvs) = value["venvs"].as_object()
    {
        let root = ctx
            .read("pipx", &["environment", "--value", "PIPX_HOME"])
            .await
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| PathBuf::from(value.trim()))
            .or_else(|| std::env::var_os("PIPX_HOME").map(PathBuf::from))
            .unwrap_or_else(|| ctx.data.join("pipx"));
        for tool in &mut provider.tools {
            if tool.source != "pipx" {
                continue;
            }
            let Some(record) = venvs.get(&tool.name) else {
                continue;
            };
            let package = record["metadata"]["main_package"]["package"]
                .as_str()
                .unwrap_or(&tool.name);
            let environment = record["metadata"]["venv_dir"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("venvs").join(&tool.name));
            tool.installation = Some(PackageInstallation {
                entry: tool.name.clone(),
                package: package.into(),
                root: Some(environment),
                evidence: Some("pipx receipt".into()),
                ..Default::default()
            });
        }
    }
}

fn merge(low: &mut toml::Value, high: toml::Value) {
    if let (Some(low), Some(high)) = (low.as_table_mut(), high.as_table()) {
        for (key, value) in high {
            if let Some(existing) = low.get_mut(key) {
                merge(existing, value.clone());
            } else {
                low.insert(key.clone(), value.clone());
            }
        }
    } else if let (Some(low), Some(high)) = (low.as_array_mut(), high.as_array()) {
        let mut combined = high.clone();
        combined.append(low);
        *low = combined;
    } else {
        *low = high;
    }
}

pub(super) fn configuration_paths(ctx: &Context) -> Vec<PathBuf> {
    let user = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(windows) {
                ctx.home.join("AppData/Roaming")
            } else {
                ctx.home.join(".config")
            }
        })
        .join("uv/uv.toml");
    let explicit = std::env::var_os("UV_CONFIG_FILE").map(PathBuf::from);
    if let Some(path) = explicit {
        vec![path]
    } else {
        let system = if cfg!(windows) {
            std::env::var_os("PROGRAMDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("C:/ProgramData"))
                .join("uv/uv.toml")
        } else {
            std::env::var_os("XDG_CONFIG_DIRS")
                .as_deref()
                .map(std::env::split_paths)
                .into_iter()
                .flatten()
                .map(|directory| directory.join("uv/uv.toml"))
                .find(|path| path.is_file())
                .unwrap_or_else(|| PathBuf::from("/etc/uv/uv.toml"))
        };
        vec![system, user]
    }
}

fn uv_configuration(ctx: &Context, receipt: &toml::Value) -> Result<toml::Value> {
    let mut config = toml::Value::Table(Default::default());
    for path in configuration_paths(ctx) {
        if path.is_file() {
            merge(
                &mut config,
                toml::from_str(&read_small(&path, 1_048_576)?)
                    .map_err(|_| Error::Unavailable("Invalid uv source configuration.".into()))?,
            );
        }
    }
    // Tool operations do not use uv's pip-only configuration section.
    if let Some(table) = config.as_table_mut() {
        table.remove("pip");
    }
    if let Some(options) = receipt.get("tool").and_then(|t| t.get("options")) {
        merge(&mut config, options.clone());
    }
    Ok(config)
}

fn requirement(value: &toml::Value) -> Result<String> {
    if let Some(text) = value.as_str() {
        return Ok(text.into());
    }
    let name = value
        .get("name")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| {
            Error::Unavailable("The Python tool receipt has no package requirement.".into())
        })?;
    if value.get("url").is_some() || value.get("git").is_some() || value.get("path").is_some() {
        return Err(Error::Unavailable("This tool uses a Git, URL, or local source; a registry version cannot replace its install specification.".into()));
    }
    let extras = value
        .get("extras")
        .and_then(toml::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(toml::Value::as_str)
                .collect::<Vec<_>>()
                .join(",")
        })
        .filter(|s| !s.is_empty())
        .map(|s| format!("[{s}]"))
        .unwrap_or_default();
    Ok(format!(
        "{name}{extras}{}",
        value
            .get("specifier")
            .and_then(toml::Value::as_str)
            .unwrap_or("")
    ))
}

fn source_args(record: &serde_json::Value) -> Vec<String> {
    // Only transfer source/transport options into read-only pip queries; never
    // replay installation destinations or build hooks in a lookup command.
    let args = record["pip_args"].as_array().cloned().unwrap_or_default();
    let mut result = vec![];
    let mut i = 0;
    while i < args.len() {
        let Some(arg) = args[i].as_str() else {
            i += 1;
            continue;
        };
        let key = arg.split('=').next().unwrap_or(arg);
        if [
            "--index-url",
            "-i",
            "--extra-index-url",
            "--find-links",
            "-f",
            "--trusted-host",
            "--cert",
            "--client-cert",
            "--proxy",
            "--timeout",
            "--retries",
            "--keyring-provider",
            "--constraint",
            "-c",
            "--only-binary",
            "--no-binary",
        ]
        .contains(&key)
        {
            result.push(arg.into());
            if !arg.contains('=')
                && let Some(value) = args.get(i + 1).and_then(serde_json::Value::as_str)
            {
                result.push(value.into());
                i += 1;
            }
        } else if ["--no-index", "--pre", "--prefer-binary"].contains(&key) {
            result.push(arg.into());
        }
        i += 1;
    }
    result
}

pub(super) async fn query(ctx: &Context, tool: &Tool) -> Result<(String, String)> {
    let root = tool
        .installation
        .as_ref()
        .and_then(|i| i.root.as_ref())
        .ok_or_else(|| {
            Error::Unavailable(
                "Refresh this tool's installation metadata before checking its source.".into(),
            )
        })?;
    if tool.source == "uv" {
        let receipt: toml::Value =
            toml::from_str(&read_small(&root.join("uv-receipt.toml"), 1_048_576)?)
                .map_err(|_| Error::Unavailable("Invalid uv installation receipt.".into()))?;
        let requirements_list = receipt
            .get("tool")
            .and_then(|t| t.get("requirements"))
            .and_then(toml::Value::as_array)
            .ok_or_else(|| {
                Error::Unavailable("The uv receipt has no install requirement.".into())
            })?;
        let mut requirements = tempfile::NamedTempFile::new()?;
        for req in requirements_list {
            writeln!(requirements, "{}", requirement(req)?)?;
        }
        let mut config = tempfile::Builder::new().suffix(".toml").tempfile()?;
        write!(
            config,
            "{}",
            toml::to_string(&uv_configuration(ctx, &receipt)?).map_err(|_| Error::Unavailable(
                "Cannot prepare uv source configuration.".into()
            ))?
        )?;
        let python = interpreter(root);
        let mut args = vec![
            "pip".into(),
            "compile".into(),
            requirements.path().to_string_lossy().into_owned(),
            "--no-build".into(),
            "--python".into(),
            python.to_string_lossy().into_owned(),
            "--emit-index-annotation".into(),
            "--config-file".into(),
            config.path().to_string_lossy().into_owned(),
        ];
        let mut constraints = tempfile::NamedTempFile::new()?;
        if let Some(values) = receipt
            .get("tool")
            .and_then(|t| t.get("constraints"))
            .and_then(toml::Value::as_array)
        {
            for value in values {
                writeln!(constraints, "{}", requirement(value)?)?;
            }
            if !values.is_empty() {
                args.extend([
                    "--constraint".into(),
                    constraints.path().to_string_lossy().into_owned(),
                ]);
            }
        }
        let mut overrides = tempfile::NamedTempFile::new()?;
        if let Some(values) = receipt
            .get("tool")
            .and_then(|t| t.get("overrides"))
            .and_then(toml::Value::as_array)
        {
            for value in values {
                writeln!(overrides, "{}", requirement(value)?)?;
            }
            if !values.is_empty() {
                args.extend([
                    "--override".into(),
                    overrides.path().to_string_lossy().into_owned(),
                ]);
            }
        }
        let spec = ctx.command("uv", &args.iter().map(String::as_str).collect::<Vec<_>>())?;
        let output = read(ctx, spec).await?;
        let prefix = format!("{}==", package(tool));
        let version = output
            .lines()
            .find_map(|line| line.trim().strip_prefix(&prefix))
            .and_then(|v| v.split_whitespace().next())
            .ok_or_else(|| {
                Error::Unavailable("The resolver did not return a compatible tool version.".into())
            })?;
        let label = output
            .lines()
            .find_map(|line| line.trim().strip_prefix("# from "))
            .map(sanitized_url)
            .or_else(|| tool.installation.as_ref().and_then(|i| i.origin.clone()))
            .unwrap_or_else(|| "uv indexes".into());
        return Ok((version.into(), label));
    }
    let metadata: serde_json::Value =
        serde_json::from_str(&read_small(&root.join("pipx_metadata.json"), 1_048_576)?)?;
    let record = &metadata["main_package"];
    if record["pinned"].as_bool() == Some(true) {
        return Ok((tool.version.clone(), "pipx pinned requirement".into()));
    }
    if record["package_or_url"]
        .as_str()
        .is_some_and(|s| s.contains("://") || s.starts_with(['/', '.']))
    {
        return Err(Error::Unavailable("This pipx tool retains a direct-source requirement; it cannot be replaced with an index package.".into()));
    }
    let report = tempfile::NamedTempFile::new()?;
    let mut args = vec![
        "-m".into(),
        "pip".into(),
        "install".into(),
        package(tool).into(),
        "--dry-run".into(),
        "--ignore-installed".into(),
        "--no-deps".into(),
        "--only-binary=:all:".into(),
        "--disable-pip-version-check".into(),
        "--report".into(),
        report.path().to_string_lossy().into_owned(),
    ];
    args.extend(source_args(record));
    let output = read(ctx, CommandSpec::new(interpreter(root), args)).await?;
    let report: serde_json::Value =
        serde_json::from_str(&read_small(report.path(), 4 * 1024 * 1024)?)?;
    let item = &report["install"][0];
    let version = item["metadata"]["version"]
        .as_str()
        .ok_or_else(|| Error::Unavailable("pip did not report a compatible candidate.".into()))?;
    // A wheel URL can point to a CDN. Only pip's configured search locations
    // describe the indexes used by this resolution.
    let label = output
        .lines()
        .find_map(|line| line.strip_prefix("Looking in indexes: "))
        .map(|indexes| {
            indexes
                .split(", ")
                .map(sanitized_url)
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .unwrap_or_else(|| "pipx / pip configuration".into());
    Ok((version.into(), label))
}

pub(crate) fn pipx_args(tool: &Tool) -> Result<Vec<String>> {
    let Some(root) = tool.installation.as_ref().and_then(|i| i.root.as_ref()) else {
        return Ok(vec![]);
    };
    let value: serde_json::Value =
        serde_json::from_str(&read_small(&root.join("pipx_metadata.json"), 1_048_576)?)?;
    Ok(source_args(&value["main_package"]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requirements_preserve_constraints_and_reject_direct_sources() {
        let value: toml::Value =
            toml::from_str("name='private-tool'\nspecifier='>=1,<2'\nextras=['cli']").unwrap();
        assert_eq!(requirement(&value).unwrap(), "private-tool[cli]>=1,<2");
        let value: toml::Value =
            toml::from_str("name='private-tool'\ngit='https://git.invalid/tool'").unwrap();
        assert!(requirement(&value).is_err());
    }

    #[test]
    fn pipx_queries_keep_index_options_without_replaying_install_destinations() {
        let record = serde_json::json!({"pip_args":["--index-url", "https://private.invalid/simple/", "--extra-index-url=https://mirror.invalid/", "--target", "/other/environment", "--no-index", "--editable", "/source"]});
        assert_eq!(
            source_args(&record),
            [
                "--index-url",
                "https://private.invalid/simple/",
                "--extra-index-url=https://mirror.invalid/",
                "--no-index"
            ]
        );
    }

    #[test]
    fn uv_config_combines_indexes_in_native_precedence_order() {
        let mut low: toml::Value =
            toml::from_str("index=[{url='https://system.invalid/simple/'}]\nresolution='highest'")
                .unwrap();
        let high: toml::Value =
            toml::from_str("index=[{url='https://user.invalid/simple/'}]\nresolution='lowest'")
                .unwrap();
        merge(&mut low, high);
        assert_eq!(
            low["index"][0]["url"].as_str(),
            Some("https://user.invalid/simple/")
        );
        assert_eq!(
            low["index"][1]["url"].as_str(),
            Some("https://system.invalid/simple/")
        );
        assert_eq!(low["resolution"].as_str(), Some("lowest"));
    }
}
