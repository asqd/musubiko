use std::collections::HashMap;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cache::{self, CacheStats, CachedStatus, CachedSync, StatusCache, status_key};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sync {
    /// Local commits not pushed.
    pub ahead: u32,
    /// Remote commits not pulled.
    pub behind: u32,
}

/// How a branch relates to its target base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The local branch's upstream was deleted on the server, usually after its merge request.
    pub upstream_gone: bool,
}

impl Repo {
    /// All branches except the two bases, newest first, each compared with its target:
    /// `base` for ordinary branches, the fixed base for hotfix halves.
    pub fn branches(&self, base: &str) -> Result<Vec<Branch>> {
        Ok(self.branches_with_stats(base)?.0)
    }

    /// Like [`Repo::branches`], also telling how much came from the status cache.
    pub fn branches_with_stats(&self, base: &str) -> Result<(Vec<Branch>, CacheStats)> {
        self.branches_where_stats(base, |_| true)
    }

    /// Like [`Repo::branches`], for the branches whose name passes `keep`.
    pub(crate) fn branches_where(
        &self,
        base: &str,
        keep: impl Fn(&str) -> bool,
    ) -> Result<Vec<Branch>> {
        Ok(self.branches_where_stats(base, keep)?.0)
    }

    fn branches_where_stats(
        &self,
        base: &str,
        keep: impl Fn(&str) -> bool,
    ) -> Result<(Vec<Branch>, CacheStats)> {
        let refs = self.refs()?;
        let head = self
            .run(&["symbolic-ref", "--short", "-q", "HEAD"])
            .ok()
            .map(|s| s.trim().to_string());

        // Resolve each target once: its full ref, tip and tree.
        let mut targets: HashMap<String, Option<Target>> = HashMap::new();
        for name in [base, &self.flow.master, &self.flow.develop] {
            if targets.contains_key(name) {
                continue;
            }
            let resolved = match self.base_ref(name, &refs) {
                Some(r) => {
                    let out = self.run(&["rev-parse", &r, &format!("{r}^{{tree}}")])?;
                    let mut lines = out.lines();
                    let tip = lines.next().unwrap_or_default().to_string();
                    let tree = lines.next().unwrap_or_default().to_string();
                    Some(Target {
                        refname: r,
                        tip,
                        tree,
                    })
                }
                None => None,
            };
            targets.insert(name.to_string(), resolved);
        }

        let cache_path = self.cache_path();
        let mut cache = cache_path
            .as_deref()
            .map(StatusCache::load)
            .unwrap_or_default();
        let list: Vec<(&String, &Refs)> = refs
            .iter()
            .filter(|(n, _)| !self.flow.is_base(n) && keep(n))
            .collect();
        let computed: Vec<Computed> = list
            .par_iter()
            .map(|(name, r)| self.branch(name, r, base, &targets, head.as_deref(), &cache))
            .collect::<Result<_>>()?;

        let mut stats = CacheStats::default();
        let (mut hit_status, mut hit_sync) = (Vec::new(), Vec::new());
        let mut branches = Vec::with_capacity(computed.len());
        for c in computed {
            match c.status {
                Lookup::Hit(k) => {
                    stats.hits += 1;
                    hit_status.push(k);
                }
                Lookup::Computed(k, e) => {
                    stats.computed += 1;
                    cache.put_status(k, e);
                }
                Lookup::None => stats.computed += 1,
            }
            match c.sync {
                Lookup::Hit(k) => hit_sync.push(k),
                Lookup::Computed(k, e) => cache.put_sync(k, e),
                Lookup::None => {}
            }
            branches.push(c.branch);
        }
        cache.touch(&hit_status, &hit_sync);
        if let Some(p) = cache_path {
            cache.save(&p);
        }

        branches.sort_by(|a, b| {
            b.tip
                .date
                .cmp(&a.tip.date)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok((branches, stats))
    }

    fn branch(
        &self,
        name: &str,
        r: &Refs,
        base: &str,
        targets: &HashMap<String, Option<Target>>,
        head: Option<&str>,
        cache: &StatusCache,
    ) -> Result<Computed> {
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

        // Local against remote, cached by the two tips.
        let mut sync_lookup = Lookup::None;
        let sync = match (&r.local, &r.remote) {
            (Some(l), Some(rm)) if l.hash != rm.hash => {
                let key = status_key(&l.hash, &rm.hash);
                match cache.sync(&key) {
                    Some(e) => {
                        sync_lookup = Lookup::Hit(key);
                        e.sync
                    }
                    None => {
                        let s = self.sync(r)?;
                        sync_lookup = Lookup::Computed(
                            key,
                            CachedSync {
                                sync: s,
                                used: cache::now(),
                            },
                        );
                        s
                    }
                }
            }
            _ => None,
        };

        let target = self.flow.target_for(name, base).to_string();
        let mut out = Branch {
            name: name.to_string(),
            owner: tip.author.clone(),
            tip,
            location,
            sync,
            target: target.clone(),
            status: Status::Unknown {
                error: format!("Нет ветки {target}"),
            },
            ahead: 0,
            behind: 0,
            is_head: head == Some(name),
            upstream_gone: r.upstream_gone,
        };
        let Some(Some(t)) = targets.get(&target) else {
            return Ok(Computed {
                branch: out,
                status: Lookup::None,
                sync: sync_lookup,
            });
        };

        let key = status_key(&out.tip.hash, &t.tip);
        if let Some(e) = cache.status(&key) {
            out.status = e.status.clone();
            out.ahead = e.ahead;
            out.behind = e.behind;
            if let Some(o) = &e.owner {
                out.owner = o.clone();
            }
            return Ok(Computed {
                branch: out,
                status: Lookup::Hit(key),
                sync: sync_lookup,
            });
        }

        let (behind, ahead) = self.left_right(&t.refname, &branch_ref)?;
        out.ahead = ahead;
        out.behind = behind;
        let mut owner = None;
        if ahead == 0 {
            out.status = Status::Merged;
        } else {
            owner = self.first_author(&t.refname, &branch_ref)?;
            if let Some(o) = &owner {
                out.owner = o.clone();
            }
            out.status = self.merge_status(&t.refname, &t.tree, &branch_ref, &target)?;
        }
        // Every outcome here, "no common history" included, depends only on the two tips.
        let status = Lookup::Computed(
            key,
            CachedStatus {
                status: out.status.clone(),
                ahead,
                behind,
                owner,
                used: cache::now(),
            },
        );
        Ok(Computed {
            branch: out,
            status,
            sync: sync_lookup,
        })
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

struct Target {
    refname: String,
    tip: String,
    tree: String,
}

enum Lookup<T> {
    Hit(String),
    Computed(String, T),
    None,
}

struct Computed {
    branch: Branch,
    status: Lookup<CachedStatus>,
    sync: Lookup<CachedSync>,
}
