//! "Where is this commit?" on real repositories.

mod common;

use common::{Fixture, with_origin};
use musubiko_core::{CommitPlace, Presence, Verdict};

fn head(f: &Fixture) -> String {
    f.git(&["rev-parse", "HEAD"]).trim().to_string()
}

/// A hotfix shipped in v1.0.1 and carried to develop; a fix made on master only; a feature in
/// develop; a feature squashed into develop; a branch whose ticket key is only in its name.
fn fixture() -> Fixture {
    let f = with_origin();
    f.git(&["checkout", "-q", "-b", "hotfix/LT-7-voucher", "master"]);
    f.commit("v.txt", "v\n", "Fix voucher LT-7");
    let fix = head(&f);
    f.git(&["checkout", "-q", "master"]);
    f.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge hotfix/LT-7-voucher",
        "hotfix/LT-7-voucher",
    ]);
    f.git(&["tag", "v1.0.1"]);
    f.git(&["checkout", "-q", "-b", "hotfix/LT-7-voucher-dev", "develop"]);
    f.git(&["cherry-pick", "-x", &fix]);
    f.git(&["checkout", "-q", "develop"]);
    f.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge hotfix/LT-7-voucher-dev",
        "hotfix/LT-7-voucher-dev",
    ]);

    f.git(&["checkout", "-q", "master"]);
    f.commit("hand.txt", "h\n", "Quick fix by hand");

    f.git(&["checkout", "-q", "-b", "feature/LT-20-filters", "develop"]);
    f.commit("filters.txt", "f\n", "Meal filter");
    f.git(&["checkout", "-q", "develop"]);
    f.git(&[
        "merge",
        "-q",
        "--no-ff",
        "-m",
        "Merge LT-20",
        "feature/LT-20-filters",
    ]);

    f.git(&["checkout", "-q", "-b", "feature/LT-50-cards", "develop"]);
    f.commit("c1.txt", "1\n", "cards part one");
    f.commit("c2.txt", "2\n", "cards part two");
    f.git(&["checkout", "-q", "develop"]);
    f.git(&["merge", "-q", "--squash", "feature/LT-50-cards"]);
    f.git(&["commit", "-q", "-m", "Hotel cards (#50)"]);

    f.git(&["checkout", "-q", "-b", "feature/LT-99-search", "develop"]);
    f.commit("s.txt", "s\n", "work in progress");

    f.git(&["push", "-q", "origin", "master", "develop"]);
    f.git(&["fetch", "-q", "origin"]);
    f.git(&["checkout", "-q", "feature/work"]);
    f
}

fn by_subject<'a>(r: &'a [CommitPlace], s: &str) -> &'a CommitPlace {
    r.iter().find(|p| p.commit.subject == s).unwrap_or_else(|| {
        panic!(
            "no {s} in {:?}",
            r.iter().map(|p| &p.commit.subject).collect::<Vec<_>>()
        )
    })
}

#[test]
fn a_shipped_hotfix_shows_its_tag_and_its_copy_in_develop() {
    let f = fixture();
    let s = f.repo().locate("LT-7").unwrap();
    let p = by_subject(&s.results, "Fix voucher LT-7");
    assert_eq!(p.master, Presence::Itself);
    assert_eq!(p.first_tag.as_deref(), Some("v1.0.1"));
    assert!(
        matches!(p.develop, Some(Presence::Copy { .. })),
        "{:?}",
        p.develop
    );
    assert_eq!(p.cherry_picks.len(), 1);
    assert!(p.branches.contains(&"hotfix/LT-7-voucher".to_string()));
    assert_eq!(p.verdict, Verdict::Everywhere);
}

#[test]
fn a_fix_made_only_on_master_is_flagged() {
    let f = fixture();
    let s = f.repo().locate("by hand").unwrap();
    assert_eq!(s.results.len(), 1);
    assert_eq!(s.results[0].verdict, Verdict::MasterOnly);
    assert_eq!(s.results[0].develop, Some(Presence::Missing));
}

#[test]
fn finds_by_hash_prefix() {
    let f = fixture();
    let hash = f
        .git(&["rev-parse", "feature/LT-20-filters"])
        .trim()
        .to_string();
    let s = f.repo().locate(&hash[..8]).unwrap();
    assert_eq!(s.results.len(), 1);
    assert_eq!(s.results[0].commit.subject, "Meal filter");
    assert_eq!(s.results[0].verdict, Verdict::DevelopOnly);
}

#[test]
fn a_ticket_key_found_only_in_the_branch_name() {
    let f = fixture();
    let s = f.repo().locate("lt-99").unwrap();
    let p = by_subject(&s.results, "work in progress");
    assert_eq!(p.verdict, Verdict::BranchesOnly);
    assert_eq!(p.branches, ["feature/LT-99-search"]);
}

#[test]
fn squashed_commits_count_as_present_by_content() {
    let f = fixture();
    let s = f.repo().locate("LT-50").unwrap();
    for subject in ["cards part one", "cards part two"] {
        let p = by_subject(&s.results, subject);
        assert_eq!(p.develop, Some(Presence::Content), "{subject}");
        assert_eq!(p.verdict, Verdict::DevelopOnly);
    }
}

#[test]
fn an_empty_query_is_refused() {
    let f = fixture();
    assert!(f.repo().locate("  ").is_err());
}

#[test]
fn a_merged_branch_name_still_finds_its_commits() {
    let f = fixture();
    // "LT-7" is in the message too; search by the part that is only in the branch name.
    let s = f.repo().locate("7-voucher").unwrap();
    let p = by_subject(&s.results, "Fix voucher LT-7");
    assert_eq!(p.master, Presence::Itself);
}
