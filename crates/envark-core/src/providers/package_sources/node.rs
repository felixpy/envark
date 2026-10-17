use super::*;

fn npm_prefix(tool: &Tool) -> Option<PathBuf> {
    if let Some(root) = tool.installation.as_ref().and_then(|i| i.root.clone()) {
        return Some(root);
    }
    let path = tool.path.as_ref()?;
    let modules = if entry(tool).starts_with('@') {
        path.parent()?.parent()?
    } else {
        path.parent()?
    };
    if modules.file_name()? != "node_modules" {
        return None;
    }
    if cfg!(windows) {
        modules.parent().map(Path::to_path_buf)
    } else {
        modules.parent()?.parent().map(Path::to_path_buf)
    }
}

pub(crate) fn npm_spec(ctx: &Context, tool: &Tool, args: &[&str]) -> Result<CommandSpec> {
    let Some(prefix) = npm_prefix(tool) else {
        return ctx.command("npm", args);
    };
    let node = prefix.join(if cfg!(windows) {
        "node.exe"
    } else {
        "bin/node"
    });
    let cli = prefix.join(if cfg!(windows) {
        "node_modules/npm/bin/npm-cli.js"
    } else {
        "lib/node_modules/npm/bin/npm-cli.js"
    });
    let mut spec = if node.is_file() && cli.is_file() {
        let mut spec = CommandSpec::new(&node, [cli.to_string_lossy().into_owned()]);
        spec.args.extend(args.iter().map(|arg| (*arg).to_owned()));
        let mut paths = vec![node.parent().unwrap().into()];
        paths.extend(
            std::env::var_os("PATH")
                .as_deref()
                .map(std::env::split_paths)
                .into_iter()
                .flatten(),
        );
        spec.env.insert(
            "PATH".into(),
            std::env::join_paths(paths)
                .map_err(|e| Error::InvalidInput(e.to_string()))?
                .to_string_lossy()
                .into_owned(),
        );
        spec
    } else {
        ctx.command("npm", args)?
    };
    spec.args
        .extend(["--prefix".into(), prefix.to_string_lossy().into_owned()]);
    Ok(spec)
}

pub(super) async fn query(ctx: &Context, tool: &Tool) -> Result<(String, String)> {
    if tool
        .installation
        .as_ref()
        .and_then(|i| i.source_kind.as_deref())
        == Some("direct")
    {
        return Err(Error::Unavailable("This installation uses a Git, local, or direct URL source; it cannot be replaced with a registry package.".into()));
    }
    let name = if tool.source == "corepack"
        && tool.name == "yarn"
        && semver::Version::parse(&tool.version).is_ok_and(|v| v.major >= 2)
    {
        "@yarnpkg/cli-dist"
    } else {
        package(tool)
    };
    super::super::tool_commands::npm_name(name)?;
    let (program, args): (&str, Vec<&str>) = match tool.source.as_str() {
        "pnpm" | "pnpm-self" => ("pnpm", vec!["view", name, "version", "--json"]),
        "yarn" => (
            "yarn",
            vec!["info", name, "version", "--json", "--non-interactive"],
        ),
        _ => ("npm", vec!["view", "--global", name, "version", "--json"]),
    };
    let spec = if program == "npm" {
        npm_spec(ctx, tool, &args)?
    } else {
        ctx.command(program, &args)?
    };
    let text = read(ctx, spec).await?;
    let version = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|value| {
            value
                .as_str()
                .or_else(|| value.get("data")?.as_str())
                .map(str::to_owned)
        })
        .or_else(|| serde_json::from_str::<String>(&text).ok())
        .ok_or_else(|| {
            Error::Unavailable("The package manager did not report a version.".into())
        })?;
    let key = name
        .strip_prefix('@')
        .and_then(|s| s.split('/').next())
        .map(|scope| format!("@{scope}:registry"));
    let mut registry = None;
    for config_key in key
        .as_deref()
        .into_iter()
        .chain(std::iter::once("registry"))
    {
        let args = if program == "npm" {
            vec!["config", "get", config_key, "--global"]
        } else {
            vec!["config", "get", config_key]
        };
        let spec = if program == "npm" {
            npm_spec(ctx, tool, &args)?
        } else {
            ctx.command(program, &args)?
        };
        if let Ok(value) = read(ctx, spec).await {
            let value = value.trim().trim_matches('"');
            if reqwest::Url::parse(value).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            {
                registry = Some(sanitized_url(value));
                break;
            }
        }
    }
    Ok((
        version,
        registry.unwrap_or_else(|| format!("{program} configuration")),
    ))
}

pub(crate) fn discover_node_evidence(tool: &mut Tool, root: &Path) {
    let Some(path) = &tool.path else { return };
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    let entry = if path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|s| s.to_string_lossy().starts_with('@'))
    {
        format!(
            "{}/{name}",
            path.parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
        )
    } else {
        name.into()
    };
    let mut origin = None;
    let mut evidence = None;
    if let Ok(text) = read_small(&path.join("package.json"), 1_048_576)
        && let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(value) = manifest["_resolved"].as_str()
    {
        origin = Some(sanitized_url(value));
        evidence = Some("legacy manifest".into());
    }
    let environment = root.parent().unwrap_or(root);
    if matches!(tool.source.as_str(), "pnpm" | "yarn") {
        if tool.source == "pnpm"
            && let Ok(text) = read_small(&root.join(".modules.yaml"), 8 * 1024 * 1024)
            && let Ok(value) = serde_yaml_ng::from_str::<serde_json::Value>(&text)
        {
            let key = entry
                .strip_prefix('@')
                .and_then(|s| s.split('/').next())
                .map(|s| format!("@{s}"));
            if let Some(value) = key
                .as_deref()
                .and_then(|key| value["registries"][key].as_str())
                .or_else(|| value["registries"]["default"].as_str())
            {
                origin = Some(sanitized_url(value));
                evidence = Some("pnpm environment".into());
            }
        }
        if tool.source == "yarn"
            && let Ok(text) = read_small(&path.join(".yarn-metadata.json"), 1_048_576)
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(url) = value["remote"]["resolved"].as_str()
        {
            origin = Some(sanitized_url(url));
            evidence = Some("Yarn artifact".into());
        }
    }
    let install_root = if tool.source == "npm" {
        if cfg!(windows) {
            root.parent()
        } else {
            root.parent().and_then(Path::parent)
        }
    } else {
        Some(environment)
    };
    let requested = read_small(&environment.join("package.json"), 1_048_576)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value["dependencies"][&entry].as_str().map(str::to_owned));
    let direct = requested.as_deref().is_some_and(|value| {
        value.starts_with(['/', '.'])
            || [
                "git:",
                "git+",
                "github:",
                "gitlab:",
                "bitbucket:",
                "file:",
                "link:",
                "workspace:",
                "http:",
                "https:",
            ]
            .iter()
            .any(|prefix| value.starts_with(prefix))
    });
    if direct {
        tool.can_update = false;
    }
    tool.installation = Some(PackageInstallation {
        entry,
        package: tool.name.clone(),
        root: install_root.map(Path::to_path_buf),
        origin,
        evidence,
        requirement: requested.map(|value| {
            if value.contains("://") {
                sanitized_url(&value)
            } else {
                value
            }
        }),
        source_kind: Some(if direct { "direct" } else { "registry" }.into()),
        ..Default::default()
    });
}

fn pnpm_entry_path(
    item: &serde_json::Value,
    entry: &str,
    dependency: &serde_json::Value,
) -> Option<PathBuf> {
    let public = PathBuf::from(item["path"].as_str()?)
        .join("node_modules")
        .join(entry);
    if public.join("package.json").is_file() {
        Some(public)
    } else {
        dependency["path"].as_str().map(PathBuf::from)
    }
}

pub(crate) async fn discover_pnpm(ctx: &Context, provider: &mut Provider) {
    let Ok(text) = ctx
        .read("pnpm", &["list", "--global", "--json", "--depth", "0"])
        .await
    else {
        return;
    };
    let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(&text) else {
        return;
    };
    for item in items {
        let Some(deps) = item["dependencies"].as_object() else {
            continue;
        };
        for (entry, value) in deps {
            // Prefer the public entry on older pnpm layouts. Version 11 reports
            // an isolated environment instead and moves it during upgrades.
            let Some(path) = pnpm_entry_path(&item, entry, value) else {
                continue;
            };
            if provider
                .tools
                .iter()
                .any(|tool| tool.path.as_ref() == Some(&path))
            {
                continue;
            }
            let Ok(manifest) = read_small(&path.join("package.json"), 1_048_576) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&manifest) else {
                continue;
            };
            let Some(name) = manifest["name"].as_str() else {
                continue;
            };
            let mut tool = basic_tool(
                name,
                manifest["version"].as_str().unwrap_or("unknown").into(),
                "pnpm",
                Some(path.clone()),
            );
            tool.can_update = true;
            tool.can_remove = !["npm", "pnpm", "yarn", "corepack"].contains(&name);
            let modules = if entry.starts_with('@') {
                path.parent().and_then(Path::parent)
            } else {
                path.parent()
            };
            if let Some(root) = modules {
                discover_node_evidence(&mut tool, root);
                if let Some(installation) = &mut tool.installation {
                    installation.entry = entry.clone();
                }
            }
            provider.tools.push(tool);
        }
    }
}

pub(crate) async fn pnpm_version(ctx: &Context, tool: &Tool) -> Result<Option<String>> {
    let text = read(
        ctx,
        ctx.command("pnpm", &["list", "--global", "--json", "--depth", "0"])?,
    )
    .await?;
    let items: Vec<serde_json::Value> = serde_json::from_str(&text)?;
    let matches: Vec<_> = items
        .iter()
        .filter_map(|item| item["dependencies"].get(entry(tool)))
        .collect();
    if matches.len() > 1 {
        return Err(Error::Conflict("More than one pnpm environment contains this entry; refresh its ownership before updating.".into()));
    }
    let Some(dep) = matches.first() else {
        return Ok(None);
    };
    let path = dep["path"]
        .as_str()
        .ok_or_else(|| Error::Conflict("pnpm did not report the installed entry path.".into()))?;
    let manifest: serde_json::Value = serde_json::from_str(&read_small(
        &Path::new(path).join("package.json"),
        1_048_576,
    )?)?;
    if manifest["name"].as_str() != Some(package(tool)) {
        return Err(Error::Conflict(
            "The pnpm entry now refers to a different package. Refresh before updating.".into(),
        ));
    }
    Ok(manifest["version"].as_str().map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_keep_entry_identity_and_do_not_invent_historical_registry() {
        let root = tempfile::tempdir().unwrap();
        let modules = root.path().join("lib/node_modules");
        let alias = modules.join("alias");
        std::fs::create_dir_all(&alias).unwrap();
        std::fs::write(
            alias.join("package.json"),
            r#"{"name":"@scope/original","version":"1.0.0"}"#,
        )
        .unwrap();
        let mut tool = basic_tool("@scope/original", "1.0.0".into(), "npm", Some(alias));
        discover_node_evidence(&mut tool, &modules);
        assert_eq!(entry(&tool), "alias");
        assert_eq!(package(&tool), "@scope/original");
        assert!(tool.installation.unwrap().origin.is_none());
    }

    #[test]
    fn direct_global_requirements_are_not_converted_to_registry_packages() {
        let directory = tempfile::tempdir().unwrap();
        let modules = directory.path().join("node_modules");
        let package = modules.join("linked-tool");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"dependencies":{"linked-tool":"file:../source"}}"#,
        )
        .unwrap();
        let mut tool = basic_tool("linked-tool", "1.0.0".into(), "pnpm", Some(package));
        tool.can_update = true;
        discover_node_evidence(&mut tool, &modules);
        assert!(!tool.can_update);
        assert_eq!(
            tool.installation.unwrap().source_kind.as_deref(),
            Some("direct")
        );
    }

    #[test]
    fn pnpm_paths_support_both_public_entries_and_v11_isolated_environments() {
        let directory = tempfile::tempdir().unwrap();
        let public = directory.path().join("node_modules/tool");
        let resolved = directory.path().join("isolated/node_modules/tool");
        let item = serde_json::json!({"path":directory.path()});
        let dependency = serde_json::json!({"path":resolved});
        assert_eq!(pnpm_entry_path(&item, "tool", &dependency), Some(resolved));
        std::fs::create_dir_all(&public).unwrap();
        std::fs::write(public.join("package.json"), "{}").unwrap();
        assert_eq!(pnpm_entry_path(&item, "tool", &dependency), Some(public));
    }
}
