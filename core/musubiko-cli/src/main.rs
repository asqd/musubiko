//! `musubiko branches [PATH] [--base develop]` prints the branch list the app would show.
//! `musubiko info [PATH]` prints the repository summary.
//! `musubiko diff BRANCH [PATH] [--base develop] [-w]` prints the branch's changes per file.
//! `musubiko conflicts BRANCH [PATH]` prints where merging into the base would conflict.
//! `musubiko hotfix NAME [PATH] [--after worktree|switch|stay] [--no-push] [--run]` plans a hotfix
//! pair and, with `--run`, creates it.
//! `musubiko new NAME [PATH] [--base develop] [--local] [--push] [--after …] [--run]` plans a new
//! branch from the base (fresh from the server unless `--local`) and, with `--run`, creates it.
//! `musubiko card BRANCH [PATH] [--base develop]` shows one branch against every base, with its
//! own commits and files.
//! `musubiko mine [PATH] [--base develop] [--author EMAIL]` shows someone's branches (yours by
//! default): what stands in the way of each, and which are merged.
//! `musubiko where QUERY [PATH]` finds commits and tells where each one is.
//! `musubiko radar [PATH]` shows which hotfix commits are not on the develop side.
//! `--json` prints exactly what the app receives.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use musubiko_core::{
    After, Git, HotfixInput, Location, NewBranchInput, OpStatus, Repo, Status, StepState,
};

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
    let mut local = false;
    let mut push_new = false;
    let mut author: Option<String> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--base" => base = args.next(),
            "--json" => json = true,
            "-w" => ignore_ws = true,
            "--no-push" => push = false,
            "--run" => execute = true,
            "--local" => local = true,
            "--push" => push_new = true,
            "--author" => author = args.next(),
            "--after" => {
                after = match args.next().as_deref() {
                    Some("worktree") => After::Worktree,
                    Some("switch") => After::Switch,
                    _ => After::Stay,
                }
            }
            _ if matches!(
                cmd.as_str(),
                "diff" | "conflicts" | "hotfix" | "new" | "card" | "where"
            ) && branch.is_none() =>
            {
                branch = Some(a)
            }
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
            println!(
                "HEAD     {} {}",
                info.head.as_deref().unwrap_or("(detached)"),
                info.head_short
            );
            println!("remote   {}", info.remote.as_deref().unwrap_or("—"));
            for b in &info.bases {
                println!(
                    "{:<8} {} {} {}",
                    b.role, b.ref_name, b.tip.short, b.tip.subject
                );
            }
            for c in &info.changes {
                println!("  {:>2} {}", c.code, c.path);
            }
        }
        "branches" => {
            let base = match base {
                Some(b) => b,
                None => repo.default_base()?,
            };
            let started = Instant::now();
            let (list, stats) = repo.branches_with_stats(&base)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
                return Ok(());
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
            for b in &list {
                let status = status_word(&b.status);
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
            eprintln!(
                "{} веток за {:.0?} (из кэша {}, посчитано {})",
                list.len(),
                started.elapsed(),
                stats.hits,
                stats.computed
            );
        }
        "diff" => {
            let branch = branch.ok_or("укажите ветку: musubiko diff BRANCH [PATH]")?;
            let base = match base {
                Some(b) => b,
                None => repo.default_base()?,
            };
            let started = Instant::now();
            let d = repo.branch_diff(&branch, &base, ignore_ws)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&d)?);
                return Ok(());
            }
            println!(
                "{} → {}  merge-base {} · {} коммитов",
                d.branch,
                d.target,
                d.merge_base.short,
                d.commits.len()
            );
            for c in &d.commits {
                println!("  {} {}", c.short, c.subject);
            }
            for f in &d.files {
                let from = f
                    .old_path
                    .as_deref()
                    .map(|p| format!("{p} → "))
                    .unwrap_or_default();
                let flags = if f.binary {
                    " [binary]"
                } else if f.truncated {
                    " [truncated]"
                } else {
                    ""
                };
                println!(
                    "  {:?} {from}{}  +{} −{}{flags}",
                    f.change, f.path, f.additions, f.deletions
                );
            }
            for c in repo.conflicts(&branch, &base)? {
                println!("  конфликт {} ({} мест)", c.path, c.regions.len());
            }
            eprintln!("за {:.0?}", started.elapsed());
        }
        "conflicts" => {
            let branch = branch.ok_or("укажите ветку: musubiko conflicts BRANCH [PATH]")?;
            let base = match base {
                Some(b) => b,
                None => repo.default_base()?,
            };
            let list = repo.conflicts(&branch, &base)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
                return Ok(());
            }
            for c in &list {
                println!(
                    "{} ({} мест){}",
                    c.path,
                    c.regions.len(),
                    c.note
                        .as_deref()
                        .map(|n| format!(" — {n}"))
                        .unwrap_or_default()
                );
                for r in &c.regions {
                    println!(
                        "  строка {}: {} ↔ {} строк",
                        r.line,
                        r.base.len(),
                        r.branch.len()
                    );
                }
            }
        }
        "hotfix" => {
            let input = HotfixInput {
                name: branch.ok_or("укажите название: musubiko hotfix NAME [PATH]")?,
                push,
                after,
            };
            let plan = repo.plan_hotfix(&input)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&plan)?);
                return Ok(());
            }
            println!("{} ← {}", plan.master_branch, plan.master_from);
            if plan.with_dev {
                println!("{} ← {}", plan.dev_branch, plan.dev_from);
            }
            for w in &plan.warnings {
                println!("· {w}");
            }
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
                    StepState::Failed { error } => {
                        println!("✗ {}\n  {error}", op.steps[i].command())
                    }
                    StepState::Pending => {}
                }
            }
            if op.status != OpStatus::Done {
                return Err("остановился; сделанное не откатывал".into());
            }
        }
        "new" => {
            let input = NewBranchInput {
                name: branch.ok_or("укажите название: musubiko new NAME [PATH]")?,
                base: match base {
                    Some(b) => b,
                    None => repo.default_base()?,
                },
                fresh: !local,
                push: push_new,
                after,
            };
            let plan = repo.plan_new_branch(&input)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&plan)?);
                return Ok(());
            }
            println!("{} ← {}", plan.name, plan.from);
            for w in &plan.warnings {
                println!("· {w}");
            }
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
            let mut op = repo.start_new_branch(&input)?;
            while op.status == OpStatus::Running {
                let i = op.cursor();
                op.step(&repo);
                match &op.states[i] {
                    StepState::Done { .. } => println!("✓ {}", op.steps[i].command()),
                    StepState::Failed { error } => {
                        println!("✗ {}\n  {error}", op.steps[i].command())
                    }
                    StepState::Pending => {}
                }
            }
            if op.status != OpStatus::Done {
                return Err("остановился; сделанное не откатывал".into());
            }
        }
        "card" => {
            let name = branch.ok_or("укажите ветку: musubiko card BRANCH [PATH]")?;
            let base = match base {
                Some(b) => b,
                None => repo.default_base()?,
            };
            let card = repo.branch_card(&name, &base)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&card)?);
                return Ok(());
            }
            let b = &card.branch;
            println!(
                "{}  {}  {}",
                b.name,
                b.owner.name,
                card.upstream.as_deref().unwrap_or("без upstream")
            );
            for x in &card.against {
                println!(
                    "  против {:<10} {:<14} ↑{} ↓{}",
                    x.target,
                    status_word(&x.status),
                    x.ahead,
                    x.behind
                );
            }
            println!("Свои коммиты против {}: {}", b.target, card.commit_count);
            for c in &card.commits {
                println!("  {} {}", c.short, c.subject);
            }
            println!("Файлы: {}", card.files.len());
            for f in &card.files {
                match (f.added, f.removed) {
                    (Some(a), Some(r)) => println!("  +{a:<5} -{r:<5} {}", f.path),
                    _ => println!("  бинарный     {}", f.path),
                }
            }
        }
        "mine" => {
            let base = match base {
                Some(b) => b,
                None => repo.default_base()?,
            };
            let m = repo.my_branches(&base, author.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&m)?);
                return Ok(());
            }
            println!("Ветки {} <{}>", m.author.name, m.author.email);
            for (title, list) in [("Требуют внимания", &m.attention), ("В работе", &m.active)]
            {
                if list.is_empty() {
                    continue;
                }
                println!("{title}:");
                for w in list {
                    println!(
                        "  {:<44} {:<14} ↑{} ↓{}  {} коммитов, {} файлов +{} -{}",
                        w.branch.name,
                        status_word(&w.branch.status),
                        w.branch.ahead,
                        w.branch.behind,
                        w.commit_count,
                        w.files,
                        w.added,
                        w.removed
                    );
                    for a in &w.attention {
                        println!("      ! {a:?}");
                    }
                }
            }
            if !m.merged.is_empty() {
                println!("Слиты, можно удалить:");
                for b in &m.merged {
                    println!("  {}", b.name);
                }
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
                println!(
                    "{:<10?} {}  ({} коммитов, не перенесено {})",
                    p.state,
                    p.master_branch,
                    p.commits.len(),
                    p.unsynced
                );
                for c in &p.commits {
                    let to = match (&c.ported, r.has_develop) {
                        (_, false) => String::new(),
                        (Some(x), true) => format!("→ {} в {}", x.short, x.place),
                        (None, true) => "✗ не перенесён".into(),
                    };
                    println!("    {} {:<50} {to}", c.commit.short, c.commit.subject);
                }
                if let Some(n) = &p.note {
                    println!("    ({n})");
                }
            }
            if !r.has_develop {
                println!("Ветки {} нет: хотфиксы переносить некуда", r.develop);
            }
            if let Some(d) = r.develop_stale_days {
                println!(
                    "{} отстаёт от {} на {d} дней: похоже, репозиторий живёт без {}",
                    r.develop, r.master, r.develop
                );
            }
            if !r.lost.is_empty() {
                println!(
                    "В {}, но не в {} (за {} дней):",
                    r.master, r.develop, r.recent_days
                );
                for c in &r.lost {
                    println!("    {} {} — {}", c.short, c.subject, c.author.name);
                }
            }
            eprintln!(
                "{} пар, старых скрыто {}, за {:.1?}",
                r.pairs.len(),
                r.older_hidden,
                started.elapsed()
            );
        }
        "where" => {
            let started = Instant::now();
            let s =
                repo.locate(&branch.ok_or("укажите, что искать: musubiko where QUERY [PATH]")?)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&s)?);
                return Ok(());
            }
            for r in &s.results {
                println!("{} {}  — {:?}", r.commit.short, r.commit.subject, r.verdict);
                let tag = r
                    .first_tag
                    .as_deref()
                    .map(|t| format!(" с {t}"))
                    .unwrap_or_default();
                println!("    {}: {:?}{tag}", s.master, r.master);
                if let (Some(d), Some(p)) = (&s.develop, &r.develop) {
                    println!("    {d}: {p:?}");
                }
                if !r.branches.is_empty() {
                    println!("    ветки: {}", r.branches.join(", "));
                }
                for c in &r.cherry_picks {
                    println!("    cherry-pick: {} {}", c.short, c.subject);
                }
            }
            let more = if s.truncated {
                " (показаны не все)"
            } else {
                ""
            };
            eprintln!(
                "{} коммитов{more} за {:.1?}",
                s.results.len(),
                started.elapsed()
            );
        }
        other => {
            return Err(format!(
                "неизвестная команда {other}; есть branches, diff, conflicts, hotfix, radar, where и info"
            )
            .into());
        }
    }
    Ok(())
}

fn status_word(s: &Status) -> String {
    match s {
        Status::Merged => "merged".to_string(),
        Status::Squashed => "squashed".to_string(),
        Status::Conflict { files } => format!("conflict ({})", files.len()),
        Status::Open => "open".to_string(),
        Status::Unknown { error } => format!("? {error}"),
    }
}
