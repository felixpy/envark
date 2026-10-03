use envark_core::{
    config::ConfigContent,
    engine::{Engine, Snapshot},
    model::{Progress, ProgressSink, Settings},
    operations::{ActionRequest, OperationResult, PlanView},
};
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

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
async fn save_settings(engine: State<'_, Engine>, settings: Settings) -> NativeResult<Snapshot> {
    engine
        .save_settings(settings)
        .await
        .map_err(|e| e.to_string())
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
async fn cancel(engine: State<'_, Engine>, job_id: String) -> NativeResult<()> {
    engine.cancel(&job_id).await;
    Ok(())
}

#[tauri::command]
async fn prepare_operation(
    engine: State<'_, Engine>,
    request: ActionRequest,
) -> NativeResult<PlanView> {
    engine.plan(request).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn execute_operation(
    app: tauri::AppHandle,
    engine: State<'_, Engine>,
    plan_id: String,
    job_id: String,
) -> NativeResult<OperationResult> {
    engine
        .execute(&plan_id, job_id, progress(&app))
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
        .setup(|app| {
            app.manage(Engine::new(app.path().app_data_dir()?)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            save_settings,
            refresh,
            cancel,
            prepare_operation,
            execute_operation,
            read_config,
            save_config
        ])
        .run(tauri::generate_context!())
        .expect("Unable to start Envark");
}
