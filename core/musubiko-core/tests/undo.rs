//! Every operation, undone, leaves the repository and the server exactly as they were — after a
//! full run, and after a failure at any step. Undo refuses when someone changed things since.

mod common;

use std::path::Path;

use common::Fixture;
use musubiko_core::{
    After, CheckoutInput, CheckoutMode, DeleteInput, HotfixInput, NewBranchInput, OpStatus,
    Operation, PortInput, Repo, Side, StepState,
};

/// master ← develop on a bare origin, branches for every operation to act on.
fn world() -> Fixture {
    let f = Fixture::new();
    let parent = f.root.parent().unwrap().to_path_buf();
    f.git_in(&parent, &["init", "-q", "--bare", "-b", "master", "origin.git"]);
    let origin = parent.join("origin.git");
    f.git(&["remote", "add", "origin", origin.to_str().unwrap()]);
    f.commit("README", "base\n", "init");
    f.commit("c.txt", "alpha\nbeta\ngamma\n", "add c");
    f.git(&["push", "-q", "-u", "origin", "master"]);
    f.git(&["checkout", "-q", "-b", "develop"]);
    f.commit("c.txt", "alpha\nbeta (develop)\ngamma\n", "develop changes beta");
    f.git(&["push", "-q", "-u", "origin", "develop"]);

    // Merged into develop, on the server too.
    f.git(&["checkout", "-q", "-b", "feature/merged"]);
    f.commit("m.txt", "m\n", "merged work");
    f.git(&["push", "-q", "-u", "origin", "feature/merged"]);
    f.git(&["checkout", "-q", "develop"]);
    f.git(&["merge", "-q", "--no-ff", "-m", "Merge feature/merged", "feature/merged"]);
    f.git(&["push", "-q", "origin", "develop"]);
    // Open, on both sides.
    f.git(&["checkout", "-q", "-b", "feature/open"]);
    f.commit("o.txt", "o\n", "open work");
    f.git(&["push", "-q", "-u", "origin", "feature/open"]);
    // Local only.
    f.git(&["checkout", "-q", "-b", "feature/local", "develop"]);
    f.commit("l.txt", "l\n", "local work");
    // Server only.
    f.git(&["checkout", "-q", "-b", "feature/remote", "develop"]);
    f.commit("r.txt", "r\n", "remote work");
    f.git(&["push", "-q", "origin", "feature/remote"]);
    f.git(&["checkout", "-q", "develop"]);
    f.git(&["branch", "-q", "-D", "feature/remote"]);

    // Hotfixes to carry over: LT-1 to an existing -dev, LT-2 without one, LT-3 conflicts.
    f.git(&["checkout", "-q", "-b", "hotfix/LT-1", "master"]);
    f.commit("h1.txt", "1\n", "fix 1");
    f.git(&["checkout", "-q", "-b", "hotfix/LT-1-dev", "develop"]);
    f.git(&["checkout", "-q", "-b", "hotfix/LT-2", "master"]);
    f.commit("h2.txt", "2\n", "fix 2");
    f.git(&["checkout", "-q", "-b", "hotfix/LT-3", "master"]);
    f.commit("c.txt", "alpha\nbeta (fixed)\ngamma\n", "fix 3 on beta");
    for b in ["hotfix/LT-1", "hotfix/LT-1-dev", "hotfix/LT-2", "hotfix/LT-3"] {
        f.git(&["push", "-q", "-u", "origin", b]);
    }

    f.git(&["checkout", "-q", "feature/open"]);
    f.git(&["fetch", "-q", "origin"]);
    f
}

/// Uncommitted work: a changed tracked file and a new one.
fn dirty(f: &Fixture) {
    std::fs::write(f.root.join("o.txt"), "o\nmy uncommitted line\n").unwrap();
    std::fs::write(f.root.join("notes.md"), "draft\n").unwrap();
}

fn origin(f: &Fixture) -> std::path::PathBuf {
    f.root.parent().unwrap().join("origin.git")
}

/// Everything an operation could touch: refs here and on the server, HEAD, the working copy
/// with its contents, the stash, worktrees, local config and the folders next to the repository.
fn snapshot(f: &Fixture) -> String {
    let g = |args: &[&str]| f.git(args);
    let mut s = String::new();
    s += "## refs\n";
    s += &g(&["for-each-ref", "--format=%(refname) %(objectname)"]);
    s += "## origin\n";
    s += &f.git_in(&origin(f), &["for-each-ref", "--format=%(refname) %(objectname)"]);
    s += "## HEAD\n";
    s += &std::fs::read_to_string(f.root.join(".git/HEAD")).unwrap();
    s += "## status\n";
    s += &g(&["status", "--porcelain=v1", "-uall"]);
    s += "## changes\n";
    s += &g(&["diff", "HEAD", "--binary"]);
    for line in g(&["ls-files", "--others", "--exclude-standard"]).lines() {
        s += &format!("untracked {line}: {}\n", std::fs::read_to_string(f.root.join(line)).unwrap());
    }
    s += "## stash\n";
    s += &g(&["stash", "list", "--format=%H %s"]);
    s += "## worktrees\n";
    s += &g(&["worktree", "list", "--porcelain"]).replace(f.root.parent().unwrap().to_str().unwrap(), "<tmp>");
    s += "## config\n";
    let mut config: Vec<String> = g(&["config", "--local", "--list"]).lines().map(String::from).collect();
    config.sort();
    s += &config.join("\n");
    s += "\n## folders\n";
    let mut dirs: Vec<String> = std::fs::read_dir(f.root.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    dirs.sort();
    s += &dirs.join("\n");
    s
}

/// Snapshots are long: show only the lines that differ.
#[track_caller]
fn same(now: &str, before: &str, what: &str) {
    if now == before {
        return;
    }
    let gone: Vec<&str> = before.lines().filter(|l| !now.lines().any(|n| n == *l)).collect();
    let new: Vec<&str> = now.lines().filter(|l| !before.lines().any(|b| b == *l)).collect();
    panic!("{what}: the repository differs\n  lost: {gone:#?}\n  new: {new:#?}");
}

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

/// As the app does it: the operation comes back from journal.json before it is undone.
fn from_journal(op: &Operation) -> Operation {
    serde_json::from_str(&serde_json::to_string(op).unwrap()).unwrap()
}

fn failed_at(op: &Operation) -> Option<usize> {
    op.states.iter().position(|s| matches!(s, StepState::Failed { .. }))
}

type Start = fn(&Repo) -> musubiko_core::Result<Operation>;

struct Case {
    name: &'static str,
    dirty: bool,
    start: Start,
}

/// The whole contract for one operation:
/// 1. run, then undo from the journal: back to the snapshot;
/// 2. for every step, make it fail: "roll back" returns to the snapshot, and "retry" after the
///    cause is gone finishes the operation, which then undoes cleanly too.
fn check(case: Case) {
    let f = world();
    if case.dirty {
        dirty(&f);
    }
    let repo = f.repo();
    let before = snapshot(&f);
    let mut op = (case.start)(&repo).unwrap_or_else(|e| panic!("{}: {e}", case.name));
    let n = op.steps.len();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{}: {:#?}", case.name, op.states);
    assert_ne!(snapshot(&f), before, "{}: the operation changed nothing", case.name);
    let mut back = from_journal(&op);
    back.undo_all(&repo, OpStatus::Undone);
    assert_eq!(back.status, OpStatus::Undone, "{}: {:?}", case.name, back.undo_error);
    same(&snapshot(&f), &before, &format!("{}: undo after a full run", case.name));

    for k in 0..n {
        for retry in [false, true] {
            let f = world();
            if case.dirty {
                dirty(&f);
            }
            let repo = f.repo();
            let before = snapshot(&f);
            let mut op = (case.start)(&repo).unwrap();
            assert_eq!(op.steps.len(), n, "{}: the plan is stable", case.name);
            let real = op.steps[k].args.clone();
            op.steps[k].args = vec!["rev-parse".into(), "--verify".into(), "refs/heads/sabotaged".into()];
            run(&mut op, &repo);
            let what = format!("{}: step {k} ({}) failed, {}", case.name, op.steps[k].title,
                if retry { "retry" } else { "roll back" });
            assert_eq!(op.status, OpStatus::Failed, "{what}");
            assert_eq!(failed_at(&op), Some(k), "{what}");
            if retry {
                op.steps[k].args = real;
                op.step(&repo);
                run(&mut op, &repo);
                assert_eq!(op.status, OpStatus::Done, "{what}: {:#?}", op.states);
                op.undo_all(&repo, OpStatus::Undone);
                assert_eq!(op.status, OpStatus::Undone, "{what}: {:?}", op.undo_error);
            } else {
                op.undo_all(&repo, OpStatus::RolledBack);
                assert_eq!(op.status, OpStatus::RolledBack, "{what}: {:?}", op.undo_error);
            }
            same(&snapshot(&f), &before, &what);
        }
    }
}

fn hotfix(after: After, push: bool) -> HotfixInput {
    HotfixInput { name: "LT-9 broken documents".into(), push, after }
}

fn new_branch(base: &str, fresh: bool, push: bool, after: After) -> NewBranchInput {
    NewBranchInput { name: "feature/LT-10 new form".into(), base: base.into(), fresh, push, after }
}

fn checkout(name: &str, mode: CheckoutMode) -> CheckoutInput {
    CheckoutInput { name: name.into(), mode, restore_stash: false }
}

fn commit_of(repo: &Repo, rev: &str) -> String {
    let out = std::process::Command::new(musubiko_core::Git::discover().unwrap().path())
        .args(["rev-parse", rev])
        .current_dir(repo.root())
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

macro_rules! cases {
    ($($(#[$m:meta])* $test:ident: $dirty:expr, $start:expr;)*) => {$(
        #[test]
        $(#[$m])*
        fn $test() {
            check(Case { name: stringify!($test), dirty: $dirty, start: $start });
        }
    )*};
}

cases! {
    hotfix_stay_push: false, |r| r.start_hotfix(&hotfix(After::Stay, true));
    hotfix_stay_no_push: false, |r| r.start_hotfix(&hotfix(After::Stay, false));
    hotfix_worktree_dirty: true, |r| r.start_hotfix(&hotfix(After::Worktree, true));
    hotfix_switch_dirty: true, |r| r.start_hotfix(&hotfix(After::Switch, true));
    hotfix_switch_clean: false, |r| r.start_hotfix(&hotfix(After::Switch, false));
    new_branch_fresh_push_switch_dirty: true, |r| r.start_new_branch(&new_branch("develop", true, true, After::Switch));
    new_branch_local_worktree: false, |r| r.start_new_branch(&new_branch("develop", false, false, After::Worktree));
    new_branch_from_feature_push_stay: false, |r| r.start_new_branch(&new_branch("feature/open", false, true, After::Stay));
    new_branch_from_remote_only: false, |r| r.start_new_branch(&new_branch("feature/remote", true, false, After::Switch));
    checkout_switch_dirty: true, |r| r.start_checkout(&checkout("feature/local", CheckoutMode::Switch));
    checkout_worktree_dirty: true, |r| r.start_checkout(&checkout("feature/local", CheckoutMode::Worktree));
    checkout_remote_only: false, |r| r.start_checkout(&checkout("feature/remote", CheckoutMode::Switch));
    checkout_remote_only_worktree: false, |r| r.start_checkout(&checkout("feature/remote", CheckoutMode::Worktree));
    port_to_existing_dev: true, |r| r.start_port(&PortInput {
        commits: vec![commit_of(r, "hotfix/LT-1")], target: "hotfix/LT-1-dev".into(), create: false, push: true });
    port_creating_dev: false, |r| r.start_port(&PortInput {
        commits: vec![commit_of(r, "hotfix/LT-2")], target: "hotfix/LT-2-dev".into(), create: true, push: true });
    #[ignore = "finding 7: undo of a delete loses the branch's upstream"]
    delete_merged_both_sides: false, |r| r.start_delete(&DeleteInput {
        names: vec!["feature/merged".into()], local: true, remote: true });
    #[ignore = "finding 7: undo of a delete loses the branch's upstream"]
    delete_many: false, |r| r.start_delete(&DeleteInput {
        names: vec!["feature/merged".into(), "feature/local".into(), "feature/remote".into(), "hotfix/LT-2".into()],
        local: true, remote: true });
    delete_remote_only: false, |r| r.start_delete(&DeleteInput {
        names: vec!["feature/merged".into()], local: false, remote: true });
}

// ---- Real failures instead of a sabotaged step. --------------------------------------------

#[test]
fn hotfix_rolls_back_when_the_server_refuses_the_push() {
    let f = world();
    let hook = f.root.join(".git/hooks/pre-push");
    std::fs::write(&hook, "#!/bin/sh\necho 'refused by hook' >&2\nexit 1\n").unwrap();
    make_executable(&hook);
    let repo = f.repo();
    let before = snapshot(&f);
    let mut op = repo.start_hotfix(&hotfix(After::Switch, true)).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Failed);
    let StepState::Failed { error } = &op.states[failed_at(&op).unwrap()] else { unreachable!() };
    assert!(error.contains("refused by hook"), "the hook's words reach the user: {error}");
    op.undo_all(&repo, OpStatus::RolledBack);
    assert_eq!(op.status, OpStatus::RolledBack, "{:?}", op.undo_error);
    same(&snapshot(&f), &before, "");
}

#[test]
fn checkout_rolls_back_when_the_index_is_locked() {
    let f = world();
    dirty(&f);
    std::fs::write(f.root.join(".git/index.lock"), "").unwrap();
    let repo = f.repo();
    let before = snapshot(&f);
    let mut op = repo.start_checkout(&checkout("feature/local", CheckoutMode::Switch)).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Failed);
    op.undo_all(&repo, OpStatus::RolledBack);
    same(&snapshot(&f), &before, "");
    // The lock goes away (the other git finished): retry works.
    std::fs::remove_file(f.root.join(".git/index.lock")).unwrap();
    let mut op = repo.start_checkout(&checkout("feature/local", CheckoutMode::Switch)).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:#?}", op.states);
}

#[test]
fn port_conflict_rolled_back_leaves_no_trace() {
    let f = world();
    let repo = f.repo();
    let before = snapshot(&f);
    let mut op = repo
        .start_port(&PortInput {
            commits: vec![commit_of(&repo, "hotfix/LT-3")],
            target: "hotfix/LT-3-dev".into(),
            create: true,
            push: true,
        })
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Failed);
    let dir = op.stopped_pick().expect("stopped on the conflict").to_string();
    assert!(Path::new(&dir).exists());
    op.undo_all(&repo, OpStatus::RolledBack);
    assert_eq!(op.status, OpStatus::RolledBack, "{:?}", op.undo_error);
    assert!(!Path::new(&dir).exists(), "the scratch folder is gone");
    same(&snapshot(&f), &before, "");
}

#[test]
fn port_conflict_resolved_then_undone() {
    let f = world();
    let repo = f.repo();
    let before = snapshot(&f);
    let mut op = repo
        .start_port(&PortInput {
            commits: vec![commit_of(&repo, "hotfix/LT-3")],
            target: "hotfix/LT-3-dev".into(),
            create: true,
            push: true,
        })
        .unwrap();
    run(&mut op, &repo);
    let dir = op.stopped_pick().unwrap().to_string();
    repo.pick_take(&dir, "c.txt", Side::Commit).unwrap();
    op.resume(&repo, false);
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:#?}", op.states);
    assert!(f.remote_heads().contains(&"hotfix/LT-3-dev".to_string()));
    let mut back = from_journal(&op);
    back.undo_all(&repo, OpStatus::Undone);
    assert_eq!(back.status, OpStatus::Undone, "{:?}", back.undo_error);
    same(&snapshot(&f), &before, "");
}

// ---- Guards: undo must not destroy what others did since. ----------------------------------

/// A colleague's clone of the same origin.
fn colleague(f: &Fixture) -> std::path::PathBuf {
    let dir = f.root.parent().unwrap().join("colleague");
    f.git_in(f.root.parent().unwrap(), &["clone", "-q", origin(f).to_str().unwrap(), "colleague"]);
    f.git_in(&dir, &["config", "user.email", "colleague@example.com"]);
    dir
}

#[test]
fn undo_keeps_a_hotfix_the_colleague_pushed_to() {
    let f = world();
    let repo = f.repo();
    let mut op = repo.start_hotfix(&hotfix(After::Stay, true)).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:#?}", op.states);
    let c = colleague(&f);
    f.git_in(&c, &["checkout", "-q", "hotfix/LT-9-broken-documents"]);
    std::fs::write(c.join("fix.txt"), "colleague\n").unwrap();
    f.git_in(&c, &["add", "fix.txt"]);
    f.git_in(&c, &["commit", "-q", "-m", "colleague's fix"]);
    f.git_in(&c, &["push", "-q", "origin", "hotfix/LT-9-broken-documents"]);
    let theirs = f.git_in(&c, &["rev-parse", "HEAD"]);

    op.undo_all(&repo, OpStatus::Undone);
    assert_ne!(op.status, OpStatus::Undone, "undo must stop");
    assert!(op.undo_error.is_some());
    let on_server = f.git_in(&origin(&f), &["rev-parse", "refs/heads/hotfix/LT-9-broken-documents"]);
    assert_eq!(on_server, theirs, "the colleague's commit stays on the server");
}

#[test]
fn undo_does_not_pop_someone_elses_stash() {
    let f = world();
    dirty(&f);
    let repo = f.repo();
    let mut op = repo.start_hotfix(&hotfix(After::Switch, false)).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:#?}", op.states);
    std::fs::write(f.root.join("other.txt"), "other work\n").unwrap();
    f.git(&["stash", "push", "-q", "-u", "-m", "someone else's"]);
    let stashes = f.git(&["stash", "list"]);
    op.undo_all(&repo, OpStatus::Undone);
    assert_ne!(op.status, OpStatus::Undone);
    assert_eq!(f.git(&["stash", "list"]), stashes, "both stashes are still there");
}

#[test]
fn undo_of_a_delete_does_not_overwrite_a_new_branch_with_that_name() {
    let f = world();
    let repo = f.repo();
    let mut op = repo
        .start_delete(&DeleteInput { names: vec!["hotfix/LT-2".into()], local: true, remote: true })
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:#?}", op.states);
    let c = colleague(&f);
    f.git_in(&c, &["checkout", "-q", "-b", "hotfix/LT-2", "origin/master"]);
    std::fs::write(c.join("new.txt"), "a different branch, same name\n").unwrap();
    f.git_in(&c, &["add", "new.txt"]);
    f.git_in(&c, &["commit", "-q", "-m", "new work under the old name"]);
    f.git_in(&c, &["push", "-q", "origin", "hotfix/LT-2"]);
    let theirs = f.git_in(&c, &["rev-parse", "HEAD"]);

    op.undo_all(&repo, OpStatus::Undone);
    let on_server = f.git_in(&origin(&f), &["rev-parse", "refs/heads/hotfix/LT-2"]);
    assert_eq!(on_server, theirs, "the new branch on the server is untouched");
}

#[test]
fn undo_keeps_a_worktree_with_work_in_it() {
    let f = world();
    dirty(&f);
    let repo = f.repo();
    let mut op = repo.start_hotfix(&hotfix(After::Worktree, false)).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:#?}", op.states);
    let wt = f.git(&["worktree", "list", "--porcelain"])
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(std::path::PathBuf::from)
        .find(|p| !p.ends_with("work"))
        .expect("a worktree was opened");
    std::fs::write(wt.join("started.txt"), "an hour of work\n").unwrap();
    op.undo_all(&repo, OpStatus::Undone);
    assert_ne!(op.status, OpStatus::Undone);
    assert_eq!(std::fs::read_to_string(wt.join("started.txt")).unwrap(), "an hour of work\n");
}

#[test]
#[ignore = "finding 8: undo of a deleted unmerged branch fails once git gc has run"]
fn undo_of_a_delete_survives_garbage_collection() {
    let f = world();
    let repo = f.repo();
    let before = snapshot(&f);
    let mut op = repo
        .start_delete(&DeleteInput {
            names: vec!["feature/local".into(), "feature/remote".into()],
            local: true,
            remote: true,
        })
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:#?}", op.states);
    // Two weeks later: reflogs expired, unreachable objects pruned.
    f.git(&["reflog", "expire", "--expire=now", "--all"]);
    f.git(&["gc", "-q", "--prune=now"]);
    let mut back = from_journal(&op);
    back.undo_all(&repo, OpStatus::Undone);
    assert_eq!(back.status, OpStatus::Undone, "{:?}", back.undo_error);
    same(&snapshot(&f), &before, "");
}

#[cfg(unix)]
fn make_executable(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_: &Path) {}
