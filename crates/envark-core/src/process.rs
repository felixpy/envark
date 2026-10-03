use crate::{Error, Result};
use std::{collections::BTreeMap, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::Semaphore,
};
use tokio_util::sync::CancellationToken;

const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
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
        let _permit = tokio::select! {
            permit = self.permits.acquire() => permit.map_err(|e| Error::Unavailable(e.to_string()))?,
            _ = cancel.cancelled() => return Err(Error::Cancelled),
        };
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command
            .spawn()
            .map_err(|e| Error::Process(format!("Cannot start {}: {e}", spec.program.display())))?;
        let stdout = tokio::spawn(read_bounded(child.stdout.take().expect("piped stdout")));
        let stderr = tokio::spawn(read_bounded(child.stderr.take().expect("piped stderr")));
        let status = tokio::select! {
            status = child.wait() => status.map_err(Error::Io),
            _ = cancel.cancelled() => { let _ = child.kill().await; Err(Error::Cancelled) },
            _ = tokio::time::sleep(spec.timeout) => { let _ = child.kill().await; Err(Error::Process(format!("{} exceeded its {} second time limit.", spec.program.display(), spec.timeout.as_secs()))) },
        };
        // A descendant may retain a pipe; do not let it prevent cancellation.
        let streams = tokio::time::timeout(Duration::from_secs(2), async {
            (stdout.await, stderr.await)
        })
        .await;
        let status = status?;
        let (out, err) = streams
            .map_err(|_| Error::Process("A child process kept its output pipe open.".into()))?;
        let out = out.map_err(|e| Error::Process(e.to_string()))??;
        let err = err.map_err(|e| Error::Process(e.to_string()))??;
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
