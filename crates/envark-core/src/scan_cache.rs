use crate::{
    Error, Result,
    model::{Progress, ProgressSink, Settings},
    scanner::{self, ScanResult},
};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const MAX_CACHE_AGE: Duration = Duration::from_secs(60);

struct WatchedRoot {
    _watcher: Option<RecommendedWatcher>,
    healthy: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    snapshot: Option<(u64, Instant, ScanResult)>,
    error: Option<String>,
}

impl WatchedRoot {
    fn new(path: &std::path::Path) -> Self {
        let generation = Arc::new(AtomicU64::new(0));
        let healthy = Arc::new(AtomicBool::new(true));
        let epoch = generation.clone();
        let health = healthy.clone();
        let watcher = RecommendedWatcher::new(
            move |event: notify::Result<Event>| match event {
                Ok(event) if matches!(event.kind, EventKind::Access(_)) && !event.need_rescan() => {
                }
                Ok(_) => {
                    epoch.fetch_add(1, Ordering::SeqCst);
                }
                Err(_) => {
                    health.store(false, Ordering::SeqCst);
                    epoch.fetch_add(1, Ordering::SeqCst);
                }
            },
            Config::default().with_follow_symlinks(false),
        )
        .and_then(|mut watcher| {
            watcher.watch(path, RecursiveMode::Recursive)?;
            Ok(watcher)
        });
        let error = watcher.as_ref().err().map(ToString::to_string);
        let watcher = watcher.ok();
        if watcher.is_none() {
            healthy.store(false, Ordering::SeqCst);
        }
        Self {
            _watcher: watcher,
            healthy,
            generation,
            snapshot: None,
            error,
        }
    }
}

/// Reuses unchanged roots only while native file notifications remain healthy.
/// Cache entries are process-local and expire; cleanup never trusts this cache.
#[derive(Default)]
pub struct ScanCache {
    settings_key: Vec<u8>,
    roots: HashMap<PathBuf, WatchedRoot>,
}

impl ScanCache {
    pub fn scan(
        &mut self,
        settings: &Settings,
        cancel: &CancellationToken,
        progress: ProgressSink,
        job_id: &str,
        force: bool,
    ) -> Result<ScanResult> {
        scanner::validate_settings(settings)?;
        let key = serde_json::to_vec(&(
            &settings.roots,
            &settings.excludes,
            &settings.protected_projects,
        ))?;
        if self.settings_key != key {
            self.roots.clear();
            self.settings_key = key;
        }
        let start = Instant::now();
        let roots = scanner::canonical_roots(settings)?;
        let mut result = ScanResult::default();
        for root in roots {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let entry = self
                .roots
                .entry(root.clone())
                .or_insert_with(|| WatchedRoot::new(&root));
            if let Some(error) = &entry.error {
                result.issues.push(format!(
                    "Using a full scan for {} because file notifications are unavailable: {error}",
                    root.display()
                ));
            }
            let generation = entry.generation.load(Ordering::SeqCst);
            let cached = entry.snapshot.as_ref().filter(|(epoch, time, _)| {
                !force
                    && *epoch == generation
                    && time.elapsed() < MAX_CACHE_AGE
                    && entry.healthy.load(Ordering::SeqCst)
            });
            let scan = if let Some((_, _, previous)) = cached {
                result.cached_roots += 1;
                progress(Progress {
                    job_id: job_id.into(),
                    stage: "reuse".into(),
                    completed: result.cached_roots,
                    total: None,
                    message: format!("Unchanged: {}", root.display()),
                });
                let mut reused = previous.clone();
                reused.visited = 0;
                reused
            } else {
                let mut scope = settings.clone();
                scope.roots = vec![root];
                let scanned = scanner::scan(&scope, cancel, progress.clone(), job_id)?;
                if scanned.issues.is_empty()
                    && generation == entry.generation.load(Ordering::SeqCst)
                    && entry.healthy.load(Ordering::SeqCst)
                {
                    entry.snapshot = Some((generation, Instant::now(), scanned.clone()));
                } else {
                    entry.snapshot = None;
                }
                scanned
            };
            result.projects.extend(scan.projects);
            result.issues.extend(scan.issues);
            result.visited += scan.visited;
        }
        result.projects.sort_by_key(|p| p.last_active);
        result.elapsed_ms = start.elapsed().as_millis();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::silent_progress;

    #[test]
    fn unchanged_roots_are_reused_and_explicit_rescans_read_the_tree() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("package.json"), "{}").unwrap();
        let path = std::fs::canonicalize(root.path()).unwrap();
        let settings = Settings {
            roots: vec![path.clone()],
            ..Default::default()
        };
        let mut cache = ScanCache::default();
        let token = CancellationToken::new();
        let first = cache
            .scan(&settings, &token, silent_progress(), "test", false)
            .unwrap();
        assert_eq!(first.projects.len(), 1);
        assert!(first.visited > 0);
        // The cache remains an optimization: unavailable native watchers fall back to scanning.
        let supported = cache.roots[&path].healthy.load(Ordering::SeqCst);
        let mut second = cache
            .scan(&settings, &token, silent_progress(), "test", false)
            .unwrap();
        for _ in 0..20 {
            if second.cached_roots > 0 || !supported {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
            second = cache
                .scan(&settings, &token, silent_progress(), "test", false)
                .unwrap();
        }
        if supported {
            assert_eq!(second.cached_roots, 1);
        }
        let forced = cache
            .scan(&settings, &token, silent_progress(), "test", true)
            .unwrap();
        assert!(forced.visited > 0);
        cache.roots[&path].generation.fetch_add(1, Ordering::SeqCst);
        assert!(
            cache
                .scan(&settings, &token, silent_progress(), "test", false)
                .unwrap()
                .visited
                > 0
        );
        cache.roots[&path].healthy.store(false, Ordering::SeqCst);
        assert!(
            cache
                .scan(&settings, &token, silent_progress(), "test", false)
                .unwrap()
                .visited
                > 0
        );
    }
}
