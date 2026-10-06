use crate::{
    Error, Result,
    filesystem::{is_link, measure, reject_links},
    git,
    model::{Measurement, Settings, Worktree},
    process::CommandSpec,
    providers::Context,
    scanner::project_in_scope,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub(crate) struct Removal {
    pub worktree: Worktree,
    pub size: Measurement,
    git_dir: PathBuf,
    common_dir: PathBuf,
    head: String,
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
        .any(|p| path.starts_with(p))
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

async fn inspect(ctx: &Context, worktree: &Worktree) -> Result<(String, bool)> {
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
                ],
            )?,
            &ctx.cancel,
        )
        .await?;
    let mut ignored = false;
    for entry in status.stdout.split('\0').filter(|s| !s.is_empty()) {
        if entry.starts_with("!! ") {
            ignored = true;
        } else {
            return Err(Error::Conflict("This worktree has uncommitted or untracked files. Commit, stash, or move them before removing it.".into()));
        }
    }
    let head = ctx
        .runner
        .run(
            &command(ctx, &worktree.path, &["rev-parse", "--verify", "HEAD"])?,
            &ctx.cancel,
        )
        .await?;
    Ok((head.stdout.trim().to_owned(), ignored))
}

fn measure_removal(worktree: &Worktree, ctx: &Context) -> Result<Measurement> {
    for entry in WalkDir::new(&worktree.path)
        .follow_links(false)
        .max_open(16)
    {
        if ctx.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let entry = entry.map_err(|e| Error::Unavailable(e.to_string()))?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if is_link(&metadata)
            || (entry.file_name() == ".git" && entry.path() != worktree.path.join(".git"))
        {
            return Err(Error::unsafe_path(
                entry.path(),
                "worktree contains a link, nested repository, or submodule; remove it through Git manually",
            ));
        }
    }
    let size = measure(&worktree.path, &ctx.cancel)?;
    if !size.complete {
        return Err(Error::Conflict(
            "Worktree size could not be measured completely. Check permissions and rescan.".into(),
        ));
    }
    Ok(size)
}

pub(crate) fn removal_command(ctx: &Context, worktree: &Worktree) -> Result<CommandSpec> {
    let mut spec = command(
        ctx,
        &worktree.repository.path,
        &["worktree", "remove", "--", &worktree.path.to_string_lossy()],
    )?;
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
    let (checkout, size) = tokio::task::spawn_blocking(move || {
        let checkout = validate(&item, &settings)?;
        let size = measure_removal(&item, &worker)?;
        let scanned = item
            .size
            .as_ref()
            .ok_or_else(|| Error::Conflict("Rescan before removing this worktree.".into()))?;
        if !scanned.complete || scanned.fingerprint.is_none() {
            return Err(Error::Conflict(
                "Rescan the complete worktree before removing it.".into(),
            ));
        }
        Ok::<_, Error>((checkout, size))
    })
    .await
    .map_err(|e| Error::Unavailable(e.to_string()))??;
    let (head, ignored) = inspect(ctx, &worktree).await?;
    Ok((
        Removal {
            worktree,
            size,
            git_dir: checkout.git_dir,
            common_dir: checkout.common_dir,
            head,
        },
        ignored,
    ))
}

pub(crate) async fn execute(
    ctx: &Context,
    removal: Removal,
    settings: Settings,
) -> Result<(u64, String)> {
    let worker = ctx.clone();
    let reviewed = removal.clone();
    tokio::task::spawn_blocking(move || {
        let checkout = validate(&reviewed.worktree, &settings)?;
        if checkout.git_dir != reviewed.git_dir
            || checkout.common_dir != reviewed.common_dir
            || measure_removal(&reviewed.worktree, &worker)?.fingerprint
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
    if inspect(ctx, &removal.worktree).await?.0 != removal.head {
        return Err(Error::Conflict(
            "Worktree HEAD changed after review. Create a new plan.".into(),
        ));
    }
    ctx.runner
        .run(&removal_command(ctx, &removal.worktree)?, &ctx.cancel)
        .await?;
    if removal.worktree.path.try_exists()? || removal.git_dir.try_exists()? {
        return Err(Error::Conflict(
            "Git did not remove the checkout and its registration. Rescan to inspect the result."
                .into(),
        ));
    }
    Ok((
        removal.size.bytes,
        "Worktree removed by Git. Its branch and commits remain in the repository.".into(),
    ))
}
