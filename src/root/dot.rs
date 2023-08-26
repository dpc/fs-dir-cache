use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct KeyData {
    pub(crate) locked_until: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) last_lock: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
/// Persistent data file at `<root>/fs_dir_cache.json`
pub(crate) struct RootData {
    pub(crate) keys: KeyData,
}
