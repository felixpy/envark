use super::*;
use crate::{
    filesystem::{measure, modified},
    model::{Asset, Runtime, ServiceStatus},
};
use std::time::Duration;

pub async fn discover(ctx: &Context, provider: &mut Provider) -> Vec<Cache> {
    if provider.id == ProviderId::Ollama {
        ollama(ctx, provider).await;
    } else {
        browsers(ctx, provider).await;
    }
    vec![]
}

async fn ollama(ctx: &Context, provider: &mut Provider) {
    if let Ok(version) = ctx.read("ollama", &["--version"]).await
        && let Some(path) = ctx.executable("ollama")
    {
        provider.runtimes.push(Runtime {
            id: id_for("runtime", &path),
            version: version.trim().into(),
            manager: "system".into(),
            path,
            active: true,
            managed: false,
            size: None,
            note: Some("Use the Ollama installer to update the application.".into()),
        });
    }
    let models_root = std::env::var_os("OLLAMA_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".ollama/models"));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .no_proxy()
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            provider.issues.push(e.to_string());
            return;
        }
    };
    let response = client.get("http://127.0.0.1:11434/api/tags").send().await;
    provider.service = Some(ServiceStatus {
        running: response.as_ref().is_ok_and(|r| r.status().is_success()),
        owned: false,
        endpoint: "http://127.0.0.1:11434".into(),
    });
    if let Ok(response) = response {
        if let Ok(data) = response.json::<serde_json::Value>().await
            && let Some(models) = data["models"].as_array()
        {
            for model in models {
                let Some(name) = model["name"].as_str() else {
                    continue;
                };
                provider.assets.push(Asset { id: format!("ollama:{name}"), name: name.into(), version: model["digest"].as_str().unwrap_or_default().chars().take(12).collect(), path: models_root.clone(), size: crate::model::Measurement { bytes: model["size"].as_u64().unwrap_or(0), complete: true, ..Default::default() }, last_used: None, modified: None, used_by: vec![], can_remove: true, note: Some("Model layers may be shared. Reported size is logical size; actual reclaimed space can be smaller.".into()) });
            }
        }
    } else if models_root.exists() {
        provider.issues.push("Ollama is stopped. Start its service to inspect model ownership before removing models.".into());
    }
}

async fn browsers(ctx: &Context, provider: &mut Provider) {
    let (root, note) = if provider.id == ProviderId::Playwright {
        (
            std::env::var_os("PLAYWRIGHT_BROWSERS_PATH")
                .filter(|s| s != "0")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    if cfg!(windows) {
                        ctx.home.join("AppData/Local/ms-playwright")
                    } else if cfg!(target_os = "macos") {
                        ctx.home.join("Library/Caches/ms-playwright")
                    } else {
                        ctx.cache.join("ms-playwright")
                    }
                }),
            "Browser revisions can be referenced by projects outside the configured scan roots. Usage is unknown.",
        )
    } else {
        (
            std::env::var_os("PUPPETEER_CACHE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| ctx.home.join(".cache/puppeteer")),
            "Puppeteer downloads are shared. Usage is unknown until every consuming project is inspected.",
        )
    };
    let mut paths = directories(&root);
    if provider.id == ProviderId::Puppeteer {
        paths = paths.into_iter().flat_map(|p| directories(&p)).collect();
    }
    for path in paths {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if name.starts_with('.') {
            continue;
        }
        let ctx_clone = ctx.clone();
        let path_clone = path.clone();
        let size = tokio::task::spawn_blocking(move || measure(&path_clone, &ctx_clone.cancel))
            .await
            .ok()
            .and_then(std::result::Result::ok)
            .unwrap_or_default();
        provider.assets.push(Asset {
            id: id_for("asset", &path),
            name: if provider.id == ProviderId::Puppeteer {
                path.parent()
                    .and_then(Path::file_name)
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                name.split('-').next().unwrap_or(&name).into()
            },
            version: name,
            modified: modified(&path),
            path,
            size,
            last_used: None,
            used_by: vec![],
            can_remove: true,
            note: Some(note.into()),
        });
    }
    if provider.id == ProviderId::Puppeteer {
        config(provider, ctx.home.join(".puppeteerrc.json"), "json", true);
    }
}
