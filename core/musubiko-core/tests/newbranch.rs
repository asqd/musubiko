//! Creating work branches from a chosen base, on real repositories.

mod common;

use common::{Fixture, with_origin};
use musubiko_core::{After, NewBranchInput, OpStatus, Operation, Repo};

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

fn input(name: &str, base: &str, fresh: bool, push: bool, after: After) -> NewBranchInput {
    NewBranchInput {
        name: name.into(),
        base: base.into(),
        fresh,
        push,
        after,
    }
}

fn rev(f: &Fixture, r: &str) -> String {
    f.git(&["rev-parse", r]).trim().to_string()
}

/// origin/develop is one commit ahead of the local develop.
fn behind_fixture() -> Fixture {
    let f = with_origin();
    f.git(&["checkout", "-q", "develop"]);
    f.commit("new.txt", "1\n", "on the server");
    f.git(&["push", "-q", "origin", "develop"]);
    f.git(&["reset", "-q", "--hard", "HEAD~1"]);
    f.git(&["checkout", "-q", "feature/work"]);
    f
}

#[test]
fn from_the_fresh_server_copy_pushed_and_undone() {
    let f = behind_fixture();
    let repo = f.repo();
    let i = input(
        "feature/Project-12 новая форма",
        "develop",
        true,
        true,
        After::Stay,
    );
    let plan = repo.plan_new_branch(&i).unwrap();
    assert!(plan.problems.is_empty(), "{:?}", plan.problems);
    assert_eq!(plan.name, "feature/PROJECT-12-novaya-forma");
    assert_eq!(plan.from, "origin/develop");
    assert!(plan.can_fresh);
    assert!(
        plan.commands[0].starts_with("git fetch origin develop"),
        "{:#?}",
        plan.commands
    );

    let mut op = repo.start_new_branch(&i).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(
        rev(&f, "feature/PROJECT-12-novaya-forma"),
        rev(&f, "origin/develop")
    );
    assert!(
        f.remote_heads()
            .contains(&"feature/PROJECT-12-novaya-forma".to_string())
    );
    assert_eq!(
        f.git(&[
            "rev-parse",
            "--abbrev-ref",
            "feature/PROJECT-12-novaya-forma@{upstream}"
        ])
        .trim(),
        "origin/feature/PROJECT-12-novaya-forma",
        "tracks its own remote branch, not the base"
    );

    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.undo_error, None);
    assert!(!f.has_branch("feature/PROJECT-12-novaya-forma"));
    assert!(
        !f.remote_heads()
            .contains(&"feature/PROJECT-12-novaya-forma".to_string())
    );
}

#[test]
fn from_the_local_base_warns_when_it_is_behind() {
    let f = behind_fixture();
    let repo = f.repo();
    let i = input("feature/local", "develop", false, false, After::Stay);
    let plan = repo.plan_new_branch(&i).unwrap();
    assert_eq!(plan.from, "develop");
    assert!(
        plan.commands.iter().all(|c| !c.contains("fetch")),
        "{:#?}",
        plan.commands
    );
    assert!(
        plan.warnings.iter().any(|w| w.contains("отстаёт")),
        "{:?}",
        plan.warnings
    );
    let mut op = repo.start_new_branch(&i).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(rev(&f, "feature/local"), rev(&f, "develop"));
}

#[test]
fn warns_that_unpushed_base_commits_stay_behind() {
    let f = with_origin();
    f.commit("mine.txt", "1\n", "unpushed work");
    let plan = f
        .repo()
        .plan_new_branch(&input(
            "feature/next",
            "feature/work",
            true,
            false,
            After::Stay,
        ))
        .unwrap();
    // feature/work is local only: there is no server copy to be fresh from.
    assert_eq!(plan.from, "feature/work");
    assert!(!plan.can_fresh);

    f.git(&["push", "-q", "origin", "feature/work"]);
    f.git(&["fetch", "-q", "origin"]);
    f.commit("more.txt", "1\n", "more unpushed work");
    let plan = f
        .repo()
        .plan_new_branch(&input(
            "feature/next",
            "feature/work",
            true,
            false,
            After::Stay,
        ))
        .unwrap();
    assert_eq!(plan.from, "origin/feature/work");
    assert!(
        plan.warnings.iter().any(|w| w.contains("не запушено")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn refuses_taken_and_broken_names() {
    let f = with_origin();
    let repo = f.repo();
    let taken = repo
        .plan_new_branch(&input("feature/work", "develop", true, false, After::Stay))
        .unwrap();
    assert!(
        taken.problems.iter().any(|p| p.contains("уже есть")),
        "{:?}",
        taken.problems
    );
    let broken = repo
        .plan_new_branch(&input("feature/a..b", "develop", true, false, After::Stay))
        .unwrap();
    assert!(
        broken.problems.iter().any(|p| p.contains("не годится")),
        "{:?}",
        broken.problems
    );
    let no_base = repo
        .plan_new_branch(&input("feature/x", "nope", true, false, After::Stay))
        .unwrap();
    assert!(
        no_base
            .problems
            .iter()
            .any(|p| p.contains("Нет ветки nope")),
        "{:?}",
        no_base.problems
    );
    assert!(
        repo.start_new_branch(&input("feature/work", "develop", true, false, After::Stay))
            .is_err()
    );
}

#[test]
fn opens_in_a_worktree_or_switches_with_a_stash() {
    let f = with_origin();
    let repo = f.repo();
    let mut op = repo
        .start_new_branch(&input(
            "feature/side",
            "develop",
            true,
            false,
            After::Worktree,
        ))
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    let wt = f.root.with_file_name("work@feature-side");
    assert_eq!(
        f.git_in(&wt, &["branch", "--show-current"]).trim(),
        "feature/side"
    );

    std::fs::write(f.root.join("README"), "dirty\n").unwrap();
    let mut op = repo
        .start_new_branch(&input(
            "feature/here",
            "develop",
            true,
            false,
            After::Switch,
        ))
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(f.git(&["branch", "--show-current"]).trim(), "feature/here");
    assert_eq!(
        f.git(&["status", "--porcelain"]),
        "",
        "changes went to the stash"
    );
    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.undo_error, None);
    assert_eq!(f.git(&["branch", "--show-current"]).trim(), "feature/work");
    assert_eq!(
        std::fs::read_to_string(f.root.join("README")).unwrap(),
        "dirty\n"
    );
}
