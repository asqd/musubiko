//! Musubiko desktop shell: exposes the core to the web UI as Tauri commands.
//! Git work runs on the blocking pool so the window never stalls.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod recent;

use std::path::PathBuf;
use std::sync::Mutex;

use musubiko_core::{Branch, Git, Repo, RepoInfo};
use tauri::{AppHandle, State};

#[derive(Default)]
struct AppState {
    git: Mutex<Option<Git>>,
    repo: Mutex<Option<Repo>>,
}

impl AppState {
    fn git(&self) -> Result<Git, String> {
        let mut git = self.git.lock().unwrap();
        if git.is_none() {
            *git = Some(Git::discover().map_err(|e| e.to_string())?);
        }
        Ok(git.clone().unwrap())
    }

    fn repo(&self) -> Result<Repo, String> {
        self.repo.lock().unwrap().clone().ok_or_else(|| "Репозиторий не открыт".to_string())
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> musubiko_core::Result<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_repo(path: String, app: AppHandle, state: State<'_, AppState>) -> Result<RepoInfo, String> {
    let git = state.git()?;
    let (repo, info) = blocking(move || {
        let repo = Repo::open(git, &PathBuf::from(path))?;
        let info = repo.info()?;
        Ok((repo, info))
    })
    .await?;
    *state.repo.lock().unwrap() = Some(repo);
    recent::remember(&app, &info.root);
    Ok(info)
}

#[tauri::command]
async fn repo_info(state: State<'_, AppState>) -> Result<RepoInfo, String> {
    let repo = state.repo()?;
    blocking(move || repo.info()).await
}

#[tauri::command]
async fn list_branches(base: String, state: State<'_, AppState>) -> Result<Vec<Branch>, String> {
    let repo = state.repo()?;
    blocking(move || repo.branches(&base)).await
}

#[tauri::command]
fn recent_repos(app: AppHandle) -> Vec<String> {
    recent::list(&app)
}

#[tauri::command]
fn forget_repo(path: String, app: AppHandle) -> Vec<String> {
    recent::forget(&app, &path)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![open_repo, repo_info, list_branches, recent_repos, forget_repo])
        .run(tauri::generate_context!())
        .expect("error while running Musubiko");
}
