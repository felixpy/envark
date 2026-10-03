use crate::{
    Error, Result,
    engine::Engine,
    filesystem::{read_small, reject_links},
    model::now,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{io::Write, path::PathBuf};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigContent {
    pub id: String,
    pub content: String,
    pub revision: String,
    pub path: PathBuf,
}

impl Engine {
    pub async fn read_config(&self, id: &str) -> Result<ConfigContent> {
        let state = self.state.read().await;
        let config = state
            .inventory
            .providers
            .iter()
            .flat_map(|p| &p.configs)
            .find(|c| c.id == id)
            .ok_or_else(|| Error::InvalidInput("Unknown configuration file.".into()))?;
        reject_links(&config.path)?;
        let content = read_small(&config.path, 1_048_576)?;
        Ok(ConfigContent {
            id: id.into(),
            revision: format!("{:x}", Sha256::digest(content.as_bytes())),
            content,
            path: config.path.clone(),
        })
    }

    pub async fn save_config(
        &self,
        id: &str,
        content: String,
        revision: &str,
    ) -> Result<ConfigContent> {
        let _guard = self
            .work
            .try_lock()
            .map_err(|_| Error::Conflict("An operation is already running.".into()))?;
        if content.len() > 1_048_576 || content.contains('\0') {
            return Err(Error::InvalidInput(
                "Invalid or oversized configuration.".into(),
            ));
        }
        let current = self.read_config(id).await?;
        if current.revision != revision {
            return Err(Error::Conflict(
                "The configuration changed outside Envark. Reopen it before saving.".into(),
            ));
        }
        let state = self.state.read().await;
        let config = state
            .inventory
            .providers
            .iter()
            .flat_map(|p| &p.configs)
            .find(|c| c.id == id)
            .ok_or_else(|| Error::InvalidInput("Unknown configuration file.".into()))?;
        if !config.editable {
            return Err(Error::Unavailable(
                "This configuration is read-only.".into(),
            ));
        }
        validate(&config.format, &content)?;
        let parent = config
            .path
            .parent()
            .ok_or_else(|| Error::InvalidInput("Missing configuration directory.".into()))?;
        let backup = self.storage.root().join("backups");
        std::fs::create_dir_all(&backup)?;
        let backup_path = backup.join(format!("{}-{}-{}.backup", id, now(), uuid::Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(backup_path)?;
        file.set_permissions(std::fs::metadata(&config.path)?.permissions())?;
        file.write_all(current.content.as_bytes())?;
        file.sync_all()?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.as_file()
            .set_permissions(std::fs::metadata(&config.path)?.permissions())?;
        temp.write_all(content.as_bytes())?;
        temp.as_file().sync_all()?;
        reject_links(&config.path)?;
        if read_small(&config.path, 1_048_576)? != current.content {
            return Err(Error::Conflict(
                "The configuration changed while saving.".into(),
            ));
        }
        temp.persist(&config.path).map_err(|e| Error::Io(e.error))?;
        drop(state);
        self.log(
            "config",
            "Save configuration".into(),
            "success",
            "A backup was saved before replacing the configuration file.".into(),
            0,
        )
        .await?;
        self.read_config(id).await
    }
}

fn validate(format: &str, content: &str) -> Result<()> {
    let error = match format {
        "json" => serde_json::from_str::<serde_json::Value>(content)
            .err()
            .map(|e| e.to_string()),
        "toml" => content.parse::<toml::Table>().err().map(|e| e.to_string()),
        "yaml" => serde_yaml_ng::from_str::<serde_yaml_ng::Value>(content)
            .err()
            .map(|e| e.to_string()),
        "xml" => roxmltree::Document::parse(content)
            .err()
            .map(|e| e.to_string()),
        // INI dialects, Java properties, and version pin files are validated by their owners.
        _ => None,
    };
    match error {
        Some(message) => Err(Error::InvalidInput(format!(
            "Invalid {format} configuration: {message}"
        ))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_structured_configuration_is_rejected() {
        for (format, content) in [
            ("json", "{broken"),
            ("toml", "[broken"),
            ("yaml", "key: [broken"),
            ("xml", "<settings></other>"),
        ] {
            assert!(validate(format, content).is_err(), "{format}");
        }
        for (format, content) in [
            ("json", "{}"),
            ("toml", "[settings]\nvalue = true"),
            ("yaml", "key: value"),
            ("xml", "<settings/>"),
            ("ini", "//registry.npmjs.org/:_authToken=${TOKEN}"),
        ] {
            assert!(validate(format, content).is_ok(), "{format}");
        }
    }
}
