//! Finishing a carried-over hotfix by hand after a cherry-pick conflict.

mod common;

use common::{Fixture, with_origin};
use musubiko_core::{OpStatus, Operation, PortInput, Repo, Side};

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

/// develop and the hotfix both add c.txt differently; the hotfix also adds d.txt in a second commit.
fn conflicting() -> (Fixture, Vec<String>) {
    let f = with_origin();
    f.git(&["checkout", "-q", "develop"]);
    f.commit("c.txt", "develop\n", "c on develop");
    f.git(&["push", "-q", "origin", "develop"]);
    f.git(&["fetch", "-q", "origin"]);
    f.git(&["checkout", "-q", "-b", "hotfix/LT-5", "master"]);
    f.commit("c.txt", "master\n", "c on hotfix");
    let first = f.git(&["rev-parse", "HEAD"]).trim().to_string();
    f.commit("d.txt", "d\n", "d on hotfix");
    let second = f.git(&["rev-parse", "HEAD"]).trim().to_string();
    f.git(&["checkout", "-q", "feature/work"]);
    (f, vec![first, second])
}

fn start(f: &Fixture, commits: Vec<String>) -> (Repo, Operation) {
    let repo = f.repo();
    let mut op = repo
        .start_port(&PortInput {
            commits,
            target: "hotfix/LT-5-dev".into(),
            create: true,
            push: false,
        })
        .unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Failed);
    (repo, op)
}

fn show(f: &Fixture, path: &str) -> String {
    f.git(&["show", &format!("hotfix/LT-5-dev:{path}")])
}

#[test]
fn resolving_by_hand_and_continuing_finishes_the_port() {
    let (f, commits) = conflicting();
    let (repo, mut op) = start(&f, commits);
    let dir = op
        .stopped_pick()
        .expect("stopped in a cherry-pick")
        .to_string();
    assert!(dir.ends_with("work@port-hotfix-LT-5-dev"), "{dir}");

    let state = repo.pick_state(&dir).unwrap();
    assert_eq!(state.current.as_ref().unwrap().subject, "c on hotfix");
    assert_eq!(state.remaining, 1);
    assert_eq!(
        (state.files[0].path.as_str(), state.files[0].resolved),
        ("c.txt", false)
    );

    // Continuing with markers still in the file is refused.
    op.resume(&repo, false);
    assert_eq!(op.status, OpStatus::Failed);
    assert!(format!("{:?}", op.states).contains("маркеры"));

    std::fs::write(std::path::Path::new(&dir).join("c.txt"), "both\n").unwrap();
    assert!(repo.pick_state(&dir).unwrap().files[0].resolved);
    op.resume(&repo, false);
    assert_eq!(op.status, OpStatus::Running, "{:?}", op.states);
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);

    assert_eq!(show(&f, "c.txt"), "both\n");
    assert_eq!(show(&f, "d.txt"), "d\n");
    assert!(
        !std::path::Path::new(&dir).exists(),
        "the scratch folder is gone"
    );
}

#[test]
fn taking_the_commit_side_resolves_the_file() {
    let (f, commits) = conflicting();
    let (repo, mut op) = start(&f, commits);
    let dir = op.stopped_pick().unwrap().to_string();
    let state = repo.pick_take(&dir, "c.txt", Side::Commit).unwrap();
    assert!(
        state.files.is_empty(),
        "staged, so no longer unmerged: {state:?}"
    );
    op.resume(&repo, false);
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(show(&f, "c.txt"), "master\n");
}

#[test]
fn skipping_the_conflicting_commit_keeps_the_rest() {
    let (f, commits) = conflicting();
    let (repo, mut op) = start(&f, commits);
    op.resume(&repo, true);
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(show(&f, "c.txt"), "develop\n");
    assert_eq!(show(&f, "d.txt"), "d\n");
}

#[test]
fn rolling_back_mid_conflict_leaves_nothing_behind() {
    let (f, commits) = conflicting();
    let (repo, mut op) = start(&f, commits);
    let dir = op.stopped_pick().unwrap().to_string();
    op.undo_all(&repo, OpStatus::RolledBack);
    assert_eq!(op.status, OpStatus::RolledBack, "{:?}", op.undo_error);
    assert!(!f.has_branch("hotfix/LT-5-dev"));
    assert!(!std::path::Path::new(&dir).exists());
}
