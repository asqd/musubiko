//! Dev only: the app's commands over HTTP, so the UI can run in a plain browser against the real
//! core and be tested there. Mirrors `src-tauri/src/main.rs` command by command; the app's own
//! journal, settings and recent list live in `$MUSUBIKO_BRIDGE_DATA` (default
//! `/tmp/musubiko-bridge`) instead of the app's data folder.
//!
//!   cargo run -p musubiko-bridge            then open http://localhost:1420/bridge.html
//!
//! Native parts are stand-ins: the folder dialog answers with the path set by
//! `POST /next-dialog` (or cancels), and "reveal in Finder", "open" and the clipboard are only
//! recorded; `GET /effects` lists them. Only the Vite dev origin may call it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use musubiko_core::{Git, OpStatus, Operation, Repo};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const ADDR: &str = "127.0.0.1:1421";
const ORIGINS: [&str; 2] = ["http://localhost:1420", "http://127.0.0.1:1420"];

#[derive(Default)]
struct State {
    git: Mutex<Option<Git>>,
    repo: Mutex<Option<Repo>>,
    op: Mutex<Option<Operation>>,
    next_dialog: Mutex<Option<String>>,
    effects: Mutex<Vec<Value>>,
    /// Serialises journal, settings and recent-list writes, as one app process would.
    files: Mutex<()>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    background_fetch: bool,
    fetch_interval_min: u32,
    compact_sidebar: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            background_fetch: false,
            fetch_interval_min: 15,
            compact_sidebar: false,
        }
    }
}

fn data_dir() -> PathBuf {
    std::env::var_os("MUSUBIKO_BRIDGE_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/musubiko-bridge"))
}

fn read_json<T: DeserializeOwned + Default>(name: &str) -> T {
    std::fs::read(data_dir().join(name))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(name: &str, value: &T) {
    let _ = std::fs::create_dir_all(data_dir());
    if let Ok(b) = serde_json::to_vec_pretty(value) {
        let _ = std::fs::write(data_dir().join(name), b);
    }
}

impl State {
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

    fn effect(&self, kind: &str, args: &Value) {
        eprintln!("bridge: {kind} {args}");
        self.effects
            .lock()
            .unwrap()
            .push(json!({ "kind": kind, "args": args }));
    }

    fn journal_upsert(&self, op: &Operation) {
        let _g = self.files.lock().unwrap();
        let mut ops: Vec<Operation> = read_json("journal.json");
        match ops.iter_mut().find(|o| o.id == op.id) {
            Some(o) => *o = op.clone(),
            None => ops.insert(0, op.clone()),
        }
        ops.truncate(300);
        write_json("journal.json", &ops);
    }

    fn busy(&self) -> bool {
        self.op
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|o| matches!(o.status, OpStatus::Running | OpStatus::Failed))
    }

    fn start_op(
        &self,
        f: impl FnOnce(&Repo) -> musubiko_core::Result<Operation>,
    ) -> Result<Value, String> {
        if self.busy() {
            return Err("Уже идёт другая операция".into());
        }
        let op = f(&self.repo()?).map_err(|e| e.to_string())?;
        self.journal_upsert(&op);
        *self.op.lock().unwrap() = Some(op.clone());
        ok(op)
    }

    fn with_op(&self, f: impl FnOnce(&mut Operation, &Repo)) -> Result<Operation, String> {
        let repo = self.repo()?;
        let mut op = self.op.lock().unwrap().clone().ok_or("Нет операции")?;
        f(&mut op, &repo);
        self.journal_upsert(&op);
        *self.op.lock().unwrap() = Some(op.clone());
        Ok(op)
    }

    fn stopped_pick(&self) -> Option<String> {
        self.op
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|o| o.stopped_pick().map(str::to_string))
    }
}

fn ok<T: Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn core<T: Serialize>(r: musubiko_core::Result<T>) -> Result<Value, String> {
    ok(r.map_err(|e| e.to_string())?)
}

fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("аргумент {name}: {e}"))
}

fn dispatch(st: &State, cmd: &str, a: &Value) -> Result<Value, String> {
    match cmd {
        "open_repo" => {
            let path: String = arg(a, "path")?;
            let repo = Repo::open(st.git()?, &PathBuf::from(path)).map_err(|e| e.to_string())?;
            let info = repo.info().map_err(|e| e.to_string())?;
            *st.repo.lock().unwrap() = Some(repo);
            let _g = st.files.lock().unwrap();
            let mut recent: Vec<String> = read_json("recent.json");
            let root = info.root.to_string_lossy().into_owned();
            recent.retain(|r| *r != root);
            recent.insert(0, root);
            recent.truncate(10);
            write_json("recent.json", &recent);
            ok(info)
        }
        "repo_info" => core(st.repo()?.info()),
        "list_branches" => core(st.repo()?.branches(&arg::<String>(a, "base")?)),
        "my_branches" => core(st.repo()?.my_branches(
            &arg::<String>(a, "base")?,
            arg::<Option<String>>(a, "author")?.as_deref(),
        )),
        "branch_card" => core(
            st.repo()?
                .branch_card(&arg::<String>(a, "name")?, &arg::<String>(a, "base")?),
        ),
        "branch_diff" => core(st.repo()?.branch_diff(
            &arg::<String>(a, "branch")?,
            &arg::<String>(a, "base")?,
            arg(a, "ignoreWhitespace")?,
        )),
        "commit_diff" => core(
            st.repo()?
                .commit_diff(&arg::<String>(a, "hash")?, arg(a, "ignoreWhitespace")?),
        ),
        "file_lines" => core(
            st.repo()?
                .file_lines(&arg::<String>(a, "rev")?, &arg::<String>(a, "path")?),
        ),
        "conflicts" => core(
            st.repo()?
                .conflicts(&arg::<String>(a, "branch")?, &arg::<String>(a, "base")?),
        ),
        "plan_hotfix" => core(st.repo()?.plan_hotfix(&arg(a, "input")?)),
        "start_hotfix" => {
            let input = arg(a, "input")?;
            st.start_op(|r| r.start_hotfix(&input))
        }
        "plan_new_branch" => core(st.repo()?.plan_new_branch(&arg(a, "input")?)),
        "start_new_branch" => {
            let input = arg(a, "input")?;
            st.start_op(|r| r.start_new_branch(&input))
        }
        "hotfix_radar" => core(st.repo()?.hotfix_radar()),
        "plan_port" => core(st.repo()?.plan_port(&arg(a, "input")?)),
        "start_port" => {
            let input = arg(a, "input")?;
            st.start_op(|r| r.start_port(&input))
        }
        "plan_delete" => core(st.repo()?.plan_delete(&arg(a, "input")?)),
        "start_delete" => {
            let input = arg(a, "input")?;
            st.start_op(|r| r.start_delete(&input))
        }
        "start_fetch" => st.start_op(|r| r.start_fetch()),
        "locate" => core(st.repo()?.locate(&arg::<String>(a, "query")?)),
        "clear_status_cache" => {
            st.repo()?.clear_status_cache();
            ok(())
        }
        "get_settings" => ok(read_json::<Settings>("settings.json")),
        "set_settings" => {
            let mut v: Settings = arg(a, "value")?;
            v.fetch_interval_min = v.fetch_interval_min.clamp(1, 24 * 60);
            write_json("settings.json", &v);
            ok(v)
        }
        "fetch_quietly" => {
            if st.busy() {
                return ok(false);
            }
            core(st.repo()?.fetch_quietly().map(|_| true))
        }
        "plan_checkout" => core(st.repo()?.plan_checkout(&arg(a, "input")?)),
        "start_checkout" => {
            let input = arg(a, "input")?;
            st.start_op(|r| r.start_checkout(&input))
        }
        "op_step" => ok(st.with_op(|op, repo| op.step(repo))?),
        "op_pick_state" => match st.stopped_pick() {
            Some(dir) => core(st.repo()?.pick_state(&dir).map(Some)),
            None => ok(Value::Null),
        },
        "op_pick_take" => {
            let dir = st.stopped_pick().ok_or("Перенос не стоит на конфликте")?;
            core(
                st.repo()?
                    .pick_take(&dir, &arg::<String>(a, "path")?, arg(a, "side")?),
            )
        }
        "op_resume" => {
            let skip: bool = arg(a, "skip")?;
            ok(st.with_op(|op, repo| op.resume(repo, skip))?)
        }
        "op_rollback" => ok(st.with_op(|op, repo| op.undo_all(repo, OpStatus::RolledBack))?),
        "op_leave" => {
            let op = st.with_op(|op, _| op.status = OpStatus::Partial)?;
            *st.op.lock().unwrap() = None;
            ok(op)
        }
        "op_close" => {
            let mut op = st.op.lock().unwrap();
            if op
                .as_ref()
                .is_some_and(|o| !matches!(o.status, OpStatus::Running | OpStatus::Failed))
            {
                *op = None;
            }
            ok(())
        }
        "journal_list" => {
            let root = st.repo()?.root().to_path_buf();
            let ops: Vec<Operation> = read_json("journal.json");
            ok(ops.into_iter().filter(|o| o.repo == root).collect::<Vec<_>>())
        }
        "journal_undo" => {
            let id: String = arg(a, "id")?;
            let ops: Vec<Operation> = read_json("journal.json");
            let mut op = ops
                .into_iter()
                .find(|o| o.id == id)
                .ok_or("Операции нет в журнале")?;
            let repo = Repo::open(st.git()?, &op.repo).map_err(|e| e.to_string())?;
            op.undo_all(&repo, OpStatus::Undone);
            st.journal_upsert(&op);
            ok(op)
        }
        "recent_repos" => ok(read_json::<Vec<String>>("recent.json")),
        "forget_repo" => {
            let path: String = arg(a, "path")?;
            let _g = st.files.lock().unwrap();
            let mut recent: Vec<String> = read_json("recent.json");
            recent.retain(|r| *r != path);
            write_json("recent.json", &recent);
            ok(recent)
        }
        "log_event" => {
            st.effect("log", a);
            ok(())
        }
        "error_report" => {
            let error: String = arg(a, "error")?;
            ok(format!("Musubiko (мост для разработки)\n\n{error}"))
        }
        "copy_text" => {
            st.effect("copy", a);
            ok(())
        }
        "log_path" => ok("/tmp/musubiko-bridge/musubiko.log"),
        "plugin:dialog|open" => {
            st.effect("dialog", a);
            ok(st.next_dialog.lock().unwrap().take())
        }
        c if c.starts_with("plugin:opener|") => {
            st.effect(c, a);
            ok(())
        }
        other => Err(format!("мост не знает команду {other}")),
    }
}

struct Request {
    method: String,
    path: String,
    origin: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &TcpStream) -> std::io::Result<Request> {
    let mut r = BufReader::new(stream);
    let mut line = String::new();
    r.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let (mut len, mut origin) = (0usize, None);
    loop {
        let mut h = String::new();
        if r.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => len = v.trim().parse().unwrap_or(0),
                "origin" => origin = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body)?;
    Ok(Request {
        method,
        path,
        origin,
        body,
    })
}

fn respond(mut stream: TcpStream, status: &str, origin: Option<&str>, body: &str) {
    let cors = origin
        .map(|o| {
            format!(
                "Access-Control-Allow-Origin: {o}\r\nAccess-Control-Allow-Methods: POST, GET\r\n\
                 Access-Control-Allow-Headers: content-type\r\n"
            )
        })
        .unwrap_or_default();
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\n{cors}Content-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn handle(st: &State, stream: TcpStream) {
    let Ok(req) = read_request(&stream) else {
        return;
    };
    // Browsers send Origin; curl from this machine does not. Any other site is refused.
    let origin = req.origin.as_deref();
    if origin.is_some_and(|o| !ORIGINS.contains(&o)) {
        return respond(stream, "403 Forbidden", None, "\"origin\"");
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("OPTIONS", _) => respond(stream, "204 No Content", origin, ""),
        ("POST", "/invoke") => {
            let v: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            let cmd = v["cmd"].as_str().unwrap_or_default().to_string();
            let args = v.get("args").cloned().unwrap_or(json!({}));
            let started = std::time::Instant::now();
            let result = dispatch(st, &cmd, &args);
            eprintln!(
                "bridge: {cmd} {:.0?}{}",
                started.elapsed(),
                result.as_ref().err().map(|e| format!(" ✗ {e}")).unwrap_or_default()
            );
            match result {
                Ok(v) => respond(stream, "200 OK", origin, &v.to_string()),
                Err(e) => respond(stream, "400 Bad Request", origin, &json!(e).to_string()),
            }
        }
        ("POST", "/next-dialog") => {
            let path = String::from_utf8_lossy(&req.body).trim().to_string();
            *st.next_dialog.lock().unwrap() = (!path.is_empty()).then_some(path);
            respond(stream, "200 OK", origin, "null")
        }
        ("GET", "/effects") => {
            let e = st.effects.lock().unwrap().clone();
            respond(stream, "200 OK", origin, &Value::Array(e).to_string())
        }
        _ => respond(stream, "404 Not Found", origin, "null"),
    }
}

fn main() {
    let listener = TcpListener::bind(ADDR).expect("bridge: port 1421 is busy");
    eprintln!(
        "bridge: {ADDR}, data in {}; open http://localhost:1420/bridge.html",
        data_dir().display()
    );
    let st = Arc::new(State::default());
    for stream in listener.incoming().flatten() {
        let st = st.clone();
        std::thread::spawn(move || handle(&st, stream));
    }
}
