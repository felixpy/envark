use serde::Deserialize;
use tauri_plugin_opener::OpenerExt;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppLink {
    Github,
    Issues,
    Releases,
    Latest,
}

impl AppLink {
    fn url(&self) -> &'static str {
        match self {
            Self::Github => "https://github.com/felixpy/envark",
            Self::Issues => "https://github.com/felixpy/envark/issues/new/choose",
            Self::Releases => "https://github.com/felixpy/envark/releases",
            Self::Latest => "https://github.com/felixpy/envark/releases/latest",
        }
    }
}

#[tauri::command]
pub async fn open_app_link(app: tauri::AppHandle, target: AppLink) -> Result<(), String> {
    // Only these application destinations can reach the system browser.
    app.opener()
        .open_url(target.url(), None::<&str>)
        .map_err(|e| e.to_string())
}
