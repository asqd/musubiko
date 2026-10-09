//! The app's log and the error report built from it.
//!
//! The log file (`musubiko.log` in the app's log folder, `~/Library/Logs/app.musubiko.Musubiko`
//! on macOS) keeps what helps after the fact: errors shown to the user, operation steps, slow
//! git commands, script errors and panics. Failed git commands are many and mostly expected
//! (probes), so they stay in memory and only go into a report.
//!
//! Home folders and passwords in remote URLs are masked before anything is written.

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use musubiko_core::{Finished, OpStatus, Operation, StepState};
use tauri::{AppHandle, Manager};

const FILE: &str = "musubiko.log";
/// Past this size the log moves to `musubiko.1.log`, replacing the older one.
const ROTATE_AT: u64 = 1 << 20;
/// git commands slower than this are logged even when they succeed.
const SLOW: Duration = Duration::from_secs(3);
/// Failed git commands kept in memory for a report.
const FAILED_KEPT: usize = 30;
/// Log lines from this session that go into a report.
const REPORT_LINES: usize = 80;
const START: &str = "=== Musubiko";

struct Log {
    path: Option<PathBuf>,
    failed: VecDeque<String>,
}

static LOG: OnceLock<Mutex<Log>> = OnceLock::new();

fn log() -> &'static Mutex<Log> {
    LOG.get_or_init(|| {
        Mutex::new(Log {
            path: None,
            failed: VecDeque::new(),
        })
    })
}

pub fn path() -> Option<PathBuf> {
    log().lock().ok()?.path.clone()
}

/// Opens the log, starts a session in it and installs the git and panic hooks.
pub fn init(app: &AppHandle) {
    let path = app.path().app_log_dir().ok().map(|d| d.join(FILE));
    start(path, &app.package_info().version.to_string());
}

fn start(path: Option<PathBuf>, version: &str) {
    if let Some(p) = &path {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
    if let Ok(mut l) = log().lock() {
        l.path = path;
    }
    write("", &format!("{START} {version} started ({}) ===", system()));

    musubiko_core::set_command_hook(on_git);

    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write("PANIC", &info.to_string());
        default(info);
    }));
}

/// Appends a line (or several) to the log. Never fails: a log that cannot be written is skipped.
pub fn write(level: &str, text: &str) {
    // The lock only guards the path: a panic while writing must not leave it held.
    let Some(path) = path() else { return };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > ROTATE_AT) {
        let _ = std::fs::rename(&path, path.with_extension("1.log"));
    }
    let mut text = mask(text.trim_end());
    if text.contains('\n') {
        text = text.replace('\n', "\n    ");
    }
    let line = if level.is_empty() {
        format!("{} {text}\n", stamp(now()))
    } else {
        format!("{} {level:<5} {text}\n", stamp(now()))
    };
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

fn on_git(f: &Finished) {
    let failed = f.code != Some(0);
    let stderr = String::from_utf8_lossy(f.stderr);
    let stderr = stderr.trim();
    // `apply --check` failing is the answer to the question asked, not a problem.
    let probe = f.args.contains(&"--check");
    if failed && !stderr.is_empty() && !probe {
        let code = f.code.map_or("signal".to_string(), |c| c.to_string());
        let entry = mask(&format!(
            "{} git {} — exit {code}, {} in {}\n    {}",
            stamp(now()),
            args_text(f.args),
            took(f.took),
            f.dir.display(),
            clip(stderr, 1500).replace('\n', "\n    ")
        ));
        if let Ok(mut l) = log().lock() {
            if l.failed.len() == FAILED_KEPT {
                l.failed.pop_front();
            }
            l.failed.push_back(entry);
        }
    }
    if f.took >= SLOW {
        write(
            "SLOW",
            &format!(
                "git {} — {} in {}",
                args_text(f.args),
                took(f.took),
                f.dir.display()
            ),
        );
    }
}

/// Logs what changed in an operation after a step, undo or close.
pub fn op_changed(before: Option<&Operation>, op: &Operation) {
    let was = before.map(|o| o.status);
    for (i, st) in op.states.iter().enumerate() {
        let had = before.and_then(|o| o.states.get(i));
        if let StepState::Failed { error } = st {
            if !matches!(had, Some(StepState::Failed { error: e }) if e == error) {
                write(
                    "OP",
                    &format!(
                        "«{}»: шаг «{}» не удался: {}\n{error}",
                        op.title,
                        op.steps[i].title,
                        op.steps[i].command()
                    ),
                );
            }
        }
    }
    if was != Some(op.status) {
        write("OP", &format!("«{}»: {:?}", op.title, op.status));
    }
    if let Some(e) = &op.undo_error {
        if before.and_then(|o| o.undo_error.as_ref()) != Some(e) {
            write("OP", &format!("«{}»: откат остановился: {e}", op.title));
        }
    }
    if op.status == OpStatus::Running && was.is_none() {
        let steps: Vec<String> = op.steps.iter().map(|s| s.command()).collect();
        write("OP", &format!("шаги:\n{}", steps.join("\n")));
    }
}

/// Text to paste into a chat: versions, the error, this session's log and recent failed commands.
pub fn report(version: &str, error: &str, git: Option<String>, repo: Option<&Path>) -> String {
    let mut out = format!("Musubiko {version} · {}\n", system());
    out += &format!("git: {}\n", git.as_deref().unwrap_or("не найден"));
    if let Some(r) = repo {
        out += &format!("Репозиторий: {}\n", r.display());
    }
    out += &format!("Время: {}\n", stamp(now()));
    out += "\nОшибка:\n";
    out += error.trim();
    out += "\n";

    let (path, failed) = match log().lock() {
        Ok(l) => (l.path.clone(), l.failed.iter().cloned().collect::<Vec<_>>()),
        Err(_) => (None, Vec::new()),
    };
    let text = path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let session = lines.iter().rposition(|l| l.contains(START)).unwrap_or(0);
    let tail = &lines[session.max(lines.len().saturating_sub(REPORT_LINES))..];
    if !tail.is_empty() {
        out += "\nЛог (время UTC):\n";
        out += &tail.join("\n");
        out += "\n";
    }
    if !failed.is_empty() {
        out += "\nНеудачные команды git:\n";
        out += &failed.join("\n");
        out += "\n";
    }
    mask(&out)
}

/// Puts text on the clipboard. The web view's clipboard is not always allowed, so this goes
/// through the system tool; the UI falls back to the web clipboard where there is none.
pub fn copy(text: &str) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Нет системного буфера обмена".into());
    }
    let mut child = std::process::Command::new("/usr/bin/pbcopy")
        // Started from Finder the app has no locale, and pbcopy would garble Cyrillic.
        .env("LANG", "en_US.UTF-8")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("pbcopy: {status}"))
    }
}

fn system() -> String {
    let os = if cfg!(target_os = "macos") {
        std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .map(|o| format!("macOS {}", String::from_utf8_lossy(&o.stdout).trim()))
            .unwrap_or_else(|| "macOS".into())
    } else if cfg!(target_os = "linux") {
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                    .map(|v| v.trim_matches('"').to_string())
            })
            .unwrap_or_else(|| "Linux".into())
    } else if cfg!(windows) {
        "Windows".into()
    } else {
        std::env::consts::OS.to_string()
    };
    format!("{os}, {}", std::env::consts::ARCH)
}

fn args_text(args: &[&str]) -> String {
    clip(&args.join(" "), 400)
}

fn took(d: Duration) -> String {
    format!("{:.1} с", d.as_secs_f64())
}

/// The first `max` characters, with a note of how much was cut.
fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}… (+{} симв.)", &s[..i], s[i..].chars().count()),
        None => s.to_string(),
    }
}

/// Masks the home folder and credentials in URLs (`https://user:token@host` → `https://***@host`).
fn mask(s: &str) -> String {
    let mut s = s.to_string();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if let Some(home) = home.map(PathBuf::from) {
        let home = home
            .to_string_lossy()
            .trim_end_matches(['/', '\\'])
            .to_string();
        if home.len() > 1 {
            s = s.replace(&home, "~");
            // git prints Windows paths with forward slashes.
            if home.contains('\\') {
                s = s.replace(&home.replace('\\', "/"), "~");
            }
        }
    }
    mask_credentials(&s)
}

fn mask_credentials(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out += head;
        let end = tail
            .find(|c: char| c == '/' || c.is_whitespace() || c == '\'' || c == '"')
            .unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(at) => {
                out += "***";
                rest = &tail[at..];
            }
            None => rest = tail,
        }
    }
    out + rest
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `2026-10-10 11:03:12` in UTC.
fn stamp(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_and_reports() {
        let dir = std::env::temp_dir().join(format!("musubiko-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        start(Some(dir.join(FILE)), "9.9.9");
        write("ERROR", "list_branches: сломалось\nвторая строка");

        // A real failed git command goes to memory, not to the file.
        let empty = dir.join("not-a-repo");
        std::fs::create_dir_all(&empty).unwrap();
        let git = musubiko_core::Git::discover().unwrap();
        assert!(musubiko_core::Repo::open(git.clone(), &empty).is_err());

        let file = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(file.contains("=== Musubiko 9.9.9 started"));
        assert!(file.contains("ERROR list_branches: сломалось\n    вторая строка"));
        assert!(!file.contains("not-a-repo"));

        let r = report("9.9.9", "Не открылся", Some(git.version()), Some(&empty));
        assert!(r.starts_with("Musubiko 9.9.9 · "));
        assert!(r.contains("Ошибка:\nНе открылся"));
        assert!(r.contains("ERROR list_branches"));
        assert!(r.contains("Неудачные команды git:"));
        assert!(r.contains("not-a-repo"), "{r}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stamps_in_utc() {
        assert_eq!(stamp(0), "1970-01-01 00:00:00");
        assert_eq!(stamp(1_791_630_192), "2026-10-10 11:03:12");
        assert_eq!(stamp(951_782_400), "2000-02-29 00:00:00");
    }

    #[test]
    fn masks_credentials() {
        assert_eq!(
            mask_credentials("fatal: https://ker:glpat-xyz@gitlab.lvtv.me/a/b.git not found"),
            "fatal: https://***@gitlab.lvtv.me/a/b.git not found"
        );
        assert_eq!(
            mask_credentials("ssh://git@host/x and https://host/y"),
            "ssh://***@host/x and https://host/y"
        );
        assert_eq!(mask_credentials("git@host:x.git"), "git@host:x.git");
    }

    #[test]
    fn clips_long_text() {
        assert_eq!(clip("абвгд", 3), "абв… (+2 симв.)");
        assert_eq!(clip("abc", 3), "abc");
    }
}
