//! `musubiko branches [PATH] [--base develop]` prints the branch list the app would show.
//! `musubiko info [PATH]` prints the repository summary.
//! `--json` prints exactly what the app receives.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use musubiko_core::{Git, Location, Repo, Status};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("musubiko: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "branches".into());
    let mut path = PathBuf::from(".");
    let mut base: Option<String> = None;
    let mut json = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--base" => base = args.next(),
            "--json" => json = true,
            _ => path = PathBuf::from(a),
        }
    }

    let git = Git::discover()?;
    let repo = Repo::open(git, &path)?;
    match cmd.as_str() {
        "info" => {
            let info = repo.info()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&info)?);
                return Ok(());
            }
            println!("{}  ({})", info.name, info.root.display());
            println!("git      {}", info.git);
            println!("HEAD     {} {}", info.head.as_deref().unwrap_or("(detached)"), info.head_short);
            println!("remote   {}", info.remote.as_deref().unwrap_or("—"));
            for b in &info.bases {
                println!("{:<8} {} {} {}", b.role, b.ref_name, b.tip.short, b.tip.subject);
            }
            for c in &info.changes {
                println!("  {:>2} {}", c.code, c.path);
            }
        }
        "branches" => {
            let base = match base { Some(b) => b, None => repo.default_base()? };
            let started = Instant::now();
            let list = repo.branches(&base)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
                return Ok(());
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
            for b in &list {
                let status = match &b.status {
                    Status::Merged => "merged".to_string(),
                    Status::Squashed => "squashed".to_string(),
                    Status::Conflict { files } => format!("conflict ({})", files.len()),
                    Status::Open => "open".to_string(),
                    Status::Unknown { error } => format!("? {error}"),
                };
                let loc = match b.location {
                    Location::Both => "",
                    Location::Local => " [local]",
                    Location::Remote => " [remote]",
                };
                println!(
                    "{}{:<44} {:<14} →{:<8} ↑{:<4} ↓{:<5} {:<20} {} {:>5}d  {}",
                    if b.is_head { "* " } else { "  " },
                    format!("{}{loc}", b.name),
                    status,
                    b.target,
                    b.ahead,
                    b.behind,
                    b.owner.name,
                    b.tip.short,
                    (now - b.tip.date) / 86_400,
                    b.tip.subject,
                );
            }
            eprintln!("{} веток за {:.0?}", list.len(), started.elapsed());
        }
        other => return Err(format!("неизвестная команда {other}; есть branches и info").into()),
    }
    Ok(())
}
