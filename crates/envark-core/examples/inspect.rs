use envark_core::{
    engine::Engine,
    model::{Settings, silent_progress},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let engine = Engine::new(temporary.path().join("envark"))?;
    let roots = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect();
    engine
        .save_settings(Settings {
            roots,
            ..Default::default()
        })
        .await?;
    let started = std::time::Instant::now();
    let snapshot = engine
        .refresh("inspection".into(), silent_progress())
        .await?;
    for provider in snapshot.inventory.providers {
        println!(
            "{}: {} runtimes, {} tools, {} resources, {} issues",
            provider.id.key(),
            provider.runtimes.len(),
            provider.tools.len(),
            provider.assets.len(),
            provider.issues.len()
        );
        for issue in provider.issues {
            eprintln!("{}: {issue}", provider.id.key());
        }
    }
    println!(
        "{} projects, {} caches, {} ms",
        snapshot.inventory.projects.len(),
        snapshot.inventory.caches.len(),
        started.elapsed().as_millis()
    );
    Ok(())
}
