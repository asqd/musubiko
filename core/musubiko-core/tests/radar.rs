//! The hotfix radar, carrying commits over and deleting branches, on real repositories.

mod common;

use common::{Fixture, with_origin};
use musubiko_core::{DeleteInput, OpStatus, Operation, PairState, PortInput, Repo, StepState};

fn run(op: &mut Operation, repo: &Repo) {
    while op.status == OpStatus::Running {
        op.step(repo);
    }
}

fn head(f: &Fixture) -> String {
    f.git(&["rev-parse", "HEAD"]).trim().to_string()
}

/// Four hotfixes in different states plus a commit made straight on master.
fn radar_fixture() -> Fixture {
    let f = with_origin();
    // LT-1: carried over to -dev, nothing merged.
    f.git(&["checkout", "-q", "-b", "hotfix/LT-1", "master"]);
    f.commit("h1.txt", "1\n", "fix 1");
    let c1 = head(&f);
    f.git(&["checkout", "-q", "-b", "hotfix/LT-1-dev", "develop"]);
    f.git(&["cherry-pick", "-x", &c1]);
    // LT-2: two commits, only the first carried over.
    f.git(&["checkout", "-q", "-b", "hotfix/LT-2", "master"]);
    f.commit("h2.txt", "a\n", "fix 2a");
    let a = head(&f);
    f.commit("h2b.txt", "b\n", "fix 2b");
    f.git(&["checkout", "-q", "-b", "hotfix/LT-2-dev", "develop"]);
    f.git(&["cherry-pick", "-x", &a]);
    // LT-3: merged into master, never carried over.
    f.git(&["checkout", "-q", "-b", "hotfix/LT-3", "master"]);
    f.commit("h3.txt", "3\n", "fix 3");
    f.git(&["checkout", "-q", "master"]);
    f.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge hotfix/LT-3",
        "hotfix/LT-3",
    ]);
    // A fix committed straight on master.
    f.commit("m.txt", "m\n", "manual fix");
    // LT-4: merged on both sides.
    f.git(&["checkout", "-q", "-b", "hotfix/LT-4", "master"]);
    f.commit("h4.txt", "4\n", "fix 4");
    let d = head(&f);
    f.git(&["checkout", "-q", "master"]);
    f.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge hotfix/LT-4",
        "hotfix/LT-4",
    ]);
    f.git(&["checkout", "-q", "-b", "hotfix/LT-4-dev", "develop"]);
    f.git(&["cherry-pick", "-x", &d]);
    f.git(&["checkout", "-q", "develop"]);
    f.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge hotfix/LT-4-dev",
        "hotfix/LT-4-dev",
    ]);
    f.git(&["push", "-q", "origin", "master", "develop"]);
    f.git(&["fetch", "-q", "origin"]);
    f.git(&["checkout", "-q", "feature/work"]);
    f
}

#[test]
fn sorts_hotfixes_by_what_is_carried_over() {
    let f = radar_fixture();
    let radar = f.repo().hotfix_radar().unwrap();
    let pair = |slug: &str| {
        radar
            .pairs
            .iter()
            .find(|p| p.slug == slug)
            .unwrap_or_else(|| panic!("no pair {slug}"))
    };

    let p1 = pair("LT-1");
    assert_eq!(p1.state, PairState::Work);
    assert_eq!(
        p1.commits[0].ported.as_ref().unwrap().place,
        "hotfix/LT-1-dev"
    );

    let p2 = pair("LT-2");
    assert_eq!((p2.state, p2.unsynced), (PairState::Unsynced, 1));
    assert!(p2.commits[0].ported.is_some() && p2.commits[1].ported.is_none());

    let p3 = pair("LT-3");
    assert_eq!(p3.state, PairState::Lost);
    assert_eq!(
        p3.commits
            .iter()
            .map(|c| c.commit.subject.as_str())
            .collect::<Vec<_>>(),
        ["fix 3"]
    );

    assert_eq!(pair("LT-4").state, PairState::Done);

    let lost: Vec<&str> = radar.lost.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(lost, ["manual fix"]);
}

#[test]
fn carries_missing_commits_over_and_undoes_it() {
    let f = radar_fixture();
    let repo = f.repo();
    let before = f.git(&["rev-parse", "hotfix/LT-2-dev"]);
    let missing = f.git(&["rev-parse", "hotfix/LT-2"]).trim().to_string();
    let input = PortInput {
        commits: vec![missing],
        target: "hotfix/LT-2-dev".into(),
        create: false,
        push: true,
    };
    let mut op = repo.start_port(&input).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert_eq!(
        f.git(&["branch", "--show-current"]).trim(),
        "feature/work",
        "the user's folder stays put"
    );

    let radar = repo.hotfix_radar().unwrap();
    assert_eq!(
        radar.pairs.iter().find(|p| p.slug == "LT-2").unwrap().state,
        PairState::Work
    );

    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.status, OpStatus::Undone, "{:?}", op.undo_error);
    assert_eq!(f.git(&["rev-parse", "hotfix/LT-2-dev"]), before);
}

#[test]
fn creates_the_missing_dev_half_and_pushes_it() {
    let f = radar_fixture();
    let repo = f.repo();
    let fix = f.git(&["rev-parse", "hotfix/LT-3"]).trim().to_string();
    let input = PortInput {
        commits: vec![fix],
        target: "hotfix/LT-3-dev".into(),
        create: true,
        push: true,
    };
    let mut op = repo.start_port(&input).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert!(f.remote_heads().contains(&"hotfix/LT-3-dev".to_string()));
    let radar = repo.hotfix_radar().unwrap();
    assert_eq!(
        radar.pairs.iter().find(|p| p.slug == "LT-3").unwrap().state,
        PairState::Released
    );

    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.status, OpStatus::Undone, "{:?}", op.undo_error);
    assert!(!f.has_branch("hotfix/LT-3-dev"));
    assert!(!f.remote_heads().contains(&"hotfix/LT-3-dev".to_string()));
}

#[test]
fn a_conflicting_pick_rolls_back_cleanly() {
    let f = with_origin();
    f.git(&["checkout", "-q", "develop"]);
    f.commit("c.txt", "develop\n", "c on develop");
    f.git(&["push", "-q", "origin", "develop"]);
    f.git(&["fetch", "-q", "origin"]);
    f.git(&["checkout", "-q", "-b", "hotfix/LT-5", "master"]);
    f.commit("c.txt", "master\n", "c on hotfix");
    let fix = head(&f);
    f.git(&["checkout", "-q", "feature/work"]);

    let repo = f.repo();
    let input = PortInput {
        commits: vec![fix],
        target: "hotfix/LT-5-dev".into(),
        create: true,
        push: false,
    };
    let mut op = repo.start_port(&input).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Failed);
    let failed = op
        .states
        .iter()
        .position(|s| matches!(s, StepState::Failed { .. }))
        .unwrap();
    assert!(op.steps[failed].args.contains(&"cherry-pick".to_string()));

    op.undo_all(&repo, OpStatus::RolledBack);
    assert_eq!(op.status, OpStatus::RolledBack, "{:?}", op.undo_error);
    assert!(!f.has_branch("hotfix/LT-5-dev"));
    assert_eq!(
        f.git(&["worktree", "list"]).lines().count(),
        1,
        "the scratch worktree is gone"
    );
}

#[test]
fn deletes_a_finished_pair_and_brings_it_back() {
    let f = radar_fixture();
    f.git(&["push", "-q", "origin", "hotfix/LT-4", "hotfix/LT-4-dev"]);
    f.git(&["fetch", "-q", "origin"]);
    let hashes = f.git(&["rev-parse", "hotfix/LT-4", "hotfix/LT-4-dev"]);
    let repo = f.repo();
    let input = DeleteInput {
        names: vec!["hotfix/LT-4".into(), "hotfix/LT-4-dev".into()],
        local: true,
        remote: true,
    };
    let plan = repo.plan_delete(&input).unwrap();
    assert!(plan.problems.is_empty(), "{plan:?}");
    // The fixture's commits are by Tester while the repository's user is Me.
    assert_eq!(plan.warnings.len(), 1, "{plan:?}");
    assert!(plan.warnings[0].contains("других авторов"));
    assert_eq!(
        plan.commands
            .iter()
            .filter(|c| c.starts_with("git push"))
            .count(),
        1,
        "one atomic push"
    );
    let mut op = repo.start_delete(&input).unwrap();
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);
    assert!(
        !f.has_branch("hotfix/LT-4")
            && !f
                .remote_heads()
                .iter()
                .any(|h| h.starts_with("hotfix/LT-4"))
    );

    op.undo_all(&repo, OpStatus::Undone);
    assert_eq!(op.status, OpStatus::Undone, "{:?}", op.undo_error);
    assert_eq!(
        f.git(&["rev-parse", "hotfix/LT-4", "hotfix/LT-4-dev"]),
        hashes
    );
    assert!(f.remote_heads().contains(&"hotfix/LT-4-dev".to_string()));
}

#[test]
fn delete_warns_about_unmerged_branches_and_refuses_the_current_one() {
    let f = radar_fixture();
    let plan = f
        .repo()
        .plan_delete(&DeleteInput {
            names: vec!["hotfix/LT-2".into(), "feature/work".into()],
            local: true,
            remote: false,
        })
        .unwrap();
    assert!(plan.warnings[0].contains("hotfix/LT-2"));
    assert!(
        plan.problems
            .iter()
            .any(|p| p.contains("feature/work") && p.contains("текущая"))
    );
}

#[test]
fn a_squash_merged_port_counts_as_carried_over() {
    let f = with_origin();
    f.git(&["checkout", "-q", "-b", "hotfix/LT-6", "master"]);
    f.commit("s1.txt", "1\n", "fix 6a");
    f.commit("s2.txt", "2\n", "fix 6b");
    f.git(&["checkout", "-q", "master"]);
    f.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge hotfix/LT-6",
        "hotfix/LT-6",
    ]);
    // The develop side got both changes as one squashed commit: no patch twins.
    f.git(&["checkout", "-q", "develop"]);
    f.git(&["merge", "-q", "--squash", "hotfix/LT-6"]);
    f.git(&["commit", "-q", "-m", "LT-6 (#123)"]);
    f.git(&["push", "-q", "origin", "master", "develop"]);
    f.git(&["fetch", "-q", "origin"]);
    f.git(&["checkout", "-q", "feature/work"]);

    let radar = f.repo().hotfix_radar().unwrap();
    let p = radar.pairs.iter().find(|p| p.slug == "LT-6").unwrap();
    assert_eq!(p.unsynced, 0, "{:?}", p.commits);
    assert!(
        p.commits
            .iter()
            .all(|c| c.ported.as_ref().unwrap().by_content)
    );
    assert_eq!(p.state, PairState::Done);
    assert!(radar.lost.is_empty(), "{:?}", radar.lost);
}

#[test]
fn a_port_with_a_resolved_conflict_counts_as_carried_over() {
    let f = with_origin();
    f.git(&["checkout", "-q", "develop"]);
    f.commit("c.txt", "develop\n", "c on develop");
    f.git(&["push", "-q", "origin", "develop"]);
    f.git(&["fetch", "-q", "origin"]);
    f.git(&["checkout", "-q", "-b", "hotfix/LT-6", "master"]);
    f.commit("c.txt", "master\n", "c on hotfix");
    let fix = head(&f);
    f.git(&["checkout", "-q", "feature/work"]);

    let repo = f.repo();
    let input = PortInput {
        commits: vec![fix.clone()],
        target: "hotfix/LT-6-dev".into(),
        create: true,
        push: false,
    };
    let mut op = repo.start_port(&input).unwrap();
    run(&mut op, &repo);
    let dir = op
        .stopped_pick()
        .expect("stopped on the conflict")
        .to_string();
    // Resolved by hand: neither side as is, so the patch differs from the original.
    std::fs::write(
        std::path::Path::new(&dir).join("c.txt"),
        "master and develop\n",
    )
    .unwrap();
    op.resume(&repo, false);
    run(&mut op, &repo);
    assert_eq!(op.status, OpStatus::Done, "{:?}", op.states);

    let radar = repo.hotfix_radar().unwrap();
    let pair = radar.pairs.iter().find(|p| p.slug == "LT-6").unwrap();
    assert_eq!(pair.unsynced, 0, "{pair:#?}");
    let ported = pair.commits[0].ported.as_ref().expect("carried over");
    assert_eq!(ported.place, "hotfix/LT-6-dev");
    assert_eq!(
        ported.hash,
        f.git(&["rev-parse", "hotfix/LT-6-dev"]).trim(),
        "points at the copy"
    );
}

#[test]
fn a_master_half_whose_name_ends_in_dev_pairs_with_its_own_dev() {
    let f = with_origin();
    f.git(&["checkout", "-q", "-b", "hotfix/LT-8-fix-on-dev", "master"]);
    f.commit("h8.txt", "8\n", "fix 8");
    let fix = head(&f);
    f.git(&[
        "checkout",
        "-q",
        "-b",
        "hotfix/LT-8-fix-on-dev-dev",
        "develop",
    ]);
    f.git(&["cherry-pick", "-x", &fix]);
    f.git(&["checkout", "-q", "feature/work"]);
    let repo = f.repo();

    let list = repo.branches("develop").unwrap();
    let target = |n: &str| list.iter().find(|b| b.name == n).unwrap().target.clone();
    assert_eq!(target("hotfix/LT-8-fix-on-dev"), "master");
    assert_eq!(target("hotfix/LT-8-fix-on-dev-dev"), "develop");

    let radar = repo.hotfix_radar().unwrap();
    let slugs: Vec<&str> = radar.pairs.iter().map(|p| p.slug.as_str()).collect();
    assert_eq!(slugs, ["LT-8-fix-on-dev"], "one pair, no phantom");
    let pair = &radar.pairs[0];
    assert_eq!(pair.state, PairState::Work);
    assert_eq!(pair.unsynced, 0);
}

/// A hotfix of `commits` (file, content) on master, and on develop's side one squashed commit
/// with `dev` (file, content).
fn squash_fixture(commits: &[(&str, &str)], dev: &[(&str, &str)]) -> Fixture {
    let f = with_origin();
    f.git(&["checkout", "-q", "-b", "hotfix/LT-9", "master"]);
    for (i, (file, content)) in commits.iter().enumerate() {
        f.commit(file, content, &format!("fix {i}"));
    }
    f.git(&["checkout", "-q", "-b", "hotfix/LT-9-dev", "develop"]);
    for (file, content) in dev {
        std::fs::write(f.root.join(file), content).unwrap();
        f.git(&["add", file]);
    }
    f.git(&["commit", "-q", "-m", "fix (squashed)"]);
    f.git(&["checkout", "-q", "feature/work"]);
    f
}

#[test]
fn commits_squashed_together_on_the_dev_side_count_as_carried_over() {
    // The second commit changes what the first added: the first is not there as it was.
    let f = squash_fixture(
        &[("h.txt", "a\n"), ("h.txt", "a\nb\n")],
        &[("h.txt", "a\nb\n")],
    );
    let radar = f.repo().hotfix_radar().unwrap();
    let pair = radar.pairs.iter().find(|p| p.slug == "LT-9").unwrap();
    assert_eq!(pair.unsynced, 0, "{pair:#?}");
    assert!(
        pair.commits
            .iter()
            .all(|c| c.ported.as_ref().is_some_and(|p| p.by_content))
    );
}

#[test]
fn a_missing_commit_after_a_squash_is_still_reported() {
    let f = squash_fixture(
        &[("h.txt", "a\n"), ("h.txt", "a\nb\n"), ("other.txt", "c\n")],
        &[("h.txt", "a\nb\n")],
    );
    let radar = f.repo().hotfix_radar().unwrap();
    let pair = radar.pairs.iter().find(|p| p.slug == "LT-9").unwrap();
    let last = pair.commits.last().unwrap();
    assert_eq!(last.commit.subject, "fix 2");
    assert!(
        last.ported.is_none(),
        "other.txt never reached the dev side"
    );
    assert_ne!(pair.state, PairState::Work);
}
