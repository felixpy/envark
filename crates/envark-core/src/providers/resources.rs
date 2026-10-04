use super::*;
use crate::{
    filesystem::{measure, modified},
    model::{Asset, Runtime, ServiceStatus},
};

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
    let response = super::ollama::models(super::ollama::ENDPOINT, &ctx.cancel).await;
    provider.service = Some(ServiceStatus {
        running: response.is_ok(),
        owned: false,
        endpoint: super::ollama::ENDPOINT.into(),
    });
    match response {
        Ok(models) => {
            for model in models {
                provider.assets.push(Asset {
                    id: format!("ollama:{}:{}", super::ollama::ENDPOINT, model.name),
                    name: model.name,
                    version: model.digest,
                    path: models_root.clone(),
                    size: crate::model::Measurement {
                        bytes: model.size,
                        complete: true,
                        ..Default::default()
                    },
                    last_used: None,
                    modified: None,
                    used_by: vec![],
                    can_remove: ctx.executable("ollama").is_some(),
                    note: Some(format!(
                        "Local service: {}. Model layers may be shared; size is logical.",
                        super::ollama::ENDPOINT
                    )),
                });
            }
        }
        Err(error) => provider.issues.push(error.to_string()),
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
