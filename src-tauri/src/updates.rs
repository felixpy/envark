use crate::NativeResult;
use envark_core::{app_release::AppRelease, engine::Engine};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    installed: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    #[serde(flatten)]
    release: AppRelease,
    installable: bool,
    install_issue: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    downloaded: usize,
    total: Option<u64>,
    installing: bool,
}

#[derive(Default)]
struct DownloadReporter {
    downloaded: usize,
    last_emit: Option<Instant>,
}

impl DownloadReporter {
    fn chunk(
        &mut self,
        bytes: usize,
        total: Option<u64>,
        now: Instant,
    ) -> Option<DownloadProgress> {
        self.downloaded += bytes;
        // Network chunks can arrive much faster than the webview can paint.
        if self
            .last_emit
            .is_some_and(|last| now.duration_since(last) < Duration::from_millis(100))
        {
            return None;
        }
        self.last_emit = Some(now);
        Some(DownloadProgress {
            downloaded: self.downloaded,
            total,
            installing: false,
        })
    }
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, UpdateState>,
) -> NativeResult<AppUpdate> {
    let mut pending = state
        .pending
        .try_lock()
        .map_err(|_| "An update is already running.")?;
    if state.installed.load(Ordering::Acquire) {
        return Err("Restart Envark to finish the installed update.".into());
    }
    *pending = None;
    match app
        .updater_builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
    {
        Ok(update) => {
            let release = AppRelease {
                available: update.is_some(),
                version: update
                    .as_ref()
                    .map(|u| u.version.clone())
                    .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into()),
            };
            *pending = update;
            Ok(AppUpdate {
                release,
                installable: pending.is_some(),
                install_issue: None,
            })
        }
        Err(error) => {
            // Older releases may not have a signed manifest yet. Keep the manual
            // release link available without treating an unsigned installer as trusted.
            let release = envark_core::app_release::check(env!("CARGO_PKG_VERSION"))
                .await
                .map_err(|e| e.to_string())?;
            Ok(AppUpdate {
                release,
                installable: false,
                install_issue: Some(error.to_string()),
            })
        }
    }
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: State<'_, UpdateState>,
    engine: State<'_, Engine>,
) -> NativeResult<()> {
    let _work = engine.reserve_work().map_err(|e| e.to_string())?;
    let mut pending = state
        .pending
        .try_lock()
        .map_err(|_| "An update is already running.")?;
    let update = pending
        .as_mut()
        .ok_or("Check for an update before installing it.")?;
    update.timeout = Some(Duration::from_secs(600));
    let mut reporter = DownloadReporter::default();
    let bytes = update
        .download(
            |chunk, total| {
                if let Some(progress) = reporter.chunk(chunk, total, Instant::now()) {
                    let _ = app.emit("envark://app-update", progress);
                }
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "envark://app-update",
        DownloadProgress {
            downloaded: bytes.len(),
            total: Some(bytes.len() as u64),
            installing: true,
        },
    );
    update.install(bytes).map_err(|e| e.to_string())?;
    state.installed.store(true, Ordering::Release);
    *pending = None;
    Ok(())
}

#[tauri::command]
pub async fn restart_after_update(
    app: AppHandle,
    state: State<'_, UpdateState>,
    engine: State<'_, Engine>,
) -> NativeResult<()> {
    if !state.installed.load(Ordering::Acquire) {
        return Err("No update is ready to restart.".into());
    }
    let _work = engine.reserve_work().map_err(|e| e.to_string())?;
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_reports_are_bounded_without_losing_chunk_bytes() {
        let mut reporter = DownloadReporter::default();
        let start = Instant::now();
        let first = reporter.chunk(10, Some(100), start).unwrap();
        assert_eq!(first.downloaded, 10);
        assert_eq!(first.total, Some(100));
        assert!(!first.installing);
        for millis in 1..100 {
            assert!(
                reporter
                    .chunk(1, Some(200), start + Duration::from_millis(millis))
                    .is_none()
            );
        }
        let next = reporter
            .chunk(1, Some(200), start + Duration::from_millis(100))
            .unwrap();
        assert_eq!(next.downloaded, 110);
        assert_eq!(next.total, Some(200));
    }

    #[test]
    fn unknown_download_length_is_not_an_invented_total() {
        let mut reporter = DownloadReporter::default();
        let progress = reporter.chunk(256, None, Instant::now()).unwrap();
        assert_eq!(progress.downloaded, 256);
        assert_eq!(progress.total, None);
    }
}
