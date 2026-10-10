//! Small projects with master (or main) only: a hotfix is one branch and nothing is carried over.

mod common;

use common::{Fixture, add_origin};
use musubiko_core::{After, HotfixInput, OpStatus, Operation, PairState, Repo};

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

/// master on origin, no develop anywhere.
fn master_only() -> Fixture {
    let f = Fixture::new();
    f.commit("README", "base\n", "init");
    add_origin(&f, &["master"]);
    f
}

fn input(name: &str) -> HotfixInput {
    HotfixInput {
        name: name.into(),
        push: true,
        after: After::Stay,
    }
}

#[test]
fn a_hotfix_is_one_branch_with_a_warning() {
    let f = master_only();
    let repo = f.repo();
    let plan = repo.plan_hotfix(&input("LT-5 fix")).unwrap();
    assert!(plan.problems.is_empty(), "{:?}", plan.problems);
    assert!(!plan.with_dev);
    assert_eq!(plan.warnings.len(), 1, "{:?}", plan.warnings);
    assert!(plan.warnings[0].contains("develop"), "{:?}", plan.warnings);
    assert!(
        plan.commands.iter().all(|c| !c.contains("-dev")),
        "{:#?}",
        plan.commands
    );

    let mut op = repo.start_hotfix(&input("LT-5 fix")).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(f.remote_heads(), vec!["hotfix/LT-5-fix", "master"]);
    assert!(!f.has_branch("hotfix/LT-5-fix-dev"));

    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.undo_error, None);
    assert_eq!(f.remote_heads(), vec!["master"]);
    assert!(!f.has_branch("hotfix/LT-5-fix"));
}

#[test]
fn the_radar_only_asks_whether_the_fix_reached_master() {
    let f = master_only();
    f.git(&["checkout", "-q", "-b", "hotfix/LT-6", "master"]);
    f.commit("fix.txt", "1\n", "fix 6");
    f.git(&["checkout", "-q", "master"]);
    f.commit("direct.txt", "1\n", "straight on master");

    let radar = f.repo().hotfix_radar().unwrap();
    assert!(!radar.has_develop);
    assert!(radar.lost.is_empty(), "{:?}", radar.lost);
    let pair = &radar.pairs[0];
    assert_eq!(pair.state, PairState::Work);
    assert_eq!(pair.unsynced, 0);

    // Merged is judged by origin/master, what the server has.
    f.git(&["merge", "-q", "--no-ff", "-m", "merge LT-6", "hotfix/LT-6"]);
    f.git(&["push", "-q", "origin", "master"]);
    let radar = f.repo().hotfix_radar().unwrap();
    assert_eq!(radar.pairs[0].state, PairState::Done);
}
