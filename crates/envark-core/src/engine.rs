use crate::{
    Error, Result,
    filesystem::measure,
    model::*,
    operations::{self, ActionRequest, OperationResult, Plan, PlanView},
    persistence::Storage,
    providers::{self, Context},
    scan_cache::ScanCache,
    scanner,
};
use serde::Serialize;
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: Settings,
    pub inventory: Inventory,
    pub activity: Vec<Activity>,
    pub data_dir: PathBuf,
    pub platform: String,
    pub version: String,
}

pub struct Engine {
    pub(crate) storage: Storage,
    pub(crate) state: RwLock<Snapshot>,
    jobs: Mutex<HashMap<String, CancellationToken>>,
    plans: Mutex<HashMap<String, Plan>>,
    pub(crate) work: Arc<Mutex<()>>,
    scans: Arc<std::sync::Mutex<ScanCache>>,
}

impl Engine {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        let storage = Storage::new(data_dir.clone())?;
        let mut state = Snapshot {
            settings: storage.settings()?,
            inventory: storage.inventory()?,
            activity: storage.activity()?,
            data_dir,
            platform: std::env::consts::OS.into(),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        state.inventory.issues.extend(storage.recovery_notices());
        if state
            .inventory
            .projects
            .iter()
            .any(|project| project.repository.is_none())
        {
            // Legacy inventories describe manifest directories rather than Git roots.
            state.inventory.projects.clear();
            state.inventory.worktrees.clear();
            state.inventory.scanned_at = None;
            state.inventory.issues.push(
                "Rescan project folders to group Git repositories and their linked worktrees."
                    .into(),
            );
        }
        Ok(Self {
            storage,
            state: RwLock::new(state),
            jobs: Mutex::new(HashMap::new()),
            plans: Mutex::new(HashMap::new()),
            work: Arc::new(Mutex::new(())),
            scans: Arc::new(std::sync::Mutex::new(ScanCache::default())),
        })
    }

    pub async fn snapshot(&self) -> Snapshot {
        self.state.read().await.clone()
    }

    pub async fn save_settings(&self, settings: Settings) -> Result<Snapshot> {
        let _guard = self.work.try_lock().map_err(|_| {
            Error::Conflict("Wait for the current operation before changing settings.".into())
        })?;
        scanner::validate_settings(&settings)?;
        self.storage.save_settings(&settings)?;
        self.plans.lock().await.clear();
        let mut state = self.state.write().await;
        state.settings = settings;
        Ok(state.clone())
    }

    pub async fn set_theme(&self, theme: String) -> Result<Snapshot> {
        if !matches!(theme.as_str(), "light" | "dark" | "system") {
            return Err(Error::InvalidInput("Unsupported theme.".into()));
        }
        // Appearance changes must not invalidate reviewed cleanup plans or wait
        // for a filesystem operation. Preserve all operation-related settings.
        let mut state = self.state.write().await;
        let mut settings = state.settings.clone();
        settings.theme = theme;
        self.storage.save_settings(&settings)?;
        state.settings = settings;
        Ok(state.clone())
    }

    pub async fn set_disabled_shortcuts(&self, disabled: BTreeSet<Shortcut>) -> Result<Snapshot> {
        // Keyboard preferences are independent of running operations and cleanup plans.
        let mut state = self.state.write().await;
        let mut settings = state.settings.clone();
        settings.disabled_shortcuts = disabled;
        self.storage.save_settings(&settings)?;
        state.settings = settings;
        Ok(state.clone())
    }

    pub async fn cancel(&self, job_id: &str) {
        if let Some(token) = self.jobs.lock().await.get(job_id) {
            token.cancel();
        }
    }

    pub(crate) async fn log(
        &self,
        kind: &str,
        title: String,
        status: &str,
        detail: String,
        removed_bytes: u64,
    ) {
        let mut state = self.state.write().await;
        state.activity.insert(
            0,
            Activity {
                id: uuid::Uuid::new_v4().to_string(),
                time: now(),
                kind: kind.into(),
                title,
                status: status.into(),
                detail,
                removed_bytes,
                reclaimed_bytes: None,
            },
        );
        state.activity.truncate(1000);
        if let Err(error) = self.storage.save_activity(&state.activity) {
            // A persistence failure must not turn a completed mutation into a failed operation.
            state.inventory.issues.push(format!(
                "Activity is available for this session but could not be saved: {error}"
            ));
        }
    }

    pub async fn refresh(&self, job_id: String, progress: ProgressSink) -> Result<Snapshot> {
        let _guard = self
            .work
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Conflict("An operation is already running.".into()))?;
        let token = CancellationToken::new();
        self.jobs.lock().await.insert(job_id.clone(), token.clone());
        let result = self.refresh_inner(&job_id, token, progress, false).await;
        self.jobs.lock().await.remove(&job_id);
        match &result {
            Ok(()) => {
                self.log(
                    "scan",
                    "Refresh inventory".into(),
                    "success",
                    "Environment and project inventory refreshed.".into(),
                    0,
                )
                .await
            }
            Err(error) => {
                self.log(
                    "scan",
                    "Refresh inventory".into(),
                    if matches!(error, Error::Cancelled) {
                        "cancelled"
                    } else {
                        "failed"
                    },
                    error.to_string(),
                    0,
                )
                .await
            }
        }
        result?;
        Ok(self.snapshot().await)
    }

    async fn refresh_inner(
        &self,
        job_id: &str,
        token: CancellationToken,
        progress: ProgressSink,
        force: bool,
    ) -> Result<()> {
        let context = Context::new(token.clone())?;
        let settings = self.state.read().await.settings.clone();
        let check_updates = settings.check_updates;
        let scan_token = token.clone();
        let scan_progress = progress.clone();
        let scan_id = job_id.to_owned();
        let scans = self.scans.clone();
        let project_job = tokio::task::spawn_blocking(move || {
            scans
                .lock()
                .map_err(|_| {
                    Error::Unavailable("The scan cache is unavailable. Restart Envark.".into())
                })?
                .scan(&settings, &scan_token, scan_progress, &scan_id, force)
        });
        progress(Progress {
            job_id: job_id.into(),
            stage: "environments".into(),
            completed: 0,
            total: None,
            message: "Discovering installed environments".into(),
        });
        let discovered = providers::discover(context).await;
        let projects = project_job
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))??;
        let mut discovered = discovered?;
        if check_updates {
            progress(Progress {
                job_id: job_id.into(),
                stage: "updates".into(),
                completed: 0,
                total: None,
                message: "Checking public package registries".into(),
            });
            providers::updates::check(&mut discovered.providers, &token).await;
        }
        let measurement_token = token.clone();
        discovered.caches = tokio::task::spawn_blocking(move || {
            use rayon::prelude::*;
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(4)
                .build()
                .map_err(|e| Error::Unavailable(e.to_string()))?;
            pool.install(|| {
                discovered.caches.par_iter_mut().for_each(|cache| {
                    if let Ok(size) = measure(&cache.path, &measurement_token) {
                        cache.size = size;
                    }
                })
            });
            Ok::<_, Error>(discovered.caches)
        })
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))??;
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let disks = tokio::task::spawn_blocking(|| {
            sysinfo::Disks::new_with_refreshed_list()
                .iter()
                .map(|disk| Disk {
                    name: disk.name().to_string_lossy().into_owned(),
                    mount: disk.mount_point().into(),
                    total: disk.total_space(),
                    available: disk.available_space(),
                })
                .collect()
        })
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))?;
        let mut inventory = Inventory {
            providers: discovered.providers,
            caches: discovered.caches,
            projects: projects.projects,
            worktrees: projects.worktrees,
            issues: projects.issues,
            disks,
            scanned_at: Some(now()),
        };
        inventory.issues.extend(self.storage.recovery_notices());
        self.storage.save_inventory(&inventory)?;
        self.state.write().await.inventory = inventory;
        progress(Progress {
            job_id: job_id.into(),
            stage: "complete".into(),
            completed: 1,
            total: Some(1),
            message: "Inventory refreshed".into(),
        });
        Ok(())
    }

    pub async fn plan(&self, request: ActionRequest) -> Result<PlanView> {
        let _guard = self
            .work
            .try_lock()
            .map_err(|_| Error::Conflict("Wait for the current operation to finish.".into()))?;
        let state = self.snapshot().await;
        let plan = operations::prepare(
            request,
            &state.inventory,
            &state.settings,
            &Context::new(CancellationToken::new())?,
        )
        .await?;
        let view = plan.view.clone();
        let mut plans = self.plans.lock().await;
        plans.retain(|_, plan| now().saturating_sub(plan.view.created_at) < 600);
        if plans.len() > 20 {
            plans.clear();
        }
        plans.insert(view.id.clone(), plan);
        Ok(view)
    }

    pub async fn execute(
        &self,
        plan_id: &str,
        job_id: String,
        progress: ProgressSink,
    ) -> Result<OperationResult> {
        let _guard = self
            .work
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Conflict("An operation is already running.".into()))?;
        let plan = self.plans.lock().await.remove(plan_id).ok_or_else(|| {
            Error::Conflict("This plan is no longer valid. Review the operation again.".into())
        })?;
        let settings = self.state.read().await.settings.clone();
        if plan.view.use_trash != settings.use_trash {
            return Err(Error::Conflict(
                "The cleanup policy changed. Review a new plan.".into(),
            ));
        }
        let kind = plan.view.kind.clone();
        let token = CancellationToken::new();
        let context = Context::new(token.clone())?;
        self.jobs.lock().await.insert(job_id.clone(), token.clone());
        let outcome =
            operations::execute(plan, settings, context, progress.clone(), job_id.clone()).await;
        match &outcome {
            Ok(result) => {
                if !token.is_cancelled()
                    && let Err(error) = self.refresh_inner(&job_id, token, progress, true).await
                {
                    self.state
                        .write()
                        .await
                        .inventory
                        .issues
                        .push(format!("Refresh after operation: {error}"));
                }
                let failed = result
                    .items
                    .iter()
                    .filter(|i| i.status != "success")
                    .count();
                self.log(
                    &kind,
                    format!("{} items processed", result.items.len()),
                    if result.cancelled {
                        "cancelled"
                    } else if failed > 0 {
                        "partial"
                    } else {
                        "success"
                    },
                    result
                        .items
                        .iter()
                        .map(|r| format!("{}: {} {}", r.title, r.status, r.message))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    result.removed_bytes,
                )
                .await;
            }
            Err(error) => {
                self.log(
                    &kind,
                    "Operation failed".into(),
                    "failed",
                    error.to_string(),
                    0,
                )
                .await
            }
        }
        self.jobs.lock().await.remove(&job_id);
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn damaged_inventory_and_activity_do_not_prevent_startup() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("inventory.json"), "corrupt inventory").unwrap();
        std::fs::write(root.path().join("activity.json"), "corrupt activity").unwrap();
        let engine = Engine::new(root.path().into()).unwrap();
        let snapshot = engine.snapshot().await;
        assert!(snapshot.inventory.projects.is_empty());
        assert!(snapshot.activity.is_empty());
        assert_eq!(snapshot.inventory.issues.len(), 2);
        engine
            .log(
                "scan",
                "Recovered".into(),
                "success",
                "New session".into(),
                0,
            )
            .await;
        assert_eq!(engine.storage.activity().unwrap()[0].title, "Recovered");
        assert_eq!(
            std::fs::read_dir(root.path())
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_name().to_string_lossy().contains(".recovery-"))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn legacy_project_inventory_requires_rescanning_without_losing_preferences() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::new(root.path().into()).unwrap();
        let settings = Settings {
            roots: vec![root.path().join("projects")],
            scan_on_launch: false,
            ..Default::default()
        };
        storage.save_settings(&settings).unwrap();
        let legacy = serde_json::json!({
            "providers": [], "caches": [], "disks": [], "issues": [], "scannedAt": 42,
            "projects": [{
                "id": "legacy", "name": "web", "path": root.path().join("projects/apps/web"),
                "providers": ["js"], "lastActive": null, "activityComplete": true,
                "branch": null, "pins": {}, "protected": false, "artifacts": []
            }]
        });
        std::fs::write(root.path().join("inventory.json"), legacy.to_string()).unwrap();
        let snapshot = Engine::new(root.path().into()).unwrap().snapshot().await;
        assert!(snapshot.inventory.projects.is_empty());
        assert!(snapshot.inventory.worktrees.is_empty());
        assert!(snapshot.inventory.scanned_at.is_none());
        assert!(
            snapshot
                .inventory
                .issues
                .iter()
                .any(|issue| issue.contains("Rescan"))
        );
        assert_eq!(snapshot.settings.roots, settings.roots);
        assert!(!snapshot.settings.scan_on_launch);
    }

    #[tokio::test]
    async fn activity_write_failure_preserves_the_result_in_memory() {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::new(root.path().into()).unwrap();
        std::fs::create_dir(root.path().join("activity.json")).unwrap();
        engine
            .log("clean", "Completed".into(), "success", "Removed".into(), 12)
            .await;
        let snapshot = engine.snapshot().await;
        assert_eq!(snapshot.activity[0].status, "success");
        assert_eq!(snapshot.activity[0].removed_bytes, 12);
        assert!(snapshot.inventory.issues[0].contains("could not be saved"));
    }

    #[tokio::test]
    async fn theme_changes_are_available_during_operations_without_changing_scan_settings() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Engine::new(temp.path().into()).unwrap();
        let original = engine.snapshot().await.settings;
        let _operation = engine.work.lock().await;
        let snapshot = engine.set_theme("dark".into()).await.unwrap();
        assert_eq!(snapshot.settings.theme, "dark");
        assert_eq!(snapshot.settings.roots, original.roots);
        assert_eq!(snapshot.settings.idle_days, original.idle_days);
        assert_eq!(engine.storage.settings().unwrap().theme, "dark");
        assert!(engine.set_theme("invalid".into()).await.is_err());
        assert_eq!(engine.snapshot().await.settings.theme, "dark");
    }

    #[tokio::test]
    async fn shortcut_preferences_migrate_persist_and_reset_during_operations() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("settings.json"),
            r#"{"language":"en","scanOnLaunch":false,"idleDays":120}"#,
        )
        .unwrap();
        let engine = Engine::new(temp.path().into()).unwrap();
        assert!(
            engine
                .snapshot()
                .await
                .settings
                .disabled_shortcuts
                .is_empty()
        );
        let _operation = engine.work.lock().await;
        let disabled = BTreeSet::from([Shortcut::Refresh, Shortcut::ToggleSidebar]);
        let snapshot = engine
            .set_disabled_shortcuts(disabled.clone())
            .await
            .unwrap();
        assert_eq!(snapshot.settings.disabled_shortcuts, disabled);
        let reopened = Engine::new(temp.path().into()).unwrap();
        let settings = reopened.snapshot().await.settings;
        assert_eq!(settings.disabled_shortcuts, disabled);
        assert_eq!(settings.language, "en");
        assert_eq!(settings.idle_days, 120);
        assert!(!settings.scan_on_launch);
        engine
            .set_disabled_shortcuts(BTreeSet::new())
            .await
            .unwrap();
        assert!(
            engine
                .storage
                .settings()
                .unwrap()
                .disabled_shortcuts
                .is_empty()
        );
    }

    #[tokio::test]
    async fn retired_worktree_shortcut_does_not_discard_other_preferences() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("settings.json"),
            r#"{"language":"en","idleDays":120,"disabledShortcuts":["worktrees","caches"]}"#,
        )
        .unwrap();
        let snapshot = Engine::new(temp.path().into()).unwrap().snapshot().await;
        assert_eq!(
            snapshot.settings.disabled_shortcuts,
            BTreeSet::from([Shortcut::Caches])
        );
        assert_eq!(snapshot.settings.language, "en");
        assert_eq!(snapshot.settings.idle_days, 120);
        assert!(snapshot.inventory.issues.is_empty());
    }

    #[tokio::test]
    async fn failed_shortcut_preference_save_preserves_the_active_settings() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Engine::new(temp.path().into()).unwrap();
        std::fs::create_dir(temp.path().join("settings.json")).unwrap();
        assert!(
            engine
                .set_disabled_shortcuts(BTreeSet::from([Shortcut::Refresh]))
                .await
                .is_err()
        );
        assert!(
            engine
                .snapshot()
                .await
                .settings
                .disabled_shortcuts
                .is_empty()
        );
    }
}
