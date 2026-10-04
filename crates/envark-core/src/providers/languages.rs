use super::*;
use crate::model::Runtime;

pub async fn discover(ctx: &Context, provider: &mut Provider) -> Vec<Cache> {
    match provider.id {
        ProviderId::Py => python(ctx, provider).await,
        ProviderId::Rust => rust(ctx, provider).await,
        ProviderId::Go => go(ctx, provider).await,
        ProviderId::Jvm => java(ctx, provider).await,
        _ => vec![],
    }
}

async fn python(ctx: &Context, provider: &mut Provider) -> Vec<Cache> {
    let mut caches = vec![];
    for name in ["uv", "pyenv"] {
        if let Some(manager) = ctx.manager(name, true, true).await {
            provider.managers.push(manager);
        }
    }
    for name in ["uv", "pyenv"] {
        if !provider.managers.iter().any(|manager| manager.name == name) {
            continue;
        }
        let result = if name == "uv" {
            super::python::uv(ctx).await
        } else {
            super::python::pyenv(ctx).await
        };
        match result {
            Ok(runtimes) => provider.runtimes.extend(runtimes),
            Err(error) => provider.issues.push(format!("{name}: {error}")),
        }
    }
    for name in ["uv", "pipx", "poetry"] {
        if let Ok(version) = ctx.read(name, &["--version"]).await {
            provider.package_managers.push(basic_tool(
                name,
                version.trim().into(),
                "PATH",
                ctx.executable(name),
            ));
        }
    }
    if let Ok(output) = ctx.read("uv", &["tool", "list"]).await {
        for line in output
            .lines()
            .filter(|l| !l.starts_with([' ', '-']) && !l.trim().is_empty())
        {
            let mut fields = line.split_whitespace();
            if let (Some(name), Some(version)) = (fields.next(), fields.next()) {
                let mut tool = basic_tool(name, version.trim_start_matches('v').into(), "uv", None);
                tool.can_update = true;
                tool.can_remove = true;
                provider.tools.push(tool);
            }
        }
    }
    if let Ok(output) = ctx.read("pipx", &["list", "--json"]).await
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&output)
        && let Some(venvs) = json["venvs"].as_object()
    {
        for (name, entry) in venvs {
            let mut tool = basic_tool(
                name,
                entry["metadata"]["main_package"]["package_version"]
                    .as_str()
                    .unwrap_or("unknown")
                    .into(),
                "pipx",
                None,
            );
            tool.can_update = true;
            tool.can_remove = true;
            provider.tools.push(tool);
        }
    }
    if let Some(path) = ctx.cache_path("uv", &["cache", "dir"]).await
        && let Some(item) = cache(ProviderId::Py, "uv", path, "uv-prune", true)
    {
        caches.push(item);
    }
    let pip_path = if cfg!(windows) {
        ctx.home.join("AppData/Local/pip/Cache")
    } else if cfg!(target_os = "macos") {
        ctx.home.join("Library/Caches/pip")
    } else {
        ctx.cache.join("pip")
    };
    if let Some(item) = cache(ProviderId::Py, "pip", pip_path, "owner-managed", false) {
        caches.push(item);
    }
    config(provider, ctx.home.join(".config/uv/uv.toml"), "toml", true);
    config(provider, ctx.data.join("uv/uv.toml"), "toml", true);
    config(provider, ctx.home.join(".config/pip/pip.conf"), "ini", true);
    config(provider, ctx.home.join("pip/pip.ini"), "ini", true);
    config(provider, ctx.home.join(".python-version"), "text", true);
    caches
}

async fn rust(ctx: &Context, provider: &mut Provider) -> Vec<Cache> {
    if let Some(manager) = ctx.manager("rustup", true, true).await {
        provider.managers.push(manager);
    }
    let rustup_root = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".rustup"));
    match ctx.read("rustup", &["toolchain", "list"]).await {
        Ok(output) => {
            for line in output.lines().filter(|l| !l.contains("no installed")) {
                let Some(version) = line.split_whitespace().next() else {
                    continue;
                };
                let path = rustup_root.join("toolchains").join(version);
                if path.is_dir() {
                    provider.runtimes.push(Runtime {
                        selector: None,
                        active_known: true,
                        id: id_for("runtime", &path),
                        version: version.into(),
                        manager: "rustup".into(),
                        path,
                        active: line.contains("default"),
                        managed: true,
                        size: None,
                        note: None,
                    });
                }
            }
        }
        Err(e) if ctx.executable("rustup").is_some() => provider.issues.push(e.to_string()),
        _ => (),
    }
    match ctx.read("cargo", &["--version"]).await {
        Ok(version) => provider.package_managers.push(basic_tool(
            "cargo",
            version.trim().into(),
            "rustup",
            ctx.executable("cargo"),
        )),
        Err(error) if ctx.executable("cargo").is_some() => provider.issues.push(format!(
            "Cargo could not be inspected without installing a toolchain: {error}"
        )),
        _ => (),
    }
    if let Ok(output) = ctx.read("cargo", &["install", "--list"]).await {
        for line in output
            .lines()
            .filter(|l| !l.starts_with(' ') && l.ends_with(':'))
        {
            let mut fields = line.split_whitespace();
            if let (Some(name), Some(version)) = (fields.next(), fields.next()) {
                let mut tool = basic_tool(
                    name,
                    version.trim_start_matches('v').trim_end_matches(':').into(),
                    "cargo",
                    None,
                );
                tool.can_update = !line.contains('(');
                tool.can_remove = true;
                if !tool.can_update {
                    tool.note = Some("Update this Git, path, or alternate source installation with its original install specification.".into());
                }
                provider.tools.push(tool);
            }
        }
    }
    let cargo_root = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".cargo"));
    config(provider, cargo_root.join("config.toml"), "toml", true);
    [
        cache(
            ProviderId::Rust,
            "Cargo registry",
            cargo_root.join("registry"),
            "owner-managed",
            false,
        ),
        cache(
            ProviderId::Rust,
            "Cargo Git checkouts",
            cargo_root.join("git"),
            "owner-managed",
            false,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

async fn go(ctx: &Context, provider: &mut Provider) -> Vec<Cache> {
    let Ok(output) = ctx
        .read(
            "go",
            &[
                "env",
                "-json",
                "GOROOT",
                "GOPATH",
                "GOBIN",
                "GOCACHE",
                "GOMODCACHE",
                "GOVERSION",
            ],
        )
        .await
    else {
        return vec![];
    };
    let Ok(env) = serde_json::from_str::<serde_json::Value>(&output) else {
        provider.issues.push("Cannot parse go env output.".into());
        return vec![];
    };
    if let Some(root) = env["GOROOT"].as_str() {
        let path = PathBuf::from(root);
        provider.runtimes.push(Runtime {
            selector: None,
            active_known: true,
            id: id_for("runtime", &path),
            version: env["GOVERSION"]
                .as_str()
                .unwrap_or("unknown")
                .trim_start_matches("go")
                .into(),
            manager: "PATH".into(),
            path,
            active: true,
            managed: false,
            size: None,
            note: Some("Manage the Go SDK with its original installer.".into()),
        });
    }
    let bin = env["GOBIN"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env["GOPATH"]
                .as_str()
                .and_then(|p| std::env::split_paths(p).next())
                .map(|p| p.join("bin"))
        });
    if let Some(bin) = bin
        && let Ok(entries) = std::fs::read_dir(&bin)
    {
        for entry in entries.flatten().take(1000) {
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let path = entry.path();
            let text_path = path.to_string_lossy();
            if let Ok(info) = ctx.read("go", &["version", "-m", &text_path]).await {
                let module = info
                    .lines()
                    .map(str::trim)
                    .find_map(|line| line.strip_prefix("mod\t"));
                if let Some(module) = module {
                    let fields: Vec<_> = module.split_whitespace().collect();
                    if fields.len() >= 2 {
                        let command = info
                            .lines()
                            .map(str::trim)
                            .find_map(|line| line.strip_prefix("path\t"))
                            .map(str::trim);
                        let name = command.unwrap_or(fields[0]);
                        let verified =
                            command.is_some_and(|name| {
                                name.rsplit('/').next() == path.file_stem().and_then(|p| p.to_str())
                            }) && !info.lines().any(|line| line.trim_start().starts_with("=>"))
                                && fields[1] != "(devel)";
                        let mut tool = basic_tool(name, fields[1].into(), "go", Some(path));
                        tool.can_update = verified;
                        tool.note = Some(if verified {
                            format!("Verified command import path; module {}.", fields[0])
                        } else {
                            "Local builds, replaced modules, and renamed executables remain read-only.".into()
                        });
                        provider.tools.push(tool);
                    }
                }
            }
        }
    }
    [
        ("Go build", "GOCACHE", "go-build"),
        ("Go modules", "GOMODCACHE", "go-modules"),
    ]
    .into_iter()
    .filter_map(|(name, key, strategy)| {
        env[key]
            .as_str()
            .and_then(|p| cache(ProviderId::Go, name, PathBuf::from(p), strategy, true))
    })
    .collect()
}

async fn java(ctx: &Context, provider: &mut Provider) -> Vec<Cache> {
    if let Some(manager) = ctx.manager("SDKMAN!", true, true).await {
        provider.managers.push(manager);
    }
    let sdkman = std::env::var_os("SDKMAN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".sdkman"));
    let mut roots = directories(&sdkman.join("candidates/java"));
    if cfg!(target_os = "macos") {
        roots.extend(directories(Path::new("/Library/Java/JavaVirtualMachines")));
    }
    if cfg!(target_os = "linux") {
        roots.extend(directories(Path::new("/usr/lib/jvm")));
    }
    if cfg!(windows)
        && let Some(program_files) = std::env::var_os("ProgramFiles")
    {
        roots.extend(directories(&PathBuf::from(&program_files).join("Java")));
        roots.extend(directories(
            &PathBuf::from(program_files).join("Eclipse Adoptium"),
        ));
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        roots.push(PathBuf::from(home));
    }
    roots.sort();
    roots.dedup();
    let active = ctx
        .executable("java")
        .and_then(|p| std::fs::canonicalize(p).ok());
    for mut path in roots {
        if path.join("Contents/Home").is_dir() {
            path = path.join("Contents/Home");
        }
        let binary = path.join(if cfg!(windows) {
            "bin/java.exe"
        } else {
            "bin/java"
        });
        if !binary.is_file() {
            continue;
        }
        let version = read_small(&path.join("release"), 32_768)
            .ok()
            .and_then(|s| {
                s.lines().find_map(|l| {
                    l.strip_prefix("JAVA_VERSION=")
                        .map(|v| v.trim_matches('"').to_owned())
                })
            })
            .unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let managed = path.starts_with(sdkman.join("candidates/java"))
            && provider.managers.iter().any(|m| m.name == "SDKMAN!");
        let selector = if managed {
            path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or(version.clone())
        } else {
            version.clone()
        };
        provider.runtimes.push(Runtime {
            selector: None,
            active_known: true,
            id: id_for("runtime", &path),
            version: selector,
            manager: if path.starts_with(&sdkman) {
                "SDKMAN!"
            } else {
                "system"
            }
            .into(),
            active: std::fs::canonicalize(&binary).ok().as_ref() == active.as_ref()
                && active.is_some(),
            path,
            managed,
            size: None,
            note: Some(if managed {
                format!("Java {version}; managed by SDKMAN!.")
            } else {
                "This JDK is managed by its original installer.".into()
            }),
        });
    }
    for name in ["mvn", "gradle"] {
        if let Ok(version) = ctx.read(name, &["--version"]).await {
            provider.package_managers.push(basic_tool(
                name,
                version
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("unknown")
                    .into(),
                "PATH",
                ctx.executable(name),
            ));
        }
    }
    config(provider, ctx.home.join(".m2/settings.xml"), "xml", true);
    config(
        provider,
        ctx.home.join(".gradle/gradle.properties"),
        "properties",
        true,
    );
    config(provider, sdkman.join("etc/config"), "properties", true);
    [
        cache(
            ProviderId::Jvm,
            "Maven repository",
            ctx.home.join(".m2/repository"),
            "owner-managed",
            false,
        ),
        cache(
            ProviderId::Jvm,
            "Gradle caches",
            ctx.home.join(".gradle/caches"),
            "owner-managed",
            false,
        ),
        cache(
            ProviderId::Jvm,
            "Gradle distributions",
            ctx.home.join(".gradle/wrapper/dists"),
            "owner-managed",
            false,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}
