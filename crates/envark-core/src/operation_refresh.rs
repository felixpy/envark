use crate::{
    Error, Result,
    filesystem::{is_link, measure_with},
    git,
    model::{Inventory, Measurement, Progress, ProgressSink},
    operations::{OperationResult, RefreshTarget},
    providers::{self, Context},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn measure_changed(
    path: &Path,
    ctx: &Context,
    progress: &ProgressSink,
    job_id: &str,
) -> Result<Measurement> {
    let mut last = Instant::now();
    let mut files = 0;
    progress(Progress {
        job_id: job_id.into(),
        stage: "refresh-affected".into(),
        completed: 0,
        total: None,
        message: path.display().to_string(),
    });
    measure_with(path, &ctx.cancel, |_, _| {
        files += 1;
        if last.elapsed() >= Duration::from_millis(150) {
            progress(Progress {
                job_id: job_id.into(),
                stage: "refresh-affected".into(),
                completed: files,
                total: None,
                message: path.display().to_string(),
            });
            last = Instant::now();
        }
        Ok(())
    })
}

// Reconcile only attempted targets, including partially failed mutations. Skipped
// and not-yet-executed items retain their previous snapshots.
pub(crate) async fn refresh(
    mut inventory: Inventory,
    targets: Vec<Option<RefreshTarget>>,
    result: &OperationResult,
    ctx: Context,
    progress: ProgressSink,
    job_id: String,
) -> Result<(Inventory, Vec<PathBuf>)> {
    let targets: Vec<_> = targets
        .into_iter()
        .zip(&result.items)
        .filter(|(_, item)| item.status != "skipped")
        .filter_map(|(target, _)| target)
        .flat_map(|target| match target {
            RefreshTarget::Caches(ids) => ids.into_iter().map(RefreshTarget::Cache).collect(),
            RefreshTarget::Providers(ids) => ids.into_iter().map(RefreshTarget::Provider).collect(),
            target => vec![target],
        })
        .collect();
    let mut selected = vec![];
    for target in &targets {
        if let RefreshTarget::Provider(id) = target
            && !selected.contains(id)
        {
            selected.push(*id);
        }
    }
    if !selected.is_empty() && !ctx.cancel.is_cancelled() {
        progress(Progress {
            job_id: job_id.clone(),
            stage: "refresh-environments".into(),
            completed: 0,
            total: None,
            message: String::new(),
        });
        let mut found = providers::discover_selected(ctx.clone(), &selected).await?;
        for provider in &mut found.providers {
            if let Some(previous) = inventory.providers.iter().find(|old| old.id == provider.id) {
                for tool in provider
                    .tools
                    .iter_mut()
                    .chain(&mut provider.package_managers)
                {
                    if let Some(old) = previous
                        .tools
                        .iter()
                        .chain(&previous.package_managers)
                        .find(|old| old.id == tool.id && old.source == tool.source)
                    {
                        tool.latest = old.latest.clone();
                        tool.update_status = providers::updates::classify(tool);
                    }
                }
            }
        }
        inventory.providers.retain(|p| !selected.contains(&p.id));
        inventory.providers.extend(found.providers);
        // Discovery has no cache-size scan. Preserve known measurements for
        // unchanged caches; cache cleanup itself is reconciled below.
        let mut caches = found.caches;
        for cache in &mut caches {
            if let Some(old) = inventory
                .caches
                .iter()
                .find(|c| c.id == cache.id && c.path == cache.path)
            {
                cache.size = old.size.clone();
            }
        }
        inventory.caches.retain(|c| !selected.contains(&c.provider));
        inventory.caches.extend(caches);
    }
    tokio::task::spawn_blocking(move || {
        let mut worktrees = HashSet::new();
        let mut invalidated = vec![];
        for target in targets {
            match target {
                RefreshTarget::Artifact {
                    project_id,
                    artifact_id,
                } => {
                    let Some(project) = inventory.projects.iter_mut().find(|p| p.id == project_id)
                    else {
                        continue;
                    };
                    invalidated.push(project.path.clone());
                    if let Some(repository) = &project.repository {
                        invalidated.push(repository.path.clone());
                    }
                    if project.is_worktree {
                        worktrees.insert(project_id);
                    }
                    let Some(index) = project.artifacts.iter().position(|a| a.id == artifact_id)
                    else {
                        continue;
                    };
                    let artifact = &mut project.artifacts[index];
                    match std::fs::symlink_metadata(&artifact.path) {
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            project.artifacts.remove(index);
                        }
                        Ok(meta) if meta.is_dir() && !is_link(&meta) => {
                            match measure_changed(&artifact.path, &ctx, &progress, &job_id) {
                                Ok(size) => artifact.size = size,
                                Err(e) => {
                                    artifact.size.complete = false;
                                    artifact.can_clean = false;
                                    artifact.cleanup_issue = Some(e.to_string());
                                }
                            }
                        }
                        _ => {
                            artifact.size.complete = false;
                            artifact.can_clean = false;
                            artifact.cleanup_issue = Some(
                                "The artifact changed or is unavailable. Rescan before cleaning."
                                    .into(),
                            );
                        }
                    }
                }
                RefreshTarget::Worktree(id) => {
                    worktrees.insert(id);
                }
                RefreshTarget::Cache(id) => {
                    if let Some(cache) = inventory.caches.iter_mut().find(|c| c.id == id) {
                        cache.size = match std::fs::symlink_metadata(&cache.path) {
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Measurement {
                                complete: true,
                                ..Default::default()
                            },
                            _ => measure_changed(&cache.path, &ctx, &progress, &job_id)
                                .unwrap_or_default(),
                        };
                    }
                }
                RefreshTarget::Provider(_)
                | RefreshTarget::Caches(_)
                | RefreshTarget::Providers(_) => {}
            }
        }
        for id in worktrees {
            let Some(index) = inventory.worktrees.iter().position(|w| w.id == id) else {
                continue;
            };
            let worktree = &mut inventory.worktrees[index];
            invalidated.extend([worktree.path.clone(), worktree.repository.path.clone()]);
            match std::fs::symlink_metadata(&worktree.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    inventory.worktrees.remove(index);
                    inventory.projects.retain(|p| p.id != id);
                }
                _ => {
                    // Refresh ownership metadata, then measure only this checkout.
                    match git::checkout(&worktree.path) {
                        Ok(Some(checkout))
                            if checkout.is_worktree
                                && checkout.repository.id == worktree.repository.id =>
                        {
                            worktree.branch = checkout.branch;
                            worktree.locked = checkout.git_dir.join("locked").exists();
                            match measure_changed(&worktree.path, &ctx, &progress, &job_id) {
                                Ok(size) => worktree.size = Some(size),
                                Err(_) => worktree.size = None,
                            }
                        }
                        _ => {
                            worktree.size = None;
                            worktree.issue = Some(
                                "Worktree registration changed. Rescan this repository.".into(),
                            );
                        }
                    }
                }
            }
        }
        Ok((inventory, invalidated))
    })
    .await
    .map_err(|e| Error::Unavailable(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{Settings, silent_progress},
        operations::ItemResult,
        scanner,
    };
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn cleanup_updates_only_attempted_artifacts_without_rediscovering_projects() {
        let root = tempfile::tempdir().unwrap();
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        for name in ["changed", "unrelated"] {
            let path = canonical_root.join(name);
            git::init(&path);
            std::fs::write(path.join("package.json"), "{}").unwrap();
            std::fs::create_dir_all(path.join("node_modules/pkg")).unwrap();
            std::fs::write(path.join("node_modules/.package-lock.json"), "{}").unwrap();
            std::fs::write(path.join("node_modules/pkg/file"), "payload").unwrap();
        }
        let settings = Settings {
            roots: vec![canonical_root],
            ..Default::default()
        };
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let scanned = scanner::scan(&settings, &ctx.cancel, silent_progress(), "scan").unwrap();
        let inventory = Inventory {
            projects: scanned.projects,
            ..Default::default()
        };
        let project = inventory
            .projects
            .iter()
            .find(|p| p.name == "changed")
            .unwrap();
        let target = RefreshTarget::Artifact {
            project_id: project.id.clone(),
            artifact_id: project.artifacts[0].id.clone(),
        };
        let unaffected = inventory
            .projects
            .iter()
            .find(|p| p.name == "unrelated")
            .unwrap()
            .clone();
        std::fs::remove_dir_all(&project.artifacts[0].path).unwrap();
        // A full rescan would discover this deletion. Targeted refresh must not
        // visit an unrelated checkout or rewrite its snapshot.
        std::fs::remove_dir_all(&unaffected.path).unwrap();
        let result = OperationResult {
            items: vec![ItemResult {
                title: "cleanup".into(),
                status: "success".into(),
                message: String::new(),
                removed_bytes: 7,
            }],
            cancelled: true,
            removed_bytes: 7,
            reclaimed_bytes: None,
        };
        let events = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let captured = events.clone();
        let progress: ProgressSink =
            std::sync::Arc::new(move |event| captured.lock().unwrap().push(event));
        ctx.cancel.cancel();
        let (updated, invalidated) = refresh(
            inventory,
            vec![Some(target)],
            &result,
            ctx,
            progress,
            "update".into(),
        )
        .await
        .unwrap();
        assert!(
            updated
                .projects
                .iter()
                .find(|p| p.name == "changed")
                .unwrap()
                .artifacts
                .is_empty()
        );
        assert_eq!(
            updated
                .projects
                .iter()
                .find(|p| p.name == "unrelated")
                .unwrap()
                .artifacts[0]
                .size
                .bytes,
            unaffected.artifacts[0].size.bytes
        );
        assert!(!invalidated.contains(&unaffected.path));
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .all(|e| !["discover", "measure-projects"].contains(&e.stage.as_str()))
        );
    }

    #[tokio::test]
    async fn environment_refresh_preserves_projects_and_other_providers() {
        let ctx = Context::new(CancellationToken::new()).unwrap();
        let mut sentinel = crate::model::Provider::empty(crate::model::ProviderId::Rust);
        sentinel.issues.push("untouched".into());
        let inventory = Inventory {
            providers: vec![sentinel],
            scanned_at: Some(123),
            ..Default::default()
        };
        let result = OperationResult {
            items: vec![ItemResult {
                title: "install".into(),
                status: "success".into(),
                message: String::new(),
                removed_bytes: 0,
            }],
            cancelled: false,
            removed_bytes: 0,
            reclaimed_bytes: None,
        };
        let (updated, invalidated) = refresh(
            inventory,
            vec![Some(RefreshTarget::Provider(crate::model::ProviderId::Go))],
            &result,
            ctx,
            silent_progress(),
            "update".into(),
        )
        .await
        .unwrap();
        assert_eq!(updated.scanned_at, Some(123));
        assert_eq!(
            updated
                .providers
                .iter()
                .find(|p| p.id == crate::model::ProviderId::Rust)
                .unwrap()
                .issues,
            vec!["untouched"]
        );
        assert!(invalidated.is_empty());
    }
}
