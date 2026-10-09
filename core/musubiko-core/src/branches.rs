use std::collections::HashMap;

use rayon::prelude::*;
use serde::Serialize;

use crate::repo::{Refs, Repo};
use crate::{Commit, Person, Result};

/// Where a branch exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Location {
    Both,
    Local,
    Remote,
}

/// Local branch against its remote copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Sync {
    /// Local commits not pushed.
    pub ahead: u32,
    /// Remote commits not pulled.
    pub behind: u32,
}

/// How a branch relates to its target base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Status {
    /// Every commit of the branch is in the base.
    Merged,
    /// The commits differ (squash or rebase merge), but merging would not change the base:
    /// all of the branch's content is already there.
    Squashed,
    /// Merging into the base would conflict in these files.
    Conflict { files: Vec<String> },
    /// The branch has changes the base does not.
    Open,
    /// The check failed, e.g. the base is missing or histories are unrelated.
    Unknown { error: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    /// Tip of the local branch, or of the remote one when there is no local copy.
    pub tip: Commit,
    pub location: Location,
    pub sync: Option<Sync>,
    /// The base this branch was compared with.
    pub target: String,
    pub status: Status,
    /// Commits in the branch and not in the target.
    pub ahead: u32,
    /// Commits in the target and not in the branch.
    pub behind: u32,
    /// Author of the branch's first own commit; git does not record who created a branch.
    pub owner: Person,
    pub is_head: bool,
}

impl Repo {
    /// All branches except the two bases, newest first, each compared with its target:
    /// `base` for ordinary branches, the fixed base for hotfix halves.
    pub fn branches(&self, base: &str) -> Result<Vec<Branch>> {
        self.branches_where(base, |_| true)
    }

    /// Like [`Repo::branches`], for the branches whose name passes `keep`.
    pub(crate) fn branches_where(
        &self,
        base: &str,
        keep: impl Fn(&str) -> bool,
    ) -> Result<Vec<Branch>> {
        let refs = self.refs()?;
        let head = self
            .run(&["symbolic-ref", "--short", "-q", "HEAD"])
            .ok()
            .map(|s| s.trim().to_string());

        // Resolve each target once: its full ref and tree.
        let mut targets: HashMap<String, Option<(String, String)>> = HashMap::new();
        for name in [base, &self.flow.master, &self.flow.develop] {
            if targets.contains_key(name) {
                continue;
            }
            let resolved = match self.base_ref(name, &refs) {
                Some(r) => Some((
                    r.clone(),
                    self.run(&["rev-parse", &format!("{r}^{{tree}}")])?
                        .trim()
                        .to_string(),
                )),
                None => None,
            };
            targets.insert(name.to_string(), resolved);
        }

        let list: Vec<(&String, &Refs)> = refs
            .iter()
            .filter(|(n, _)| !self.flow.is_base(n) && keep(n))
            .collect();
        let mut branches: Vec<Branch> = list
            .par_iter()
            .map(|(name, r)| self.branch(name, r, base, &targets, head.as_deref()))
            .collect::<Result<_>>()?;
        branches.sort_by(|a, b| {
            b.tip
                .date
                .cmp(&a.tip.date)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(branches)
    }

    fn branch(
        &self,
        name: &str,
        r: &Refs,
        base: &str,
        targets: &HashMap<String, Option<(String, String)>>,
        head: Option<&str>,
    ) -> Result<Branch> {
        let (tip, location, branch_ref) = match (&r.local, &r.remote) {
            (Some(l), Some(_)) => (l.clone(), Location::Both, format!("refs/heads/{name}")),
            (Some(l), None) => (l.clone(), Location::Local, format!("refs/heads/{name}")),
            (None, Some(rm)) => {
                let remote = self.remote.as_deref().unwrap_or("origin");
                (
                    rm.clone(),
                    Location::Remote,
                    format!("refs/remotes/{remote}/{name}"),
                )
            }
            (None, None) => unreachable!("refs map holds at least one side"),
        };
        let target = self.flow.target_for(name, base).to_string();
        let mut out = Branch {
            name: name.to_string(),
            owner: tip.author.clone(),
            tip,
            location,
            sync: self.sync(r)?,
            target: target.clone(),
            status: Status::Unknown {
                error: format!("Нет ветки {target}"),
            },
            ahead: 0,
            behind: 0,
            is_head: head == Some(name),
        };
        let Some(Some((target_ref, target_tree))) = targets.get(&target) else {
            return Ok(out);
        };

        let (behind, ahead) = self.left_right(target_ref, &branch_ref)?;
        out.ahead = ahead;
        out.behind = behind;
        if ahead == 0 {
            out.status = Status::Merged;
            return Ok(out);
        }
        if let Some(owner) = self.first_author(target_ref, &branch_ref)? {
            out.owner = owner;
        }
        out.status = self.merge_status(target_ref, target_tree, &branch_ref, &target)?;
        Ok(out)
    }

    /// Author of the oldest commit that is in `branch` and not in `target`.
    fn first_author(&self, target: &str, branch: &str) -> Result<Option<Person>> {
        let out = self.run(&["log", "--format=%an%x00%ae", &format!("{target}..{branch}")])?;
        Ok(out.lines().rfind(|l| !l.is_empty()).map(|l| {
            let (name, email) = l.split_once('\0').unwrap_or((l, ""));
            Person {
                name: name.into(),
                email: email.into(),
            }
        }))
    }

    /// Merges in memory with `git merge-tree`; the working tree and index stay untouched.
    fn merge_status(
        &self,
        target: &str,
        target_tree: &str,
        branch: &str,
        target_name: &str,
    ) -> Result<Status> {
        let out = self.git.output(
            &self.root,
            &[
                "merge-tree",
                "--write-tree",
                "-z",
                "--name-only",
                "--no-messages",
                target,
                branch,
            ],
        )?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut parts = stdout.split(['\0', '\n']).filter(|p| !p.is_empty());
        let tree = parts.next().unwrap_or_default();
        Ok(match out.status.code() {
            Some(0) if tree == target_tree => Status::Squashed,
            Some(0) => Status::Open,
            Some(1) => {
                let mut files: Vec<String> = parts.map(str::to_string).collect();
                files.dedup();
                Status::Conflict { files }
            }
            _ => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let error = if stderr.contains("unrelated histories") {
                    format!("Нет общей истории с {target_name}")
                } else {
                    stderr.trim().to_string()
                };
                Status::Unknown { error }
            }
        })
    }
}
