//! Recently opened repositories, newest first, in the app's data folder.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

const LIMIT: usize = 12;

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("recent.json"))
}

pub fn list(app: &AppHandle) -> Vec<String> {
    file(app)
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| Path::new(p).exists())
        .collect()
}

fn save(app: &AppHandle, items: &[String]) {
    let Some(f) = file(app) else { return };
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(items) {
        let _ = std::fs::write(f, json);
    }
}

pub fn remember(app: &AppHandle, root: &Path) {
    let root = root.to_string_lossy().into_owned();
    let mut items = list(app);
    items.retain(|p| *p != root);
    items.insert(0, root);
    items.truncate(LIMIT);
    save(app, &items);
}

pub fn forget(app: &AppHandle, path: &str) -> Vec<String> {
    let mut items = list(app);
    items.retain(|p| p != path);
    save(app, &items);
    items
}
