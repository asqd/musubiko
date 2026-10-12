//! Keeping the branch list clean: gone upstreams, fetching with prune, bulk deletion.

mod common;

use common::with_origin;
use musubiko_core::{DeleteInput, OpStatus, Operation, Repo};

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

#[test]
fn fetch_prune_reveals_branches_deleted_on_the_server() {
    let f = with_origin();
    f.git(&["checkout", "-q", "-b", "feature/merged-mr", "develop"]);
    f.commit("x.txt", "x\n", "work");
    f.git(&["push", "-q", "-u", "origin", "feature/merged-mr"]);
    f.git(&["checkout", "-q", "feature/work"]);
    // The merge request was merged and the branch removed on the server, by someone else.
    let origin = f.root.parent().unwrap().join("origin.git");
    f.git_in(&origin, &["branch", "-q", "-D", "feature/merged-mr"]);
    let repo = f.repo();
    let before = repo.branches("develop").unwrap();
    let b = before
        .iter()
        .find(|b| b.name == "feature/merged-mr")
        .unwrap();
    assert!(
        !b.upstream_gone,
        "still has origin/feature/merged-mr until a prune"
    );

    let mut op = repo.start_fetch().unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert!(op.undo.is_empty(), "fetching has nothing to undo");

    let after = repo.branches("develop").unwrap();
    let b = after
        .iter()
        .find(|b| b.name == "feature/merged-mr")
        .unwrap();
    assert!(b.upstream_gone);
    assert!(repo.info().unwrap().last_fetch.is_some());
}

#[test]
fn deletes_many_branches_and_restores_them_all() {
    let f = with_origin();
    for n in ["feature/a", "feature/b", "feature/c"] {
        f.git(&["branch", n, "develop"]);
    }
    f.git(&["push", "-q", "origin", "feature/a", "feature/b"]);
    f.git(&["fetch", "-q", "origin"]);
    let before = f.git(&["rev-parse", "feature/a", "feature/b", "feature/c"]);
    let repo = f.repo();
    let input = DeleteInput {
        names: vec!["feature/a".into(), "feature/b".into(), "feature/c".into()],
        local: true,
        remote: true,
    };
    let plan = repo.plan_delete(&input).unwrap();
    assert!(plan.problems.is_empty(), "{plan:?}");
    assert_eq!(
        plan.steps.len(),
        5,
        "keep the tips, three local deletions and one push"
    );

    let mut op = repo.start_delete(&input).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert!(!f.has_branch("feature/a") && !f.has_branch("feature/c"));
    assert!(
        !f.remote_heads()
            .iter()
            .any(|h| h.starts_with("feature/a") || h.starts_with("feature/b"))
    );

    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.status, OpStatus::Undone, "{:?}", op.undo_error);
    assert_eq!(
        f.git(&["rev-parse", "feature/a", "feature/b", "feature/c"]),
        before
    );
    let heads = f.remote_heads();
    assert!(heads.contains(&"feature/a".to_string()) && heads.contains(&"feature/b".to_string()));
}

#[test]
fn a_branch_moved_on_the_server_stops_the_whole_push() {
    let f = with_origin();
    f.git(&["branch", "feature/a", "develop"]);
    f.git(&["push", "-q", "origin", "feature/a"]);
    f.git(&["fetch", "-q", "origin"]);
    let repo = f.repo();
    let input = DeleteInput {
        names: vec!["feature/a".into()],
        local: false,
        remote: true,
    };
    let mut op = repo.start_delete(&input).unwrap();
    // Someone pushes to feature/a after the plan was made.
    f.git(&["checkout", "-q", "-b", "elsewhere", "develop"]);
    f.commit("new.txt", "new\n", "their commit");
    f.git(&["push", "-q", "origin", "elsewhere:refs/heads/feature/a"]);
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Failed);
    assert!(
        f.remote_heads().contains(&"feature/a".to_string()),
        "lease kept their work"
    );
}

#[test]
fn deleting_on_the_server_only_keeps_the_local_branch() {
    let f = with_origin();
    f.git(&["branch", "feature/a", "develop"]);
    f.git(&["push", "-q", "origin", "feature/a"]);
    f.git(&["fetch", "-q", "origin"]);
    let repo = f.repo();
    let mut op = repo
        .start_delete(&DeleteInput {
            names: vec!["feature/a".into()],
            local: false,
            remote: true,
        })
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert!(f.has_branch("feature/a"));
    assert!(!f.remote_heads().contains(&"feature/a".to_string()));
}

#[test]
fn a_quiet_fetch_updates_without_an_operation() {
    let f = with_origin();
    let origin = f.root.parent().unwrap().join("origin.git");
    // Someone else pushes a branch.
    f.git_in(&origin, &["branch", "feature/theirs", "develop"]);
    let repo = f.repo();
    assert!(
        !repo
            .branches("develop")
            .unwrap()
            .iter()
            .any(|b| b.name == "feature/theirs")
    );
    repo.fetch_quietly().unwrap();
    assert!(
        repo.branches("develop")
            .unwrap()
            .iter()
            .any(|b| b.name == "feature/theirs")
    );
    assert!(repo.info().unwrap().last_fetch.is_some());
}
