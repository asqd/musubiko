//! Builds small repositories with the real git and checks how branches are classified.

mod common;

use common::{flow_fixture, Fixture};
use musubiko_core::{Branch, Location, Status, Sync};

fn find<'a>(list: &'a [Branch], name: &str) -> &'a Branch {
    list.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("no branch {name}"))
}

#[test]
fn classifies_branches_against_develop() {
    let f = flow_fixture();
    let list = f.repo().branches("develop").unwrap();

    assert!(list.iter().all(|b| b.name != "master" && b.name != "develop"), "bases are listed separately");

    let merged = find(&list, "feature/merged");
    assert_eq!(merged.status, Status::Merged);
    assert_eq!(merged.ahead, 0);

    let squash = find(&list, "feature/squash");
    assert_eq!(squash.status, Status::Squashed);
    assert_eq!(squash.ahead, 2);

    let conflict = find(&list, "feature/conflict");
    assert_eq!(conflict.status, Status::Conflict { files: vec!["c.txt".into()] });

    let open = find(&list, "feature/open");
    assert_eq!(open.status, Status::Open);
    assert_eq!((open.ahead, open.behind), (1, 0));
    assert!(open.is_head);
    assert_eq!(open.location, Location::Local);
    assert_eq!(open.owner.email, "tester@example.com");
}

#[test]
fn hotfix_halves_use_their_own_base() {
    let f = flow_fixture();
    for base in ["develop", "master"] {
        let list = f.repo().branches(base).unwrap();
        let m = find(&list, "hotfix/LT-1-fix");
        assert_eq!((m.target.as_str(), &m.status, m.ahead), ("master", &Status::Open, 1));
        let d = find(&list, "hotfix/LT-1-fix-dev");
        assert_eq!((d.target.as_str(), &d.status, d.ahead), ("develop", &Status::Open, 1));
    }
}

#[test]
fn switching_base_changes_status() {
    let f = flow_fixture();
    let list = f.repo().branches("master").unwrap();
    // Merged into develop, but master has not been released from develop yet.
    assert_eq!(find(&list, "feature/merged").status, Status::Open);
    // Master still has the original line, so the change applies cleanly.
    assert_eq!(find(&list, "feature/conflict").status, Status::Open);
}

#[test]
fn reports_head_and_uncommitted_changes() {
    let f = flow_fixture();
    std::fs::write(f.root.join("o.txt"), "edited\n").unwrap();
    std::fs::write(f.root.join("notes.md"), "new\n").unwrap();
    let info = f.repo().info().unwrap();
    assert_eq!(info.head.as_deref(), Some("feature/open"));
    let changes: Vec<(&str, &str)> = info.changes.iter().map(|c| (c.code.as_str(), c.path.as_str())).collect();
    assert_eq!(changes, [("??", "notes.md"), ("M", "o.txt")]);
    assert_eq!(info.user.email, "me@example.com");
    let roles: Vec<&str> = info.bases.iter().map(|b| b.role.as_str()).collect();
    assert_eq!(roles, ["master", "develop"]);
}

#[test]
fn tracks_remote_and_local_copies() {
    let f = Fixture::new();
    let origin = f.root.parent().unwrap().join("origin.git");
    f.git_in(f.root.parent().unwrap(), &["init", "-q", "--bare", "origin.git"]);
    f.commit("README", "base\n", "init");
    f.git(&["branch", "develop"]);
    f.git(&["remote", "add", "origin", origin.to_str().unwrap()]);
    f.git(&["push", "-q", "origin", "master", "develop"]);

    f.git(&["checkout", "-q", "-b", "feature/pushed", "develop"]);
    f.commit("p.txt", "1\n", "pushed");
    f.git(&["push", "-q", "-u", "origin", "feature/pushed"]);
    f.commit("p.txt", "2\n", "not pushed yet");

    f.git(&["checkout", "-q", "-b", "feature/remote-only", "develop"]);
    f.commit("r.txt", "r\n", "remote only");
    f.git(&["push", "-q", "origin", "feature/remote-only"]);
    f.git(&["checkout", "-q", "develop"]);
    f.git(&["branch", "-q", "-D", "feature/remote-only"]);

    f.git(&["checkout", "-q", "-b", "feature/local-only"]);
    f.commit("l.txt", "l\n", "local only");

    let repo = f.repo();
    let list = repo.branches("develop").unwrap();
    let pushed = find(&list, "feature/pushed");
    assert_eq!(pushed.location, Location::Both);
    assert_eq!(pushed.sync, Some(Sync { ahead: 1, behind: 0 }));
    assert_eq!(pushed.ahead, 2);
    let remote = find(&list, "feature/remote-only");
    assert_eq!((remote.location, &remote.status), (Location::Remote, &Status::Open));
    assert_eq!(find(&list, "feature/local-only").location, Location::Local);

    let info = repo.info().unwrap();
    assert_eq!(info.remote.as_deref(), Some("origin"));
    let develop = info.bases.iter().find(|b| b.role == "develop").unwrap();
    assert_eq!(develop.ref_name, "origin/develop");
}

#[test]
fn main_stands_in_for_master() {
    let f = Fixture::new();
    f.git(&["checkout", "-q", "-b", "main"]);
    f.commit("README", "base\n", "init");
    f.git(&["branch", "develop"]);
    let info = f.repo().info().unwrap();
    assert_eq!(info.flow.master, "main");
    assert_eq!(info.bases.len(), 2);
}
