//! "My branches": whose branch is whose and what stands in the way, on real repositories.

mod common;

use common::{Fixture, with_origin};
use musubiko_core::Attention;

/// The fixture's own commits are authored by this address (see `Fixture::git_in`).
const ME: &str = "tester@example.com";

/// A commit by someone else, or at another date.
fn commit_as(f: &Fixture, file: &str, author: (&str, &str), date: Option<&str>) {
    std::fs::write(f.root.join(file), file).unwrap();
    f.git(&["add", file]);
    let mut cmd = std::process::Command::new(f.git.path());
    cmd.args(["commit", "-q", "-m", file])
        .current_dir(&f.root)
        .env("GIT_AUTHOR_NAME", author.0)
        .env("GIT_AUTHOR_EMAIL", author.1)
        .env("GIT_COMMITTER_NAME", author.0)
        .env("GIT_COMMITTER_EMAIL", author.1);
    if let Some(d) = date {
        cmd.env("GIT_AUTHOR_DATE", d).env("GIT_COMMITTER_DATE", d);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn kinds(a: &[Attention]) -> Vec<&'static str> {
    a.iter()
        .map(|x| match x {
            Attention::Conflict { .. } => "conflict",
            Attention::Behind { .. } => "behind",
            Attention::Unpushed { .. } => "unpushed",
            Attention::NeverPushed => "neverPushed",
            Attention::GoneWithWork => "gone",
            Attention::Quiet { .. } => "quiet",
            Attention::Unknown { .. } => "unknown",
        })
        .collect()
}

#[test]
fn sorts_my_branches_by_what_stands_in_the_way() {
    let f = with_origin();
    // feature/work: one commit, never pushed.
    f.commit("w.txt", "w\n", "work");

    f.git(&["checkout", "-q", "-b", "feature/clean", "develop"]);
    f.commit("c.txt", "c\n", "clean");
    f.git(&["push", "-q", "-u", "origin", "feature/clean"]);

    f.git(&["checkout", "-q", "-b", "feature/unpushed", "develop"]);
    f.commit("u.txt", "1\n", "pushed");
    f.git(&["push", "-q", "-u", "origin", "feature/unpushed"]);
    f.commit("u2.txt", "2\n", "not pushed");

    f.git(&["checkout", "-q", "-b", "feature/conflict", "develop"]);
    f.commit("README", "mine\n", "my readme");
    f.git(&["push", "-q", "-u", "origin", "feature/conflict"]);

    f.git(&["checkout", "-q", "-b", "feature/merged", "develop"]);
    f.commit("m.txt", "m\n", "merged work");
    f.git(&["push", "-q", "-u", "origin", "feature/merged"]);

    f.git(&["checkout", "-q", "-b", "feature/old", "develop"]);
    commit_as(&f, "old.txt", ("Tester", ME), Some("2020-01-01T12:00:00Z"));
    f.git(&["push", "-q", "-u", "origin", "feature/old"]);

    f.git(&["checkout", "-q", "-b", "feature/theirs", "develop"]);
    commit_as(&f, "t.txt", ("Мария Ли", "maria@example.com"), None);
    f.git(&["push", "-q", "-u", "origin", "feature/theirs"]);

    // develop: the README changes the other way, and feature/merged goes in.
    f.git(&["checkout", "-q", "develop"]);
    f.commit("README", "theirs\n", "their readme");
    f.git(&["merge", "-q", "--no-ff", "-m", "merge", "feature/merged"]);
    f.git(&["push", "-q", "origin", "develop"]);
    f.git(&["checkout", "-q", "feature/work"]);

    let mine = f.repo().my_branches("develop", Some(ME)).unwrap();
    assert_eq!(mine.author.email, ME);
    let names = |l: &[musubiko_core::WorkBranch]| {
        let mut v: Vec<(String, Vec<&'static str>)> = l
            .iter()
            .map(|w| (w.branch.name.clone(), kinds(&w.attention)))
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        names(&mine.attention),
        [
            ("feature/conflict".to_string(), vec!["conflict"]),
            ("feature/old".to_string(), vec!["quiet"]),
            ("feature/unpushed".to_string(), vec!["unpushed"]),
            ("feature/work".to_string(), vec!["neverPushed"]),
        ]
    );
    assert_eq!(names(&mine.active), [("feature/clean".to_string(), vec![])]);
    let merged: Vec<&str> = mine.merged.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(merged, ["feature/merged"]);

    let clean = &mine.active[0];
    assert_eq!(
        (clean.commit_count, clean.files, clean.added, clean.removed),
        (1, 1, 1, 0)
    );
    assert_eq!(clean.recent[0].subject, "clean");
    assert!(clean.started.is_some());

    let owners: Vec<(&str, u32)> = mine
        .owners
        .iter()
        .map(|o| (o.person.email.as_str(), o.active))
        .collect();
    assert_eq!(owners, [(ME, 5), ("maria@example.com", 1)]);

    let hers = f
        .repo()
        .my_branches("develop", Some("maria@example.com"))
        .unwrap();
    assert_eq!(hers.author.name, "Мария Ли");
    assert_eq!(
        names(&hers.active),
        [("feature/theirs".to_string(), vec![])]
    );
}

#[test]
fn warns_when_the_base_moved_far_ahead() {
    let f = with_origin();
    f.git(&["checkout", "-q", "-b", "feature/slow", "develop"]);
    f.commit("s.txt", "s\n", "slow");
    f.git(&["push", "-q", "-u", "origin", "feature/slow"]);
    f.git(&["checkout", "-q", "develop"]);
    for i in 0..20 {
        f.git(&["commit", "-q", "--allow-empty", "-m", &format!("d{i}")]);
    }
    f.git(&["push", "-q", "origin", "develop"]);
    let mine = f.repo().my_branches("develop", Some(ME)).unwrap();
    let slow = mine
        .attention
        .iter()
        .find(|w| w.branch.name == "feature/slow")
        .expect("needs attention");
    assert_eq!(slow.attention, [Attention::Behind { count: 20 }]);
}
