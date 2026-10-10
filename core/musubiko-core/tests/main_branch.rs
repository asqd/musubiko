//! Repositories whose stable base is `main` instead of `master`, and ones that have both.

mod common;

use common::{Fixture, add_origin};
use musubiko_core::{After, HotfixInput, OpStatus, Operation, Repo};

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

/// `main` and `develop` on origin, no `master` anywhere.
fn main_fixture() -> Fixture {
    let f = Fixture::new();
    f.commit("README", "base\n", "init");
    f.git(&["branch", "-m", "master", "main"]);
    f.git(&["branch", "develop"]);
    add_origin(&f, &["main", "develop"]);
    f.git(&["checkout", "-q", "-b", "feature/work", "develop"]);
    f
}

/// A commit on `branch` dated in the future, so that branch has the newer tip.
fn newer_commit(f: &Fixture, branch: &str) {
    f.git(&["checkout", "-q", branch]);
    let out = std::process::Command::new(f.git.path())
        .args(["commit", "-q", "--allow-empty", "-m", "newer"])
        .current_dir(&f.root)
        .env("GIT_COMMITTER_DATE", "2030-01-01T00:00:00Z")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Both bases from one commit, so neither is newer.
fn both_fixture() -> Fixture {
    let f = Fixture::new();
    f.commit("README", "base\n", "init");
    f.git(&["branch", "main"]);
    f.git(&["branch", "develop"]);
    f
}

#[test]
fn hotfix_branches_from_main() {
    let f = main_fixture();
    let repo = f.repo();
    assert_eq!(repo.flow().master, "main");

    let input = HotfixInput {
        name: "LT-9 fix".into(),
        push: true,
        after: After::Stay,
    };
    let plan = repo.plan_hotfix(&input).unwrap();
    assert!(plan.problems.is_empty(), "{:?}", plan.problems);
    assert_eq!(
        (plan.master_from.as_str(), plan.dev_from.as_str()),
        ("origin/main", "origin/develop")
    );
    let mut op = repo.start_hotfix(&input).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);

    let branches = repo.branches("develop").unwrap();
    let target = |name: &str| {
        branches
            .iter()
            .find(|b| b.name == name)
            .map(|b| b.target.clone())
    };
    assert_eq!(target("hotfix/LT-9-fix").as_deref(), Some("main"));
    assert_eq!(target("hotfix/LT-9-fix-dev").as_deref(), Some("develop"));
    assert!(
        target("main").is_none(),
        "the base is not listed as a branch"
    );
}

#[test]
fn radar_watches_main() {
    let f = main_fixture();
    f.git(&["checkout", "-q", "main"]);
    f.commit("fix.txt", "1\n", "fix straight on main");
    f.git(&["push", "-q", "origin", "main"]);
    let radar = f.repo().hotfix_radar().unwrap();
    assert_eq!(radar.master, "main");
    assert!(
        radar
            .lost
            .iter()
            .any(|c| c.subject == "fix straight on main"),
        "{:?}",
        radar.lost
    );
}

#[test]
fn with_both_the_servers_default_branch_wins() {
    let f = both_fixture();
    add_origin(&f, &["master", "main", "develop"]);
    f.git(&["remote", "set-head", "origin", "main"]);
    assert_eq!(f.repo().flow().master, "main");

    // Even when main is newer, the default branch decides.
    newer_commit(&f, "main");
    f.git(&["push", "-q", "origin", "main"]);
    f.git(&["remote", "set-head", "origin", "master"]);
    assert_eq!(f.repo().flow().master, "master");
}

#[test]
fn with_both_the_one_on_the_server_wins() {
    // Moved to main on the server; the old master is only a local leftover.
    let f = both_fixture();
    add_origin(&f, &["main", "develop"]);
    assert_eq!(f.repo().flow().master, "main");
}

#[test]
fn with_both_and_no_other_clue_the_newer_wins() {
    let f = both_fixture();
    assert_eq!(f.repo().flow().master, "master", "a tie keeps master");

    newer_commit(&f, "main");
    assert_eq!(f.repo().flow().master, "main");
}
