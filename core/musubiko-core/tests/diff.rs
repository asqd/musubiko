//! Branch and commit diffs, file contents and conflict previews on real repositories.

mod common;

use common::{Fixture, flow_fixture};
use musubiko_core::{FileChange, LineKind};

#[test]
fn branch_diff_starts_at_the_merge_base() {
    let f = flow_fixture();
    let d = f
        .repo()
        .branch_diff("feature/open", "develop", false)
        .unwrap();
    assert_eq!(d.target, "develop");
    assert_eq!(d.merge_base.hash, f.git(&["rev-parse", "develop"]).trim());
    assert_eq!(d.tip, f.git(&["rev-parse", "feature/open"]).trim());
    let subjects: Vec<&str> = d.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["open work"]);
    assert_eq!(d.files.len(), 1);
    let file = &d.files[0];
    assert_eq!(
        (
            file.path.as_str(),
            file.change,
            file.additions,
            file.deletions
        ),
        ("o.txt", FileChange::Added, 1, 0)
    );
    assert_eq!(file.hunks[0].lines[0].kind, LineKind::Add);
}

#[test]
fn develop_moving_on_does_not_leak_into_the_diff() {
    // develop changed c.txt after feature/conflict left it; the diff shows only the branch's work.
    let f = flow_fixture();
    let d = f
        .repo()
        .branch_diff("feature/conflict", "develop", false)
        .unwrap();
    let paths: Vec<&str> = d.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["c.txt"]);
    let lines: Vec<(LineKind, &str)> = d.files[0].hunks[0]
        .lines
        .iter()
        .map(|l| (l.kind, l.text.as_str()))
        .collect();
    assert_eq!(lines, [(LineKind::Del, "line"), (LineKind::Add, "ours")]);
}

#[test]
fn hotfix_dev_half_diffs_against_develop() {
    let f = flow_fixture();
    let d = f
        .repo()
        .branch_diff("hotfix/LT-1-fix-dev", "master", false)
        .unwrap();
    assert_eq!(d.target, "develop");
    assert_eq!(
        d.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["h.txt"]
    );
}

#[test]
fn commit_diff_handles_the_root_commit() {
    let f = flow_fixture();
    let root = f
        .git(&["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_string();
    let files = f.repo().commit_diff(&root, false).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(
        (files[0].path.as_str(), files[0].change),
        ("README", FileChange::Added)
    );
}

#[test]
fn whitespace_only_changes_can_be_hidden() {
    let f = Fixture::new();
    f.commit("a.rb", "def x\n  1\nend\n", "init");
    f.git(&["checkout", "-q", "-b", "develop"]);
    f.git(&["checkout", "-q", "-b", "feature/indent"]);
    f.commit("a.rb", "def x\n    1\nend\n", "indent");
    let repo = f.repo();
    let shown = repo
        .branch_diff("feature/indent", "develop", false)
        .unwrap();
    assert_eq!((shown.files[0].additions, shown.files[0].deletions), (1, 1));
    let hidden = repo.branch_diff("feature/indent", "develop", true).unwrap();
    assert!(hidden.files.iter().all(|f| f.hunks.is_empty()));
}

#[test]
fn reads_file_lines_at_a_revision() {
    let f = flow_fixture();
    let lines = f.repo().file_lines("develop", "c.txt").unwrap();
    assert_eq!(lines, ["theirs"]);
}

#[test]
fn previews_conflicts_without_touching_the_work_tree() {
    let f = flow_fixture();
    let before = f.git(&["status", "--porcelain"]);
    let conflicts = f.repo().conflicts("feature/conflict", "develop").unwrap();
    assert_eq!(conflicts.len(), 1);
    let c = &conflicts[0];
    assert_eq!(c.path, "c.txt");
    assert_eq!(c.regions[0].base, ["theirs"]);
    assert_eq!(c.regions[0].branch, ["ours"]);
    assert_eq!(f.git(&["status", "--porcelain"]), before);

    assert!(
        f.repo()
            .conflicts("feature/open", "develop")
            .unwrap()
            .is_empty()
    );
}
