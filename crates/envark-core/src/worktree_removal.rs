use crate::{
    Error, Result,
    filesystem::{measure_with, reject_links},
    git,
    model::{Measurement, Settings, Worktree},
    operations::WorktreeFileChange,
    process::{CommandSpec, OUTPUT_LIMIT},
    providers::Context,
    scanner::project_in_scope,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone)]
pub(crate) struct Removal {
    pub worktree: Worktree,
    pub size: Measurement,
    pub changes: Vec<WorktreeFileChange>,
    git_dir: PathBuf,
    common_dir: PathBuf,
    head: String,
    status: String,
    index_digest: String,
}

fn validate(worktree: &Worktree, settings: &Settings) -> Result<git::Checkout> {
    reject_links(&worktree.path)?;
    reject_links(&worktree.repository.path)?;
    let path = fs::canonicalize(&worktree.path)?;
    if path != worktree.path || !project_in_scope(&path, settings)? {
        return Err(Error::unsafe_path(
            &path,
            "worktree is outside the current scan scope or moved",
        ));
    }
    if settings
        .protected_projects
        .iter()
        .filter_map(|p| fs::canonicalize(p).ok())
        .any(|p| path.starts_with(&p) || p.starts_with(&path))
    {
        return Err(Error::unsafe_path(&path, "worktree is protected"));
    }
    let checkout = git::checkout(&path)?
        .ok_or_else(|| Error::unsafe_path(&path, "worktree registration is missing"))?;
    reject_links(&checkout.git_dir.join("locked"))?;
    if !checkout.is_worktree
        || checkout.repository.id != worktree.repository.id
        || checkout.repository.path != worktree.repository.path
        || checkout.branch != worktree.branch
        || checkout.repository.path.starts_with(&path)
        || checkout.common_dir.starts_with(&path)
        || checkout.git_dir.join("locked").try_exists()?
    {
        return Err(Error::unsafe_path(
            &path,
            "worktree ownership changed, it is locked, or it contains its repository",
        ));
    }
    Ok(checkout)
}

fn command(ctx: &Context, repository: &Path, args: &[&str]) -> Result<CommandSpec> {
    let mut command = ctx.command(
        "git",
        &[
            "--no-optional-locks",
            "-c",
            "core.hooksPath=",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "submodule.recurse=false",
            "-C",
            &repository.to_string_lossy(),
        ],
    )?;
    command.args.extend(args.iter().map(|a| (*a).to_owned()));
    command.remove_env.extend(
        [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_CONFIG",
            "GIT_CONFIG_COUNT",
        ]
        .map(String::from),
    );
    Ok(command)
}

fn parse_status(status: &str) -> Result<(Vec<WorktreeFileChange>, bool)> {
    // Never approve removal using truncated or lossily decoded Git output.
    if status.len() >= OUTPUT_LIMIT
        || status.contains('\u{fffd}')
        || (!status.is_empty() && !status.ends_with('\0'))
    {
        return Err(Error::Conflict(
            "The complete change list could not be read. Inspect this worktree through Git.".into(),
        ));
    }
    let mut changes = vec![];
    let mut ignored = false;
    let mut entries = status.split_terminator('\0');
    while let Some(entry) = entries.next() {
        let bytes = entry.as_bytes();
        if bytes.len() < 4
            || bytes[2] != b' '
            || !bytes[..2].iter().all(|b| b" MADRCUT?!".contains(b))
        {
            return Err(Error::Conflict(
                "Git returned an invalid change list.".into(),
            ));
        }
        if entry.starts_with("!! ") {
            ignored = true;
            continue;
        }
        let original_path = if bytes[..2].iter().any(|b| b"RC".contains(b)) {
            Some(
                entries
                    .next()
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| Error::Conflict("Git returned an incomplete rename.".into()))?
                    .to_owned(),
            )
        } else {
            None
        };
        changes.push(WorktreeFileChange {
            status: entry[..2].to_owned(),
            path: entry[3..].to_owned(),
            original_path,
        });
    }
    Ok((changes, ignored))
}

fn index_digest(git_dir: &Path, ctx: &Context) -> Result<String> {
    let path = git_dir.join("index");
    reject_links(&path)?;
    let mut hash = Sha256::new();
    if path.try_exists()? {
        let mut file = fs::File::open(path)?;
        let mut buffer = [0; 64 * 1024];
        loop {
            if ctx.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
    }
    Ok(hex::encode(hash.finalize()))
}

async fn inspect(ctx: &Context, worktree: &Worktree) -> Result<(String, String)> {
    let status = ctx
        .runner
        .run(
            &command(
                ctx,
                &worktree.path,
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                    "--ignored=matching",
                    "--ignore-submodules=none",
                ],
            )?,
            &ctx.cancel,
        )
        .await?;
    parse_status(&status.stdout)?;
    let head = ctx
        .runner
        .run(
            &command(ctx, &worktree.path, &["rev-parse", "--verify", "HEAD"])?,
            &ctx.cancel,
        )
        .await?;
    Ok((head.stdout.trim().to_owned(), status.stdout))
}

fn measure_removal(
    worktree: &Worktree,
    ctx: &Context,
) -> Result<(Measurement, Vec<WorktreeFileChange>)> {
    let mut nested = vec![];
    let size = measure_with(&worktree.path, &ctx.cancel, |path, metadata| {
        if path.file_name().is_some_and(|name| name == ".git") && path != worktree.path.join(".git")
        {
            nested.push(WorktreeFileChange {
                path: path
                    .parent()
                    .unwrap_or(path)
                    .strip_prefix(&worktree.path)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned(),
                original_path: None,
                status: if metadata.is_dir() {
                    "repository"
                } else {
                    "submodule"
                }
                .into(),
            });
        }
        Ok(())
    })?;
    if !size.complete {
        return Err(Error::Conflict(
            "Worktree size could not be measured completely. Check permissions and rescan.".into(),
        ));
    }
    Ok((size, nested))
}

pub(crate) fn removal_command(
    ctx: &Context,
    worktree: &Worktree,
    force: bool,
) -> Result<CommandSpec> {
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    let path = worktree.path.to_string_lossy();
    args.extend(["--", &path]);
    let mut spec = command(ctx, &worktree.repository.path, &args)?;
    spec.timeout = Duration::from_secs(600);
    Ok(spec)
}

pub(crate) async fn prepare(
    ctx: &Context,
    worktree: Worktree,
    settings: Settings,
) -> Result<(Removal, bool)> {
    let worker = ctx.clone();
    let item = worktree.clone();
    let (checkout, size, nested, index_digest) = tokio::task::spawn_blocking(move || {
        let checkout = validate(&item, &settings)?;
        let (size, nested) = measure_removal(&item, &worker)?;
        let scanned = item
            .size
            .as_ref()
            .ok_or_else(|| Error::Conflict("Rescan before removing this worktree.".into()))?;
        if !scanned.complete || scanned.fingerprint.is_none() {
            return Err(Error::Conflict(
                "Rescan the complete worktree before removing it.".into(),
            ));
        }
        let digest = index_digest(&checkout.git_dir, &worker)?;
        Ok::<_, Error>((checkout, size, nested, digest))
    })
    .await
    .map_err(|e| Error::Unavailable(e.to_string()))??;
    let (head, status) = inspect(ctx, &worktree).await?;
    let (mut changes, ignored) = parse_status(&status)?;
    changes.extend(nested);
    Ok((
        Removal {
            worktree,
            size,
            changes,
            git_dir: checkout.git_dir,
            common_dir: checkout.common_dir,
            head,
            status,
            index_digest,
        },
        ignored,
    ))
}

pub(crate) async fn execute(
    ctx: &Context,
    removal: Removal,
    settings: Settings,
    discard_changes: bool,
) -> Result<(u64, String)> {
    if !removal.changes.is_empty() && !discard_changes {
        return Err(Error::Conflict(
            "Discarding worktree changes requires explicit confirmation.".into(),
        ));
    }
    let worker = ctx.clone();
    let reviewed = removal.clone();
    tokio::task::spawn_blocking(move || {
        let checkout = validate(&reviewed.worktree, &settings)?;
        if checkout.git_dir != reviewed.git_dir
            || checkout.common_dir != reviewed.common_dir
            || index_digest(&checkout.git_dir, &worker)? != reviewed.index_digest
            || measure_removal(&reviewed.worktree, &worker)?.0.fingerprint
                != reviewed.size.fingerprint
        {
            return Err(Error::Conflict(
                "The worktree changed after review. Create a new plan.".into(),
            ));
        }
        Ok::<_, Error>(())
    })
    .await
    .map_err(|e| Error::Unavailable(e.to_string()))??;
    let (head, status) = inspect(ctx, &removal.worktree).await?;
    if head != removal.head {
        return Err(Error::Conflict(
            "Worktree HEAD changed after review. Create a new plan.".into(),
        ));
    }
    if status != removal.status {
        return Err(Error::Conflict(
            "Worktree changes differ from the reviewed list. Create a new plan.".into(),
        ));
    }
    ctx.runner
        .run(
            &removal_command(
                ctx,
                &removal.worktree,
                discard_changes && !removal.changes.is_empty(),
            )?,
            &ctx.cancel,
        )
        .await?;
    if removal.worktree.path.try_exists()? || removal.git_dir.try_exists()? {
        return Err(Error::Conflict(
            "Git did not remove the checkout and its registration. Rescan to inspect the result."
                .into(),
        ));
    }
    Ok((
        removal.size.bytes,
        if removal.changes.is_empty() {
            "Worktree removed by Git. Its branch and commits remain in the repository."
        } else {
            "Worktree and reviewed uncommitted/untracked changes permanently removed. Its branch and commits remain in the repository."
        }.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    #[test]
    fn status_preserves_rename_paths_and_ignored_markers_inside_filenames() {
        let (changes, ignored) =
            parse_status("R  renamed\nfile\0!! original\0?? 新文件.txt\0!! node_modules/\0")
                .unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].path, "renamed\nfile");
        assert_eq!(changes[0].original_path.as_deref(), Some("!! original"));
        assert_eq!(changes[1].path, "新文件.txt");
        assert!(ignored);
    }

    #[test]
    fn incomplete_or_lossy_change_lists_never_authorize_removal() {
        for status in [
            " M unfinished",
            "R  missing-original\0",
            "broken\0",
            "?? lossy\u{fffd}\0",
        ] {
            assert!(parse_status(status).is_err());
        }
    }
}
