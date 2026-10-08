use crate::{Error, Result};
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use std::{collections::BTreeMap, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::Semaphore,
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

pub(crate) const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;

struct ProcessGuard {
    child: Box<dyn ChildWrapper>,
    completed: bool,
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.child.start_kill();
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub remove_env: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
    pub successful_codes: Vec<i32>,
}

impl CommandSpec {
    pub fn new(
        program: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            env: BTreeMap::from([("NO_COLOR".into(), "1".into()), ("CI".into(), "1".into())]),
            remove_env: vec![],
            cwd: None,
            timeout: Duration::from_secs(20),
            successful_codes: vec![0],
        }
    }
    pub fn display(&self) -> String {
        std::iter::once(self.program.to_string_lossy().into_owned())
            .chain(self.args.iter().map(|a| {
                if a.contains(' ') {
                    format!("{a:?}")
                } else {
                    a.clone()
                }
            }))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

#[derive(Clone)]
pub struct Runner {
    permits: Arc<Semaphore>,
}

impl Default for Runner {
    fn default() -> Self {
        Self {
            permits: Arc::new(Semaphore::new(4)),
        }
    }
}

async fn read_bounded(mut stream: impl AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        // Keep draining after the limit so a verbose child cannot deadlock.
        let keep = (OUTPUT_LIMIT - output.len()).min(count);
        output.extend_from_slice(&buffer[..keep]);
    }
    Ok(output)
}

impl Runner {
    pub async fn run(&self, spec: &CommandSpec, cancel: &CancellationToken) -> Result<Output> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let _permit = tokio::select! {
            permit = self.permits.acquire() => permit.map_err(|e| Error::Unavailable(e.to_string()))?,
            _ = cancel.cancelled() => return Err(Error::Cancelled),
        };
        let mut command = Command::new(&spec.program);
        for name in &spec.remove_env {
            command.env_remove(name);
        }
        command
            .args(&spec.args)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        let mut command = CommandWrap::from(command);
        command.wrap(KillOnDrop);
        #[cfg(windows)]
        command
            .wrap(process_wrap::tokio::CreationFlags(
                windows::Win32::System::Threading::CREATE_NO_WINDOW,
            ))
            .wrap(process_wrap::tokio::JobObject);
        #[cfg(unix)]
        command.wrap(process_wrap::tokio::ProcessGroup::leader());
        let child = command
            .spawn()
            .map_err(|e| Error::Process(format!("Cannot start {}: {e}", spec.program.display())))?;
        let mut guard = ProcessGuard {
            child,
            completed: false,
        };
        let child = &mut guard.child;
        let out = child.stdout().take().expect("piped stdout");
        let err = child.stderr().take().expect("piped stderr");
        // JoinSet aborts the readers on every early return, including a dropped run future.
        let mut readers = JoinSet::new();
        readers.spawn(async move { (true, read_bounded(out).await) });
        readers.spawn(async move { (false, read_bounded(err).await) });
        let status = tokio::select! {
            status = child.wait() => status.map_err(Error::Io),
            _ = cancel.cancelled() => Err(Error::Cancelled),
            _ = tokio::time::sleep(spec.timeout) => Err(Error::Process(format!("{} exceeded its {} second time limit.", spec.program.display(), spec.timeout.as_secs()))),
        };
        if status.is_err() {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
        let status = status?;
        let mut out = Vec::new();
        let mut err = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(result) = readers.join_next().await {
                let (is_stdout, bytes) = result.map_err(|e| Error::Process(e.to_string()))?;
                if is_stdout {
                    out = bytes?;
                } else {
                    err = bytes?;
                }
            }
            Ok::<_, Error>(())
        })
        .await
        .map_err(|_| Error::Process("A child process kept its output pipe open.".into()))??;
        guard.completed = true;
        let output = Output {
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
            code: status.code().unwrap_or(-1),
        };
        if !spec.successful_codes.contains(&output.code) {
            let detail = if output.stderr.trim().is_empty() {
                &output.stdout
            } else {
                &output.stderr
            };
            return Err(Error::Process(format!(
                "{} exited with {}: {}",
                spec.program.display(),
                output.code,
                detail.chars().take(4000).collect::<String>().trim()
            )));
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "subprocess fixture launched by the supervision test"]
    fn process_tree_fixture() {
        let Ok(root) = std::env::var("ENVARK_PROCESS_FIXTURE") else {
            return;
        };
        let root = PathBuf::from(root);
        if std::env::var_os("ENVARK_PROCESS_DESCENDANT").is_some() {
            std::fs::write(root.join("ready"), "ready").unwrap();
            std::thread::sleep(Duration::from_secs(1));
            std::fs::write(root.join("escaped"), "should not survive cancellation").unwrap();
        } else {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "process::tests::process_tree_fixture",
                ])
                .env("ENVARK_PROCESS_DESCENDANT", "1")
                .spawn()
                .unwrap();
            child.wait().unwrap();
        }
    }

    #[tokio::test]
    async fn cancellation_terminates_descendants_and_releases_the_slot() {
        let root = tempfile::tempdir().unwrap();
        let mut spec = CommandSpec::new(
            std::env::current_exe().unwrap(),
            [
                "--ignored",
                "--exact",
                "process::tests::process_tree_fixture",
            ],
        );
        spec.env.insert(
            "ENVARK_PROCESS_FIXTURE".into(),
            root.path().to_string_lossy().into_owned(),
        );
        let runner = Runner::default();
        let token = CancellationToken::new();
        let job = tokio::spawn({
            let runner = runner.clone();
            let token = token.clone();
            async move { runner.run(&spec, &token).await }
        });
        tokio::time::timeout(Duration::from_secs(10), async {
            while !root.path().join("ready").exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        token.cancel();
        assert!(matches!(job.await.unwrap(), Err(Error::Cancelled)));
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(!root.path().join("escaped").exists());
        assert_eq!(runner.permits.available_permits(), 4);
    }

    #[tokio::test]
    async fn a_verbose_child_cannot_fill_the_output_buffer() {
        use tokio::io::AsyncWriteExt;
        let (reader, mut writer) = tokio::io::duplex(4096);
        let task = tokio::spawn(async move {
            writer
                .write_all(&vec![b'x'; OUTPUT_LIMIT + 65_536])
                .await
                .unwrap();
        });
        let output = tokio::time::timeout(Duration::from_secs(5), read_bounded(reader))
            .await
            .unwrap()
            .unwrap();
        task.await.unwrap();
        assert_eq!(output.len(), OUTPUT_LIMIT);
    }

    #[tokio::test]
    async fn cancellation_while_waiting_for_a_slot_is_prompt() {
        let runner = Runner {
            permits: Arc::new(Semaphore::new(0)),
        };
        let token = CancellationToken::new();
        token.cancel();
        let result = runner
            .run(&CommandSpec::new("does-not-exist", ["--version"]), &token)
            .await;
        assert!(matches!(result, Err(Error::Cancelled)));
    }

    #[tokio::test]
    async fn missing_executables_are_reported_as_failures() {
        let result = Runner::default()
            .run(
                &CommandSpec::new("envark-test-missing-executable", ["--version"]),
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(result, Err(Error::Process(_))));
    }
}
