use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Js,
    Py,
    Jvm,
    Rust,
    Go,
    Ollama,
    Puppeteer,
    Playwright,
}

impl ProviderId {
    pub const ALL: [Self; 8] = [
        Self::Js,
        Self::Py,
        Self::Jvm,
        Self::Rust,
        Self::Go,
        Self::Ollama,
        Self::Puppeteer,
        Self::Playwright,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Js => "js",
            Self::Py => "py",
            Self::Jvm => "jvm",
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Ollama => "ollama",
            Self::Puppeteer => "puppeteer",
            Self::Playwright => "playwright",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub language: String,
    pub theme: String,
    pub scan_on_launch: bool,
    pub check_updates: bool,
    pub roots: Vec<PathBuf>,
    pub excludes: Vec<String>,
    pub idle_days: u16,
    pub use_trash: bool,
    pub preferred: BTreeMap<ProviderId, String>,
    pub protected_projects: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "zh-CN".into(),
            theme: "system".into(),
            scan_on_launch: true,
            check_updates: false,
            roots: vec![],
            excludes: vec![".git".into(), ".svn".into(), ".hg".into()],
            idle_days: 90,
            use_trash: true,
            protected_projects: vec![],
            preferred: BTreeMap::from([
                (ProviderId::Js, "fnm".into()),
                (ProviderId::Py, "uv".into()),
            ]),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub bytes: u64,
    pub files: u64,
    pub skipped: u64,
    pub complete: bool,
    #[serde(default)]
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub selector: Option<String>,
    pub manager: String,
    pub path: PathBuf,
    pub active: bool,
    #[serde(default)]
    pub active_known: bool,
    pub managed: bool,
    pub size: Option<Measurement>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manager {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub supports_install: bool,
    pub supports_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub version: String,
    pub latest: Option<String>,
    #[serde(default)]
    pub update_status: UpdateStatus,
    pub source: String,
    pub runtime: Option<String>,
    pub path: Option<PathBuf>,
    pub size: Option<Measurement>,
    pub can_update: bool,
    pub can_remove: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateStatus {
    #[default]
    Unknown,
    Latest,
    Ahead,
    Major,
    Minor,
}

impl UpdateStatus {
    pub fn is_upgrade(self) -> bool {
        matches!(self, Self::Major | Self::Minor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub size: Measurement,
    pub last_used: Option<u64>,
    pub modified: Option<u64>,
    pub used_by: Vec<PathBuf>,
    pub can_remove: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub id: String,
    pub path: PathBuf,
    pub format: String,
    pub editable: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub running: bool,
    pub owned: bool,
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: ProviderId,
    pub detected: bool,
    pub managers: Vec<Manager>,
    pub runtimes: Vec<Runtime>,
    pub package_managers: Vec<Tool>,
    pub tools: Vec<Tool>,
    pub assets: Vec<Asset>,
    pub configs: Vec<ConfigFile>,
    pub service: Option<ServiceStatus>,
    pub issues: Vec<String>,
}

impl Provider {
    pub fn empty(id: ProviderId) -> Self {
        Self {
            id,
            detected: false,
            managers: vec![],
            runtimes: vec![],
            package_managers: vec![],
            tools: vec![],
            assets: vec![],
            configs: vec![],
            service: None,
            issues: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub kind: String,
    pub size: Measurement,
    pub restore: String,
    #[serde(default)]
    pub can_clean: bool,
    #[serde(default)]
    pub cleanup_issue: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub providers: Vec<ProviderId>,
    pub last_active: Option<u64>,
    pub activity_complete: bool,
    pub branch: Option<String>,
    pub pins: BTreeMap<String, String>,
    pub protected: bool,
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub repository: Option<Repository>,
    #[serde(default)]
    pub is_worktree: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub id: String,
    pub repository: Repository,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub locked: bool,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cache {
    pub id: String,
    pub provider: ProviderId,
    pub name: String,
    pub path: PathBuf,
    pub size: Measurement,
    pub strategy: String,
    pub warning: String,
    pub can_clean: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub name: String,
    pub mount: PathBuf,
    pub total: u64,
    pub available: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub providers: Vec<Provider>,
    pub projects: Vec<Project>,
    #[serde(default)]
    pub worktrees: Vec<Worktree>,
    pub caches: Vec<Cache>,
    pub disks: Vec<Disk>,
    pub scanned_at: Option<u64>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub id: String,
    pub time: u64,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub detail: String,
    #[serde(default, alias = "freedBytes")]
    pub removed_bytes: u64,
    #[serde(default)]
    pub reclaimed_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub job_id: String,
    pub stage: String,
    pub completed: u64,
    pub total: Option<u64>,
    pub message: String,
}

pub type ProgressSink = std::sync::Arc<dyn Fn(Progress) + Send + Sync>;

pub fn silent_progress() -> ProgressSink {
    std::sync::Arc::new(|_| {})
}
