use envark_core::{
    config::ConfigContent,
    engine::{Engine, Snapshot},
    model::{Progress, ProgressSink, Provider, ProviderId, Settings, Shortcut},
    operations::{ActionRequest, OperationResult, PlanView},
};
use std::{collections::BTreeSet, sync::Arc};
use tauri::{Emitter, Manager, State};
mod links;
mod menu;
mod updates;

type NativeResult<T> = std::result::Result<T, String>;

fn progress(app: &tauri::AppHandle) -> ProgressSink {
    let app = app.clone();
    Arc::new(move |event: Progress| {
        let _ = app.emit("envark://progress", event);
    })
}

#[tauri::command]
async fn snapshot(engine: State<'_, Engine>) -> NativeResult<Snapshot> {
    Ok(engine.snapshot().await)
}

#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    engine: State<'_, Engine>,
    settings: Settings,
) -> NativeResult<Snapshot> {
    let language_changed = engine.snapshot().await.settings.language != settings.language;
    let snapshot = engine
        .save_settings(settings)
        .await
        .map_err(|e| e.to_string())?;
    if language_changed {
        menu::install(&app, &snapshot.settings).map_err(|e| e.to_string())?;
        let state = *app
            .state::<menu::MenuState>()
            .0
            .lock()
            .map_err(|e| e.to_string())?;
        menu::sync(&app, state).map_err(|e| e.to_string())?;
    } else {
        menu::sync_shortcuts(&app, &snapshot.settings.disabled_shortcuts)
            .map_err(|e| e.to_string())?;
    }
    Ok(snapshot)
}

#[tauri::command]
async fn set_app_theme(engine: State<'_, Engine>, theme: String) -> NativeResult<Snapshot> {
    engine.set_theme(theme).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_disabled_shortcuts(
    app: tauri::AppHandle,
    engine: State<'_, Engine>,
    disabled: BTreeSet<Shortcut>,
) -> NativeResult<Snapshot> {
    let previous = engine.snapshot().await.settings.disabled_shortcuts;
    if let Err(error) = menu::sync_shortcuts(&app, &disabled) {
        let _ = menu::sync_shortcuts(&app, &previous);
        return Err(error.to_string());
    }
    match engine.set_disabled_shortcuts(disabled).await {
        Ok(snapshot) => Ok(snapshot),
        Err(error) => {
            let _ = menu::sync_shortcuts(&app, &previous);
            Err(error.to_string())
        }
    }
}

#[tauri::command]
async fn sync_view_state(app: tauri::AppHandle, state: menu::ViewState) -> NativeResult<()> {
    if ![0.8, 0.9, 1.0, 1.1, 1.25, 1.5].contains(&state.zoom) {
        return Err("Unsupported zoom level.".into());
    }
    let previous = *app
        .state::<menu::MenuState>()
        .0
        .lock()
        .map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window("main") {
        if previous.zoom != state.zoom {
            window.set_zoom(state.zoom).map_err(|e| e.to_string())?;
        }
        if previous.theme != state.theme {
            window
                .set_theme(state.theme.native())
                .map_err(|e| e.to_string())?;
        }
    }
    menu::sync(&app, state).map_err(|e| e.to_string())?;
    *app.state::<menu::MenuState>()
        .0
        .lock()
        .map_err(|e| e.to_string())? = state;
    Ok(())
}

#[tauri::command]
async fn refresh(
    app: tauri::AppHandle,
    engine: State<'_, Engine>,
    job_id: String,
) -> NativeResult<Snapshot> {
    engine
        .refresh(job_id, progress(&app))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn manager_options(
    engine: State<'_, Engine>,
    provider: ProviderId,
) -> NativeResult<Vec<envark_core::providers::manager_install::OptionView>> {
    engine
        .manager_options(provider)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_tool_updates(
    app: tauri::AppHandle,
    engine: State<'_, Engine>,
    provider: ProviderId,
    job_id: String,
) -> NativeResult<Provider> {
    engine
        .check_tool_updates(provider, job_id, progress(&app))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn cancel(engine: State<'_, Engine>, job_id: String) -> NativeResult<()> {
    engine.cancel(&job_id).await;
    Ok(())
}

#[tauri::command]
async fn prepare_operation(
    app: tauri::AppHandle,
    engine: State<'_, Engine>,
    request: ActionRequest,
    job_id: String,
) -> NativeResult<PlanView> {
    engine
        .plan(request, job_id, progress(&app))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn refresh_disks(engine: State<'_, Engine>) -> NativeResult<Vec<envark_core::model::Disk>> {
    engine.refresh_disks().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn execute_operation(
    app: tauri::AppHandle,
    engine: State<'_, Engine>,
    plan_id: String,
    job_id: String,
    discard_worktree_changes: Option<bool>,
) -> NativeResult<OperationResult> {
    engine
        .execute(
            &plan_id,
            job_id,
            progress(&app),
            discard_worktree_changes.unwrap_or(false),
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_config(engine: State<'_, Engine>, id: String) -> NativeResult<ConfigContent> {
    engine.read_config(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(
    engine: State<'_, Engine>,
    id: String,
    content: String,
    revision: String,
) -> NativeResult<ConfigContent> {
    engine
        .save_config(&id, content, &revision)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::UpdateState::default())
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .manage(menu::MenuState::default())
        .setup(|app| {
            let engine = Engine::new(app.path().app_data_dir()?)?;
            let settings = tauri::async_runtime::block_on(engine.snapshot()).settings;
            menu::install(app.handle(), &settings)?;
            app.manage(engine);
            Ok(())
        })
        .on_menu_event(|app, event| {
            // Native check items toggle before emitting. Restore the authoritative
            // state until the frontend applies the requested change successfully.
            let state = app
                .state::<menu::MenuState>()
                .0
                .lock()
                .ok()
                .map(|state| *state);
            if let Some(state) = state {
                let _ = menu::sync(app, state);
            }
            let _ = app.emit("envark://menu", event.id().as_ref());
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            save_settings,
            refresh,
            check_tool_updates,
            manager_options,
            cancel,
            prepare_operation,
            refresh_disks,
            execute_operation,
            read_config,
            save_config,
            links::open_app_link,
            updates::check_app_update,
            updates::install_app_update,
            updates::restart_after_update,
            set_app_theme,
            set_disabled_shortcuts,
            sync_view_state
        ])
        .run(tauri::generate_context!())
        .expect("Unable to start Envark");
}
