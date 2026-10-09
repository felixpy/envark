use super::Context;
use crate::{
    Error, Result,
    model::{Progress, ProgressSink},
    process::CommandSpec,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;

pub const ENDPOINT: &str = "http://127.0.0.1:11434";

pub(crate) fn validate(endpoint: &str, name: &str) -> Result<()> {
    if endpoint != ENDPOINT {
        return Err(Error::Conflict(
            "Ollama's endpoint changed. Refresh the local inventory.".into(),
        ));
    }
    super::valid_identifier(name)
}

fn canonical_name(name: &str) -> String {
    if name
        .rsplit('/')
        .next()
        .is_some_and(|part| part.contains(':'))
    {
        name.into()
    } else {
        format!("{name}:latest")
    }
}

fn api_client(timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|error| Error::Unavailable(error.to_string()))
}

async fn checked(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| Error::Unavailable(error.to_string()))?
    {
        let remaining = 65536usize.saturating_sub(body.len());
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if body.len() == 65536 {
            break;
        }
    }
    let detail = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| value["error"].as_str().map(str::to_owned))
        .unwrap_or_else(|| status.to_string());
    Err(Error::Process(format!(
        "Ollama request failed: {}",
        detail.chars().take(4000).collect::<String>()
    )))
}

fn pull_status(
    line: &[u8],
    progress: &ProgressSink,
    job_id: &str,
    layers: &mut BTreeMap<String, u64>,
) -> Result<bool> {
    let value: serde_json::Value = serde_json::from_slice(line).map_err(|_| {
        Error::Process("Ollama returned an invalid download progress message.".into())
    })?;
    if let Some(error) = value["error"].as_str() {
        return Err(Error::Process(format!(
            "Ollama download failed: {}",
            error.chars().take(4000).collect::<String>()
        )));
    }
    let status = value["status"]
        .as_str()
        .ok_or_else(|| Error::Process("Ollama download progress omitted its status.".into()))?;
    let mut message: String = status.chars().take(512).collect();
    if let Some(completed) = value["completed"].as_u64() {
        let layer = value["digest"]
            .as_str()
            .unwrap_or("current layer")
            .to_owned();
        let previous = layers.entry(layer).or_default();
        *previous = (*previous).max(completed);
        if let Some(total) = value["total"].as_u64() {
            message.push_str(&format!(" ({completed}/{total} bytes for this layer)"));
        }
    }
    // Layer totals are not an overall total. Retain monotonic aggregate bytes and
    // leave overall progress indeterminate while reporting the current layer.
    progress(Progress {
        job_id: job_id.into(),
        stage: "download-model".into(),
        completed: layers
            .values()
            .fold(0u64, |sum, bytes| sum.saturating_add(*bytes)),
        total: None,
        message,
    });
    Ok(status == "success")
}

pub(crate) async fn execute_model(
    ctx: &Context,
    endpoint: &str,
    name: &str,
    digest: Option<&str>,
    progress: &ProgressSink,
    job_id: &str,
) -> Result<(u64, String)> {
    validate(endpoint, name)?;
    execute_model_at(ctx, endpoint, name, digest, progress, job_id).await
}

async fn execute_model_at(
    ctx: &Context,
    endpoint: &str,
    name: &str,
    digest: Option<&str>,
    progress: &ProgressSink,
    job_id: &str,
) -> Result<(u64, String)> {
    let request = async {
        let client = api_client(Duration::from_secs(if digest.is_some() {
            60
        } else {
            7200
        }))?;
        if let Some(digest) = digest {
            verify(ctx, endpoint, name, digest).await?;
            let response = client
                .delete(format!("{endpoint}/api/delete"))
                .json(&serde_json::json!({"model":name}))
                .send()
                .await
                .map_err(|error| Error::Unavailable(error.to_string()))?;
            let mut response = checked(response).await?;
            let mut body = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| Error::Unavailable(error.to_string()))?
            {
                if body.len().saturating_add(chunk.len()) > 65536 {
                    return Err(Error::Process(
                        "Ollama deletion returned an oversized response.".into(),
                    ));
                }
                body.extend_from_slice(&chunk);
            }
            if let Some(error) = serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| value["error"].as_str().map(str::to_owned))
            {
                return Err(Error::Process(format!(
                    "Ollama deletion failed: {}",
                    error.chars().take(4000).collect::<String>()
                )));
            }
            if models(endpoint, &ctx.cancel)
                .await?
                .iter()
                .any(|model| canonical_name(&model.name) == canonical_name(name))
            {
                return Err(Error::Conflict(
                    "Ollama still reports the model after deletion. Refresh before trying again."
                        .into(),
                ));
            }
            return Ok((
                0,
                format!("Removed model {name}. The Ollama program and service were preserved."),
            ));
        }
        let response = client
            .post(format!("{endpoint}/api/pull"))
            .json(&serde_json::json!({"model":name,"stream":true}))
            .send()
            .await
            .map_err(|error| Error::Unavailable(error.to_string()))?;
        let mut response = checked(response).await?;
        let mut pending = Vec::new();
        let mut layers = BTreeMap::new();
        let mut success = false;
        'stream: while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| Error::Unavailable(error.to_string()))?
        {
            for part in chunk.split_inclusive(|byte| *byte == b'\n') {
                if pending.len().saturating_add(part.len()) > 65536 {
                    return Err(Error::Process(
                        "Ollama returned an oversized download progress message.".into(),
                    ));
                }
                pending.extend_from_slice(part);
                if pending.last() == Some(&b'\n') {
                    if !pending.iter().all(u8::is_ascii_whitespace) {
                        success = pull_status(&pending, progress, job_id, &mut layers)?;
                    }
                    pending.clear();
                    if success {
                        break 'stream;
                    }
                }
            }
        }
        if !success && !pending.iter().all(u8::is_ascii_whitespace) {
            success = pull_status(&pending, progress, job_id, &mut layers)?;
        }
        if !success {
            return Err(Error::Process("Ollama's download ended without a success response. The model may be incomplete; retry the download to resume.".into()));
        }
        if !models(endpoint, &ctx.cancel)
            .await?
            .iter()
            .any(|model| canonical_name(&model.name) == canonical_name(name))
        {
            return Err(Error::Conflict(
                "Ollama reported success, but the requested model is missing from its inventory."
                    .into(),
            ));
        }
        Ok((0, format!("Downloaded model {}.", canonical_name(name))))
    };
    tokio::select! { value = request => value, _ = ctx.cancel.cancelled() => Err(Error::Cancelled) }
}

#[derive(Debug, Deserialize)]
pub struct Model {
    pub name: String,
    pub digest: String,
    pub size: u64,
}

pub async fn models(
    endpoint: &str,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Vec<Model>> {
    #[derive(Deserialize)]
    struct Tags {
        models: Vec<Model>,
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    let request = async {
        let response = client
            .get(format!("{endpoint}/api/tags"))
            .send()
            .await?
            .error_for_status()?;
        response.json::<Tags>().await
    };
    let tags = tokio::select! {
        value = request => value.map_err(|e| Error::Unavailable(format!("Cannot inspect Ollama at {endpoint}: {e}")))?,
        _ = cancel.cancelled() => return Err(Error::Cancelled),
    };
    Ok(tags.models)
}

pub fn command(ctx: &Context, endpoint: &str, verb: &str, name: &str) -> Result<CommandSpec> {
    if endpoint != ENDPOINT {
        return Err(Error::Conflict(
            "Ollama's endpoint changed. Refresh the local inventory.".into(),
        ));
    }
    super::valid_identifier(name)?;
    let mut spec = ctx.command("ollama", &[verb, name])?;
    bind_endpoint(&mut spec, endpoint);
    spec.cwd = Some(ctx.home.clone());
    spec.timeout = Duration::from_secs(if verb == "pull" { 7200 } else { 60 });
    Ok(spec)
}

fn bind_endpoint(command: &mut CommandSpec, endpoint: &str) {
    // Discovery intentionally manages only the local service, regardless of inherited CLI settings.
    command.env.insert("OLLAMA_HOST".into(), endpoint.into());
    command.remove_env.extend(
        [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ]
        .map(str::to_owned),
    );
}

pub async fn verify(ctx: &Context, endpoint: &str, name: &str, digest: &str) -> Result<()> {
    let current = models(endpoint, &ctx.cancel).await?;
    if !current
        .iter()
        .any(|model| model.name == name && model.digest == digest)
    {
        return Err(Error::Conflict(
            "The reviewed Ollama model changed or disappeared. Refresh before removing it.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(replies: Vec<(&'static str, &'static str)>) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let job = std::thread::spawn(move || {
            for (expected, body) in replies {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let size = socket.read(&mut buffer).unwrap();
                    assert!(size > 0);
                    request.extend_from_slice(&buffer[..size]);
                    if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|value| value.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8(request).unwrap();
                assert!(
                    request.starts_with(expected),
                    "unexpected request: {request}"
                );
                if expected.starts_with("POST") || expected.starts_with("DELETE") {
                    let value: serde_json::Value =
                        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                    assert!(value["model"].as_str().is_some());
                }
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
        });
        (endpoint, job)
    }

    #[tokio::test]
    async fn api_pull_and_delete_work_without_a_cli_and_verify_postconditions() {
        let ctx = Context::new(tokio_util::sync::CancellationToken::new()).unwrap();
        let (endpoint, server) = server(vec![
            (
                "POST /api/pull ",
                "{\"status\":\"pulling a\",\"digest\":\"a\",\"completed\":10,\"total\":10}\n{\"status\":\"pulling b\",\"digest\":\"b\",\"completed\":3,\"total\":8}\n{\"status\":\"success\"}\n",
            ),
            (
                "GET /api/tags ",
                r#"{"models":[{"name":"tiny:latest","digest":"sha256:reviewed","size":18}]}"#,
            ),
            (
                "GET /api/tags ",
                r#"{"models":[{"name":"tiny:latest","digest":"sha256:reviewed","size":18}]}"#,
            ),
            ("DELETE /api/delete ", ""),
            ("GET /api/tags ", r#"{"models":[]}"#),
        ]);
        let updates = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink: ProgressSink = {
            let updates = updates.clone();
            std::sync::Arc::new(move |event| updates.lock().unwrap().push(event))
        };
        execute_model_at(&ctx, &endpoint, "tiny", None, &sink, "pull-job")
            .await
            .unwrap();
        execute_model_at(
            &ctx,
            &endpoint,
            "tiny:latest",
            Some("sha256:reviewed"),
            &sink,
            "remove-job",
        )
        .await
        .unwrap();
        server.join().unwrap();
        let updates = updates.lock().unwrap();
        assert_eq!(
            updates
                .iter()
                .map(|event| event.completed)
                .collect::<Vec<_>>(),
            [10, 13, 13]
        );
        assert!(updates.iter().all(|event| event.job_id == "pull-job"
            && event.stage == "download-model"
            && event.total.is_none()));
    }

    #[tokio::test]
    async fn api_rejects_changed_digests_embedded_errors_and_missing_models() {
        let ctx = Context::new(tokio_util::sync::CancellationToken::new()).unwrap();
        let sink = crate::model::silent_progress();
        for (replies, digest) in [
            (
                vec![(
                    "GET /api/tags ",
                    r#"{"models":[{"name":"tiny:latest","digest":"changed","size":18}]}"#,
                )],
                Some("reviewed"),
            ),
            (
                vec![("POST /api/pull ", "{\"error\":\"download denied\"}\n")],
                None,
            ),
            (
                vec![
                    ("POST /api/pull ", "{\"status\":\"success\"}\n"),
                    ("GET /api/tags ", r#"{"models":[]}"#),
                ],
                None,
            ),
            (
                vec![
                    (
                        "GET /api/tags ",
                        r#"{"models":[{"name":"tiny:latest","digest":"reviewed","size":18}]}"#,
                    ),
                    ("DELETE /api/delete ", r#"{"error":"delete denied"}"#),
                ],
                Some("reviewed"),
            ),
        ] {
            let (endpoint, server) = server(replies);
            assert!(
                execute_model_at(&ctx, &endpoint, "tiny:latest", digest, &sink, "job")
                    .await
                    .is_err()
            );
            server.join().unwrap();
        }
    }

    #[tokio::test]
    async fn cancelled_pull_closes_the_stream_without_reporting_success() {
        use std::io::{Read, Write};
        let ctx = Context::new(tokio_util::sync::CancellationToken::new()).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (ready, received) = tokio::sync::oneshot::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\n{{\"status\":\"pulling\"}}\n"
            )
            .unwrap();
            ready.send(()).unwrap();
            // A cancelled request must drop its connection instead of waiting for EOF.
            loop {
                match stream.read(&mut request) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
                    Err(error) => panic!("stream remained open: {error}"),
                }
            }
        });
        let task = tokio::spawn({
            let ctx = ctx.clone();
            async move {
                execute_model_at(
                    &ctx,
                    &endpoint,
                    "tiny",
                    None,
                    &crate::model::silent_progress(),
                    "job",
                )
                .await
            }
        });
        received.await.unwrap();
        ctx.cancel.cancel();
        assert!(matches!(task.await.unwrap(), Err(Error::Cancelled)));
        server.join().unwrap();
    }

    #[test]
    fn model_api_binds_loopback_and_normalizes_only_the_missing_tag() {
        assert!(validate("http://remote.example:11434", "tiny").is_err());
        assert_eq!(canonical_name("tiny"), "tiny:latest");
        assert_eq!(canonical_name("namespace/tiny:7b"), "namespace/tiny:7b");
        assert_eq!(
            canonical_name("host:443/namespace/tiny"),
            "host:443/namespace/tiny:latest"
        );
    }

    #[test]
    fn reviewed_endpoint_overrides_inherited_cli_configuration() {
        let mut command = CommandSpec::new("ollama", ["rm", "example:latest"]);
        command
            .env
            .insert("OLLAMA_HOST".into(), "http://remote.example:11434".into());
        bind_endpoint(&mut command, ENDPOINT);
        assert_eq!(command.env["OLLAMA_HOST"], ENDPOINT);
        assert!(command.remove_env.iter().any(|key| key == "HTTP_PROXY"));
    }

    #[tokio::test]
    async fn removal_requires_the_reviewed_model_digest() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0; 4096];
                assert!(socket.read(&mut request).unwrap() > 0);
                let body =
                    r#"{"models":[{"name":"example:latest","digest":"new-revision","size":10}]}"#;
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
        });
        let ctx = Context::new(tokio_util::sync::CancellationToken::new()).unwrap();
        assert!(
            verify(&ctx, &endpoint, "example:latest", "old-revision")
                .await
                .is_err()
        );
        verify(&ctx, &endpoint, "example:latest", "new-revision")
            .await
            .unwrap();
        server.join().unwrap();
    }
}
