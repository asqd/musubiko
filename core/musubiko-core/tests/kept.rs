//! Refs that keep deleted branches for undo exist only while they are needed.

mod common;

use common::{Fixture, with_origin};
use musubiko_core::{DeleteInput, Journal, OpStatus, Operation, Repo};
use tempfile::TempDir;

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

fn kept(f: &Fixture) -> Vec<String> {
    f.git(&[
        "for-each-ref",
        "--format=%(refname)",
        "refs/musubiko/deleted",
    ])
    .lines()
    .map(String::from)
    .collect()
}

/// develop on origin; feature/merged is in it, feature/a and feature/b are not.
fn fixture() -> Fixture {
    let f = with_origin();
    f.git(&["checkout", "-q", "-b", "feature/merged", "develop"]);
    f.commit("m.txt", "m\n", "merged work");
    f.git(&["checkout", "-q", "develop"]);
    f.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge feature/merged",
        "feature/merged",
    ]);
    f.git(&["push", "-q", "origin", "develop"]);
    for b in ["feature/a", "feature/b"] {
        f.git(&["checkout", "-q", "-b", b, "develop"]);
        f.commit(&format!("{}.txt", &b[8..]), "work\n", &format!("{b} work"));
    }
    f.git(&["checkout", "-q", "feature/work"]);
    f.git(&["fetch", "-q", "origin"]);
    f
}

fn delete(repo: &Repo, names: &[&str]) -> Operation {
    let mut op = repo
        .start_delete(&DeleteInput {
            names: names.iter().map(|n| n.to_string()).collect(),
            local: true,
            remote: true,
        })
        .unwrap();
    run(&mut op, repo);
    assert_eq!(op.status, OpStatus::Done, "{:#?}", op.states);
    op
}

#[test]
fn only_branches_outside_the_bases_are_kept() {
    let f = fixture();
    let repo = f.repo();
    let plan = repo
        .plan_delete(&DeleteInput {
            names: vec!["feature/merged".into()],
            local: true,
            remote: false,
        })
        .unwrap();
    assert!(
        plan.steps.iter().all(|s| s.args[0] != "update-ref"),
        "a merged branch needs nothing kept: {:?}",
        plan.commands
    );

    let op = delete(&repo, &["feature/merged", "feature/a"]);
    let refs = kept(&f);
    assert_eq!(refs.len(), 1, "{refs:?}");
    assert!(refs[0].ends_with("/local/feature/a"));
    assert_eq!(op.kept_refs().len(), 1);
}

#[test]
fn a_kept_branch_that_reaches_develop_is_let_go_and_still_undoes() {
    let f = fixture();
    let repo = f.repo();
    let a = f.git(&["rev-parse", "feature/a"]).trim().to_string();
    let op = delete(&repo, &["feature/a", "feature/b"]);
    assert_eq!(kept(&f).len(), 2);
    assert_eq!(repo.prune_kept().unwrap(), 0, "nothing reached a base yet");

    // Someone merges feature/a's commit into develop on the server.
    f.git(&["checkout", "-q", "develop"]);
    f.git(&["merge", "-q", "--no-ff", "-m", "Merge a", &a]);
    f.git(&["push", "-q", "origin", "develop"]);
    f.git(&["checkout", "-q", "feature/work"]);
    f.git(&["fetch", "-q", "origin"]);

    assert_eq!(repo.prune_kept().unwrap(), 1);
    let left = kept(&f);
    assert_eq!(left.len(), 1);
    assert!(left[0].ends_with("/local/feature/b"), "{left:?}");

    // Undo still works: a's commit is reachable from develop.
    f.git(&["reflog", "expire", "--expire=now", "--all"]);
    f.git(&["gc", "-q", "--prune=now"]);
    let mut back = op.clone();
    back.undo_all(&repo, OpStatus::Undone);
    assert_eq!(back.status, OpStatus::Undone, "{:?}", back.undo_error);
    assert_eq!(f.git(&["rev-parse", "feature/a"]).trim(), a);
    assert!(
        kept(&f).is_empty(),
        "undo removed b's ref, a's was already gone"
    );
}

#[test]
fn an_operation_pushed_out_of_the_journal_releases_its_refs() {
    let f = fixture();
    let repo = f.repo();
    let dir = TempDir::new().unwrap();
    let journal = Journal::with_limit(dir.path().join("journal.json"), 1);
    let op = delete(&repo, &["feature/a"]);
    assert!(journal.upsert(&op).unwrap().dropped.is_empty());
    assert_eq!(kept(&f).len(), 1);

    let next = delete(&repo, &["feature/b"]);
    let saved = journal.upsert(&next).unwrap();
    assert_eq!(saved.dropped.len(), 1);
    assert_eq!(saved.dropped[0].id, op.id);
    repo.release_kept(&saved.dropped[0].kept_refs()).unwrap();
    let left = kept(&f);
    assert_eq!(
        left.len(),
        1,
        "only the operation still in the journal keeps: {left:?}"
    );
    assert!(left[0].ends_with("/local/feature/b"));
}
