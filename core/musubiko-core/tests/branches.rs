//! Builds small repositories with the real git and checks how branches are classified.

mod common;

use common::{Fixture, flow_fixture, with_origin};
use musubiko_core::{Branch, Location, Status, Sync};

fn find<'a>(list: &'a [Branch], name: &str) -> &'a Branch {
    list.iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("no branch {name}"))
}

#[test]
fn classifies_branches_against_develop() {
    let f = flow_fixture();
    let list = f.repo().branches("develop").unwrap();

    assert!(
        list.iter()
            .all(|b| b.name != "master" && b.name != "develop"),
        "bases are listed separately"
    );

    let merged = find(&list, "feature/merged");
    assert_eq!(merged.status, Status::Merged);
    assert_eq!(merged.ahead, 0);

    let squash = find(&list, "feature/squash");
    assert_eq!(squash.status, Status::Squashed);
    assert_eq!(squash.ahead, 2);

    let conflict = find(&list, "feature/conflict");
    assert_eq!(
        conflict.status,
        Status::Conflict {
            files: vec!["c.txt".into()]
        }
    );

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
        assert_eq!(
            (m.target.as_str(), &m.status, m.ahead),
            ("master", &Status::Open, 1)
        );
        let d = find(&list, "hotfix/LT-1-fix-dev");
        assert_eq!(
            (d.target.as_str(), &d.status, d.ahead),
            ("develop", &Status::Open, 1)
        );
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
    let changes: Vec<(&str, &str)> = info
        .changes
        .iter()
        .map(|c| (c.code.as_str(), c.path.as_str()))
        .collect();
    assert_eq!(changes, [("??", "notes.md"), ("M", "o.txt")]);
    assert_eq!(info.user.email, "me@example.com");
    let roles: Vec<&str> = info.bases.iter().map(|b| b.role.as_str()).collect();
    assert_eq!(roles, ["master", "develop"]);
}

#[test]
fn tracks_remote_and_local_copies() {
    let f = Fixture::new();
    let origin = f.root.parent().unwrap().join("origin.git");
    f.git_in(
        f.root.parent().unwrap(),
        &["init", "-q", "--bare", "origin.git"],
    );
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
    assert_eq!(
        pushed.sync,
        Some(Sync {
            ahead: 1,
            behind: 0
        })
    );
    assert_eq!(pushed.ahead, 2);
    let remote = find(&list, "feature/remote-only");
    assert_eq!(
        (remote.location, &remote.status),
        (Location::Remote, &Status::Open)
    );
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

#[test]
fn any_branch_can_be_the_base() {
    let f = with_origin();
    f.git(&["checkout", "-q", "-b", "feature/base", "develop"]);
    f.commit("a.txt", "a\n", "base work");
    f.git(&["push", "-q", "origin", "feature/base"]);
    // A local commit the server has not seen yet: the base is taken as the user has it.
    f.commit("b.txt", "b\n", "unpushed base work");
    f.git(&["checkout", "-q", "-b", "feature/on-top"]);
    f.git(&["checkout", "-q", "-b", "feature/more"]);
    f.commit("c.txt", "c\n", "more");

    let list = f.repo().branches("feature/base").unwrap();
    let get = |n: &str| list.iter().find(|b| b.name == n);
    assert!(
        get("feature/base").is_none(),
        "the base is not compared with itself"
    );
    let on_top = get("feature/on-top").unwrap();
    assert_eq!(on_top.target, "feature/base");
    assert_eq!(on_top.status, Status::New, "created and never committed to");
    let more = get("feature/more").unwrap();
    assert_eq!((more.status.clone(), more.ahead), (Status::Open, 1));
}

#[test]
fn a_branch_without_commits_is_new_and_belongs_to_its_creator() {
    let f = with_origin();
    // The base's last commit is someone else's.
    f.git(&["checkout", "-q", "develop"]);
    std::fs::write(f.root.join("d.txt"), "d\n").unwrap();
    f.git(&["add", "d.txt"]);
    f.git(&[
        "-c",
        "user.name=Colleague",
        "-c",
        "user.email=colleague@example.com",
        "commit",
        "-q",
        "--author=Colleague <colleague@example.com>",
        "-m",
        "colleague's work",
    ]);
    f.git(&["push", "-q", "origin", "develop"]);
    f.git(&["fetch", "-q", "origin"]);
    // Fresh, as the hotfix and new-branch dialogs make it.
    let repo = f.repo();
    repo.run_step(&musubiko_core::Step::new(
        "x",
        &["branch", "--no-track", "feature/fresh", "origin/develop"],
    ))
    .unwrap();
    // Merged by fast-forward: had a commit of its own.
    f.git(&["checkout", "-q", "-b", "feature/ff", "develop"]);
    f.commit("ff.txt", "ff\n", "ff work");
    f.git(&["checkout", "-q", "develop"]);
    f.git(&["merge", "-q", "--ff-only", "feature/ff"]);
    f.git(&["push", "-q", "origin", "develop"]);
    f.git(&["fetch", "-q", "origin"]);
    f.git(&["checkout", "-q", "feature/work"]);

    let list = repo.branches("develop").unwrap();
    let get = |n: &str| list.iter().find(|b| b.name == n).unwrap();
    let fresh = get("feature/fresh");
    assert_eq!(fresh.status, Status::New);
    assert_eq!(
        fresh.owner.email, "me@example.com",
        "the creator, not the base's last author"
    );
    assert_eq!(get("feature/ff").status, Status::Merged);
    assert_eq!(
        get("feature/work").status,
        Status::New,
        "made by with_origin, never committed"
    );

    // The same answer from the cache.
    let again = repo.branches("develop").unwrap();
    let fresh = again.iter().find(|b| b.name == "feature/fresh").unwrap();
    assert_eq!(
        (fresh.status.clone(), fresh.owner.email.as_str()),
        (Status::New, "me@example.com")
    );
}

#[test]
fn the_owner_is_whoever_started_the_branch_whatever_the_commit_dates() {
    let f = with_origin();
    let commit = |file: &str, who: &str, email: &str, date: &str| {
        std::fs::write(f.root.join(file), format!("{file}\n")).unwrap();
        f.git(&["add", file]);
        let out = std::process::Command::new(f.git.path())
            .args(["commit", "-q", "-m", file])
            .current_dir(&f.root)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", who)
            .env("GIT_AUTHOR_EMAIL", email)
            .env("GIT_COMMITTER_NAME", who)
            .env("GIT_COMMITTER_EMAIL", email)
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .output()
            .unwrap();
        assert!(out.status.success());
    };
    // Started by Ann; later Bob's work, committed earlier, is merged into it (as on LT-22277).
    f.git(&["checkout", "-q", "-b", "bob/side", "develop"]);
    commit("b.txt", "Bob", "bob@example.com", "1700000100 +0000");
    f.git(&["checkout", "-q", "-b", "feature/started-by-ann", "develop"]);
    commit("a.txt", "Ann", "ann@example.com", "1700000200 +0000");
    let out = std::process::Command::new(f.git.path())
        .args(["merge", "-q", "--no-ff", "-m", "Merge bob/side", "bob/side"])
        .current_dir(&f.root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Ann")
        .env("GIT_AUTHOR_EMAIL", "ann@example.com")
        .env("GIT_COMMITTER_NAME", "Ann")
        .env("GIT_COMMITTER_EMAIL", "ann@example.com")
        .env("GIT_AUTHOR_DATE", "1700000300 +0000")
        .env("GIT_COMMITTER_DATE", "1700000300 +0000")
        .output()
        .unwrap();
    assert!(out.status.success());
    f.git(&["branch", "-q", "-D", "bob/side"]);
    f.git(&["checkout", "-q", "feature/work"]);

    let list = f.repo().branches("develop").unwrap();
    let b = list
        .iter()
        .find(|b| b.name == "feature/started-by-ann")
        .unwrap();
    assert_eq!(b.owner.email, "ann@example.com");
    let mine = f
        .repo()
        .my_branches("develop", Some("ann@example.com"))
        .unwrap();
    let w = mine
        .attention
        .iter()
        .chain(mine.active.iter())
        .find(|w| w.branch.name == "feature/started-by-ann")
        .expect("in Ann's branches");
    assert_eq!(
        w.started,
        Some(1_700_000_200),
        "when Ann wrote the first commit"
    );
}

#[test]
fn a_shallow_clone_says_the_history_is_missing_until_it_is_fetched() {
    let f = with_origin();
    f.git(&["checkout", "-q", "develop"]);
    for i in 0..5 {
        f.commit("d.txt", &format!("{i}\n"), &format!("develop {i}"));
    }
    f.git(&["checkout", "-q", "-b", "feature/old", "develop~4"]);
    f.commit("o.txt", "o\n", "old work");
    f.git(&["push", "-q", "origin", "develop", "feature/old"]);
    let parent = f.root.parent().unwrap();
    // --depth needs a URL; on Windows the path is C:\... and wants file:///C:/...
    let path = parent
        .join("origin.git")
        .to_string_lossy()
        .replace('\\', "/");
    let url = if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    };
    f.git_in(
        parent,
        &[
            "clone",
            "-q",
            "--depth",
            "1",
            "--no-single-branch",
            &url,
            "shallow",
        ],
    );
    let dir = parent.join("shallow");
    let repo = musubiko_core::Repo::open(f.git.clone(), &dir).unwrap();

    let get = |repo: &musubiko_core::Repo| {
        repo.branches("develop")
            .unwrap()
            .into_iter()
            .find(|b| b.name == "feature/old")
            .unwrap()
    };
    let b = get(&repo);
    let Status::Unknown { error } = &b.status else {
        panic!("{:?}", b.status)
    };
    assert!(error.contains("git fetch --unshallow"), "{error}");
    assert_eq!(
        (b.ahead, b.behind),
        (0, 0),
        "counts from a cut history mean nothing"
    );

    f.git_in(&dir, &["fetch", "-q", "--unshallow"]);
    let b = get(&repo);
    assert_eq!(
        (b.status, b.ahead, b.behind),
        (Status::Open, 1, 4),
        "not stuck in the cache"
    );
}
