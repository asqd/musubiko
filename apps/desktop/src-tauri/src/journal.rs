//! Every operation Musubiko ran, with its commands and what is left to undo.
//! Stored by `musubiko_core::Journal` in the app's data folder; updated after each step.

use std::path::Path;

use musubiko_core::{Journal, Operation};
use tauri::{AppHandle, Manager};

fn journal(app: &AppHandle) -> Option<Journal> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| Journal::new(d.join("journal.json")))
}

/// Adds the operation or replaces the stored copy with the same id.
pub fn upsert(app: &AppHandle, op: &Operation) {
    let Some(j) = journal(app) else { return };
    match j.upsert(op) {
        Ok(Some(aside)) => crate::log::write(
            "WARN",
            &format!(
                "Журнал не читался, отложил его в {} и начал новый",
                aside.display()
            ),
        ),
        Ok(None) => {}
        Err(e) => crate::log::write("ERROR", &format!("Журнал не записался: {e}")),
    }
}

/// Operations in one repository, newest first.
pub fn list(app: &AppHandle, repo: &Path) -> Vec<Operation> {
    journal(app).map(|j| j.list(repo)).unwrap_or_default()
}

pub fn get(app: &AppHandle, id: &str) -> Option<Operation> {
    journal(app)?.get(id)
}
