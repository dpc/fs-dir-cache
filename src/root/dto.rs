use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyData {
    pub locked_until: chrono::DateTime<chrono::Utc>,
    pub lock_id: String,
    pub last_lock: chrono::DateTime<chrono::Utc>,
}

impl KeyData {
    pub fn is_locked(&self, now: DateTime<Utc>) -> bool {
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
        timeout_secs: u64,
    ) -> anyhow::Result<&mut Self> {
        debug_assert!(!self.is_locked(now));
        self.locked_until = now
            .checked_add_signed(chrono::Duration::seconds(i64::try_from(timeout_secs)?))
            .ok_or_else(|| anyhow::format_err!("Timeout overflow"))?;
        self.last_lock = now;
        self.lock_id = lock_id.to_owned();

        debug_assert!(self.is_locked(now));
        Ok(self)
    }
    pub fn unlock(&mut self, now: DateTime<Utc>) -> &mut Self {
        self.locked_until = now;
        debug_assert!(!self.is_locked(now));
        self
    }

    pub fn new(now: DateTime<Utc>) -> Self {
        let s = Self {
            locked_until: now,
            lock_id: "".to_owned(),
            last_lock: now,
        };
        debug_assert!(!s.is_locked(now));
        s
    }
}
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "snake_case")]
/// Persistent data file at `<root>/fs_dir_cache.json`
pub struct RootData {
    pub keys: BTreeMap<String, KeyData>,
}
