//! The page about one branch, on real repositories.

mod common;

use common::with_origin;
use musubiko_core::Status;

#[test]
fn shows_the_branch_against_each_base_with_its_commits_and_files() {
    let f = with_origin();
    // master moves on, so the branch differs against master and develop.
    f.git(&["checkout", "-q", "master"]);
    f.commit("m.txt", "m\n", "master only");
    f.git(&["push", "-q", "origin", "master"]);
    f.git(&["checkout", "-q", "feature/work"]);
    f.commit("a.txt", "1\n2\n", "first");
    f.git(&["mv", "README", "README.md"]);
    f.git(&["commit", "-q", "-m", "rename"]);
    std::fs::write(f.root.join("logo.bin"), [0u8, 1, 2, 0, 255]).unwrap();
    f.git(&["add", "logo.bin"]);
    f.git(&["commit", "-q", "-m", "binary"]);
    f.git(&["push", "-q", "-u", "origin", "feature/work"]);

    let card = f.repo().branch_card("feature/work", "develop").unwrap();
    assert_eq!(card.branch.target, "develop");
    assert_eq!(card.commit_count, 3);
    assert_eq!(
        card.commits
            .iter()
            .map(|c| c.subject.as_str())
            .collect::<Vec<_>>(),
        ["binary", "rename", "first"]
    );
    assert!(card.started.is_some());
    assert_eq!(card.upstream.as_deref(), Some("origin/feature/work"));

    let against: Vec<(&str, u32, u32)> = card
        .against
        .iter()
        .map(|b| (b.target.as_str(), b.ahead, b.behind))
        .collect();
    assert_eq!(against, [("master", 3, 1), ("develop", 3, 0)]);

    let file = |p: &str| card.files.iter().find(|x| x.path == p).unwrap();
    assert_eq!(
        (file("a.txt").added, file("a.txt").removed),
        (Some(2), Some(0))
    );
    assert_eq!(
        file("README.md").added,
        Some(0),
        "a rename shows under its new name"
    );
    assert_eq!(file("README.md").old_path.as_deref(), Some("README"));
    assert_eq!(file("a.txt").old_path, None);
    assert_eq!(file("logo.bin").added, None, "binary");
    assert_eq!(card.files.len(), 3);
}

#[test]
fn a_hotfix_half_keeps_its_base_but_is_shown_against_both() {
    let f = with_origin();
    f.git(&["checkout", "-q", "-b", "hotfix/LT-1", "master"]);
    f.commit("h.txt", "fix\n", "fix");
    let card = f.repo().branch_card("hotfix/LT-1", "develop").unwrap();
    assert_eq!(card.branch.target, "master");
    assert_eq!(card.upstream, None, "never pushed");
    let targets: Vec<&str> = card.against.iter().map(|b| b.target.as_str()).collect();
    assert_eq!(targets, ["master", "develop"]);
    assert!(card.against.iter().all(|b| b.status == Status::Open));
}

#[test]
fn a_custom_base_is_shown_too_and_bases_have_no_card() {
    let f = with_origin();
    f.git(&["branch", "feature/other", "develop"]);
    let card = f
        .repo()
        .branch_card("feature/work", "feature/other")
        .unwrap();
    let targets: Vec<&str> = card.against.iter().map(|b| b.target.as_str()).collect();
    assert_eq!(targets, ["master", "develop", "feature/other"]);
    assert!(f.repo().branch_card("develop", "master").is_err());
}
