use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyData {
    pub locked_until: chrono::DateTime<chrono::Utc>,
    pub lock_id: String,
    pub last_lock: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "snake_case")]
/// Persistent data file at `<root>/fs_dir_cache.json`
pub struct RootData {
    pub keys: BTreeMap<String, KeyData>,
}
