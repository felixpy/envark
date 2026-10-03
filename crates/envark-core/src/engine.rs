use crate::{
    Error, Result,
    filesystem::measure,
    model::*,
    operations::{self, ActionRequest, OperationResult, Plan, PlanView},
    persistence::Storage,
    providers::{self, Context},
    scanner,
};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
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
}

impl Engine {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        let storage = Storage::new(data_dir.clone())?;
        let state = Snapshot {
            settings: storage.settings()?,
            inventory: storage.inventory()?,
            activity: storage.activity()?,
            data_dir,
            platform: std::env::consts::OS.into(),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        Ok(Self {
            storage,
            state: RwLock::new(state),
            jobs: Mutex::new(HashMap::new()),
            plans: Mutex::new(HashMap::new()),
            work: Arc::new(Mutex::new(())),
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
        let mut state = self.state.write().await;
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
        freed_bytes: u64,
    ) -> Result<()> {
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
                freed_bytes,
            },
        );
        state.activity.truncate(1000);
        self.storage.save_activity(&state.activity)
    }

    pub async fn refresh(&self, job_id: String, progress: ProgressSink) -> Result<Snapshot> {
        let _guard = self
            .work
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Conflict("An operation is already running.".into()))?;
        let token = CancellationToken::new();
        self.jobs.lock().await.insert(job_id.clone(), token.clone());
        let result = self.refresh_inner(&job_id, token, progress).await;
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
                .await?
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
                .await?
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
    ) -> Result<()> {
        let settings = self.state.read().await.settings.clone();
        let check_updates = settings.check_updates;
        let scan_token = token.clone();
        let scan_progress = progress.clone();
        let scan_id = job_id.to_owned();
        let project_job = tokio::task::spawn_blocking(move || {
            scanner::scan(&settings, &scan_token, scan_progress, &scan_id)
        });
        progress(Progress {
            job_id: job_id.into(),
            stage: "environments".into(),
            completed: 0,
            total: None,
            message: "Discovering installed environments".into(),
        });
        let discovered = providers::discover(Context::new(token.clone())?).await;
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
        let inventory = Inventory {
            providers: discovered.providers,
            caches: discovered.caches,
            projects: projects.projects,
            issues: projects.issues,
            disks,
            scanned_at: Some(now()),
        };
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
        let state = self.state.read().await;
        let plan = operations::prepare(
            request,
            &state.inventory,
            &state.settings,
            &Context::new(CancellationToken::new())?,
        )?;
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
                    result.freed_bytes,
                )
                .await?;
                if !token.is_cancelled() {
                    if let Err(error) = self.refresh_inner(&job_id, token, progress).await {
                        self.state
                            .write()
                            .await
                            .inventory
                            .issues
                            .push(format!("Refresh after operation: {error}"));
                    }
                }
            }
            Err(error) => {
                self.log(
                    &kind,
                    "Operation failed".into(),
                    "failed",
                    error.to_string(),
                    0,
                )
                .await?
            }
        }
        self.jobs.lock().await.remove(&job_id);
        outcome
    }
}
