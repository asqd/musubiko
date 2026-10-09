//! Builds small repositories with the real git and checks how branches are classified.

use std::path::{Path, PathBuf};
use std::process::Command;

use musubiko_core::{Branch, Git, Location, Repo, Status, Sync};
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
    git: Git,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("work");
        std::fs::create_dir(&root).unwrap();
        let f = Fixture { git: Git::discover().expect("git 2.38+"), root, _dir: dir };
        f.git_in(&f.root, &["init", "-q", "-b", "master"]);
        f.git(&["config", "user.email", "me@example.com"]);
        f
    }

    /// Runs git isolated from the user's global and system config.
    fn git_in(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new(self.git.path())
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Tester")
            .env("GIT_AUTHOR_EMAIL", "tester@example.com")
            .env("GIT_COMMITTER_NAME", "Tester")
            .env("GIT_COMMITTER_EMAIL", "tester@example.com")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn git(&self, args: &[&str]) -> String {
        self.git_in(&self.root, args)
    }

    fn commit(&self, file: &str, content: &str, msg: &str) {
        std::fs::write(self.root.join(file), content).unwrap();
        self.git(&["add", file]);
        self.git(&["commit", "-q", "-m", msg]);
    }

    fn repo(&self) -> Repo {
        Repo::open(self.git.clone(), &self.root).unwrap()
    }
}

fn find<'a>(list: &'a [Branch], name: &str) -> &'a Branch {
    list.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("no branch {name}"))
}

/// master ← develop with one branch of each kind, plus a hotfix pair.
fn flow_fixture() -> Fixture {
    let f = Fixture::new();
    f.commit("README", "base\n", "init");
    f.commit("c.txt", "line\n", "add c");
    f.git(&["checkout", "-q", "-b", "develop"]);

    f.git(&["checkout", "-q", "-b", "feature/merged"]);
    f.commit("m.txt", "m\n", "merged work");
    f.git(&["checkout", "-q", "develop"]);
    f.git(&["merge", "-q", "--no-ff", "-m", "Merge feature/merged", "feature/merged"]);

    f.git(&["checkout", "-q", "-b", "feature/squash"]);
    f.commit("s.txt", "s1\n", "squash 1");
    f.commit("s.txt", "s2\n", "squash 2");
    f.git(&["checkout", "-q", "develop"]);
    f.git(&["merge", "-q", "--squash", "feature/squash"]);
    f.git(&["commit", "-q", "-m", "Squashed feature/squash"]);

    f.git(&["checkout", "-q", "-b", "feature/conflict"]);
    f.commit("c.txt", "ours\n", "change c");
    f.git(&["checkout", "-q", "develop"]);
    f.commit("c.txt", "theirs\n", "change c on develop");

    f.git(&["checkout", "-q", "-b", "feature/open"]);
    f.commit("o.txt", "o\n", "open work");

    f.git(&["checkout", "-q", "-b", "hotfix/LT-1-fix", "master"]);
    f.commit("h.txt", "fix\n", "hotfix");
    f.git(&["checkout", "-q", "-b", "hotfix/LT-1-fix-dev", "develop"]);
    f.commit("h.txt", "fix\n", "hotfix");

    f.git(&["checkout", "-q", "feature/open"]);
    f
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
