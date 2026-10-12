use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::branches::Sync;
use crate::{Error, Flow, Git, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Person {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub subject: String,
    pub author: Person,
    /// Committer date, Unix seconds.
    pub date: i64,
}

/// A path with uncommitted changes, as `git status --porcelain` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    /// `M`, `A`, `D`, `R`, `??`, `MM`…
    pub code: String,
    pub path: String,
}

/// One of the two long-lived branches.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseTip {
    /// `master` or `develop`: the role in the flow, not the branch name.
    pub role: String,
    pub name: String,
    /// The ref comparisons use: the remote-tracking one when it exists.
    pub ref_name: String,
    pub tip: Commit,
    /// Local branch against its remote-tracking branch, when both exist and differ.
    pub sync: Option<Sync>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub root: PathBuf,
    pub name: String,
    /// Current branch; `None` when HEAD is detached.
    pub head: Option<String>,
    pub head_short: String,
    pub changes: Vec<Change>,
    pub user: Person,
    pub remote: Option<String>,
    pub flow: Flow,
    pub bases: Vec<BaseTip>,
    pub git: String,
    /// When anything was last fetched (Unix seconds), from FETCH_HEAD.
    pub last_fetch: Option<i64>,
}

#[derive(Debug, Default)]
pub(crate) struct Refs {
    pub local: Option<Commit>,
    pub remote: Option<Commit>,
    /// The local branch tracks a remote branch that no longer exists (deleted after a merge).
    pub upstream_gone: bool,
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub(crate) git: Git,
    pub(crate) root: PathBuf,
    pub(crate) remote: Option<String>,
    pub(crate) flow: Flow,
}

impl Repo {
    /// Opens the repository containing `path` and settles the flow's branch names
    /// (`main` stands in for `master` when only it exists).
    pub fn open(git: Git, path: &Path) -> Result<Repo> {
        let out = git.output(path, &["rev-parse", "--show-toplevel"]);
        let root = match out {
            Ok(o) if o.status.success() => PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()),
            _ => return Err(Error::NotARepo(path.to_path_buf())),
        };
        let remotes = git.run(&root, &["remote"])?;
        let remotes: Vec<&str> = remotes
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        let remote = remotes
            .iter()
            .find(|r| **r == "origin")
            .or(remotes.first())
            .map(|r| r.to_string());
        let mut repo = Repo {
            git,
            root,
            remote,
            flow: Flow::default(),
        };
        let refs = repo.refs()?;
        repo.flow.master = repo.pick_master(&refs);
        Ok(repo)
    }

    /// `master` or `main`. With only one of them it is that one. With both (a repository that
    /// moved to `main` and kept the old branch, or the other way round) it is the server's
    /// default branch, then the one the server has, then the one with the newer tip.
    fn pick_master(&self, refs: &BTreeMap<String, Refs>) -> String {
        const NAMES: [&str; 2] = ["master", "main"];
        let present: Vec<&str> = NAMES
            .into_iter()
            .filter(|n| refs.contains_key(*n))
            .collect();
        match present[..] {
            [] => return NAMES[0].into(),
            [one] => return one.into(),
            _ => {}
        }
        if let Some(remote) = &self.remote {
            let head = self
                .run(&[
                    "symbolic-ref",
                    "-q",
                    "--short",
                    &format!("refs/remotes/{remote}/HEAD"),
                ])
                .unwrap_or_default();
            let default = head
                .trim()
                .strip_prefix(&format!("{remote}/"))
                .unwrap_or("");
            if let Some(n) = NAMES.into_iter().find(|n| *n == default) {
                return n.into();
            }
        }
        let on_server: Vec<&str> = NAMES
            .into_iter()
            .filter(|n| refs[*n].remote.is_some())
            .collect();
        if let [one] = on_server[..] {
            return one.into();
        }
        let tip = |n: &str| {
            let r = &refs[n];
            r.remote.as_ref().or(r.local.as_ref()).map_or(0, |c| c.date)
        };
        if tip("main") > tip("master") {
            "main".into()
        } else {
            "master".into()
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn flow(&self) -> &Flow {
        &self.flow
    }

    /// Whether there is a branch with this name, here or on the remote.
    pub fn has_branch(&self, name: &str) -> Result<bool> {
        Ok(self.refs()?.contains_key(name))
    }

    /// develop when the repository has it, otherwise master.
    pub fn default_base(&self) -> Result<String> {
        let refs = self.refs()?;
        Ok(if refs.contains_key(&self.flow.develop) {
            self.flow.develop.clone()
        } else {
            self.flow.master.clone()
        })
    }

    pub(crate) fn run(&self, args: &[&str]) -> Result<String> {
        self.git.run(&self.root, args)
    }

    pub fn info(&self) -> Result<RepoInfo> {
        let head_out = self
            .git
            .output(&self.root, &["symbolic-ref", "--short", "-q", "HEAD"])?;
        let head = head_out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&head_out.stdout).trim().to_string());
        let head_short = self
            .run(&["rev-parse", "--short", "HEAD"])
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let mut changes = parse_status(&self.run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
        ])?);
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        let config = |key: &str| {
            self.run(&["config", key])
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let user = Person {
            name: config("user.name"),
            email: config("user.email"),
        };

        let refs = self.refs()?;
        let mut bases = Vec::new();
        for (role, name) in [
            ("master", &self.flow.master),
            ("develop", &self.flow.develop),
        ] {
            let Some(r) = refs.get(name) else { continue };
            let Some(ref_name) = self.base_ref(name, &refs) else {
                continue;
            };
            let tip = r
                .remote
                .clone()
                .or_else(|| r.local.clone())
                .expect("ref without commit");
            let sync = self.sync(r)?;
            bases.push(BaseTip {
                role: role.into(),
                name: name.clone(),
                ref_name: ref_name
                    .trim_start_matches("refs/remotes/")
                    .trim_start_matches("refs/heads/")
                    .into(),
                tip,
                sync,
            });
        }
        Ok(RepoInfo {
            name: self
                .root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            root: self.root.clone(),
            head,
            head_short,
            changes,
            user,
            remote: self.remote.clone(),
            flow: self.flow.clone(),
            bases,
            git: format!("{} {}", self.git.version(), self.git.path().display()),
            last_fetch: self.last_fetch(),
        })
    }

    /// Local and remote-tracking branches by short name. Only the chosen remote is read.
    pub(crate) fn refs(&self) -> Result<BTreeMap<String, Refs>> {
        let fmt = "--format=%(refname)%00%(objectname)%00%(objectname:short)%00%(subject)%00%(authorname)%00%(authoremail:trim)%00%(committerdate:unix)%00%(upstream:track)";
        let mut args = vec!["for-each-ref", fmt, "refs/heads"];
        let remote_prefix = self.remote.as_ref().map(|r| format!("refs/remotes/{r}/"));
        if let Some(p) = &remote_prefix {
            args.push(p.as_str());
        }
        let out = self.run(&args)?;
        let mut map: BTreeMap<String, Refs> = BTreeMap::new();
        for line in out.lines() {
            let f: Vec<&str> = line.split('\0').collect();
            if f.len() < 7 {
                continue;
            }
            let commit = Commit {
                hash: f[1].into(),
                short: f[2].into(),
                subject: f[3].into(),
                author: Person {
                    name: f[4].into(),
                    email: f[5].into(),
                },
                date: f[6].parse().unwrap_or(0),
            };
            if let Some(name) = f[0].strip_prefix("refs/heads/") {
                let e = map.entry(name.into()).or_default();
                e.local = Some(commit);
                e.upstream_gone = f.get(7).is_some_and(|t| t.contains("gone"));
            } else if let Some(name) = remote_prefix.as_deref().and_then(|p| f[0].strip_prefix(p)) {
                if name != "HEAD" {
                    map.entry(name.into()).or_default().remote = Some(commit);
                }
            }
        }
        Ok(map)
    }

    /// Full ref for a base branch: the remote-tracking one first, since that is what was merged
    /// on the server; the local branch only when there is no remote copy. Any other branch
    /// picked as a base is taken as the user has it, like a work branch.
    pub(crate) fn base_ref(&self, name: &str, refs: &BTreeMap<String, Refs>) -> Option<String> {
        if !self.flow.is_base(name) {
            return self.branch_ref(name, refs);
        }
        let r = refs.get(name)?;
        match (&r.remote, &self.remote) {
            (Some(_), Some(remote)) => Some(format!("refs/remotes/{remote}/{name}")),
            _ => r.local.as_ref().map(|_| format!("refs/heads/{name}")),
        }
    }

    /// Ref for a work branch: the local one when it exists, otherwise the remote-tracking one.
    pub(crate) fn branch_ref(&self, name: &str, refs: &BTreeMap<String, Refs>) -> Option<String> {
        let r = refs.get(name)?;
        if r.local.is_some() {
            return Some(format!("refs/heads/{name}"));
        }
        let remote = self.remote.as_deref()?;
        r.remote
            .as_ref()
            .map(|_| format!("refs/remotes/{remote}/{name}"))
    }

    /// The branch's ref, the base it is compared with (`base`, or the fixed one for hotfix
    /// halves) and that base's ref.
    pub(crate) fn resolve(&self, name: &str, base: &str) -> Result<(String, String, String)> {
        let refs = self.refs()?;
        let branch_ref = self
            .branch_ref(name, &refs)
            .ok_or_else(|| Error::NoBranch(name.into()))?;
        let target = self
            .flow
            .target_for(name, base, |n| refs.contains_key(n))
            .to_string();
        let target_ref = self
            .base_ref(&target, &refs)
            .ok_or_else(|| Error::NoBranch(target.clone()))?;
        Ok((branch_ref, target, target_ref))
    }

    fn last_fetch(&self) -> Option<i64> {
        let p = self.run(&["rev-parse", "--git-path", "FETCH_HEAD"]).ok()?;
        let p = self.root.join(p.trim());
        let t = std::fs::metadata(p).ok()?.modified().ok()?;
        Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
    }

    /// Local branch against its remote copy: commits not pushed and not pulled.
    pub(crate) fn sync(&self, r: &Refs) -> Result<Option<Sync>> {
        let (Some(l), Some(rm)) = (&r.local, &r.remote) else {
            return Ok(None);
        };
        if l.hash == rm.hash {
            return Ok(None);
        }
        let (ahead, behind) = self.left_right(&l.hash, &rm.hash)?;
        Ok(Some(Sync { ahead, behind }))
    }

    /// Commits only in `a` and only in `b`.
    pub(crate) fn left_right(&self, a: &str, b: &str) -> Result<(u32, u32)> {
        let out = self.run(&["rev-list", "--left-right", "--count", &format!("{a}...{b}")])?;
        let mut it = out
            .split_whitespace()
            .map(|n| n.parse::<u32>().unwrap_or(0));
        Ok((it.next().unwrap_or(0), it.next().unwrap_or(0)))
    }
}

/// Parses `git status --porcelain=v1 -z`. A rename carries its old path as an extra entry.
pub(crate) fn parse_status(out: &str) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut entries = out.split('\0').filter(|e| !e.is_empty());
    while let Some(e) = entries.next() {
        if e.len() < 4 {
            continue;
        }
        let (xy, path) = e.split_at(3);
        let xy = &xy[..2];
        if xy.contains('R') || xy.contains('C') {
            entries.next();
        }
        changes.push(Change {
            code: xy.trim().to_string(),
            path: path.to_string(),
        });
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_status() {
        let out = " M app/a.rb\0?? notes/plan.md\0R  new.rb\0old.rb\0MM both.rb\0";
        let got = parse_status(out);
        let codes: Vec<(&str, &str)> = got
            .iter()
            .map(|c| (c.code.as_str(), c.path.as_str()))
            .collect();
        assert_eq!(
            codes,
            [
                ("M", "app/a.rb"),
                ("??", "notes/plan.md"),
                ("R", "new.rb"),
                ("MM", "both.rb")
            ]
        );
    }
}
