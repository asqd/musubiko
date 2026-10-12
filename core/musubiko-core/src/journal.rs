//! Every operation Musubiko ran, with its commands and what is left to undo: one JSON array in a
//! file the app picks, newest first, rewritten after each step.
//!
//! Nothing in it is dropped by accident. An entry this version cannot read (written by a newer
//! one, say) is not listed but stays in the file. A file that is not a JSON array at all is set
//! aside as `journal.broken-<time>.json` before the next write, never overwritten.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::Operation;

/// The journal keeps this many operations.
const LIMIT: usize = 300;

pub struct Journal {
    path: PathBuf,
}

/// The journal file could not be read as a list of operations.
struct Broken;

impl Journal {
    pub fn new(path: impl Into<PathBuf>) -> Journal {
        Journal { path: path.into() }
    }

    /// Operations in one repository, newest first.
    pub fn list(&self, repo: &Path) -> Vec<Operation> {
        self.ops().into_iter().filter(|o| o.repo == repo).collect()
    }

    pub fn get(&self, id: &str) -> Option<Operation> {
        self.ops().into_iter().find(|o| o.id == id)
    }

    /// Adds the operation or replaces the stored copy with the same id. Returns where an
    /// unreadable journal was set aside, if it had to be.
    pub fn upsert(&self, op: &Operation) -> std::io::Result<Option<PathBuf>> {
        let (mut entries, aside) = match self.entries() {
            Ok(e) => (e, None),
            Err(Broken) => (Vec::new(), Some(self.set_aside()?)),
        };
        let value = serde_json::to_value(op).map_err(std::io::Error::other)?;
        match entries
            .iter_mut()
            .find(|e| e.get("id").and_then(Value::as_str) == Some(op.id.as_str()))
        {
            Some(e) => *e = value,
            None => entries.insert(0, value),
        }
        entries.truncate(LIMIT);
        self.save(&entries)?;
        Ok(aside)
    }

    fn ops(&self) -> Vec<Operation> {
        self.entries()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| serde_json::from_value(e).ok())
            .collect()
    }

    /// The raw entries; a missing file is an empty journal.
    fn entries(&self) -> Result<Vec<Value>, Broken> {
        match std::fs::read(&self.path) {
            Ok(b) => serde_json::from_slice(&b).map_err(|_| Broken),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(_) => Err(Broken),
        }
    }

    fn set_aside(&self) -> std::io::Result<PathBuf> {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let aside = self.path.with_file_name(format!("journal.broken-{t}.json"));
        std::fs::rename(&self.path, &aside)?;
        Ok(aside)
    }

    fn save(&self, entries: &[Value]) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(entries).map_err(std::io::Error::other)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, &self.path)
    }
}
