use super::*;

fn cargo_root(ctx: &Context) -> PathBuf {
    if let Some(root) = std::env::var_os("CARGO_INSTALL_ROOT") {
        return root.into();
    }
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".cargo"));
    for name in ["config.toml", "config"] {
        if let Ok(text) = read_small(&home.join(name), 1_048_576)
            && let Ok(value) = toml::from_str::<toml::Value>(&text)
            && let Some(root) = value
                .get("install")
                .and_then(|v| v.get("root"))
                .and_then(toml::Value::as_str)
        {
            return root.into();
        }
    }
    home
}

pub(crate) fn discover_cargo(ctx: &Context, provider: &mut Provider) {
    let root = cargo_root(ctx);
    let Ok(text) = read_small(&root.join(".crates2.json"), 8 * 1024 * 1024) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(installs) = value["installs"].as_object() else {
        return;
    };
    provider.tools.retain(|tool| tool.source != "cargo");
    for (key, record) in installs {
        let mut fields = key.splitn(3, ' ');
        let (Some(name), Some(version), Some(source)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let source = source.trim_matches(['(', ')']);
        let mut tool = basic_tool(
            name,
            version.into(),
            "cargo",
            record["bins"]
                .as_array()
                .and_then(|bins| bins.first()?.as_str())
                .map(|bin| {
                    root.join("bin").join(if cfg!(windows) {
                        format!("{bin}.exe")
                    } else {
                        bin.into()
                    })
                }),
        );
        tool.can_update = source.starts_with("registry+") || source.starts_with("sparse+");
        tool.can_remove = true;
        tool.installation = Some(PackageInstallation {
            entry: name.into(),
            package: name.into(),
            root: Some(root.clone()),
            origin: Some(sanitized_url(source)),
            evidence: Some("Cargo installation record".into()),
            ..Default::default()
        });
        provider.tools.push(tool);
    }
}

fn cargo_record(tool: &Tool) -> Result<(String, serde_json::Value)> {
    let root = tool
        .installation
        .as_ref()
        .and_then(|i| i.root.as_ref())
        .ok_or_else(|| {
            Error::Unavailable(
                "Refresh Cargo installation metadata before checking its source.".into(),
            )
        })?;
    let value: serde_json::Value =
        serde_json::from_str(&read_small(&root.join(".crates2.json"), 8 * 1024 * 1024)?)?;
    let records = value["installs"]
        .as_object()
        .ok_or_else(|| Error::Unavailable("Cargo installation records are unavailable.".into()))?;
    let (key, record) = records
        .iter()
        .find(|(key, _)| key.split_whitespace().next() == Some(package(tool)))
        .ok_or_else(|| Error::Conflict("This Cargo installation no longer exists.".into()))?;
    let source = key
        .splitn(3, ' ')
        .nth(2)
        .unwrap_or("")
        .trim_matches(['(', ')'])
        .to_owned();
    Ok((source, record.clone()))
}

fn cargo_source_args(source: &str) -> Result<Vec<String>> {
    if matches!(
        source,
        "registry+https://github.com/rust-lang/crates.io-index" | "sparse+https://index.crates.io/"
    ) {
        return Ok(vec!["--registry".into(), "crates-io".into()]);
    }
    let index = source.strip_prefix("registry+").unwrap_or(source);
    if !source.starts_with("registry+") && !source.starts_with("sparse+") {
        return Err(Error::Unavailable("This Cargo tool retains a Git or path source; it cannot be replaced with a registry package.".into()));
    }
    Ok(vec!["--index".into(), index.into()])
}

fn cargo_source_label(ctx: &Context, source: &str) -> String {
    // Display configured replacement chains separately from the logical source
    // in the installation receipt. Cargo remains authoritative for resolution.
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".cargo"));
    let mut paths = vec![home.join("config"), home.join("config.toml")];
    let mut ancestors: Vec<_> = ctx.home.ancestors().collect();
    ancestors.reverse();
    for directory in ancestors {
        paths.extend([
            directory.join(".cargo/config"),
            directory.join(".cargo/config.toml"),
        ]);
    }
    let mut sources = toml::map::Map::<String, toml::Value>::new();
    for path in paths {
        if let Ok(text) = read_small(&path, 1_048_576)
            && let Ok(value) = toml::from_str::<toml::Value>(&text)
            && let Some(table) = value.get("source").and_then(toml::Value::as_table)
        {
            for (name, record) in table {
                let entry = sources
                    .entry(name.clone())
                    .or_insert(toml::Value::Table(Default::default()));
                if let (Some(entry), Some(record)) = (entry.as_table_mut(), record.as_table()) {
                    entry.extend(record.clone());
                }
            }
        }
    }
    let logical = source.strip_prefix("registry+").unwrap_or(source);
    let mut name = if cargo_source_args(source)
        .ok()
        .is_some_and(|args| args == ["--registry", "crates-io"])
    {
        Some("crates-io".to_owned())
    } else {
        sources.iter().find_map(|(name, record)| {
            (record.get("registry").and_then(toml::Value::as_str) == Some(logical))
                .then(|| name.clone())
        })
    };
    let mut effective = None;
    for _ in 0..8 {
        let Some(record) = name.as_ref().and_then(|name| sources.get(name)) else {
            break;
        };
        if let Some(url) = record.get("registry").and_then(toml::Value::as_str) {
            effective = Some(sanitized_url(url));
        }
        name = record
            .get("replace-with")
            .and_then(toml::Value::as_str)
            .map(str::to_owned);
    }
    effective.unwrap_or_else(|| sanitized_url(source))
}

pub(super) async fn query(ctx: &Context, tool: &Tool) -> Result<(String, String)> {
    if tool.source == "cargo" {
        let (source, _) = cargo_record(tool)?;
        let mut args = vec![
            "info".into(),
            package(tool).into(),
            "--color".into(),
            "never".into(),
        ];
        args.extend(cargo_source_args(&source)?);
        let output = read(
            ctx,
            ctx.command(
                "cargo",
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
            )?,
        )
        .await?;
        let version = output
            .lines()
            .find_map(|line| line.trim().strip_prefix("version: "))
            .and_then(|v| v.split_whitespace().next())
            .ok_or_else(|| Error::Unavailable("Cargo did not report a registry version.".into()))?;
        return Ok((version.into(), cargo_source_label(ctx, &source)));
    }
    let module = tool
        .installation
        .as_ref()
        .and_then(|i| i.module.as_ref())
        .ok_or_else(|| {
            Error::Unavailable(
                "Refresh the Go binary's module identity before checking updates.".into(),
            )
        })?;
    super::super::valid_identifier(module)?;
    let selector = format!("{module}@latest");
    let output = read(ctx, ctx.command("go", &["list", "-m", "-json", &selector])?).await?;
    let value: serde_json::Value = serde_json::from_str(&output)?;
    if value["Path"].as_str() != Some(module) {
        return Err(Error::Conflict("Go resolved a different module.".into()));
    }
    let version = value["Version"]
        .as_str()
        .ok_or_else(|| Error::Unavailable("Go did not report a module version.".into()))?;
    let environment = read(
        ctx,
        ctx.command("go", &["env", "-json", "GOPROXY", "GOPRIVATE", "GONOPROXY"])?,
    )
    .await?;
    let environment: serde_json::Value = serde_json::from_str(&environment)?;
    let label = environment["GOPROXY"]
        .as_str()
        .unwrap_or("Go proxy configuration")
        .split([',', '|'])
        .map(sanitized_url)
        .collect::<Vec<_>>()
        .join(" · ");
    Ok((version.into(), label))
}

pub(crate) fn cargo_command(ctx: &Context, tool: &Tool, remove: bool) -> Result<CommandSpec> {
    let (source, record) = cargo_record(tool)?;
    let mut args = vec![
        if remove { "uninstall" } else { "install" }.into(),
        package(tool).into(),
    ];
    if !remove {
        args.extend(cargo_source_args(&source)?);
        args.extend([
            "--version".into(),
            tool.latest
                .clone()
                .ok_or_else(|| Error::Conflict("Check updates first.".into()))?,
        ]);
        for (key, flag) in [
            ("all_features", "--all-features"),
            ("no_default_features", "--no-default-features"),
        ] {
            if record[key].as_bool() == Some(true) {
                args.push(flag.into());
            }
        }
        if let Some(features) = record["features"].as_array() {
            let features = features
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(",");
            if !features.is_empty() {
                args.extend(["--features".into(), features]);
            }
        }
        for (key, flag) in [("target", "--target"), ("profile", "--profile")] {
            if let Some(value) = record[key].as_str() {
                args.extend([flag.into(), value.into()]);
            }
        }
    }
    if let Some(root) = tool.installation.as_ref().and_then(|i| i.root.as_ref()) {
        args.extend(["--root".into(), root.to_string_lossy().into_owned()]);
    }
    ctx.command(
        "cargo",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
}

pub(crate) async fn installed_version(ctx: &Context, tool: &Tool) -> Result<Option<String>> {
    if tool.source == "go" {
        let path = tool
            .path
            .as_ref()
            .ok_or_else(|| Error::Conflict("The Go executable path is missing.".into()))?;
        let output = read(
            ctx,
            ctx.command("go", &["version", "-m", &path.to_string_lossy()])?,
        )
        .await?;
        return Ok(output
            .lines()
            .find_map(|line| line.trim().strip_prefix("mod\t"))
            .and_then(|line| line.split_whitespace().nth(1))
            .map(str::to_owned));
    }
    let root = tool.installation.as_ref().and_then(|i| i.root.as_ref());
    let mut args = vec!["install".into(), "--list".into()];
    if let Some(root) = root {
        args.extend(["--root".into(), root.to_string_lossy().into_owned()]);
    }
    let output = read(
        ctx,
        ctx.command(
            "cargo",
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )?,
    )
    .await?;
    Ok(output
        .lines()
        .filter(|l| !l.starts_with(' '))
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next()? == package(tool))
                .then(|| {
                    fields
                        .next()
                        .map(|v| v.trim_start_matches('v').trim_end_matches(':').to_owned())
                })
                .flatten()
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_source_selection_preserves_mirrors_and_rejects_git_conversion() {
        assert_eq!(
            cargo_source_args("registry+https://github.com/rust-lang/crates.io-index").unwrap(),
            ["--registry", "crates-io"]
        );
        assert_eq!(
            cargo_source_args("sparse+https://private.invalid/index/").unwrap(),
            ["--index", "sparse+https://private.invalid/index/"]
        );
        assert!(cargo_source_args("git+https://git.invalid/tool").is_err());
    }
}
