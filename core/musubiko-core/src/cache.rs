//! Branch statuses depend only on two commits: the branch tip and the base tip. They are kept in
//! `.git/musubiko/status-cache.json` under that pair, so a refresh recomputes only branches where
//! one of the two moved. Shared by the app and the CLI, and by all worktrees of a repository.
//!
//! A missing base is not cached: that answer is about refs, not about the two tips.
//!
//! The cache is best effort: a missing or broken file means "compute everything", and a failed
//! write is ignored.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::branches::{Status, Sync};
use crate::{Person, Repo};

/// Bump when the way statuses are computed changes; older files are then ignored.
const VERSION: u32 = 1;
/// Entries not used for this long are dropped on save.
const KEEP_DAYS: i64 = 30;
/// Don't rewrite the file only to refresh "last used" more often than this.
const TOUCH_AFTER: i64 = 86_400;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedStatus {
    pub status: Status,
    pub ahead: u32,
    pub behind: u32,
    /// Author of the branch's first own commit, when it has any.
    pub owner: Option<Person>,
    pub used: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedSync {
    pub sync: Option<Sync>,
    pub used: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct StatusCache {
    version: u32,
    /// `"<branch tip>:<base tip>"` → status.
    statuses: HashMap<String, CachedStatus>,
    /// `"<local tip>:<remote tip>"` → local against remote.
    syncs: HashMap<String, CachedSync>,
    #[serde(skip)]
    dirty: bool,
}

/// How a listing used the cache.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    /// Branches whose status came from the cache.
    pub hits: u32,
    /// Branches computed now.
    pub computed: u32,
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn status_key(branch_tip: &str, base_tip: &str) -> String {
    format!("{branch_tip}:{base_tip}")
}

impl StatusCache {
    pub fn load(path: &Path) -> StatusCache {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<StatusCache>(&b).ok())
            .filter(|c| c.version == VERSION)
            .unwrap_or_else(|| StatusCache {
                version: VERSION,
                ..Default::default()
            })
    }

    pub fn status(&self, key: &str) -> Option<&CachedStatus> {
        self.statuses.get(key)
    }

    pub fn sync(&self, key: &str) -> Option<&CachedSync> {
        self.syncs.get(key)
    }

    pub fn put_status(&mut self, key: String, entry: CachedStatus) {
        self.statuses.insert(key, entry);
        self.dirty = true;
    }

    pub fn put_sync(&mut self, key: String, entry: CachedSync) {
        self.syncs.insert(key, entry);
        self.dirty = true;
    }

    /// Marks entries as used now, if they were last marked long enough ago to be worth a write.
    pub fn touch(&mut self, status_keys: &[String], sync_keys: &[String]) {
        let t = now();
        for k in status_keys {
            if let Some(e) = self.statuses.get_mut(k) {
                if t - e.used > TOUCH_AFTER {
                    e.used = t;
                    self.dirty = true;
                }
            }
        }
        for k in sync_keys {
            if let Some(e) = self.syncs.get_mut(k) {
                if t - e.used > TOUCH_AFTER {
                    e.used = t;
                    self.dirty = true;
                }
            }
        }
    }

    /// Writes the file if anything changed, dropping entries unused for [`KEEP_DAYS`].
    pub fn save(mut self, path: &Path) {
        if !self.dirty {
            return;
        }
        let cutoff = now() - KEEP_DAYS * 86_400;
        self.statuses.retain(|_, e| e.used >= cutoff);
        self.syncs.retain(|_, e| e.used >= cutoff);
        let Some(dir) = path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        if let Ok(json) = serde_json::to_vec(&self) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(tmp, path);
            }
        }
    }
}

impl Repo {
    /// `.git/musubiko/status-cache.json` of the main repository (shared by worktrees).
    pub(crate) fn cache_path(&self) -> Option<PathBuf> {
        let common = self.run(&["rev-parse", "--git-common-dir"]).ok()?;
        Some(
            self.root
                .join(common.trim())
                .join("musubiko")
                .join("status-cache.json"),
        )
    }

    /// Forgets every cached status; the next listing computes everything.
    pub fn clear_status_cache(&self) {
        if let Some(p) = self.cache_path() {
            let _ = std::fs::remove_file(p);
        }
    }
}
