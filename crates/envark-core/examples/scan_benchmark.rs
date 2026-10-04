use envark_core::{
    model::{Settings, silent_progress},
    scan_cache::ScanCache,
};
use std::{
    fs,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let projects: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "200".into())
        .parse()?;
    let files: usize = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "100".into())
        .parse()?;
    if projects == 0 || projects > 1000 || files == 0 || files > 1000 {
        return Err("Use 1–1000 projects and files per artifact".into());
    }
    let fixture = tempfile::tempdir()?;
    let root = fs::canonicalize(fixture.path())?;
    let started = Instant::now();
    for index in 0..projects {
        let project = root.join(format!("project-{index}"));
        let (manifest, artifact) = match index % 4 {
            0 => ("package.json", "node_modules/package"),
            1 => ("pyproject.toml", ".venv/lib"),
            2 => ("Cargo.toml", "target/debug"),
            _ => ("pom.xml", "target/classes"),
        };
        let generated = project.join(artifact);
        fs::create_dir_all(&generated)?;
        fs::create_dir_all(project.join(".git/objects"))?;
        fs::create_dir_all(project.join(".git/refs"))?;
        fs::write(project.join(".git/HEAD"), "ref: refs/heads/main\n")?;
        fs::write(project.join(manifest), "{}")?;
        fs::write(project.join("source.txt"), "source")?;
        if index % 4 == 1 {
            fs::write(project.join(".venv/pyvenv.cfg"), "home = fixture")?;
        }
        for file in 0..files {
            fs::write(generated.join(format!("file-{file}")), [b'x'; 1024])?;
        }
    }
    println!(
        "fixture: projects={projects} generated_files={} setup_ms={} os={} available_threads={}",
        projects * files,
        started.elapsed().as_millis(),
        std::env::consts::OS,
        std::thread::available_parallelism()?
    );
    let settings = Settings {
        roots: vec![root.clone()],
        ..Default::default()
    };
    let token = CancellationToken::new();
    let mut cache = ScanCache::default();
    for (label, force) in [
        ("cold", false),
        ("warm", false),
        ("warm-repeat", false),
        ("forced", true),
    ] {
        let result = cache.scan(&settings, &token, silent_progress(), "benchmark", force)?;
        assert_eq!(result.projects.len(), projects);
        assert!(result.issues.is_empty());
        println!(
            "{label}: elapsed_ms={} discovery_entries={} cached_roots={} inventory_files={}",
            result.elapsed_ms,
            result.visited,
            result.cached_roots,
            result
                .projects
                .iter()
                .flat_map(|p| &p.artifacts)
                .map(|a| a.size.files)
                .sum::<u64>()
        );
    }
    fs::write(
        root.join("project-0/node_modules/package/new-file"),
        "added",
    )?;
    std::thread::sleep(Duration::from_secs(2));
    let changed = cache.scan(&settings, &token, silent_progress(), "benchmark", false)?;
    let project = changed
        .projects
        .iter()
        .find(|p| p.name == "project-0")
        .ok_or("Missing fixture project")?;
    assert_eq!(project.artifacts[0].size.files, files as u64 + 1);
    println!(
        "changed: elapsed_ms={} cached_roots={} changed_file_detected=true",
        changed.elapsed_ms, changed.cached_roots
    );
    Ok(())
}
