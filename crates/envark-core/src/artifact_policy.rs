use crate::{
    Error, Result,
    filesystem::{read_small, reject_links},
    model::{Artifact, Project},
    providers::Context,
};
use std::path::Path;

const CACHE_SIGNATURE: &str = "Signature: 8a477f597d28d172789f06886806bc55";

fn marker(path: &Path) -> bool {
    reject_links(path).is_ok() && path.is_file()
}

fn dependency_lockfile(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let manifest = parent.join("package.json");
    if !marker(&manifest)
        || !read_small(&manifest, 1024 * 1024)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .is_some_and(|value| value.is_object())
    {
        return false;
    }
    // Workspace packages inherit the repository's lockfile. Never cross a
    // nested checkout boundary or accept a symlink as ownership evidence.
    for ancestor in parent.ancestors() {
        if [
            "package-lock.json",
            "npm-shrinkwrap.json",
            "pnpm-lock.yaml",
            "yarn.lock",
            "bun.lock",
            "bun.lockb",
        ]
        .iter()
        .any(|name| marker(&ancestor.join(name)))
        {
            return true;
        }
        if ancestor.join(".git").exists() {
            break;
        }
    }
    false
}

/// Conventional directory names alone do not establish generated-file ownership.
pub fn ownership_issue(path: &Path, name: &str) -> Option<String> {
    let cache_tag = || {
        let tag = path.join("CACHEDIR.TAG");
        marker(&tag) && read_small(&tag, 4096).is_ok_and(|text| text.starts_with(CACHE_SIGNATURE))
    };
    let owned = match name {
        "node_modules" => {
            [
                ".package-lock.json",
                ".modules.yaml",
                ".yarn-integrity",
                ".yarn-state.yml",
            ]
            .iter()
            .any(|name| marker(&path.join(name)))
                || dependency_lockfile(path)
        }
        ".next" => ["BUILD_ID", "routes-manifest.json"]
            .iter()
            .any(|name| marker(&path.join(name))),
        ".nuxt" => marker(&path.join("nuxt.json")),
        ".svelte-kit" => marker(&path.join("tsconfig.json")),
        ".venv" => marker(&path.join("pyvenv.cfg")),
        "target" => cache_tag() && marker(&path.join(".rustc_info.json")),
        ".pytest_cache" | ".ruff_cache" | ".mypy_cache" | ".gradle" => cache_tag(),
        "__pycache__" => std::fs::read_dir(path).is_ok_and(|mut entries| {
            entries.all(|entry| {
                entry.is_ok_and(|entry| {
                    entry.file_type().is_ok_and(|kind| kind.is_file())
                        && entry.path().extension().is_some_and(|ext| ext == "pyc")
                })
            })
        }),
        _ => false,
    };
    (!owned).then(|| {
        "Generated-directory ownership is unverified. Clean this directory with its build tool."
            .into()
    })
}

/// Query the index without hooks, optional writes, or inherited repository overrides.
pub async fn protect_tracked_files(
    ctx: &Context,
    project: &Project,
    artifact: &Artifact,
) -> Result<()> {
    if !project
        .path
        .ancestors()
        .any(|path| path.join(".git").exists())
    {
        if project
            .path
            .ancestors()
            .any(|path| path.join(".hg").exists() || path.join(".svn").exists())
        {
            return Err(Error::Unavailable("Tracked-file protection is unavailable for this repository. Use its build tool to clean it.".into()));
        }
        return Ok(());
    }
    let relative = artifact
        .path
        .strip_prefix(&project.path)
        .map_err(|_| Error::unsafe_path(&artifact.path, "artifact is outside its project"))?;
    let mut command = ctx.command(
        "git",
        &[
            "-c",
            "core.fsmonitor=false",
            "--literal-pathspecs",
            "ls-files",
            "--cached",
            "-z",
            "--",
            &relative.to_string_lossy(),
        ],
    )?;
    command.cwd = Some(project.path.clone());
    command.env.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
    command.remove_env.extend(
        [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
        ]
        .map(str::to_owned),
    );
    let output = ctx.runner.run(&command, &ctx.cancel).await?;
    if !output.stdout.is_empty() {
        return Err(Error::unsafe_path(
            &artifact.path,
            "directory contains Git-tracked files",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_build_directories_are_never_owned_by_name() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("build/src")).unwrap();
        std::fs::write(root.path().join("build/src/Main.java"), "class Main {}").unwrap();
        assert!(ownership_issue(&root.path().join("build"), "build").is_some());
        assert!(ownership_issue(&root.path().join("target"), "target").is_some());
    }

    #[test]
    fn workspace_dependencies_use_the_repository_lockfile_without_crossing_nested_repositories() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let modules = root.join("packages/app/node_modules");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(
            root.join("package.json"),
            r#"{"workspaces":["packages/*"]}"#,
        )
        .unwrap();
        std::fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").unwrap();
        std::fs::write(root.join("packages/app/package.json"), r#"{"name":"app"}"#).unwrap();
        assert!(ownership_issue(&modules, "node_modules").is_none());
        std::fs::create_dir(root.join("packages/app/.git")).unwrap();
        assert!(ownership_issue(&modules, "node_modules").is_some());
        std::fs::write(root.join("packages/app/bun.lock"), "{}").unwrap();
        assert!(ownership_issue(&modules, "node_modules").is_none());
        std::fs::write(root.join("packages/app/package.json"), "not json").unwrap();
        assert!(ownership_issue(&modules, "node_modules").is_some());
    }

    #[test]
    fn cache_ownership_requires_a_valid_signature() {
        let root = tempfile::tempdir().unwrap();
        let path = std::fs::canonicalize(root.path()).unwrap();
        std::fs::write(path.join("CACHEDIR.TAG"), "source").unwrap();
        assert!(ownership_issue(&path, ".pytest_cache").is_some());
        std::fs::write(path.join("CACHEDIR.TAG"), CACHE_SIGNATURE).unwrap();
        assert!(ownership_issue(&path, ".pytest_cache").is_none());
    }
}
