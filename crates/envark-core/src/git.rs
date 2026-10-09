use crate::{
    Error, Result,
    filesystem::{id_for, read_small, reject_links},
    model::{Repository, Worktree},
};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

pub(crate) struct Checkout {
    pub repository: Repository,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub branch: Option<String>,
    pub is_worktree: bool,
}

fn text(path: &Path) -> Result<String> {
    reject_links(path)?;
    read_small(path, 16_384)
}

fn resolve(base: &Path, value: &str) -> Result<PathBuf> {
    let path = Path::new(value.trim());
    if path.as_os_str().is_empty()
        || (!path.is_absolute()
            && (path.has_root() || path.components().any(|c| matches!(c, Component::Prefix(_)))))
    {
        return Err(Error::unsafe_path(path, "invalid Git path"));
    }
    let mut resolved = if path.is_absolute() {
        PathBuf::new()
    } else {
        reject_links(base)?;
        base.to_path_buf()
    };
    // Git normally writes commondir as ../... Check every traversed prefix
    // before resolving a parent, so normalization cannot hide a symlink.
    // Do not join the full value first: Windows verbatim paths normalize '..'
    // during PathBuf::push, before the filesystem checks can inspect it.
    for component in path.components() {
        match component {
            Component::ParentDir => {
                reject_links(&resolved)?;
                if !fs::metadata(&resolved)?.is_dir() {
                    return Err(Error::unsafe_path(
                        path,
                        "Git parent path is not a directory",
                    ));
                }
                if !resolved.pop() {
                    return Err(Error::unsafe_path(path, "invalid Git relative path"));
                }
            }
            Component::CurDir => {}
            component => {
                resolved.push(component);
                if matches!(component, Component::Normal(_)) {
                    reject_links(&resolved)?;
                }
            }
        }
    }
    reject_links(&resolved)?;
    Ok(fs::canonicalize(resolved)?)
}

fn branch(git_dir: &Path) -> Result<Option<String>> {
    let head = text(&git_dir.join("HEAD"))?;
    let head = head.trim();
    if let Some(name) = head.strip_prefix("ref: refs/heads/") {
        Ok(Some(name.to_owned()))
    } else if !head.is_empty() && head.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(Some(format!("detached: {}", &head[..head.len().min(12)])))
    } else {
        Err(Error::unsafe_path(git_dir, "invalid Git HEAD"))
    }
}

/// Read Git's on-disk layout without executing repository configuration or hooks.
/// A gitfile must resolve to a valid Git directory; linked worktrees additionally
/// require the reciprocal gitdir pointer to match this checkout.
pub(crate) fn checkout(path: &Path) -> Result<Option<Checkout>> {
    let marker = path.join(".git");
    if !marker.try_exists()? {
        return Ok(None);
    }
    reject_links(&marker)?;
    let git_dir = if marker.is_dir() {
        fs::canonicalize(&marker)?
    } else {
        let pointer = text(&marker)?;
        let value = pointer
            .trim()
            .strip_prefix("gitdir: ")
            .ok_or_else(|| Error::unsafe_path(&marker, "invalid Git directory pointer"))?;
        resolve(path, value)?
    };
    let branch = branch(&git_dir)?;
    let common_marker = git_dir.join("commondir");
    let is_worktree = common_marker.try_exists()?;
    let common_dir = if is_worktree {
        let common = resolve(&git_dir, &text(&common_marker)?)?;
        let registry = common.join("worktrees");
        if git_dir.parent() != Some(registry.as_path())
            || resolve(&git_dir, &text(&git_dir.join("gitdir"))?)? != fs::canonicalize(&marker)?
        {
            return Err(Error::unsafe_path(
                &marker,
                "worktree registration does not match",
            ));
        }
        common
    } else {
        git_dir.clone()
    };
    for name in ["objects", "refs"] {
        let directory = common_dir.join(name);
        reject_links(&directory)?;
        if !directory.is_dir() {
            return Err(Error::unsafe_path(&marker, "Git metadata is incomplete"));
        }
    }
    let repository_path = if !is_worktree {
        path.to_path_buf()
    } else if common_dir.file_name().is_some_and(|name| name == ".git") {
        common_dir.parent().unwrap_or(&common_dir).to_path_buf()
    } else {
        // Bare repositories may also own linked worktrees.
        common_dir.clone()
    };
    Ok(Some(Checkout {
        repository: Repository {
            id: id_for("repository", &common_dir),
            name: repository_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: repository_path,
        },
        git_dir,
        common_dir,
        branch,
        is_worktree,
    }))
}

#[derive(Default)]
pub(crate) struct RegisteredWorktrees {
    pub entries: Vec<Worktree>,
    pub issues: Vec<String>,
}

fn registered_worktree(checkout: &Checkout, git_dir: &Path) -> Result<Worktree> {
    reject_links(git_dir)?;
    let pointer = text(&git_dir.join("gitdir"))?;
    let marker = git_dir.join(pointer.trim());
    let marker = fs::canonicalize(&marker).unwrap_or(marker);
    let path = marker
        .parent()
        .ok_or_else(|| Error::unsafe_path(&marker, "invalid worktree path"))?
        .to_path_buf();
    let issue = match self::checkout(&path) {
        Ok(Some(found)) if found.git_dir == git_dir && found.common_dir == checkout.common_dir => None,
        _ => Some("Worktree is missing or its registration needs repair. Use git worktree repair or prune.".into()),
    };
    Ok(Worktree {
        id: id_for("project", &path),
        repository: checkout.repository.clone(),
        path,
        branch: branch(git_dir).ok().flatten(),
        locked: git_dir.join("locked").exists(),
        issue,
        size: None,
    })
}

/// Read registered worktrees. The scanner includes them when their main
/// repository is in scope; cleanup independently revalidates that association.
pub(crate) fn worktrees(
    checkout: &Checkout,
    cancel: &CancellationToken,
) -> Result<RegisteredWorktrees> {
    let registry = checkout.common_dir.join("worktrees");
    if !registry.try_exists()? {
        return Ok(RegisteredWorktrees::default());
    }
    reject_links(&registry)?;
    let mut result = RegisteredWorktrees::default();
    for (index, entry) in fs::read_dir(registry)?.enumerate() {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if index >= 4096 {
            result
                .issues
                .push("Too many registered Git worktrees; discovery is incomplete.".into());
            break;
        }
        let discovered = entry
            .map_err(Error::from)
            .and_then(|entry| registered_worktree(checkout, &entry.path()));
        match discovered {
            Ok(worktree) => result.entries.push(worktree),
            Err(error) if result.issues.len() < 100 => result.issues.push(error.to_string()),
            Err(_) => {}
        }
    }
    result.entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

#[cfg(test)]
pub(crate) fn init(path: &Path) {
    // Git cannot mkdir a Windows verbatim path supplied as an argument. Let
    // Rust create the fixture and initialize it from its working directory.
    fs::create_dir_all(path).unwrap();
    let output = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(test)]
pub(crate) fn add_worktree(repo: &Path, linked: &Path) {
    for args in [
        vec!["commit", "--quiet", "--allow-empty", "-m", "fixture"],
        vec![
            "worktree",
            "add",
            "--quiet",
            "-b",
            "feature",
            linked.to_str().unwrap(),
        ],
    ] {
        let output = std::process::Command::new("git")
            .current_dir(repo)
            .args([
                "-c",
                "core.hooksPath=",
                "-c",
                "commit.gpgSign=false",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_relative_git_metadata_and_checks_traversed_components() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let entry = root.join(".git/worktrees/feature");
        fs::create_dir_all(&entry).unwrap();
        assert_eq!(resolve(&entry, "../..").unwrap(), root.join(".git"));
        assert!(resolve(&entry, "missing/../..").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn cannot_hide_symlinks_in_relative_git_metadata() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::create_dir(root.join("real")).unwrap();
        symlink(root.join("real"), root.join("link")).unwrap();
        assert!(resolve(&root, "link/../real").is_err());
    }
}
