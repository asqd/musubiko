//! `musubiko branches [PATH] [--base develop]` prints the branch list the app would show.
//! `musubiko info [PATH]` prints the repository summary.
//! `musubiko diff BRANCH [PATH] [--base develop] [-w]` prints the branch's changes per file.
//! `musubiko conflicts BRANCH [PATH]` prints where merging into the base would conflict.
//! `musubiko hotfix NAME [PATH] [--after worktree|switch|stay] [--no-push] [--run]` plans a hotfix
//! pair and, with `--run`, creates it.
//! `musubiko radar [PATH]` shows which hotfix commits are not on the develop side.
//! `--json` prints exactly what the app receives.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use musubiko_core::{After, Git, HotfixInput, Location, OpStatus, Repo, Status, StepState};

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
    let mut after = After::Stay;
    let mut push = true;
    let mut execute = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--base" => base = args.next(),
            "--json" => json = true,
            "-w" => ignore_ws = true,
            "--no-push" => push = false,
            "--run" => execute = true,
            "--after" => {
                after = match args.next().as_deref() {
                    Some("worktree") => After::Worktree,
                    Some("switch") => After::Switch,
                    _ => After::Stay,
                }
            }
            _ if matches!(cmd.as_str(), "diff" | "conflicts" | "hotfix") && branch.is_none() => branch = Some(a),
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
        "hotfix" => {
            let input = HotfixInput { name: branch.ok_or("укажите название: musubiko hotfix NAME [PATH]")?, push, after };
            let plan = repo.plan_hotfix(&input)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&plan)?);
                return Ok(());
            }
            println!("{} ← {}\n{} ← {}", plan.master_branch, plan.master_from, plan.dev_branch, plan.dev_from);
            for p in &plan.problems {
                println!("! {p}");
            }
            if !execute {
                for c in &plan.commands {
                    println!("  $ {c}");
                }
                eprintln!("Это план. Чтобы выполнить, добавьте --run");
                return Ok(());
            }
            let mut op = repo.start_hotfix(&input)?;
            while op.status == OpStatus::Running {
                let i = op.cursor();
                op.step(&repo);
                match &op.states[i] {
                    StepState::Done { .. } => println!("✓ {}", op.steps[i].command()),
                    StepState::Failed { error } => println!("✗ {}\n  {error}", op.steps[i].command()),
                    StepState::Pending => {}
                }
            }
            if op.status != OpStatus::Done {
                return Err("остановился; сделанное не откатывал".into());
            }
        }
        "radar" => {
            let started = Instant::now();
            let r = repo.hotfix_radar()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
                return Ok(());
            }
            for p in &r.pairs {
                println!("{:<10?} {}  ({} коммитов, не перенесено {})", p.state, p.master_branch, p.commits.len(), p.unsynced);
                for c in &p.commits {
                    let to = c.ported.as_ref().map(|x| format!("→ {} в {}", x.short, x.place)).unwrap_or_else(|| "✗ не перенесён".into());
                    println!("    {} {:<50} {to}", c.commit.short, c.commit.subject);
                }
                if let Some(n) = &p.note {
                    println!("    ({n})");
                }
            }
            if let Some(d) = r.develop_stale_days {
                println!("{} отстаёт от {} на {d} дней: похоже, репозиторий живёт без {}", r.develop, r.master, r.develop);
            }
            if !r.lost.is_empty() {
                println!("В {}, но не в {} (за {} дней):", r.master, r.develop, r.recent_days);
                for c in &r.lost {
                    println!("    {} {} — {}", c.short, c.subject, c.author.name);
                }
            }
            eprintln!("{} пар, старых скрыто {}, за {:.1?}", r.pairs.len(), r.older_hidden, started.elapsed());
        }
        other => return Err(format!("неизвестная команда {other}; есть branches, diff, conflicts, hotfix, radar и info").into()),
    }
    Ok(())
}
