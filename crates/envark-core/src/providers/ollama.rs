use super::Context;
use crate::{Error, Result, process::CommandSpec};
use serde::Deserialize;
use std::time::Duration;

pub const ENDPOINT: &str = "http://127.0.0.1:11434";

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
