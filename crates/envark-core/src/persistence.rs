use crate::{
    Error, Result,
    model::{Activity, Inventory, Settings},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn load<T: DeserializeOwned + Default>(&self, file: &str) -> Result<T> {
        match fs::read(self.root.join(file)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(e.into()),
        }
    }

    fn save<T: Serialize>(&self, file: &str, data: &T) -> Result<()> {
        let mut tmp = tempfile::NamedTempFile::new_in(&self.root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tmp.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        tmp.write_all(&serde_json::to_vec_pretty(data)?)?;
        tmp.as_file().sync_all()?;
        tmp.persist(self.root.join(file))
            .map_err(|e| Error::Io(e.error))?;
        Ok(())
    }

    pub fn settings(&self) -> Result<Settings> {
        self.load("settings.json")
    }
    pub fn save_settings(&self, data: &Settings) -> Result<()> {
        self.save("settings.json", data)
    }
    pub fn inventory(&self) -> Result<Inventory> {
        self.load("inventory.json")
    }
    pub fn save_inventory(&self, data: &Inventory) -> Result<()> {
        self.save("inventory.json", data)
    }
    pub fn activity(&self) -> Result<Vec<Activity>> {
        self.load("activity.json")
    }
    pub fn save_activity(&self, data: &[Activity]) -> Result<()> {
        self.save("activity.json", &data)
    }
}
