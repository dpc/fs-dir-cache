use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyData {
    pub locked_until: chrono::DateTime<chrono::Utc>,
    pub lock_id: String,
    pub last_lock: chrono::DateTime<chrono::Utc>,
    pub socket_path: Option<PathBuf>,
}

impl KeyData {
    pub fn is_timelocked(&self, now: DateTime<Utc>) -> bool {
        now < self.locked_until
    }

    pub fn is_last_used_before(&self, deadline: DateTime<Utc>) -> bool {
        self.last_lock < deadline
    }

    pub fn expires_in(&self, now: DateTime<Utc>) -> chrono::Duration {
        self.locked_until.signed_duration_since(now)
    }

    pub fn lock(
        &mut self,
        now: DateTime<Utc>,
        lock_id: &str,
        timeout_secs: f64,
        socket_path: Option<PathBuf>,
    ) -> anyhow::Result<&mut Self> {
        self.locked_until = now
            .checked_add_signed(chrono::Duration::milliseconds(
                (timeout_secs * 1000.0).round() as i64,
            ))
            .ok_or_else(|| anyhow::format_err!("Timeout overflow"))?;
        self.last_lock = now;
        self.lock_id = lock_id.to_owned();
        self.socket_path = socket_path;

        Ok(self)
    }

    pub fn unlock(&mut self, now: DateTime<Utc>) -> &mut Self {
        self.locked_until = now;
        debug_assert!(!self.is_timelocked(now));
        self
    }

    pub fn new(now: DateTime<Utc>) -> Self {
        let s = Self {
            locked_until: now,
            lock_id: "".to_owned(),
            last_lock: now,
            socket_path: None,
        };
        debug_assert!(!s.is_timelocked(now));
        s
    }
}
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "snake_case")]
/// Persistent data file at `<root>/fs_dir_cache.json`
pub struct RootData {
    pub keys: BTreeMap<String, KeyData>,
}
