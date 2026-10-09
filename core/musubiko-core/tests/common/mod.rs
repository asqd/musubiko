//! Throwaway repositories built with the real git, isolated from the user's config.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use musubiko_core::{Git, Repo};
use tempfile::TempDir;

pub struct Fixture {
    _dir: TempDir,
    pub root: PathBuf,
    pub git: Git,
}

impl Fixture {
    pub fn new() -> Fixture {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("work");
        std::fs::create_dir(&root).unwrap();
        let f = Fixture { git: Git::discover().expect("git 2.38+"), root, _dir: dir };
        f.git_in(&f.root, &["init", "-q", "-b", "master"]);
        f.git(&["config", "user.email", "me@example.com"]);
        f
    }

    /// Runs git isolated from the user's global and system config.
    pub fn git_in(&self, dir: &Path, args: &[&str]) -> String {
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

    pub fn git(&self, args: &[&str]) -> String {
        self.git_in(&self.root, args)
    }

    pub fn commit(&self, file: &str, content: &str, msg: &str) {
        std::fs::write(self.root.join(file), content).unwrap();
        self.git(&["add", file]);
        self.git(&["commit", "-q", "-m", msg]);
    }

    pub fn repo(&self) -> Repo {
        Repo::open(self.git.clone(), &self.root).unwrap()
    }
}

/// master ← develop with one branch of each kind, plus a hotfix pair.
pub fn flow_fixture() -> Fixture {
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

