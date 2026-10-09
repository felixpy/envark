use super::*;
use std::process::{Child, Stdio};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceReceipt {
    pid: u32,
    started: u64,
    binary: PathBuf,
}

fn record_path(ctx: &Context) -> PathBuf {
    ctx.data.join("envark/ollama/service.json")
}

fn processes() -> System {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::Always)
            .with_cmd(UpdateKind::Always),
    );
    system
}

fn matching(system: &System, record: &ServiceReceipt) -> bool {
    system
        .process(Pid::from_u32(record.pid))
        .is_some_and(|process| {
            process.start_time() == record.started
                && process.exe().is_some_and(|path| {
                    let mut deleted = record.binary.as_os_str().to_os_string();
                    deleted.push(" (deleted)");
                    path == record.binary
                        || path.canonicalize().ok().as_ref() == Some(&record.binary)
                        || (cfg!(target_os = "linux") && path.as_os_str() == deleted)
                })
                && process.cmd().iter().skip(1).any(|arg| arg == "serve")
        })
}

fn read_record(ctx: &Context) -> Option<ServiceReceipt> {
    let path = record_path(ctx);
    reject_links(&path).ok()?;
    serde_json::from_str(&read_small(&path, 16384).ok()?).ok()
}

pub(super) fn owned(ctx: &Context) -> Option<PathBuf> {
    let record = read_record(ctx)?;
    matching(&processes(), &record).then_some(record.binary)
}

async fn endpoint_busy() -> bool {
    tokio::time::timeout(
        Duration::from_secs(1),
        tokio::net::TcpStream::connect("127.0.0.1:11434"),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

async fn listener_owned(ctx: &Context, pid: u32) -> Result<bool> {
    if cfg!(target_os = "linux") {
        let sockets = std::fs::read_to_string(format!("/proc/{pid}/net/tcp"))?;
        let inodes: Vec<&str> = sockets
            .lines()
            .filter_map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                // 127.0.0.1:11434 in Linux's little-endian /proc socket table.
                (fields.get(1) == Some(&"0100007F:2CAA") && fields.get(3) == Some(&"0A"))
                    .then(|| fields.get(9).copied())
                    .flatten()
            })
            .collect();
        return Ok(std::fs::read_dir(format!("/proc/{pid}/fd"))?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| std::fs::read_link(entry.path()).ok())
            .any(|path| {
                inodes
                    .iter()
                    .any(|inode| path == Path::new(&format!("socket:[{inode}]")))
            }));
    }
    let command = if cfg!(target_os = "macos") {
        CommandSpec::new(
            "/usr/sbin/lsof",
            [
                "-nP",
                "-a",
                "-p",
                &pid.to_string(),
                "-iTCP@127.0.0.1:11434",
                "-sTCP:LISTEN",
                "-Fn",
            ],
        )
    } else {
        let mut command = CommandSpec::new(
            ctx.executable("powershell")
                .or_else(|| ctx.executable("pwsh"))
                .ok_or_else(|| {
                    Error::Unavailable(
                        "PowerShell is required to verify the Ollama listener owner.".into(),
                    )
                })?,
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-NetTCPConnection -State Listen -LocalAddress 127.0.0.1 -LocalPort 11434 -ErrorAction Stop | Select-Object -ExpandProperty OwningProcess) -contains [int]$env:ENVARK_OLLAMA_PID",
            ],
        );
        command
            .env
            .insert("ENVARK_OLLAMA_PID".into(), pid.to_string());
        command
    };
    let output = ctx.runner.run(&command, &ctx.cancel).await?;
    Ok(if cfg!(target_os = "macos") {
        output.stdout.lines().any(|line| line == "n127.0.0.1:11434")
    } else {
        output.stdout.trim().eq_ignore_ascii_case("true")
    })
}

pub(super) async fn require_stopped(ctx: &Context, root: &Path) -> Result<()> {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.into());
    if processes()
        .processes()
        .values()
        .any(|p| p.exe().is_some_and(|p| p.starts_with(&canonical)))
    {
        return Err(Error::Conflict("Ollama is still running from this program directory. Stop its Envark-owned service, or quit its owning application, then review the program action again.".into()));
    }
    if owned(ctx).is_some() {
        return Err(Error::Conflict(
            "Stop the Envark-owned Ollama service before changing the program.".into(),
        ));
    }
    Ok(())
}

struct StartedChild {
    child: Option<Child>,
}
impl Drop for StartedChild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            // The Child handle belongs only to the process just created by this operation.
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(super) async fn start(ctx: &Context, binary: &Path) -> Result<(u64, String)> {
    if endpoint_busy().await {
        return Err(Error::Conflict("Port 11434 is already in use. Envark will not replace, stop, or adopt the existing service.".into()));
    }
    if owned(ctx).is_some() {
        return Err(Error::Conflict(
            "An Envark-owned Ollama process is already running.".into(),
        ));
    }
    let binary = binary.canonicalize()?;
    reject_links(&binary)?;
    let directory = ctx.data.join("envark/ollama");
    reject_links(&directory)?;
    std::fs::create_dir_all(&directory)?;
    let log = directory.join("server.log");
    reject_links(&log)?;
    let output = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let mut command = std::process::Command::new(&binary);
    command
        .arg("serve")
        .current_dir(&ctx.home)
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        .env("OLLAMA_HOST", super::super::ollama::ENDPOINT);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008 | 0x00000200);
    }
    if ctx.cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let mut started = StartedChild {
        child: Some(command.spawn()?),
    };
    let pid = started.child.as_ref().unwrap().id();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    let mut healthy = false;
    for _ in 0..60 {
        if ctx.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if started.child.as_mut().unwrap().try_wait()?.is_some() {
            return Err(Error::Process(format!(
                "Ollama exited before its local API became ready. See {}.",
                log.display()
            )));
        }
        let health = async {
            let response = client
                .get(format!("{}/api/version", super::super::ollama::ENDPOINT))
                .send()
                .await
                .ok()?
                .error_for_status()
                .ok()?;
            let value: serde_json::Value = response.json().await.ok()?;
            semver::Version::parse(value["version"].as_str()?).ok()
        };
        let response = tokio::select! { value = health => value, _ = ctx.cancel.cancelled() => return Err(Error::Cancelled) };
        if response.is_some() && started.child.as_mut().unwrap().try_wait()?.is_none() {
            healthy = true;
            break;
        }
        tokio::select! { _ = tokio::time::sleep(Duration::from_millis(250)) => {}, _ = ctx.cancel.cancelled() => return Err(Error::Cancelled) }
    }
    if !healthy {
        return Err(Error::Unavailable(format!(
            "Ollama's local API did not become ready. The new process was stopped. See {}.",
            log.display()
        )));
    }
    let system = processes();
    let process = system
        .process(Pid::from_u32(pid))
        .ok_or_else(|| Error::Conflict("The new Ollama process disappeared.".into()))?;
    let record = ServiceReceipt {
        pid,
        started: process.start_time(),
        binary,
    };
    if !matching(&system, &record) {
        return Err(Error::Conflict(
            "The new Ollama process identity could not be verified.".into(),
        ));
    }
    if !listener_owned(ctx, pid).await? {
        return Err(Error::Conflict("The local API listener does not belong to the new Ollama process. Only the new process was stopped.".into()));
    }
    let path = record_path(ctx);
    reject_links(&path)?;
    std::fs::write(&path, serde_json::to_vec(&record)?)?;
    let mut child = started.child.take().unwrap();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    // The service intentionally outlives this action and its cancellation token.
    Ok((
        0,
        format!(
            "Ollama service started at {}. Program files and models were not changed.",
            super::super::ollama::ENDPOINT
        ),
    ))
}

pub(super) fn snapshot(ctx: &Context) -> Result<String> {
    let path = record_path(ctx);
    reject_links(&path)?;
    read_small(&path, 16384)
}

pub(super) fn review(ctx: &Context) -> Result<(PathBuf, String)> {
    let snapshot = snapshot(ctx)?;
    let record: ServiceReceipt = serde_json::from_str(&snapshot)?;
    if !matching(&processes(), &record) {
        return Err(Error::Conflict(
            "The recorded Ollama process identity changed. No process will be stopped.".into(),
        ));
    }
    Ok((record.binary, snapshot))
}

pub(super) async fn stop(ctx: &Context, expected: &str) -> Result<(u64, String)> {
    if snapshot(ctx)? != expected {
        return Err(Error::Conflict(
            "The reviewed Ollama service identity changed. No process was stopped.".into(),
        ));
    }
    let record = read_record(ctx).ok_or_else(|| {
        Error::Conflict(
            "No Envark-owned Ollama service is recorded. The existing service will not be stopped."
                .into(),
        )
    })?;
    let system = processes();
    if !matching(&system, &record) {
        return Err(Error::Conflict(
            "The Ollama process identity changed. No process was stopped.".into(),
        ));
    }
    if ctx.cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let process = system.process(Pid::from_u32(record.pid)).unwrap();
    let stopped = if cfg!(windows) {
        process.kill()
    } else {
        process.kill_with(sysinfo::Signal::Term).unwrap_or(false)
    };
    if !stopped {
        return Err(Error::Unavailable(
            "The operating system refused to stop the owned Ollama process.".into(),
        ));
    }
    for _ in 0..40 {
        if !matching(&processes(), &record) {
            let path = record_path(ctx);
            reject_links(&path)?;
            std::fs::remove_file(path)?;
            return Ok((0, "Envark-owned Ollama service stopped. The program, models, and configuration were preserved.".into()));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(Error::Unavailable("The stop signal was sent, but Ollama is still running. Its program and data were preserved.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn fixture(root: &Path, responds: bool) -> PathBuf {
        let source = root.join("fixture.rs");
        let executable = root.join("ollama-fixture");
        let pid_path = serde_json::to_string(&root.join("pid").to_string_lossy()).unwrap();
        std::fs::write(&source, format!(r#"
use std::io::{{Read, Write}};
fn main() {{
    assert_eq!(std::env::args().nth(1).as_deref(), Some("serve"));
    assert_eq!(std::env::var("OLLAMA_HOST").unwrap(), "http://127.0.0.1:11434");
    std::fs::write({pid_path}, std::process::id().to_string()).unwrap();
    if !{responds} {{ loop {{ std::thread::sleep(std::time::Duration::from_secs(1)); }} }}
    let listener = std::net::TcpListener::bind("127.0.0.1:11434").unwrap();
    for stream in listener.incoming() {{
        let mut stream = stream.unwrap();
        let mut request = [0; 4096];
        let _ = stream.read(&mut request);
        let body = "{{\"version\":\"1.2.3\"}}";
        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {{}}\r\nConnection: close\r\n\r\n{{}}", body.len(), body);
    }}
}}
"#)).unwrap();
        let output =
            std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        executable
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn daemon_roundtrip_and_cancellation_are_scoped_to_created_process() {
        // Never interfere with a developer's real Ollama service when running tests locally.
        if endpoint_busy().await {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let mut ctx = Context::new(CancellationToken::new()).unwrap();
        ctx.home = temp.path().canonicalize().unwrap();
        ctx.data = ctx.home.join("data");
        let executable = fixture(&ctx.home, true);
        let result = start(&ctx, &executable).await;
        assert!(
            result.is_ok(),
            "{result:?}: {}",
            std::fs::read_to_string(ctx.data.join("envark/ollama/server.log")).unwrap_or_default()
        );
        assert_eq!(owned(&ctx), Some(executable.canonicalize().unwrap()));
        assert!(start(&ctx, &executable).await.is_err());
        assert!(stop(&ctx, "stale identity").await.is_err());
        assert!(owned(&ctx).is_some());
        // The service receipt remains sufficient even if the installation disappeared.
        std::fs::remove_file(&executable).unwrap();
        let plan = super::super::prepare(
            &ctx,
            ActionRequest::ServiceAction {
                provider: ProviderId::Ollama,
                action: "stop".into(),
            },
            &Inventory::default(),
            &Settings::default(),
        )
        .await
        .unwrap();
        assert!(plan.fingerprint.is_none());
        assert!(
            plan.warnings()
                .iter()
                .all(|warning| !warning.contains("ROCm")
                    && !warning.contains("tray")
                    && !warning.contains("install"))
        );
        plan.execute(&ctx, false).await.unwrap();
        assert!(owned(&ctx).is_none());
        assert!(!executable.exists());

        let stubborn = ctx.home.join("stubborn");
        std::fs::create_dir(&stubborn).unwrap();
        let executable = fixture(&stubborn, false);
        let task = tokio::spawn({
            let ctx = ctx.clone();
            async move { start(&ctx, &executable).await }
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !stubborn.join("pid").exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let pid = std::fs::read_to_string(stubborn.join("pid"))
            .unwrap()
            .parse::<u32>()
            .unwrap();
        ctx.cancel.cancel();
        assert!(matches!(task.await.unwrap(), Err(Error::Cancelled)));
        assert!(processes().process(Pid::from_u32(pid)).is_none());
        assert!(owned(&ctx).is_none());
    }

    #[test]
    fn stale_or_reused_pid_never_proves_ownership() {
        let record = ServiceReceipt {
            pid: std::process::id(),
            started: 0,
            binary: PathBuf::from("/not/ollama"),
        };
        assert!(!matching(&processes(), &record));
    }
}
