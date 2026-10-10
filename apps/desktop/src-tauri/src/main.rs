//! Musubiko desktop shell: exposes the core to the web UI as Tauri commands.
//! Git work runs on the blocking pool so the window never stalls.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod journal;
mod log;
mod recent;
mod settings;

use std::path::PathBuf;
use std::sync::Mutex;

use musubiko_core::{
    Branch, BranchCard, BranchDiff, CheckoutInput, CheckoutPlan, CommitSearch, ConflictFile,
    DeleteInput, FileDiff, Git, HotfixInput, HotfixPlan, NewBranchInput, NewBranchPlan, OpPlan,
    OpStatus, Operation, PickState, PortInput, Radar, Repo, RepoInfo, Side,
};
use tauri::{AppHandle, State};

#[derive(Default)]
struct AppState {
    git: Mutex<Option<Git>>,
    repo: Mutex<Option<Repo>>,
    /// The operation in progress, run one step per command so the UI can show each.
    op: Mutex<Option<Operation>>,
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
        self.repo
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "Репозиторий не открыт".to_string())
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
async fn open_repo(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RepoInfo, String> {
    let git = state.git()?;
    let (repo, info) = blocking(move || {
        let repo = Repo::open(git, &PathBuf::from(path))?;
        let info = repo.info()?;
        Ok((repo, info))
    })
    .await?;
    *state.repo.lock().unwrap() = Some(repo);
    log::write("REPO", &info.root.to_string_lossy());
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
async fn branch_card(
    name: String,
    base: String,
    state: State<'_, AppState>,
) -> Result<BranchCard, String> {
    let repo = state.repo()?;
    blocking(move || repo.branch_card(&name, &base)).await
}

#[tauri::command]
async fn branch_diff(
    branch: String,
    base: String,
    ignore_whitespace: bool,
    state: State<'_, AppState>,
) -> Result<BranchDiff, String> {
    let repo = state.repo()?;
    blocking(move || repo.branch_diff(&branch, &base, ignore_whitespace)).await
}

#[tauri::command]
async fn commit_diff(
    hash: String,
    ignore_whitespace: bool,
    state: State<'_, AppState>,
) -> Result<Vec<FileDiff>, String> {
    let repo = state.repo()?;
    blocking(move || repo.commit_diff(&hash, ignore_whitespace)).await
}

#[tauri::command]
async fn file_lines(
    rev: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let repo = state.repo()?;
    blocking(move || repo.file_lines(&rev, &path)).await
}

#[tauri::command]
async fn conflicts(
    branch: String,
    base: String,
    state: State<'_, AppState>,
) -> Result<Vec<ConflictFile>, String> {
    let repo = state.repo()?;
    blocking(move || repo.conflicts(&branch, &base)).await
}

#[tauri::command]
async fn plan_hotfix(input: HotfixInput, state: State<'_, AppState>) -> Result<HotfixPlan, String> {
    let repo = state.repo()?;
    blocking(move || repo.plan_hotfix(&input)).await
}

/// Builds an operation with `f` and makes it the current one; steps run through `op_step`.
async fn start_op(
    app: &AppHandle,
    state: &State<'_, AppState>,
    f: impl FnOnce(&Repo) -> musubiko_core::Result<Operation> + Send + 'static,
) -> Result<Operation, String> {
    if state
        .op
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|o| matches!(o.status, OpStatus::Running | OpStatus::Failed))
    {
        return Err("Уже идёт другая операция".into());
    }
    let repo = state.repo()?;
    let op = blocking(move || f(&repo)).await?;
    log::op_changed(None, &op);
    journal::upsert(app, &op);
    *state.op.lock().unwrap() = Some(op.clone());
    Ok(op)
}

#[tauri::command]
async fn start_hotfix(
    input: HotfixInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Operation, String> {
    start_op(&app, &state, move |repo| repo.start_hotfix(&input)).await
}

#[tauri::command]
async fn plan_new_branch(
    input: NewBranchInput,
    state: State<'_, AppState>,
) -> Result<NewBranchPlan, String> {
    let repo = state.repo()?;
    blocking(move || repo.plan_new_branch(&input)).await
}

#[tauri::command]
async fn start_new_branch(
    input: NewBranchInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Operation, String> {
    start_op(&app, &state, move |repo| repo.start_new_branch(&input)).await
}

#[tauri::command]
async fn hotfix_radar(state: State<'_, AppState>) -> Result<Radar, String> {
    let repo = state.repo()?;
    blocking(move || repo.hotfix_radar()).await
}

#[tauri::command]
async fn plan_port(input: PortInput, state: State<'_, AppState>) -> Result<OpPlan, String> {
    let repo = state.repo()?;
    blocking(move || repo.plan_port(&input)).await
}

#[tauri::command]
async fn start_port(
    input: PortInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Operation, String> {
    start_op(&app, &state, move |repo| repo.start_port(&input)).await
}

#[tauri::command]
async fn plan_checkout(
    input: CheckoutInput,
    state: State<'_, AppState>,
) -> Result<CheckoutPlan, String> {
    let repo = state.repo()?;
    blocking(move || repo.plan_checkout(&input)).await
}

#[tauri::command]
async fn start_checkout(
    input: CheckoutInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Operation, String> {
    start_op(&app, &state, move |repo| repo.start_checkout(&input)).await
}

#[tauri::command]
fn get_settings(app: AppHandle) -> settings::Settings {
    settings::load(&app)
}

#[tauri::command]
fn set_settings(value: settings::Settings, app: AppHandle) -> settings::Settings {
    let value = settings::Settings {
        fetch_interval_min: value.fetch_interval_min.clamp(1, 24 * 60),
        ..value
    };
    settings::save(&app, &value);
    value
}

/// A background fetch: not journaled, skipped while an operation is running.
/// Returns false when it was skipped.
#[tauri::command]
async fn fetch_quietly(state: State<'_, AppState>) -> Result<bool, String> {
    if state
        .op
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|o| matches!(o.status, OpStatus::Running | OpStatus::Failed))
    {
        return Ok(false);
    }
    let repo = state.repo()?;
    blocking(move || repo.fetch_quietly().map(|_| true)).await
}

/// Drops cached branch statuses; the next listing computes everything again.
#[tauri::command]
async fn clear_status_cache(state: State<'_, AppState>) -> Result<(), String> {
    let repo = state.repo()?;
    tauri::async_runtime::spawn_blocking(move || repo.clear_status_cache())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn locate(query: String, state: State<'_, AppState>) -> Result<CommitSearch, String> {
    let repo = state.repo()?;
    blocking(move || repo.locate(&query)).await
}

#[tauri::command]
async fn start_fetch(app: AppHandle, state: State<'_, AppState>) -> Result<Operation, String> {
    start_op(&app, &state, |repo| repo.start_fetch()).await
}

#[tauri::command]
async fn plan_delete(input: DeleteInput, state: State<'_, AppState>) -> Result<OpPlan, String> {
    let repo = state.repo()?;
    blocking(move || repo.plan_delete(&input)).await
}

#[tauri::command]
async fn start_delete(
    input: DeleteInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Operation, String> {
    start_op(&app, &state, move |repo| repo.start_delete(&input)).await
}

/// Applies `f` to the current operation off the main thread and records the result.
async fn with_op(
    app: &AppHandle,
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Operation, &Repo) + Send + 'static,
) -> Result<Operation, String> {
    let repo = state.repo()?;
    let before = state.op.lock().unwrap().clone().ok_or("Нет операции")?;
    let mut op = before.clone();
    let op = tauri::async_runtime::spawn_blocking(move || {
        f(&mut op, &repo);
        op
    })
    .await
    .map_err(|e| e.to_string())?;
    log::op_changed(Some(&before), &op);
    journal::upsert(app, &op);
    *state.op.lock().unwrap() = Some(op.clone());
    Ok(op)
}

/// Runs the next step, or retries the failed one.
#[tauri::command]
async fn op_step(app: AppHandle, state: State<'_, AppState>) -> Result<Operation, String> {
    with_op(&app, &state, |op, repo| op.step(repo)).await
}

/// Where a cherry-pick stopped on a conflict, for the current operation.
#[tauri::command]
async fn op_pick_state(state: State<'_, AppState>) -> Result<Option<PickState>, String> {
    let repo = state.repo()?;
    let dir = state
        .op
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|o| o.stopped_pick().map(str::to_string));
    match dir {
        Some(dir) => blocking(move || repo.pick_state(&dir).map(Some)).await,
        None => Ok(None),
    }
}

/// Keeps one side of a conflicting file in the stopped cherry-pick.
#[tauri::command]
async fn op_pick_take(
    path: String,
    side: Side,
    state: State<'_, AppState>,
) -> Result<PickState, String> {
    let repo = state.repo()?;
    let dir = state
        .op
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|o| o.stopped_pick().map(str::to_string))
        .ok_or("Перенос не стоит на конфликте")?;
    blocking(move || repo.pick_take(&dir, &path, side)).await
}

/// Continues the stopped cherry-pick after the conflict was resolved, or skips its commit.
#[tauri::command]
async fn op_resume(
    skip: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Operation, String> {
    with_op(&app, &state, move |op, repo| op.resume(repo, skip)).await
}

/// Undoes what the stopped operation already did.
#[tauri::command]
async fn op_rollback(app: AppHandle, state: State<'_, AppState>) -> Result<Operation, String> {
    with_op(&app, &state, |op, repo| {
        op.undo_all(repo, OpStatus::RolledBack)
    })
    .await
}

/// Keeps what the stopped operation did and closes it.
#[tauri::command]
async fn op_leave(app: AppHandle, state: State<'_, AppState>) -> Result<Operation, String> {
    let op = with_op(&app, &state, |op, _| op.status = OpStatus::Partial).await?;
    *state.op.lock().unwrap() = None;
    Ok(op)
}

#[tauri::command]
fn op_close(state: State<'_, AppState>) {
    let mut op = state.op.lock().unwrap();
    if op
        .as_ref()
        .is_some_and(|o| !matches!(o.status, OpStatus::Running | OpStatus::Failed))
    {
        *op = None;
    }
}

#[tauri::command]
fn journal_list(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Operation>, String> {
    let repo = state.repo()?;
    Ok(journal::list(&app, repo.root()))
}

/// Undoes a finished operation from the journal, guarded against later changes.
#[tauri::command]
async fn journal_undo(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Operation, String> {
    let git = state.git()?;
    let mut op = journal::get(&app, &id).ok_or("Операции нет в журнале")?;
    let before = op.clone();
    let op = blocking(move || {
        let repo = Repo::open(git, &op.repo)?;
        op.undo_all(&repo, OpStatus::Undone);
        Ok(op)
    })
    .await?;
    log::op_changed(Some(&before), &op);
    journal::upsert(&app, &op);
    Ok(op)
}

#[tauri::command]
fn recent_repos(app: AppHandle) -> Vec<String> {
    recent::list(&app)
}

#[tauri::command]
fn forget_repo(path: String, app: AppHandle) -> Vec<String> {
    recent::forget(&app, &path)
}

/// A line from the web UI: an error it showed, or a script error.
#[tauri::command]
fn log_event(level: String, text: String) {
    log::write(&level, &text);
}

/// The report for `error`, for the user to paste into a chat.
#[tauri::command]
fn error_report(error: String, app: AppHandle, state: State<'_, AppState>) -> String {
    let git = state
        .git
        .lock()
        .unwrap()
        .as_ref()
        .map(|g| format!("{} — {}", g.version(), g.path().display()));
    let repo = state
        .repo
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.root().to_path_buf());
    log::report(
        &app.package_info().version.to_string(),
        &error,
        git,
        repo.as_deref(),
    )
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    log::copy(&text)
}

#[tauri::command]
fn log_path() -> Option<String> {
    log::path().map(|p| p.to_string_lossy().into_owned())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            log::init(app.handle());
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_repo,
            repo_info,
            list_branches,
            branch_card,
            branch_diff,
            commit_diff,
            file_lines,
            conflicts,
            plan_hotfix,
            start_hotfix,
            plan_new_branch,
            start_new_branch,
            hotfix_radar,
            plan_port,
            start_port,
            plan_delete,
            start_delete,
            start_fetch,
            locate,
            clear_status_cache,
            get_settings,
            set_settings,
            fetch_quietly,
            plan_checkout,
            start_checkout,
            op_step,
            op_pick_state,
            op_pick_take,
            op_resume,
            op_rollback,
            op_leave,
            op_close,
            journal_list,
            journal_undo,
            recent_repos,
            forget_repo,
            log_event,
            error_report,
            copy_text,
            log_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running Musubiko");
}
