use crate::NativeResult;
use envark_core::{app_release::AppRelease, engine::Engine};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
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
    let mut downloaded = 0;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk;
                let _ = app.emit(
                    "envark://app-update",
                    DownloadProgress {
                        downloaded,
                        total,
                        installing: false,
                    },
                );
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
