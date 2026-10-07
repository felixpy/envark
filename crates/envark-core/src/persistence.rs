use crate::{
    Error, Result,
    model::{Activity, Inventory, Settings},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const SCHEMA_VERSION: u64 = 1;

#[derive(Default)]
struct Recovery {
    notices: Vec<String>,
    blocked: HashSet<String>,
}

struct Loaded<T> {
    value: T,
    recovered: bool,
}

#[derive(Clone)]
pub struct Storage {
    root: PathBuf,
    recovery: Arc<Mutex<Recovery>>,
}

impl Storage {
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            recovery: Arc::default(),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn recovery_notices(&self) -> Vec<String> {
        self.recovery
            .lock()
            .map(|state| state.notices.clone())
            .unwrap_or_default()
    }

    fn recover<T: Default>(
        &self,
        file: &str,
        reason: String,
        quarantine: bool,
    ) -> Result<Loaded<T>> {
        let mut state = self
            .recovery
            .lock()
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        let backup = self
            .root
            .join(format!("{file}.recovery-{}", uuid::Uuid::new_v4()));
        let preservation = if quarantine {
            match fs::rename(self.root.join(file), &backup) {
                Ok(()) => format!("The original was preserved at {}.", backup.display()),
                Err(error) => {
                    state.blocked.insert(file.into());
                    format!(
                        "The original could not be moved ({error}); writes to {file} are disabled for this session."
                    )
                }
            }
        } else {
            state.blocked.insert(file.into());
            format!(
                "The original was left untouched; writes to {file} are disabled for this session."
            )
        };
        state.notices.push(format!(
            "Could not load {file}: {reason}. {preservation} Temporary defaults are in use."
        ));
        Ok(Loaded {
            value: T::default(),
            recovered: true,
        })
    }

    fn load<T: DeserializeOwned + Default>(&self, file: &str) -> Result<Loaded<T>> {
        match fs::read(self.root.join(file)) {
            Ok(bytes) => {
                let parse = || -> std::result::Result<T, String> {
                    let mut json: serde_json::Value =
                        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                    if let Some(version) = json.get("schemaVersion") {
                        if version.as_u64() != Some(SCHEMA_VERSION) {
                            return Err(format!("unsupported schema version {version}"));
                        }
                        json = json.get_mut("data").ok_or("missing schema data")?.take();
                    }
                    // Accept legacy unwrapped data, then migrate on the next successful save.
                    serde_json::from_value(json).map_err(|error| error.to_string())
                };
                match parse() {
                    Ok(value) => Ok(Loaded {
                        value,
                        recovered: false,
                    }),
                    Err(reason) => self.recover(file, reason, true),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Loaded {
                value: T::default(),
                recovered: false,
            }),
            Err(e) => self.recover(file, e.to_string(), false),
        }
    }

    fn save<T: Serialize>(&self, file: &str, data: &T) -> Result<()> {
        if self
            .recovery
            .lock()
            .map_err(|e| Error::Unavailable(e.to_string()))?
            .blocked
            .contains(file)
        {
            return Err(Error::Unavailable(format!(
                "Saving {file} is disabled to preserve unreadable data. Fix its access permissions and restart Envark."
            )));
        }
        let mut tmp = tempfile::NamedTempFile::new_in(&self.root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tmp.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        tmp.write_all(&serde_json::to_vec_pretty(
            &serde_json::json!({ "schemaVersion": SCHEMA_VERSION, "data": data }),
        )?)?;
        tmp.as_file().sync_all()?;
        tmp.persist(self.root.join(file))
            .map_err(|e| Error::Io(e.error))?;
        Ok(())
    }

    pub fn settings(&self) -> Result<Settings> {
        let mut loaded: Loaded<Settings> = self.load("settings.json")?;
        if loaded.recovered {
            loaded.value.scan_on_launch = false;
        }
        Ok(loaded.value)
    }
    pub fn save_settings(&self, data: &Settings) -> Result<()> {
        self.save("settings.json", data)
    }
    pub fn inventory(&self) -> Result<Inventory> {
        Ok(self.load("inventory.json")?.value)
    }
    pub fn save_inventory(&self, data: &Inventory) -> Result<()> {
        self.save("inventory.json", data)
    }
    pub fn activity(&self) -> Result<Vec<Activity>> {
        Ok(self.load("activity.json")?.value)
    }
    pub fn save_activity(&self, data: &[Activity]) -> Result<()> {
        self.save("activity.json", &data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_preferences_override_new_startup_defaults() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::new(root.path().into()).unwrap();
        for language in ["en", "zh-TW"] {
            let settings = Settings {
                language: language.into(),
                roots: Vec::new(),
                check_updates: false,
                scan_on_launch: false,
                ..Settings::default()
            };
            storage.save_settings(&settings).unwrap();
            let reopened = Storage::new(root.path().into())
                .unwrap()
                .settings()
                .unwrap();
            assert_eq!(reopened.language, language);
            assert!(reopened.roots.is_empty());
            assert!(!reopened.check_updates);
            assert!(!reopened.scan_on_launch);
        }
    }

    #[test]
    fn damaged_and_future_data_are_preserved_before_recovery() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::new(root.path().into()).unwrap();
        for (file, bytes) in [
            ("inventory.json", "{broken"),
            ("activity.json", r#"{"schemaVersion":99,"data":[]}"#),
            ("settings.json", "invalid preferences"),
        ] {
            fs::write(root.path().join(file), bytes).unwrap();
        }
        assert!(storage.inventory().unwrap().projects.is_empty());
        assert!(storage.activity().unwrap().is_empty());
        assert!(!storage.settings().unwrap().scan_on_launch);
        assert_eq!(storage.recovery_notices().len(), 3);
        let backups = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect::<Vec<_>>();
        assert!(backups.contains(&"{broken".into()));
        assert!(backups.contains(&r#"{"schemaVersion":99,"data":[]}"#.into()));
        assert!(backups.contains(&"invalid preferences".into()));
        storage.save_inventory(&Inventory::default()).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(root.path().join("inventory.json")).unwrap()).unwrap();
        assert_eq!(saved["schemaVersion"], SCHEMA_VERSION);
        assert!(storage.inventory().unwrap().projects.is_empty());
    }

    #[test]
    fn unreadable_data_is_not_overwritten_by_later_session_saves() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("activity.json")).unwrap();
        let storage = Storage::new(root.path().into()).unwrap();
        assert!(storage.activity().unwrap().is_empty());
        assert!(storage.save_activity(&[]).is_err());
        assert!(root.path().join("activity.json").is_dir());
        assert!(storage.recovery_notices()[0].contains("left untouched"));
    }

    #[test]
    fn legacy_settings_and_activity_migrate_without_losing_user_data() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::new(root.path().into()).unwrap();
        fs::write(
            root.path().join("settings.json"),
            r#"{"language":"en","idleDays":45}"#,
        )
        .unwrap();
        fs::write(root.path().join("activity.json"), r#"[{"id":"1","time":1,"kind":"clean","title":"Preserved","status":"success","detail":"legacy","freedBytes":123}]"#).unwrap();
        let settings = storage.settings().unwrap();
        assert_eq!(settings.idle_days, 45);
        let activity = storage.activity().unwrap();
        assert_eq!(activity[0].removed_bytes, 123);
        assert_eq!(activity[0].reclaimed_bytes, None);
        storage.save_settings(&settings).unwrap();
        storage.save_activity(&activity).unwrap();
        assert_eq!(storage.settings().unwrap().language, "en");
        assert_eq!(storage.activity().unwrap()[0].title, "Preserved");
        assert!(storage.recovery_notices().is_empty());
    }
}
