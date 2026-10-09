//! Switching branches with uncommitted work, on real repositories.

mod common;

use common::{Fixture, with_origin};
use musubiko_core::{CheckoutInput, CheckoutMode, OpStatus, Operation, Repo};

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

fn input(name: &str, mode: CheckoutMode, restore: bool) -> CheckoutInput {
    CheckoutInput {
        name: name.into(),
        mode,
        restore_stash: restore,
    }
}

fn current(f: &Fixture) -> String {
    f.git(&["branch", "--show-current"]).trim().to_string()
}

#[test]
fn a_clean_switch_and_its_undo() {
    let f = with_origin();
    f.git(&["branch", "feature/x", "develop"]);
    let repo = f.repo();
    let mut op = repo
        .start_checkout(&input("feature/x", CheckoutMode::Switch, false))
        .unwrap();
    assert_eq!(op.steps.len(), 1, "nothing to stash");
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(current(&f), "feature/x");
    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(current(&f), "feature/work");
}

#[test]
fn stashes_on_the_way_out_and_brings_it_back_on_return() {
    let f = with_origin();
    f.git(&["branch", "feature/x", "develop"]);
    std::fs::write(f.root.join("README"), "mine\n").unwrap();
    std::fs::write(f.root.join("notes.md"), "draft\n").unwrap();
    let repo = f.repo();

    let mut away = repo
        .start_checkout(&input("feature/x", CheckoutMode::Switch, false))
        .unwrap();
    run(&mut away, &repo);
    assert_eq!(away.status, OpStatus::Done, "{:?}", away.states);
    assert_eq!(f.git(&["status", "--porcelain"]), "");

    let plan = repo
        .plan_checkout(&input("feature/work", CheckoutMode::Switch, true))
        .unwrap();
    assert!(
        plan.saved_stash
            .as_deref()
            .unwrap()
            .contains("feature/work перед feature/x"),
        "{plan:?}"
    );
    let mut back = repo
        .start_checkout(&input("feature/work", CheckoutMode::Switch, true))
        .unwrap();
    run(&mut back, &repo);
    assert_eq!(back.status, OpStatus::Done, "{:?}", back.states);
    assert_eq!(current(&f), "feature/work");
    assert_eq!(
        std::fs::read_to_string(f.root.join("README")).unwrap(),
        "mine\n"
    );
    assert!(f.root.join("notes.md").exists());
    assert_eq!(f.git(&["stash", "list"]), "", "the stash was popped");
}

#[test]
fn opens_a_worktree_and_leaves_this_folder_alone() {
    let f = with_origin();
    f.git(&["branch", "feature/x", "develop"]);
    std::fs::write(f.root.join("README"), "mine\n").unwrap();
    let repo = f.repo();
    let plan = repo
        .plan_checkout(&input("feature/x", CheckoutMode::Worktree, false))
        .unwrap();
    let mut op = repo
        .start_checkout(&input("feature/x", CheckoutMode::Worktree, false))
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    let wt = std::path::Path::new(&plan.worktree_path);
    assert_eq!(
        f.git_in(wt, &["branch", "--show-current"]).trim(),
        "feature/x"
    );
    assert_eq!(current(&f), "feature/work");
    assert_eq!(
        std::fs::read_to_string(f.root.join("README")).unwrap(),
        "mine\n"
    );

    // Now it is open there, so switching here is refused.
    let again = repo
        .plan_checkout(&input("feature/x", CheckoutMode::Switch, false))
        .unwrap();
    assert!(
        again.problems.iter().any(|p| p.contains("уже открыта")),
        "{again:?}"
    );
}

#[test]
fn a_server_only_branch_gets_a_tracking_copy() {
    let f = with_origin();
    f.git(&["push", "-q", "origin", "develop:refs/heads/feature/theirs"]);
    f.git(&["fetch", "-q", "origin"]);
    let repo = f.repo();
    let mut op = repo
        .start_checkout(&input("feature/theirs", CheckoutMode::Switch, false))
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(current(&f), "feature/theirs");
    assert_eq!(
        f.git(&["rev-parse", "--abbrev-ref", "feature/theirs@{upstream}"])
            .trim(),
        "origin/feature/theirs"
    );

    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.status, OpStatus::Undone, "{:?}", op.undo_error);
    assert_eq!(current(&f), "feature/work");
    assert!(!f.has_branch("feature/theirs"));
}

#[test]
fn the_current_branch_is_not_a_target() {
    let f = with_origin();
    let plan = f
        .repo()
        .plan_checkout(&input("feature/work", CheckoutMode::Switch, false))
        .unwrap();
    assert!(plan.problems.iter().any(|p| p.contains("уже на")));
}
