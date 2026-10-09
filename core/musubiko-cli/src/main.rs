//! `musubiko branches [PATH] [--base develop]` prints the branch list the app would show.
//! `musubiko info [PATH]` prints the repository summary.
//! `musubiko diff BRANCH [PATH] [--base develop] [-w]` prints the branch's changes per file.
//! `musubiko conflicts BRANCH [PATH]` prints where merging into the base would conflict.
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
    let mut ignore_ws = false;
    let mut branch: Option<String> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--base" => base = args.next(),
            "--json" => json = true,
            "-w" => ignore_ws = true,
            _ if (cmd == "diff" || cmd == "conflicts") && branch.is_none() => branch = Some(a),
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
        "diff" => {
            let branch = branch.ok_or("укажите ветку: musubiko diff BRANCH [PATH]")?;
            let base = match base { Some(b) => b, None => repo.default_base()? };
            let started = Instant::now();
            let d = repo.branch_diff(&branch, &base, ignore_ws)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&d)?);
                return Ok(());
            }
            println!("{} → {}  merge-base {} · {} коммитов", d.branch, d.target, d.merge_base.short, d.commits.len());
            for c in &d.commits {
                println!("  {} {}", c.short, c.subject);
            }
            for f in &d.files {
                let from = f.old_path.as_deref().map(|p| format!("{p} → ")).unwrap_or_default();
                let flags = if f.binary { " [binary]" } else if f.truncated { " [truncated]" } else { "" };
                println!("  {:?} {from}{}  +{} −{}{flags}", f.change, f.path, f.additions, f.deletions);
            }
            for c in repo.conflicts(&branch, &base)? {
                println!("  конфликт {} ({} мест)", c.path, c.regions.len());
            }
            eprintln!("за {:.0?}", started.elapsed());
        }
        "conflicts" => {
            let branch = branch.ok_or("укажите ветку: musubiko conflicts BRANCH [PATH]")?;
            let base = match base { Some(b) => b, None => repo.default_base()? };
            let list = repo.conflicts(&branch, &base)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
                return Ok(());
            }
            for c in &list {
                println!("{} ({} мест){}", c.path, c.regions.len(), c.note.as_deref().map(|n| format!(" — {n}")).unwrap_or_default());
                for r in &c.regions {
                    println!("  строка {}: {} ↔ {} строк", r.line, r.base.len(), r.branch.len());
                }
            }
        }
        other => return Err(format!("неизвестная команда {other}; есть branches, diff, conflicts и info").into()),
    }
    Ok(())
}
