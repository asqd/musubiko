//! Every operation Musubiko ran, with its commands and what is left to undo.
//! Stored by `musubiko_core::Journal` in the app's data folder; updated after each step.

use std::path::Path;

use musubiko_core::{Git, Journal, Operation, Repo};
use tauri::{AppHandle, Manager};

fn journal(app: &AppHandle) -> Option<Journal> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| Journal::new(d.join("journal.json")))
}

/// Adds the operation or replaces the stored copy with the same id. Operations pushed out of
/// the journal release the refs that kept their deleted branches for undo.
pub fn upsert(app: &AppHandle, op: &Operation) {
    let Some(j) = journal(app) else { return };
    let saved = match j.upsert(op) {
        Ok(s) => s,
        Err(e) => return crate::log::write("ERROR", &format!("Журнал не записался: {e}")),
    };
    if let Some(aside) = saved.set_aside {
        crate::log::write(
            "WARN",
            &format!(
                "Журнал не читался, отложил его в {} и начал новый",
                aside.display()
            ),
        );
    }
    release(&saved.dropped);
}

/// Lets go of what dropped operations kept for their undo, in their own repositories; a
/// repository that is gone or does not open is skipped.
fn release(dropped: &[Operation]) {
    let with_refs: Vec<&Operation> = dropped
        .iter()
        .filter(|o| !o.kept_refs().is_empty())
        .collect();
    if with_refs.is_empty() {
        return;
    }
    let Ok(git) = Git::discover() else { return };
    for op in with_refs {
        let done = Repo::open(git.clone(), &op.repo).and_then(|r| r.release_kept(&op.kept_refs()));
        if let Err(e) = done {
            crate::log::write(
                "WARN",
                &format!("{}: не убрал сохранённые коммиты: {e}", op.repo.display()),
            );
        }
    }
}

/// Operations in one repository, newest first.
pub fn list(app: &AppHandle, repo: &Path) -> Vec<Operation> {
    journal(app).map(|j| j.list(repo)).unwrap_or_default()
}

pub fn get(app: &AppHandle, id: &str) -> Option<Operation> {
    journal(app)?.get(id)
}
