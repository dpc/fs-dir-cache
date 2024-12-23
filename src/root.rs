mod dto;

use std::collections::btree_map::Entry;
use std::io::{self, Read as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, thread};

use anyhow::{bail, Result};
use chrono::Utc;
use convi::ExpectFrom;
use fs2::FileExt;
use tracing::{debug, info, warn};

use crate::{util, LOG_TARGET};

/// Root directory of a cache
pub struct Root {
    path: PathBuf,
    lock_file: fs::File,
}

impl Root {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        ensure_root_exists(&path)?;

        let lock = util::open_lock_file(&path)?;

        Ok(Self {
            path,
            lock_file: lock,
        })
    }

    pub fn with_lock<T>(&mut self, f: impl FnOnce(&mut LockedRoot) -> Result<T>) -> Result<T> {
        f(&mut LockedRoot::new(&self.path, &mut self.lock_file)?)
    }
}

fn ensure_root_exists(dir: &PathBuf) -> Result<()> {
    if !dir.try_exists()? {
        info!(
            target: LOG_TARGET,
            dir = %dir.display(), "Creating root dir"
        );
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// A handle passed to `with_lock` argument after root was acquired
pub struct LockedRoot<'a> {
    path: &'a PathBuf,
    lock_file: &'a mut fs::File,
    locked: bool,
}

impl<'a> Drop for LockedRoot<'a> {
    fn drop(&mut self) {
        if self.locked {
            let Ok(()) = self.lock_file.unlock() else {
                warn!("Failed to release the cache lock file");
                return;
            };
            self.locked = false;
        }
    }
}

impl<'a> LockedRoot<'a> {
    fn new(path: &'a PathBuf, lock_file: &'a mut fs::File) -> Result<Self> {
        let mut locked_root = Self {
            path,
            lock_file,
            locked: false,
        };
        locked_root.lock()?;
        Ok(locked_root)
    }

    fn lock(&mut self) -> Result<()> {
        debug!(
            target: LOG_TARGET,
            path = %self.path.display(), "Acquiring cache lock..."
        );
        if self.lock_file.try_lock_exclusive().is_err() {
            info!(
                target: LOG_TARGET,
                "Cache lock taken, waiting..."
            );
            self.lock_file.lock_exclusive()?;
        };
        debug!(
            target: LOG_TARGET,
            "Acquired cache lock"
        );
        self.locked = true;
        Ok(())
    }

    fn unlock(&mut self) -> Result<()> {
        self.ensure_locked()?;
        debug!(
            target: LOG_TARGET,
            path = %self.path.display(), "Releasing cache lock..."
        );
        self.lock_file.unlock()?;
        self.locked = false;
        Ok(())
    }

    fn data_file_path(&self) -> PathBuf {
        self.path.join("fs-dir-cache.json")
    }

    fn ensure_locked(&self) -> anyhow::Result<()> {
        if !self.locked {
            bail!("LockedRoot no longer valid");
        }
        Ok(())
    }

    pub fn r#yield(&mut self, duration: Duration) -> Result<()> {
        self.yield_with(|| {
            thread::sleep(duration);
        })
    }

    pub fn r#yield_with<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce() -> R,
    {
        self.unlock()?;
        let r = f();
        self.lock()?;
        Ok(r)
    }

    pub fn load_data(&self) -> Result<dto::RootData> {
        self.ensure_locked()?;
        let path = self.data_file_path();
        if !path.try_exists()? {
            return Ok(Default::default());
        }
        Ok(serde_json::from_reader::<_, _>(std::fs::File::open(path)?)?)
    }

    pub fn store_data(&mut self, data: &dto::RootData) -> Result<()> {
        util::store_json_pretty_to_file(&self.data_file_path(), data)
    }

    pub fn lock_key(
        &mut self,
        key: &str,
        lock_id: &str,
        timeout_secs: f64,
        new_socket_path: Option<PathBuf>,
    ) -> Result<PathBuf> {
        let locking_start = Utc::now();
        let mut had_to_wait = false;
        let data = loop {
            let mut data = self.load_data()?;

            let now = Utc::now();
            match data.keys.entry(key.to_owned()) {
                Entry::Vacant(e) => {
                    e.insert(
                        dto::KeyData::new(now)
                            .lock(now, lock_id, timeout_secs, new_socket_path.clone())?
                            .to_owned(),
                    );
                    break data;
                }
                Entry::Occupied(mut e) => {
                    if let Some(prev_sock_path) = e.get().socket_path.as_ref() {
                        if let Ok(mut s) = UnixStream::connect(prev_sock_path) {
                            info!(
                                target: LOG_TARGET,
                                key,
                                lock_id,
                                sock_path = %prev_sock_path.display(),
                                "Previous lock holder still alive"
                            );
                            had_to_wait |= true;
                            self.r#yield_with(|| {
                                // we are just waiting to get disconnected here
                                let _ = s.read(&mut [0]);
                            })?;
                        } else {
                            debug!(
                                target: LOG_TARGET,
                                key,
                                lock_id,
                                sock_path = %prev_sock_path.display(),
                                "Previous lock holder gone"
                            );
                            rm_prev_sock_path(prev_sock_path);
                            e.get_mut().lock(
                                now,
                                lock_id,
                                timeout_secs,
                                new_socket_path.clone(),
                            )?;
                            break data;
                        }
                    } else if !e.get().is_timelocked(now) {
                        debug!(
                            target: LOG_TARGET,
                            key, lock_id, "Previous lock expired"
                        );
                        if let Some(prev_sock_path) = e.get().socket_path.as_ref() {
                            rm_prev_sock_path(prev_sock_path);
                        }
                        e.get_mut()
                            .lock(now, lock_id, timeout_secs, new_socket_path.clone())?;
                        break data;
                    } else {
                        let expires_in_msecs = e.get().expires_in(now).num_milliseconds();
                        let duration = Duration::from_millis(u64::expect_from(
                            (expires_in_msecs / 100).clamp(10, 10000),
                        ));
                        info!(
                            target: LOG_TARGET,
                            key,
                            lock_id,
                            expires_in_msecs,
                            "Waiting for the key lock to be released..."
                        );
                        had_to_wait |= true;
                        self.r#yield(duration)?;
                    }
                }
            }
        };

        self.store_data(&data)?;

        if had_to_wait {
            info!(
                target: LOG_TARGET,
                key,
                lock_id,
                wait_secs=%Utc::now().signed_duration_since(locking_start).num_seconds(),
                "Acquired lock"
            );
        }
        Ok(self.path.join(key))
    }

    pub fn unlock_key(&mut self, key: &str, lock_id: String) -> Result<()> {
        let mut data = self.load_data()?;

        if let Some(key_data) = data.keys.get_mut(key) {
            if key_data.lock_id != lock_id {
                bail!(
                    "Key {} lock id does not match; used = {}, owner = {}",
                    key,
                    lock_id,
                    key_data.lock_id
                );
            }
            let now = Utc::now();
            if !key_data.is_timelocked(now) {
                warn!(key, "Lock already expired");
            }
            key_data.unlock(now);
            self.store_data(&data)?;
        } else {
            bail!("Key {} does not exist", key);
        }

        Ok(())
    }

    pub fn key_dir_path(&self, key: &str) -> PathBuf {
        self.path.join(key)
    }
}

fn rm_prev_sock_path(prev_sock_path: &Path) {
    if let Err(err) = fs::remove_file(prev_sock_path) {
        if err.kind() != io::ErrorKind::NotFound {
            warn!(target: LOG_TARGET,
                %err,
                sock_path = %prev_sock_path.display(),
                "Could not remove stale unix socket");
        }
    }
}
