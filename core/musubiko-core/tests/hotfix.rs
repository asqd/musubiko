//! Creating hotfix pairs on real repositories, and undoing them safely.

mod common;

use common::with_origin;
use musubiko_core::{After, HotfixInput, OpStatus, Operation, Repo, StepState};

fn input(name: &str, after: After) -> HotfixInput {
    HotfixInput {
        name: name.into(),
        push: true,
        after,
    }
}

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

#[test]
fn creates_the_pair_pushes_it_and_opens_a_worktree() {
    let f = with_origin();
    let repo = f.repo();
    let plan = repo
        .plan_hotfix(&input("LT-7 fix voucher", After::Worktree))
        .unwrap();
    assert!(plan.problems.is_empty(), "{:?}", plan.problems);
    assert_eq!(
        (plan.master_branch.as_str(), plan.dev_branch.as_str()),
        ("hotfix/LT-7-fix-voucher", "hotfix/LT-7-fix-voucher-dev")
    );
    assert_eq!(
        (plan.master_from.as_str(), plan.dev_from.as_str()),
        ("origin/master", "origin/develop")
    );

    let mut op = repo
        .start_hotfix(&input("LT-7 fix voucher", After::Worktree))
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);

    assert_eq!(
        f.git(&["rev-parse", "hotfix/LT-7-fix-voucher"]),
        f.git(&["rev-parse", "origin/master"])
    );
    assert_eq!(
        f.git(&["rev-parse", "hotfix/LT-7-fix-voucher-dev"]),
        f.git(&["rev-parse", "origin/develop"])
    );
    let heads = f.remote_heads();
    assert!(
        heads.contains(&"hotfix/LT-7-fix-voucher".to_string())
            && heads.contains(&"hotfix/LT-7-fix-voucher-dev".to_string())
    );
    let wt = plan.worktree_path.unwrap();
    assert_eq!(
        f.git_in(std::path::Path::new(&wt), &["branch", "--show-current"])
            .trim(),
        "hotfix/LT-7-fix-voucher"
    );
    assert_eq!(
        f.git(&["branch", "--show-current"]).trim(),
        "feature/work",
        "the current folder stays put"
    );

    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.status, OpStatus::Undone, "{:?}", op.undo_error);
    assert!(
        !f.has_branch("hotfix/LT-7-fix-voucher") && !f.has_branch("hotfix/LT-7-fix-voucher-dev")
    );
    assert!(!f.remote_heads().iter().any(|h| h.starts_with("hotfix/")));
    assert!(!std::path::Path::new(&wt).exists());
}

#[test]
fn stashes_switches_and_puts_everything_back() {
    let f = with_origin();
    std::fs::write(f.root.join("README"), "edited\n").unwrap();
    std::fs::write(f.root.join("notes.md"), "untracked\n").unwrap();
    let repo = f.repo();
    let mut op = repo
        .start_hotfix(&HotfixInput {
            name: "LT-8".into(),
            push: false,
            after: After::Switch,
        })
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(f.git(&["branch", "--show-current"]).trim(), "hotfix/LT-8");
    assert_eq!(
        f.git(&["status", "--porcelain"]),
        "",
        "changes went to the stash"
    );
    assert!(
        f.remote_heads().iter().all(|h| !h.starts_with("hotfix/")),
        "push was off"
    );

    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.status, OpStatus::Undone, "{:?}", op.undo_error);
    assert_eq!(f.git(&["branch", "--show-current"]).trim(), "feature/work");
    assert_eq!(
        std::fs::read_to_string(f.root.join("README")).unwrap(),
        "edited\n"
    );
    assert!(f.root.join("notes.md").exists());
}

#[test]
fn undo_keeps_a_branch_that_gained_commits() {
    let f = with_origin();
    let repo = f.repo();
    let mut op = repo
        .start_hotfix(&HotfixInput {
            name: "LT-9".into(),
            push: false,
            after: After::Switch,
        })
        .unwrap();
    run(&mut op, &repo);
    f.commit("fix.txt", "fix\n", "the actual fix");

    op.undo_all(&repo, OpStatus::Undone);
    assert_ne!(op.status, OpStatus::Undone);
    assert!(
        op.undo_error.as_deref().unwrap().contains("новые коммиты"),
        "{:?}",
        op.undo_error
    );
    assert!(f.has_branch("hotfix/LT-9"));
}

#[test]
fn stops_when_the_name_is_taken_on_origin() {
    let f = with_origin();
    // Someone else pushed hotfix/LT-10; this clone has not fetched it yet.
    f.git(&["push", "-q", "origin", "develop:refs/heads/hotfix/LT-10"]);
    f.git(&["update-ref", "-d", "refs/remotes/origin/hotfix/LT-10"]);
    let repo = f.repo();
    let plan = repo.plan_hotfix(&input("LT-10", After::Stay)).unwrap();
    assert!(plan.problems.is_empty());

    let mut op = repo.start_hotfix(&input("LT-10", After::Stay)).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Failed);
    let failed = op
        .states
        .iter()
        .position(|s| matches!(s, StepState::Failed { .. }))
        .unwrap();
    assert!(op.steps[failed].args.contains(&"ls-remote".to_string()));
    assert!(!f.has_branch("hotfix/LT-10"), "nothing was created");
}

#[test]
fn plan_reports_names_that_already_exist() {
    let f = with_origin();
    f.git(&["branch", "hotfix/LT-11-dev"]);
    let plan = f.repo().plan_hotfix(&input("LT-11", After::Stay)).unwrap();
    assert_eq!(plan.problems, ["Ветка hotfix/LT-11-dev уже есть"]);
    assert!(f.repo().start_hotfix(&input("LT-11", After::Stay)).is_err());
}
