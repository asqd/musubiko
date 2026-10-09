//! Every operation Musubiko ran, with its commands and what is left to undo.
//! Stored as one JSON array in the app's data folder; updated after each step.

use std::path::{Path, PathBuf};

use musubiko_core::Operation;
use tauri::{AppHandle, Manager};

const LIMIT: usize = 300;

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("journal.json"))
}

fn load(app: &AppHandle) -> Vec<Operation> {
    file(app)
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, ops: &[Operation]) {
    let Some(f) = file(app) else { return };
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(ops) {
        let tmp = f.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, f);
        }
    }
}

/// Adds the operation or replaces the stored copy with the same id.
pub fn upsert(app: &AppHandle, op: &Operation) {
    let mut ops = load(app);
    match ops.iter_mut().find(|o| o.id == op.id) {
        Some(o) => *o = op.clone(),
        None => ops.insert(0, op.clone()),
    }
    ops.truncate(LIMIT);
    save(app, &ops);
}

/// Operations in one repository, newest first.
pub fn list(app: &AppHandle, repo: &Path) -> Vec<Operation> {
    load(app).into_iter().filter(|o| o.repo == repo).collect()
}

pub fn get(app: &AppHandle, id: &str) -> Option<Operation> {
    load(app).into_iter().find(|o| o.id == id)
}
