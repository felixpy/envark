use crate::{
    Error, Result,
    filesystem::{contained_directory, measure, modified, reject_links},
    model::{
        Artifact, Cache, Inventory, Progress, ProgressSink, Project, ProviderId, Settings, now,
    },
    process::CommandSpec,
    providers::{self, Context},
    scanner::is_project_artifact,
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
    Project {
        project: Project,
        artifact: Artifact,
        modified: Option<u64>,
    },
    Asset {
        root: PathBuf,
        path: PathBuf,
        bytes: u64,
        modified: Option<u64>,
    },
    Cache {
        cache: Cache,
        command: CommandSpec,
    },
    Command(CommandSpec),
}

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
    pub freed_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub items: Vec<ItemResult>,
    pub cancelled: bool,
    pub freed_bytes: u64,
}

fn eligible_project(project: &Project, settings: &Settings) -> Result<()> {
    reject_links(&project.path)?;
    if project.protected
        || settings
            .protected_projects
            .iter()
            .any(|p| project.path.starts_with(p))
    {
        return Err(Error::UnsafePath("A selected project is protected.".into()));
    }
    let canonical = std::fs::canonicalize(&project.path)?;
    let allowed = settings
        .roots
        .iter()
        .filter_map(|p| std::fs::canonicalize(p).ok())
        .any(|root| canonical.starts_with(root));
    if !allowed {
        return Err(Error::unsafe_path(
            &project.path,
            "the project is outside the current scan roots",
        ));
    }
    Ok(())
}

fn command_item(title: String, command: CommandSpec, view: &mut PlanView, steps: &mut Vec<Step>) {
    view.items.push(PlanItem {
        title,
        path: None,
        command: Some(command.display()),
        bytes: 0,
        restore: None,
    });
    steps.push(Step::Command(command));
}

pub fn prepare(
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
        ActionRequest::CleanProjects { artifact_ids } => {
            view.kind = "clean".into();
            let mut pending: HashSet<_> = artifact_ids.into_iter().collect();
            for project in &inventory.projects {
                for artifact in &project.artifacts {
                    if !pending.remove(&artifact.id) {
                        continue;
                    }
                    eligible_project(project, settings)?;
                    if !is_project_artifact(project, artifact) || !artifact.size.complete {
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
            if !runtime.managed || (remove && runtime.active) {
                return Err(Error::Unavailable(
                    "Active or externally managed runtimes cannot be removed.".into(),
                ));
            }
            if remove {
                for project in &inventory.projects {
                    if project.providers.contains(&provider) && !project.pins.is_empty() {
                        view.warnings.push(format!("{} has runtime version pins. Verify compatibility before uninstalling.", project.path.display()));
                    }
                }
            }
            command_item(
                format!(
                    "{} {}",
                    if remove { "Remove" } else { "Set default" },
                    runtime.version
                ),
                providers::runtime_command(
                    ctx,
                    &runtime.manager,
                    if remove { "remove" } else { "default" },
                    &runtime.version,
                )?,
                &mut view,
                &mut steps,
            );
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
            command_item(
                format!("{} {}", if remove { "Remove" } else { "Update" }, tool.name),
                providers::tool_command(ctx, tool, remove)?,
                &mut view,
                &mut steps,
            );
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
                    providers::valid_identifier(&asset.name)?;
                    command_item(
                        format!("Remove {}", asset.name),
                        ctx.command("ollama", &["rm", &asset.name])?,
                        &mut view,
                        &mut steps,
                    );
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
                        bytes: asset.size.bytes,
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
            providers::valid_identifier(&name)?;
            let mut command = ctx.command("ollama", &["pull", &name])?;
            command.timeout = std::time::Duration::from_secs(7200);
            command_item(format!("Download {name}"), command, &mut view, &mut steps);
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
    bytes: u64,
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
    if !current.complete || current.bytes != bytes {
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
        Ok(0)
    } else {
        std::fs::remove_dir_all(path)?;
        Ok(current.bytes)
    }
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
        freed_bytes: 0,
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
            Step::Command(command) => ctx.runner.run(&command, &ctx.cancel).await.map(|output| {
                (
                    0,
                    output.stdout.trim().chars().take(4000).collect::<String>(),
                )
            }),
            Step::Cache { cache, command } => match ctx.runner.run(&command, &ctx.cancel).await {
                Ok(output) => {
                    let cancel = ctx.cancel.clone();
                    let path = cache.path.clone();
                    let after = tokio::task::spawn_blocking(move || measure(&path, &cancel))
                        .await
                        .ok()
                        .and_then(std::result::Result::ok);
                    Ok((
                        after
                            .filter(|m| m.complete)
                            .map(|m| cache.size.bytes.saturating_sub(m.bytes))
                            .unwrap_or(0),
                        output.stdout.trim().chars().take(4000).collect::<String>(),
                    ))
                }
                Err(e) => Err(e),
            },
            Step::Project {
                project,
                artifact,
                modified,
            } => {
                let token = ctx.cancel.clone();
                let settings = settings.clone();
                let use_trash = settings.use_trash;
                tokio::task::spawn_blocking(move || {
                    eligible_project(&project, &settings)?;
                    if !is_project_artifact(&project, &artifact) {
                        return Err(Error::Conflict(
                            "Project markers or artifact ownership changed.".into(),
                        ));
                    }
                    remove_directory(
                        &project.path,
                        &artifact.path,
                        artifact.size.bytes,
                        modified,
                        settings.use_trash,
                        &token,
                    )
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
            Step::Asset {
                root,
                path,
                bytes,
                modified,
            } => {
                let token = ctx.cancel.clone();
                let use_trash = settings.use_trash;
                tokio::task::spawn_blocking(move || {
                    remove_directory(&root, &path, bytes, modified, use_trash, &token)
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
                result.freed_bytes += bytes;
                result.items.push(ItemResult {
                    title,
                    status: "success".into(),
                    message,
                    freed_bytes: bytes,
                });
            }
            Err(Error::Cancelled) => {
                result.cancelled = true;
                result.items.push(ItemResult {
                    title,
                    status: "cancelled".into(),
                    message: "Cancellation requested; inspect the inventory for partial changes."
                        .into(),
                    freed_bytes: 0,
                });
                break;
            }
            Err(e) => result.items.push(ItemResult {
                title,
                status: "failed".into(),
                message: e.to_string(),
                freed_bytes: 0,
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
    async fn cleanup_preserves_source_and_rejects_changed_content() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir_all(project.join("node_modules/pkg")).unwrap();
        std::fs::write(project.join("package.json"), "{}").unwrap();
        std::fs::write(project.join("index.ts"), "source").unwrap();
        std::fs::write(project.join("node_modules/pkg/index.js"), "generated").unwrap();
        let settings = Settings {
            roots: vec![root.path().into()],
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
}
