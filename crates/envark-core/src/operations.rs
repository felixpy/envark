use crate::{
    Error, Result,
    artifact_policy::protect_tracked_files,
    filesystem::{contained_directory, measure, modified, reject_links},
    model::{
        Artifact, Cache, Inventory, Measurement, Progress, ProgressSink, Project, ProviderId,
        Settings, Worktree, now,
    },
    process::CommandSpec,
    providers::{self, Context},
    scanner::{artifact_in_scope, is_project_artifact, project_in_scope},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ActionRequest {
    RemoveWorktrees {
        ids: Vec<String>,
    },
    RemoveWorktree {
        id: String,
    },
    CleanProjects {
        artifact_ids: Vec<String>,
    },
    CleanCaches {
        ids: Vec<String>,
    },
    InstallRuntime {
        provider: ProviderId,
        manager: String,
        version: String,
    },
    SetDefault {
        provider: ProviderId,
        id: String,
    },
    RemoveRuntime {
        provider: ProviderId,
        id: String,
    },
    UpdateTool {
        provider: ProviderId,
        id: String,
    },
    UpdateTools {
        provider: ProviderId,
        ids: Vec<String>,
    },
    RemoveTool {
        provider: ProviderId,
        id: String,
    },
    RemoveAssets {
        provider: ProviderId,
        ids: Vec<String>,
    },
    DownloadAsset {
        provider: ProviderId,
        name: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub title: String,
    pub path: Option<PathBuf>,
    pub command: Option<String>,
    pub bytes: u64,
    pub restore: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub id: String,
    pub kind: String,
    pub created_at: u64,
    pub items: Vec<PlanItem>,
    pub warnings: Vec<String>,
    pub use_trash: bool,
}

#[derive(Debug, Clone)]
enum Step {
    Worktree {
        worktree: Worktree,
        removal: Option<crate::worktree_removal::Removal>,
    },
    Project {
        project: Project,
        artifact: Artifact,
        modified: Option<u64>,
    },
    Asset {
        root: PathBuf,
        path: PathBuf,
        size: Measurement,
        modified: Option<u64>,
    },
    Cache {
        cache: Cache,
        command: CommandSpec,
    },
    Command(CommandSpec),
    Runtime {
        runtime: crate::model::Runtime,
        remove: bool,
        command: CommandSpec,
    },
    Ollama {
        endpoint: String,
        name: String,
        digest: String,
        command: CommandSpec,
    },
    Tool {
        tool: crate::model::Tool,
        remove: bool,
        command: CommandSpec,
    },
}

#[derive(Debug)]
pub struct Plan {
    pub view: PlanView,
    steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub title: String,
    pub status: String,
    pub message: String,
    pub removed_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub items: Vec<ItemResult>,
    pub cancelled: bool,
    pub removed_bytes: u64,
    pub reclaimed_bytes: Option<u64>,
}

fn eligible_project(project: &Project, settings: &Settings) -> Result<()> {
    reject_links(&project.path)?;
    let canonical = std::fs::canonicalize(&project.path)?;
    if project.protected
        || settings
            .protected_projects
            .iter()
            .filter_map(|p| std::fs::canonicalize(p).ok())
            .any(|p| canonical.starts_with(p))
    {
        return Err(Error::UnsafePath("A selected project is protected.".into()));
    }
    if !project_in_scope(&canonical, settings)? {
        return Err(Error::unsafe_path(
            &project.path,
            "the project is outside the current scan scope or is excluded",
        ));
    }
    Ok(())
}

fn command_description(title: String, command: &CommandSpec) -> PlanItem {
    PlanItem {
        title,
        path: None,
        command: Some(command.display()),
        bytes: 0,
        restore: None,
    }
}

fn command_item(title: String, command: CommandSpec, view: &mut PlanView, steps: &mut Vec<Step>) {
    view.items.push(command_description(title, &command));
    steps.push(Step::Command(command));
}

pub async fn prepare(
    request: ActionRequest,
    inventory: &Inventory,
    settings: &Settings,
    ctx: &Context,
) -> Result<Plan> {
    let (inventory, worker_settings, worker) = (inventory.clone(), settings.clone(), ctx.clone());
    let mut plan = tokio::task::spawn_blocking(move || {
        prepare_steps(request, &inventory, &worker_settings, &worker)
    })
    .await
    .map_err(|e| Error::Unavailable(e.to_string()))??;
    for (index, step) in plan.steps.iter_mut().enumerate() {
        if let Step::Worktree { worktree, removal } = step {
            let (reviewed, ignored) =
                crate::worktree_removal::prepare(ctx, worktree.clone(), settings.clone()).await?;
            plan.view.items[index].bytes = reviewed.size.bytes;
            if ignored {
                plan.view.warnings.push("This worktree contains ignored files. They will also be permanently removed and cannot be restored from Git.".into());
            }
            *removal = Some(reviewed);
        }
        if let Step::Runtime {
            runtime,
            remove: true,
            ..
        } = step
        {
            providers::python::verify_removal(ctx, runtime).await?;
        }
        if let Step::Project {
            project, artifact, ..
        } = step
        {
            protect_tracked_files(ctx, project, artifact).await?;
        }
        if let Step::Ollama {
            endpoint,
            name,
            digest,
            ..
        } = step
        {
            providers::ollama::verify(ctx, endpoint, name, digest).await?;
        }
    }
    Ok(plan)
}

fn prepare_steps(
    request: ActionRequest,
    inventory: &Inventory,
    settings: &Settings,
    ctx: &Context,
) -> Result<Plan> {
    let mut view = PlanView {
        id: uuid::Uuid::new_v4().to_string(),
        kind: "operation".into(),
        created_at: now(),
        items: vec![],
        warnings: vec![],
        use_trash: settings.use_trash,
    };
    let mut steps = vec![];
    let is_remove_runtime = matches!(&request, ActionRequest::RemoveRuntime { .. });
    let is_remove_tool = matches!(&request, ActionRequest::RemoveTool { .. });
    match request {
        ActionRequest::RemoveWorktrees { ids } => {
            if ids.is_empty() || ids.len() > 500 {
                return Err(Error::InvalidInput(
                    "Select between 1 and 500 worktrees.".into(),
                ));
            }
            view.kind = "removeWorktree".into();
            let mut seen = HashSet::new();
            for id in ids {
                if !seen.insert(id.clone()) {
                    continue;
                }
                let child = prepare_steps(
                    ActionRequest::RemoveWorktree { id },
                    inventory,
                    settings,
                    ctx,
                )?;
                view.items.extend(child.view.items);
                for warning in child.view.warnings {
                    if !view.warnings.contains(&warning) {
                        view.warnings.push(warning);
                    }
                }
                steps.extend(child.steps);
            }
        }
        ActionRequest::RemoveWorktree { id } => {
            let worktree = inventory
                .worktrees
                .iter()
                .find(|w| w.id == id)
                .ok_or_else(|| Error::Conflict("Worktree is no longer in the inventory.".into()))?;
            let project = inventory
                .projects
                .iter()
                .find(|p| p.id == id)
                .ok_or_else(|| Error::Conflict("Rescan the worktree before removing it.".into()))?;
            eligible_project(project, settings)?;
            if !project.is_worktree || worktree.issue.is_some() || worktree.locked {
                return Err(Error::Conflict(
                    "Only healthy, unlocked linked worktrees can be removed.".into(),
                ));
            }
            let size = worktree
                .size
                .as_ref()
                .filter(|s| s.complete)
                .ok_or_else(|| {
                    Error::Conflict("Rescan the complete worktree before removing it.".into())
                })?;
            view.kind = "removeWorktree".into();
            view.items.push(PlanItem { title: format!("Remove worktree {}", project.name), path: Some(worktree.path.clone()), command: Some(crate::worktree_removal::removal_command(ctx, worktree)?.display()), bytes: size.bytes, restore: Some("Recreate the checkout with git worktree add. The branch and committed source remain in the main repository.".into()) });
            view.warnings.push("Git permanently removes this entire checkout without using Trash. Its branch and commits remain in the main repository. Uncommitted and untracked files, locks, links, and nested repositories block removal; force removal is never used.".into());
            steps.push(Step::Worktree {
                worktree: worktree.clone(),
                removal: None,
            });
        }
        ActionRequest::UpdateTools { provider, ids } => {
            view.kind = "update".into();
            if ids.len() > 500 {
                return Err(Error::InvalidInput("Select at most 500 tools.".into()));
            }
            for id in ids.into_iter().collect::<HashSet<_>>() {
                let child = prepare_steps(
                    ActionRequest::UpdateTool { provider, id },
                    inventory,
                    settings,
                    ctx,
                )?;
                view.items.extend(child.view.items);
                view.warnings.extend(child.view.warnings);
                steps.extend(child.steps);
            }
        }
        ActionRequest::CleanProjects { artifact_ids } => {
            view.kind = "clean".into();
            let mut pending: HashSet<_> = artifact_ids.into_iter().collect();
            for project in &inventory.projects {
                for artifact in &project.artifacts {
                    if !pending.remove(&artifact.id) {
                        continue;
                    }
                    eligible_project(project, settings)?;
                    artifact_in_scope(&artifact.path, settings, &ctx.cancel)?;
                    if !artifact.can_clean
                        || !is_project_artifact(project, artifact)
                        || !artifact.size.complete
                    {
                        return Err(Error::Conflict("An artifact changed or its size scan was incomplete. Scan again before cleaning.".into()));
                    }
                    contained_directory(&project.path, &artifact.path)?;
                    view.items.push(PlanItem {
                        title: format!("{} / {}", project.name, artifact.name),
                        path: Some(artifact.path.clone()),
                        command: None,
                        bytes: artifact.size.bytes,
                        restore: Some(artifact.restore.clone()),
                    });
                    steps.push(Step::Project {
                        project: project.clone(),
                        artifact: artifact.clone(),
                        modified: modified(&artifact.path),
                    });
                }
            }
            if !pending.is_empty() {
                return Err(Error::Conflict(
                    "Some artifacts are no longer in the current inventory.".into(),
                ));
            }
            view.warnings.push(if settings.use_trash { "Items will move to the system Trash. Disk space is released only after the Trash is emptied." } else { "Items will be permanently deleted. Reinstall dependencies or rebuild to restore them." }.into());
        }
        ActionRequest::CleanCaches { ids } => {
            view.kind = "clean".into();
            for id in ids.into_iter().collect::<HashSet<_>>() {
                let cache = inventory
                    .caches
                    .iter()
                    .find(|c| c.id == id)
                    .ok_or_else(|| Error::Conflict("Cache no longer exists.".into()))?;
                if !cache.can_clean {
                    return Err(Error::Unavailable(
                        "The cache owner does not support automatic cleanup.".into(),
                    ));
                }
                reject_links(&cache.path)?;
                let command = providers::cache_command(ctx, cache)?;
                view.items.push(PlanItem {
                    title: cache.name.clone(),
                    path: Some(cache.path.clone()),
                    command: Some(command.display()),
                    bytes: cache.size.bytes,
                    restore: None,
                });
                steps.push(Step::Cache {
                    cache: cache.clone(),
                    command,
                });
            }
            view.warnings.push("Native cache cleanup can permanently remove entries. The displayed size is an upper bound, not a promise of reclaimed space.".into());
        }
        ActionRequest::InstallRuntime {
            provider,
            manager,
            version,
        } => {
            let found = inventory
                .providers
                .iter()
                .find(|p| p.id == provider)
                .and_then(|p| {
                    p.managers
                        .iter()
                        .find(|m| m.name == manager && m.supports_install)
                })
                .ok_or_else(|| {
                    Error::Unavailable("The requested version manager is not available.".into())
                })?;
            command_item(
                format!("Install {version} with {}", found.name),
                providers::runtime_command(ctx, &manager, "install", &version)?,
                &mut view,
                &mut steps,
            );
        }
        ActionRequest::SetDefault { provider, id }
        | ActionRequest::RemoveRuntime { provider, id } => {
            let remove = is_remove_runtime;
            let runtime = inventory
                .providers
                .iter()
                .find(|p| p.id == provider)
                .and_then(|p| p.runtimes.iter().find(|r| r.id == id))
                .ok_or_else(|| Error::Conflict("Runtime no longer exists.".into()))?;
            if !runtime.managed || (remove && (runtime.active || !runtime.active_known)) {
                return Err(Error::Unavailable(
                    "Active, unverified, or externally managed runtimes cannot be removed.".into(),
                ));
            }
            if remove {
                for project in &inventory.projects {
                    if project.providers.contains(&provider) && !project.pins.is_empty() {
                        view.warnings.push(format!("{} has runtime version pins. Verify compatibility before uninstalling.", project.path.display()));
                    }
                }
            } else {
                view.warnings.push("This changes the manager's default. Already-open terminals and Envark may keep their inherited environment until restarted.".into());
            }
            let verb = if remove { "remove" } else { "default" };
            let command = if ["uv", "pyenv"].contains(&runtime.manager.as_str()) {
                providers::python::command(ctx, runtime, verb)?
            } else {
                providers::runtime_command(
                    ctx,
                    &runtime.manager,
                    verb,
                    runtime.selector.as_deref().unwrap_or(&runtime.version),
                )?
            };
            let mut item = command_description(
                format!(
                    "{} {}",
                    if remove { "Remove" } else { "Set default" },
                    runtime.version
                ),
                &command,
            );
            item.path = Some(runtime.path.clone());
            view.items.push(item);
            steps.push(Step::Runtime {
                runtime: runtime.clone(),
                remove,
                command,
            });
        }
        ActionRequest::UpdateTool { provider, id } | ActionRequest::RemoveTool { provider, id } => {
            let remove = is_remove_tool;
            let tool = inventory
                .providers
                .iter()
                .find(|p| p.id == provider)
                .and_then(|p| {
                    p.tools
                        .iter()
                        .chain(p.package_managers.iter())
                        .find(|t| t.id == id)
                })
                .ok_or_else(|| Error::Conflict("Tool no longer exists.".into()))?;
            if (remove && !tool.can_remove) || (!remove && !tool.can_update) {
                return Err(Error::Unavailable(
                    "This installation must be managed with its original installer.".into(),
                ));
            }
            let command = providers::tool_command(ctx, tool, remove)?;
            view.items.push(command_description(
                format!("{} {}", if remove { "Remove" } else { "Update" }, tool.name),
                &command,
            ));
            steps.push(Step::Tool {
                tool: tool.clone(),
                remove,
                command,
            });
            if !remove {
                view.warnings.push(format!("{} will be updated by {}. Updates can include breaking changes and may update its dependencies.", tool.name, tool.source));
            }
        }
        ActionRequest::RemoveAssets { provider, ids } => {
            let owner = inventory
                .providers
                .iter()
                .find(|p| p.id == provider)
                .ok_or_else(|| Error::InvalidInput("Unknown provider.".into()))?;
            for id in ids.into_iter().collect::<HashSet<_>>() {
                let asset = owner
                    .assets
                    .iter()
                    .find(|a| a.id == id)
                    .ok_or_else(|| Error::Conflict("Resource no longer exists.".into()))?;
                if !asset.can_remove {
                    return Err(Error::Unavailable(
                        "Resource ownership is not verified.".into(),
                    ));
                }
                if provider == ProviderId::Ollama {
                    let endpoint = owner
                        .service
                        .as_ref()
                        .filter(|service| service.running)
                        .map(|service| service.endpoint.clone())
                        .ok_or_else(|| {
                            Error::Unavailable(
                                "Refresh the local Ollama service before removing models.".into(),
                            )
                        })?;
                    let command = providers::ollama::command(ctx, &endpoint, "rm", &asset.name)?;
                    view.items.push(command_description(
                        format!("Remove {} at {endpoint}", asset.name),
                        &command,
                    ));
                    steps.push(Step::Ollama {
                        endpoint,
                        name: asset.name.clone(),
                        digest: asset.version.clone(),
                        command,
                    });
                } else {
                    let root = asset
                        .path
                        .parent()
                        .ok_or_else(|| Error::unsafe_path(&asset.path, "missing resource root"))?
                        .to_path_buf();
                    contained_directory(&root, &asset.path)?;
                    view.items.push(PlanItem { title: format!("{} {}", asset.name, asset.version), path: Some(asset.path.clone()), command: None, bytes: asset.size.bytes, restore: Some("Reinstall the browser with the owning project's browser automation CLI.".into()) });
                    steps.push(Step::Asset {
                        root,
                        path: asset.path.clone(),
                        size: asset.size.clone(),
                        modified: modified(&asset.path),
                    });
                }
            }
            view.warnings.push("Usage is not fully observable. Projects outside your scan roots may still need these resources. Shared model layers can reduce actual reclaimed space.".into());
        }
        ActionRequest::DownloadAsset {
            provider: ProviderId::Ollama,
            name,
        } => {
            let endpoint = providers::ollama::ENDPOINT;
            let command = providers::ollama::command(ctx, endpoint, "pull", &name)?;
            command_item(
                format!("Download {name} at {endpoint}"),
                command,
                &mut view,
                &mut steps,
            );
        }
        ActionRequest::DownloadAsset { .. } => {
            return Err(Error::Unavailable(
                "Install browsers through the package version used by a selected project.".into(),
            ));
        }
    }
    if steps.is_empty() || steps.len() > 500 {
        return Err(Error::InvalidInput(
            "Select between 1 and 500 items.".into(),
        ));
    }
    Ok(Plan { view, steps })
}

fn remove_directory(
    root: &Path,
    path: &Path,
    expected: &Measurement,
    timestamp: Option<u64>,
    use_trash: bool,
    token: &CancellationToken,
) -> Result<u64> {
    let canonical = contained_directory(root, path)?;
    if modified(path) != timestamp {
        return Err(Error::Conflict(
            "The directory changed after review. Create a new plan.".into(),
        ));
    }
    let current = measure(path, token)?;
    if !current.complete
        || current.bytes != expected.bytes
        || expected.fingerprint.is_none()
        || current.fingerprint != expected.fingerprint
    {
        return Err(Error::Conflict(
            "The contents changed after review. Scan again before cleaning.".into(),
        ));
    }
    if token.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if contained_directory(root, path)? != canonical || modified(path) != timestamp {
        return Err(Error::Conflict(
            "The directory changed during validation.".into(),
        ));
    }
    if use_trash {
        trash::delete(path).map_err(|e| Error::Unavailable(e.to_string()))?;
    } else {
        std::fs::remove_dir_all(path)?;
    }
    // Logical bytes removed from this location; hard links and Trash can retain disk blocks.
    Ok(current.bytes)
}

async fn clean_project(
    ctx: &Context,
    project: Project,
    artifact: Artifact,
    timestamp: Option<u64>,
    settings: Settings,
) -> Result<(u64, String)> {
    protect_tracked_files(ctx, &project, &artifact).await?;
    let token = ctx.cancel.clone();
    let use_trash = settings.use_trash;
    let bytes = tokio::task::spawn_blocking(move || {
        eligible_project(&project, &settings)?;
        artifact_in_scope(&artifact.path, &settings, &token)?;
        if !is_project_artifact(&project, &artifact) {
            return Err(Error::Conflict(
                "Project markers or artifact ownership changed.".into(),
            ));
        }
        remove_directory(
            &project.path,
            &artifact.path,
            &artifact.size,
            timestamp,
            use_trash,
            &token,
        )
    })
    .await
    .map_err(|e| Error::Unavailable(e.to_string()))??;
    Ok((
        bytes,
        if use_trash {
            "Moved to Trash."
        } else {
            "Removed."
        }
        .into(),
    ))
}

async fn measured_bytes(path: PathBuf, token: CancellationToken) -> Result<u64> {
    tokio::task::spawn_blocking(move || {
        let size = measure(&path, &token)?;
        if !size.complete {
            return Err(Error::Conflict("The cache cannot be measured completely. Check access permissions before cleaning.".into()));
        }
        Ok(size.bytes)
    }).await.map_err(|e| Error::Unavailable(e.to_string()))?
}

async fn clean_cache(ctx: &Context, cache: Cache, command: CommandSpec) -> Result<(u64, String)> {
    reject_links(&cache.path)?;
    let approved = std::fs::canonicalize(&cache.path)?;
    if let Some(probe) = providers::cache_probe(&cache, &command) {
        let output = ctx.runner.run(&probe, &ctx.cancel).await?;
        let resolved = output
            .stdout
            .lines()
            .rev()
            .map(str::trim)
            .filter(|line| Path::new(line).is_absolute())
            .find_map(|line| std::fs::canonicalize(line).ok());
        if resolved.as_ref() != Some(&approved) {
            return Err(Error::Conflict("The tool resolved a different cache directory. Refresh the inventory before cleaning.".into()));
        }
    }
    let before = measured_bytes(cache.path.clone(), ctx.cancel.clone()).await?;
    reject_links(&cache.path)?;
    if std::fs::canonicalize(&cache.path)? != approved {
        return Err(Error::Conflict("The cache moved after review.".into()));
    }
    let output = ctx.runner.run(&command, &ctx.cancel).await?;
    let after = if cache.path.try_exists()? {
        measured_bytes(cache.path, ctx.cancel.clone()).await?
    } else {
        0
    };
    Ok((
        before.saturating_sub(after),
        output.stdout.trim().chars().take(4000).collect(),
    ))
}

async fn run_runtime(
    ctx: &Context,
    runtime: crate::model::Runtime,
    remove: bool,
    command: CommandSpec,
) -> Result<(u64, String)> {
    if remove {
        providers::python::verify_removal(ctx, &runtime).await?;
    }
    if ["uv", "pyenv"].contains(&runtime.manager.as_str()) {
        let current =
            providers::python::command(ctx, &runtime, if remove { "remove" } else { "default" })?;
        if current.program != command.program
            || current.args != command.args
            || current.env != command.env
        {
            return Err(Error::Conflict(
                "The runtime manager changed after review. Create a new plan.".into(),
            ));
        }
    }
    let output = ctx.runner.run(&command, &ctx.cancel).await?;
    Ok((0, output.stdout.trim().chars().take(4000).collect()))
}

async fn run_tool(
    ctx: &Context,
    tool: crate::model::Tool,
    remove: bool,
    command: CommandSpec,
) -> Result<(u64, String)> {
    if !remove {
        providers::updates::verify_installed(ctx, &tool).await?;
    }
    if tool.source == "pnpm" {
        let path = tool.path.as_ref().ok_or_else(|| {
            Error::Conflict("The tool no longer has an installation path.".into())
        })?;
        let root = if tool.name.starts_with('@') {
            path.parent().and_then(Path::parent)
        } else {
            path.parent()
        }
        .ok_or_else(|| Error::Conflict("The tool no longer has an installation root.".into()))?;
        let mut probe = ctx.command("pnpm", &["root", "--global"])?;
        probe.cwd = command.cwd.clone();
        probe.env = command.env.clone();
        let output = ctx.runner.run(&probe, &ctx.cancel).await?;
        let current = output
            .stdout
            .lines()
            .rev()
            .map(str::trim)
            .filter(|line| Path::new(line).is_absolute())
            .find_map(|line| std::fs::canonicalize(line).ok());
        if probe.program != command.program
            || current.as_ref() != Some(&std::fs::canonicalize(root)?)
        {
            return Err(Error::Conflict(
                "The pnpm global installation changed. Refresh before updating or removing tools."
                    .into(),
            ));
        }
    }
    let output = ctx.runner.run(&command, &ctx.cancel).await?;
    Ok((0, output.stdout.trim().chars().take(4000).collect()))
}

pub async fn execute(
    plan: Plan,
    settings: Settings,
    ctx: Context,
    progress: ProgressSink,
    job_id: String,
) -> Result<OperationResult> {
    if now().saturating_sub(plan.view.created_at) > 600 {
        return Err(Error::Conflict(
            "This plan expired. Review the operation again.".into(),
        ));
    }
    let mut result = OperationResult {
        items: vec![],
        cancelled: false,
        removed_bytes: 0,
        reclaimed_bytes: None,
    };
    for (index, step) in plan.steps.into_iter().enumerate() {
        if ctx.cancel.is_cancelled() {
            result.cancelled = true;
            break;
        }
        let title = plan.view.items[index].title.clone();
        progress(Progress {
            job_id: job_id.clone(),
            stage: "execute".into(),
            completed: index as u64,
            total: Some(plan.view.items.len() as u64),
            message: title.clone(),
        });
        let outcome = match step {
            Step::Worktree {
                removal: Some(removal),
                ..
            } => crate::worktree_removal::execute(&ctx, removal, settings.clone()).await,
            Step::Worktree { removal: None, .. } => Err(Error::Conflict(
                "Worktree removal was not validated.".into(),
            )),
            Step::Runtime {
                runtime,
                remove,
                command,
            } => run_runtime(&ctx, runtime, remove, command).await,
            Step::Ollama {
                endpoint,
                name,
                digest,
                command,
            } => match providers::ollama::verify(&ctx, &endpoint, &name, &digest).await {
                Ok(()) => ctx
                    .runner
                    .run(&command, &ctx.cancel)
                    .await
                    .map(|output| (0, output.stdout.trim().chars().take(4000).collect())),
                Err(error) => Err(error),
            },
            Step::Tool {
                tool,
                remove,
                command,
            } => run_tool(&ctx, tool, remove, command).await,
            Step::Command(command) => ctx.runner.run(&command, &ctx.cancel).await.map(|output| {
                (
                    0,
                    output.stdout.trim().chars().take(4000).collect::<String>(),
                )
            }),
            Step::Cache { cache, command } => clean_cache(&ctx, cache, command).await,
            Step::Project {
                project,
                artifact,
                modified,
            } => clean_project(&ctx, project, artifact, modified, settings.clone()).await,
            Step::Asset {
                root,
                path,
                size,
                modified,
            } => {
                let token = ctx.cancel.clone();
                let use_trash = settings.use_trash;
                tokio::task::spawn_blocking(move || {
                    remove_directory(&root, &path, &size, modified, use_trash, &token)
                })
                .await
                .map_err(|e| Error::Unavailable(e.to_string()))?
                .map(|bytes| {
                    (
                        bytes,
                        if use_trash {
                            "Moved to Trash."
                        } else {
                            "Removed."
                        }
                        .into(),
                    )
                })
            }
        };
        match outcome {
            Ok((bytes, message)) => {
                result.removed_bytes += bytes;
                result.items.push(ItemResult {
                    title,
                    status: "success".into(),
                    message,
                    removed_bytes: bytes,
                });
            }
            Err(Error::Cancelled) => {
                result.cancelled = true;
                result.items.push(ItemResult {
                    title,
                    status: "cancelled".into(),
                    message: "Cancellation requested; inspect the inventory for partial changes."
                        .into(),
                    removed_bytes: 0,
                });
                break;
            }
            Err(e) => result.items.push(ItemResult {
                title,
                status: "failed".into(),
                message: e.to_string(),
                removed_bytes: 0,
            }),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::silent_progress, scanner};

    #[tokio::test]
    async fn rust_runtime_actions_use_the_toolchain_selector_instead_of_the_compiler_version() {
        let root = tempfile::tempdir().unwrap();
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = root.path().into();
        let bin = ctx.home.join(".cargo/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(
            bin.join(if cfg!(windows) {
                "rustup.exe"
            } else {
                "rustup"
            }),
            "fixture",
        )
        .unwrap();
        let selector = "nightly-aarch64-apple-darwin";
        let mut provider = crate::model::Provider::empty(ProviderId::Rust);
        provider.runtimes.push(crate::model::Runtime {
            id: "nightly".into(),
            version: "1.100.0-nightly".into(),
            selector: Some(selector.into()),
            manager: "rustup".into(),
            path: root.path().join("toolchains").join(selector),
            active: false,
            active_known: true,
            managed: true,
            size: None,
            note: None,
        });
        let inventory = Inventory {
            providers: vec![provider],
            ..Default::default()
        };
        for (request, args) in [
            (
                ActionRequest::SetDefault {
                    provider: ProviderId::Rust,
                    id: "nightly".into(),
                },
                vec!["default", selector],
            ),
            (
                ActionRequest::RemoveRuntime {
                    provider: ProviderId::Rust,
                    id: "nightly".into(),
                },
                vec!["toolchain", "uninstall", selector],
            ),
        ] {
            let plan = prepare(request, &inventory, &Settings::default(), &ctx)
                .await
                .unwrap();
            let Step::Runtime { command, .. } = &plan.steps[0] else {
                panic!("expected a runtime operation");
            };
            assert_eq!(command.args, args);
        }
    }

    #[tokio::test]
    async fn hard_link_cleanup_reports_logical_bytes_without_claiming_disk_reclamation() {
        let root = tempfile::tempdir().unwrap();
        let root_path = std::fs::canonicalize(root.path()).unwrap();
        let path = root_path.join("artifact");
        std::fs::create_dir(&path).unwrap();
        let shared = root_path.join("shared-store");
        std::fs::write(&shared, vec![1_u8; 2 * 1024 * 1024]).unwrap();
        std::fs::hard_link(&shared, path.join("package")).unwrap();
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let size = measure(&path, &ctx.cancel).unwrap();
        let plan = Plan {
            view: PlanView {
                id: "test".into(),
                kind: "clean".into(),
                created_at: now(),
                items: vec![PlanItem {
                    title: "fixture".into(),
                    path: Some(path.clone()),
                    command: None,
                    bytes: size.bytes,
                    restore: None,
                }],
                warnings: vec![],
                use_trash: false,
            },
            steps: vec![Step::Asset {
                root: root_path,
                modified: modified(&path),
                path,
                size,
            }],
        };
        let result = execute(
            plan,
            Settings {
                use_trash: false,
                ..Default::default()
            },
            ctx,
            silent_progress(),
            "test".into(),
        )
        .await
        .unwrap();
        assert_eq!(result.items[0].status, "success");
        assert_eq!(result.removed_bytes, 2 * 1024 * 1024);
        assert_eq!(result.reclaimed_bytes, None);
        assert_eq!(
            std::fs::metadata(shared).unwrap().len(),
            result.removed_bytes
        );
    }

    #[tokio::test]
    async fn workspace_dependencies_without_local_markers_can_be_cleaned_without_removing_source() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        crate::git::init(&root);
        std::fs::write(
            root.join("package.json"),
            r#"{"workspaces":["packages/*"]}"#,
        )
        .unwrap();
        std::fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").unwrap();
        for name in ["web", "api"] {
            let package = root.join("packages").join(name);
            std::fs::create_dir_all(package.join("node_modules/dep")).unwrap();
            std::fs::write(package.join("package.json"), "{}").unwrap();
            std::fs::write(package.join("source.ts"), "source").unwrap();
            std::fs::write(package.join("node_modules/dep/index.js"), "generated").unwrap();
        }
        let settings = Settings {
            roots: vec![root.clone()],
            use_trash: false,
            ..Default::default()
        };
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let scanned = scanner::scan(&settings, &ctx.cancel, silent_progress(), "test").unwrap();
        let inventory = Inventory {
            projects: scanned.projects,
            ..Default::default()
        };
        let artifacts = &inventory.projects[0].artifacts;
        assert_eq!(artifacts.len(), 2);
        assert!(artifacts.iter().all(|a| a.can_clean));
        let plan = prepare(
            ActionRequest::CleanProjects {
                artifact_ids: artifacts.iter().map(|a| a.id.clone()).collect(),
            },
            &inventory,
            &settings,
            &ctx,
        )
        .await
        .unwrap();
        let result = execute(plan, settings, ctx, silent_progress(), "test".into())
            .await
            .unwrap();
        assert!(result.items.iter().all(|item| item.status == "success"));
        for name in ["web", "api"] {
            let package = root.join("packages").join(name);
            assert!(!package.join("node_modules").exists());
            assert!(package.join("source.ts").is_file());
            assert!(package.join("package.json").is_file());
        }
        assert!(root.join("pnpm-lock.yaml").is_file());
    }

    #[tokio::test]
    async fn cleanup_preserves_custom_build_sources_and_git_tracked_dependencies() {
        let root = tempfile::tempdir().unwrap();
        let project = std::fs::canonicalize(root.path()).unwrap();
        std::fs::create_dir_all(project.join("build/src")).unwrap();
        std::fs::create_dir_all(project.join("node_modules/pkg")).unwrap();
        std::fs::write(
            project.join("build.gradle"),
            "layout.buildDirectory = file('out')",
        )
        .unwrap();
        std::fs::write(project.join("build/src/Main.java"), "class Main {}").unwrap();
        std::fs::write(project.join("package.json"), "{}").unwrap();
        std::fs::write(project.join("node_modules/.package-lock.json"), "{}").unwrap();
        std::fs::write(project.join("node_modules/pkg/code.js"), "tracked source").unwrap();
        let ctx = Context::new(CancellationToken::new()).unwrap();
        for args in [vec!["init", "--quiet"], vec!["add", "--", "node_modules"]] {
            let mut command = ctx.command("git", &args).unwrap();
            command.cwd = Some(project.clone());
            ctx.runner.run(&command, &ctx.cancel).await.unwrap();
        }
        let settings = Settings {
            roots: vec![project.clone()],
            use_trash: false,
            ..Default::default()
        };
        let scan = scanner::scan(&settings, &ctx.cancel, silent_progress(), "test").unwrap();
        let inventory = Inventory {
            projects: scan.projects,
            ..Default::default()
        };
        for artifact in &inventory.projects[0].artifacts {
            assert!(
                prepare(
                    ActionRequest::CleanProjects {
                        artifact_ids: vec![artifact.id.clone()]
                    },
                    &inventory,
                    &settings,
                    &ctx
                )
                .await
                .is_err()
            );
        }
        assert_eq!(
            std::fs::read_to_string(project.join("build/src/Main.java")).unwrap(),
            "class Main {}"
        );

        let mut reset = ctx
            .command("git", &["rm", "--cached", "-r", "--", "node_modules"])
            .unwrap();
        reset.cwd = Some(project.clone());
        ctx.runner.run(&reset, &ctx.cancel).await.unwrap();
        let artifact = inventory.projects[0]
            .artifacts
            .iter()
            .find(|a| a.name == "node_modules")
            .unwrap();
        let plan = prepare(
            ActionRequest::CleanProjects {
                artifact_ids: vec![artifact.id.clone()],
            },
            &inventory,
            &settings,
            &ctx,
        )
        .await
        .unwrap();
        let mut add = ctx.command("git", &["add", "--", "node_modules"]).unwrap();
        add.cwd = Some(project.clone());
        ctx.runner.run(&add, &ctx.cancel).await.unwrap();
        let result = execute(plan, settings, ctx, silent_progress(), "test".into())
            .await
            .unwrap();
        assert_eq!(result.items[0].status, "failed");
        assert!(project.join("node_modules/pkg/code.js").exists());
    }

    #[tokio::test]
    async fn cleanup_rechecks_protection_exclusions_and_scan_roots() {
        let root = tempfile::tempdir().unwrap();
        let root_path = std::fs::canonicalize(root.path()).unwrap();
        let project = root_path.join("archived/project");
        std::fs::create_dir_all(project.join("node_modules/pkg")).unwrap();
        crate::git::init(&project);
        std::fs::write(project.join("package.json"), "{}").unwrap();
        std::fs::write(project.join("node_modules/.package-lock.json"), "{}").unwrap();
        std::fs::write(project.join("node_modules/pkg/code.js"), "generated").unwrap();
        let settings = Settings {
            roots: vec![root_path],
            use_trash: false,
            ..Default::default()
        };
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let scan = scanner::scan(&settings, &ctx.cancel, silent_progress(), "test").unwrap();
        let id = scan.projects[0].artifacts[0].id.clone();
        let inventory = Inventory {
            projects: scan.projects,
            ..Default::default()
        };
        let mut excluded = settings.clone();
        excluded.excludes.push("archived".into());
        let mut protected = settings.clone();
        protected.protected_projects.push(project.clone());
        let mut removed_root = settings.clone();
        removed_root.roots.clear();
        let mut excluded_artifact = settings.clone();
        excluded_artifact.excludes.push("node_modules".into());
        let mut excluded_child = settings.clone();
        excluded_child
            .excludes
            .push("archived/project/node_modules/pkg/**".into());
        for changed in [
            excluded,
            protected,
            removed_root,
            excluded_artifact,
            excluded_child,
        ] {
            let request = ActionRequest::CleanProjects {
                artifact_ids: vec![id.clone()],
            };
            assert!(
                prepare(request.clone(), &inventory, &changed, &ctx)
                    .await
                    .is_err()
            );
            let plan = prepare(request, &inventory, &settings, &ctx).await.unwrap();
            let result = execute(plan, changed, ctx.clone(), silent_progress(), "test".into())
                .await
                .unwrap();
            assert_eq!(result.items[0].status, "failed");
            assert!(project.join("node_modules/pkg/code.js").is_file());
        }
    }

    #[tokio::test]
    async fn cleanup_preserves_source_and_rejects_changed_content() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir_all(project.join("node_modules/pkg")).unwrap();
        crate::git::init(&project);
        std::fs::write(project.join("package.json"), "{}").unwrap();
        std::fs::write(project.join("index.ts"), "source").unwrap();
        std::fs::write(project.join("node_modules/.package-lock.json"), "{}").unwrap();
        std::fs::write(project.join("node_modules/pkg/index.js"), "generated").unwrap();
        let settings = Settings {
            roots: vec![std::fs::canonicalize(root.path()).unwrap()],
            use_trash: false,
            ..Default::default()
        };
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let snapshot = scanner::scan(&settings, &ctx.cancel, silent_progress(), "test").unwrap();
        let id = snapshot.projects[0].artifacts[0].id.clone();
        let inventory = Inventory {
            projects: snapshot.projects,
            ..Default::default()
        };
        let plan = prepare(
            ActionRequest::CleanProjects {
                artifact_ids: vec![id.clone()],
            },
            &inventory,
            &settings,
            &ctx,
        )
        .await
        .unwrap();
        std::fs::write(
            project.join("node_modules/pkg/index.js"),
            "changed contents",
        )
        .unwrap();
        let result = execute(
            plan,
            settings.clone(),
            ctx.clone(),
            silent_progress(),
            "test".into(),
        )
        .await
        .unwrap();
        assert_eq!(result.items[0].status, "failed");
        assert!(project.join("node_modules").exists());
        let snapshot = scanner::scan(&settings, &ctx.cancel, silent_progress(), "test").unwrap();
        let inventory = Inventory {
            projects: snapshot.projects,
            ..Default::default()
        };
        let plan = prepare(
            ActionRequest::CleanProjects {
                artifact_ids: vec![id],
            },
            &inventory,
            &settings,
            &ctx,
        )
        .await
        .unwrap();
        let result = execute(plan, settings, ctx, silent_progress(), "test".into())
            .await
            .unwrap();
        assert_eq!(result.items[0].status, "success");
        assert!(!project.join("node_modules").exists());
        assert_eq!(
            std::fs::read_to_string(project.join("index.ts")).unwrap(),
            "source"
        );
        assert!(project.join("package.json").exists());
    }

    #[tokio::test]
    async fn linked_worktree_cleanup_revalidates_main_repository_scope_and_preserves_checkout() {
        let root = tempfile::tempdir().unwrap();
        let main = root.path().join("main");
        let linked = root.path().join("linked elsewhere");
        std::fs::create_dir(&main).unwrap();
        crate::git::init(&main);
        crate::git::add_worktree(&main, &linked);
        let nested = linked.join("apps/web");
        std::fs::create_dir_all(nested.join("node_modules/pkg")).unwrap();
        std::fs::write(nested.join("package.json"), "{}").unwrap();
        std::fs::write(nested.join("source.ts"), "source").unwrap();
        std::fs::write(nested.join("node_modules/.package-lock.json"), "{}").unwrap();
        std::fs::write(nested.join("node_modules/pkg/index.js"), "generated").unwrap();
        let settings = Settings {
            roots: vec![std::fs::canonicalize(main).unwrap()],
            use_trash: false,
            ..Default::default()
        };
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let scanned = scanner::scan(&settings, &ctx.cancel, silent_progress(), "test").unwrap();
        let project = scanned.projects.iter().find(|p| p.is_worktree).unwrap();
        assert!(project.artifacts[0].can_clean);
        let request = ActionRequest::CleanProjects {
            artifact_ids: vec![project.artifacts[0].id.clone()],
        };
        let inventory = Inventory {
            projects: scanned.projects,
            ..Default::default()
        };
        let plan = prepare(request.clone(), &inventory, &settings, &ctx)
            .await
            .unwrap();
        let mut changed = settings.clone();
        changed.roots.clear();
        let result = execute(plan, changed, ctx.clone(), silent_progress(), "test".into())
            .await
            .unwrap();
        assert_eq!(result.items[0].status, "failed");
        assert!(nested.join("node_modules").is_dir());
        let plan = prepare(request.clone(), &inventory, &settings, &ctx)
            .await
            .unwrap();
        let pointer = std::fs::read(linked.join(".git")).unwrap();
        std::fs::write(linked.join(".git"), "gitdir: missing").unwrap();
        let result = execute(
            plan,
            settings.clone(),
            ctx.clone(),
            silent_progress(),
            "test".into(),
        )
        .await
        .unwrap();
        assert_eq!(result.items[0].status, "failed");
        std::fs::write(linked.join(".git"), pointer).unwrap();
        let plan = prepare(request, &inventory, &settings, &ctx).await.unwrap();
        let result = execute(plan, settings, ctx, silent_progress(), "test".into())
            .await
            .unwrap();
        assert_eq!(result.items[0].status, "success");
        assert!(!nested.join("node_modules").exists());
        assert!(nested.join("source.ts").is_file());
        assert!(linked.join(".git").is_file());
    }
}
