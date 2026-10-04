use super::{Context, directories, id_for};
use crate::{Error, Result, model::Runtime, process::CommandSpec};
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct UvInstallation {
    key: String,
    version: String,
    path: Option<PathBuf>,
}

fn uv_inventory(json: &str, root: &Path, active: Option<&Path>) -> Result<Vec<Runtime>> {
    let items: Vec<UvInstallation> = serde_json::from_str(json)?;
    let mut seen = HashSet::new();
    let mut runtimes = vec![];
    for item in items {
        let Some(binary) = item.path.and_then(|path| std::fs::canonicalize(path).ok()) else {
            continue;
        };
        if !seen.insert(binary.clone()) {
            continue;
        }
        let installation = binary.ancestors().find(|path| path.parent() == Some(root));
        let owned = installation
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name == item.key.as_str())
            })
            .filter(|_| {
                item.key.split('-').count() >= 5 && super::valid_identifier(&item.key).is_ok()
            });
        runtimes.push(Runtime {
            id: id_for("runtime", &binary),
            version: item.version,
            selector: owned.map(|_| item.key.clone()),
            manager: if owned.is_some() { "uv" } else { "PATH" }.into(),
            path: owned
                .unwrap_or_else(|| binary.parent().unwrap_or(&binary))
                .into(),
            active: active == Some(binary.as_path()),
            active_known: active.is_some(),
            managed: owned.is_some(),
            size: None,
            note: if active.is_none() {
                Some("The current interpreter is unknown; removal is disabled.".into())
            } else if owned.is_none() {
                Some("Installation ownership is not verified.".into())
            } else {
                Some(item.key)
            },
        });
    }
    Ok(runtimes)
}

pub async fn uv(ctx: &Context) -> Result<Vec<Runtime>> {
    let root = ctx.read("uv", &["python", "dir"]).await?;
    let root = std::fs::canonicalize(root.trim())?;
    let active = ctx
        .read("uv", &["python", "find", "--no-python-downloads"])
        .await
        .ok()
        .and_then(|value| std::fs::canonicalize(value.trim()).ok());
    let json = ctx
        .read(
            "uv",
            &[
                "python",
                "list",
                "--only-installed",
                "--all-platforms",
                "--output-format",
                "json",
            ],
        )
        .await?;
    uv_inventory(&json, &root, active.as_deref())
}

fn pyenv_inventory(root: &Path, selected: Option<&str>, current: Option<&Path>) -> Vec<Runtime> {
    let selected = selected.map(|value| {
        value
            .split([':', '\n', '\r'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<HashSet<_>>()
    });
    let known = selected.as_ref().is_some_and(|names| !names.is_empty()) && current.is_some();
    directories(&root.join("versions"))
        .into_iter()
        .filter_map(|path| {
            let path = std::fs::canonicalize(path).ok()?;
            let version = path.file_name()?.to_string_lossy().into_owned();
            Some(Runtime {
                id: id_for("runtime", &path),
                selector: Some(version.clone()),
                active: selected
                    .as_ref()
                    .is_some_and(|names| names.contains(version.as_str()))
                    || current.is_some_and(|binary| binary.starts_with(&path)),
                active_known: known,
                version,
                manager: "pyenv".into(),
                path,
                managed: true,
                size: None,
                note: (!known).then(|| {
                    "The current pyenv interpreter is unknown; removal is disabled.".into()
                }),
            })
        })
        .collect()
}

pub async fn pyenv(ctx: &Context) -> Result<Vec<Runtime>> {
    let root = ctx.read("pyenv", &["root"]).await?;
    let root = std::fs::canonicalize(root.trim())?;
    let selected = ctx.read("pyenv", &["version-name"]).await.ok();
    let current = ctx
        .read("pyenv", &["which", "python"])
        .await
        .ok()
        .and_then(|value| std::fs::canonicalize(value.trim()).ok());
    Ok(pyenv_inventory(
        &root,
        selected.as_deref(),
        current.as_deref(),
    ))
}

pub fn command(ctx: &Context, runtime: &Runtime, verb: &str) -> Result<CommandSpec> {
    let selector = runtime.selector.as_deref().ok_or_else(|| {
        Error::Conflict(
            "The exact Python installation is unknown. Refresh before changing it.".into(),
        )
    })?;
    let mut command = super::runtime_command(ctx, &runtime.manager, verb, selector)?;
    command.cwd = Some(ctx.home.clone());
    match runtime.manager.as_str() {
        "uv" => {
            let root = runtime
                .path
                .parent()
                .ok_or_else(|| Error::unsafe_path(&runtime.path, "missing installation root"))?;
            command.env.insert(
                "UV_PYTHON_INSTALL_DIR".into(),
                root.to_string_lossy().into_owned(),
            );
            command
                .env
                .insert("UV_PYTHON_DOWNLOADS".into(), "never".into());
        }
        "pyenv" => {
            let root = runtime
                .path
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| Error::unsafe_path(&runtime.path, "missing pyenv root"))?;
            command
                .env
                .insert("PYENV_ROOT".into(), root.to_string_lossy().into_owned());
        }
        _ => return Err(Error::Unavailable("Unsupported Python manager.".into())),
    }
    Ok(command)
}

fn verify_inventory(runtime: &Runtime, current: &[Runtime]) -> Result<()> {
    let matches = current
        .iter()
        .filter(|item| item.selector == runtime.selector && item.manager == runtime.manager)
        .collect::<Vec<_>>();
    if matches.len() != 1
        || matches[0].path != runtime.path
        || matches[0].id != runtime.id
        || !matches[0].managed
        || !matches[0].active_known
        || matches[0].active
    {
        return Err(Error::Conflict("The selected Python installation changed, is active, or cannot be verified. Refresh before uninstalling.".into()));
    }
    Ok(())
}

pub async fn verify_removal(ctx: &Context, runtime: &Runtime) -> Result<()> {
    let current = match runtime.manager.as_str() {
        "uv" => uv(ctx).await?,
        "pyenv" => pyenv(ctx).await?,
        _ => return Ok(()),
    };
    verify_inventory(runtime, &current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_keeps_architecture_implementation_and_variant_installations_distinct() {
        let root = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();
        let keys = [
            "cpython-3.13.2-windows-x86_64-none",
            "cpython-3.13.2-windows-aarch64-none",
            "pypy-3.13.2-windows-x86_64-none",
        ];
        let mut items = vec![];
        for key in keys {
            let binary = root.join(key).join("bin/python");
            std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
            std::fs::write(&binary, "fixture").unwrap();
            items.push(serde_json::json!({"key": key, "version": "3.13.2", "path": binary}));
        }
        let active = PathBuf::from(items[0]["path"].as_str().unwrap());
        let json = serde_json::to_string(&items).unwrap();
        let runtimes = uv_inventory(&json, &root, Some(&active)).unwrap();
        assert_eq!(runtimes.len(), 3);
        assert_eq!(runtimes[1].selector.as_deref(), Some(keys[1]));
        let mut ctx = Context::new(tokio_util::sync::CancellationToken::new()).unwrap();
        ctx.home = root.clone();
        std::fs::create_dir_all(root.join(".local/bin")).unwrap();
        std::fs::write(
            root.join(if cfg!(windows) {
                ".local/bin/uv.exe"
            } else {
                ".local/bin/uv"
            }),
            "fixture",
        )
        .unwrap();
        let removal = command(&ctx, &runtimes[1], "remove").unwrap();
        assert_eq!(removal.args, ["python", "uninstall", keys[1]]);
        assert_eq!(removal.env["UV_PYTHON_INSTALL_DIR"], root.to_string_lossy());
        assert!(verify_inventory(&runtimes[0], &runtimes).is_err());
        verify_inventory(&runtimes[1], &runtimes).unwrap();
        let unknown = uv_inventory(&json, &root, None).unwrap();
        assert!(verify_inventory(&runtimes[1], &unknown).is_err());
        let changed = uv_inventory(
            &json,
            &root,
            Some(Path::new(items[1]["path"].as_str().unwrap())),
        )
        .unwrap();
        assert!(verify_inventory(&runtimes[1], &changed).is_err());
    }

    #[test]
    fn pyenv_protects_selected_versions_and_unknown_interpreters() {
        let root = tempfile::tempdir().unwrap();
        for version in ["3.12.9", "3.13.2"] {
            std::fs::create_dir_all(root.path().join("versions").join(version)).unwrap();
        }
        let root = std::fs::canonicalize(root.path()).unwrap();
        let current = root.join("versions/3.13.2/bin/python");
        let runtimes = pyenv_inventory(&root, Some("3.13.2:3.12.9"), Some(&current));
        assert!(
            runtimes
                .iter()
                .all(|runtime| runtime.active && runtime.active_known)
        );
        let unknown = pyenv_inventory(&root, None, None);
        assert!(unknown.iter().all(|runtime| !runtime.active_known));
        assert!(verify_inventory(&runtimes[0], &unknown).is_err());
    }
}
