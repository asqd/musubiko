//! "Where is this commit?": by hash, ticket key or message text, find commits and tell whether
//! each one is in master and develop (as itself, as a cherry-pick, or by content), which
//! branches hold it, which tag first shipped it, and where it was cherry-picked to.

use std::collections::{BTreeSet, HashMap};

use rayon::prelude::*;
use serde::Serialize;

use crate::radar::{ContentProbe, STALE_DEVELOP_DAYS};
use crate::{Commit, Error, Repo, Result};

const MAX_RESULTS: usize = 30;

/// How a base branch holds a commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Presence {
    /// The commit itself is in the branch's history.
    Itself,
    /// A commit with the same patch is there (cherry-pick).
    Copy {
        hash: String,
        short: String,
    },
    /// No single commit matches, but the changes are there (squash merge).
    Content,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// In master and develop.
    Everywhere,
    /// In master, not in develop: the fix will be lost on the next release.
    MasterOnly,
    /// In develop, waiting for a release to master.
    DevelopOnly,
    /// Only in work branches so far.
    BranchesOnly,
    /// Nowhere reachable from a branch (a dangling or deleted-branch commit).
    Nowhere,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitPlace {
    pub commit: Commit,
    pub is_merge: bool,
    pub master: Presence,
    pub develop: Option<Presence>,
    /// First tag that contains the commit, when it is in master.
    pub first_tag: Option<String>,
    /// Work branches (not the bases) that contain the commit, local and remote merged by name.
    pub branches: Vec<String>,
    /// Commits made from this one with `cherry-pick -x`.
    pub cherry_picks: Vec<Commit>,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSearch {
    pub query: String,
    pub results: Vec<CommitPlace>,
    /// More commits matched than are shown.
    pub truncated: bool,
    pub master: String,
    pub develop: Option<String>,
    /// develop exists but is abandoned; develop presence is not reported then.
    pub develop_stale_days: Option<i64>,
}

impl Repo {
    pub fn locate(&self, query: &str) -> Result<CommitSearch> {
        let q = query.trim();
        if q.is_empty() {
            return Err(Error::Refused(
                "Введите хэш, номер задачи или текст из сообщения коммита".into(),
            ));
        }
        let refs = self.refs()?;
        let flow = &self.flow;
        let master_ref = self
            .base_ref(&flow.master, &refs)
            .ok_or_else(|| Error::NoBranch(flow.master.clone()))?;
        let develop_ref = self.base_ref(&flow.develop, &refs);
        let develop_stale_days = match &develop_ref {
            Some(d) => {
                let date = |r: &str| -> Result<i64> {
                    Ok(self
                        .run(&["log", "-1", "--format=%ct", r])?
                        .trim()
                        .parse()
                        .unwrap_or(0))
                };
                let lag = (date(&master_ref)? - date(d)?) / 86_400;
                (lag > STALE_DEVELOP_DAYS).then_some(lag)
            }
            None => None,
        };
        let develop_ref = develop_ref.filter(|_| develop_stale_days.is_none());

        let (mut commits, truncated) = self.find_commits(q, &refs)?;
        commits.sort_by(|a, b| b.date.cmp(&a.date));

        // Patch ids of recent history on each base, to recognise cherry-picks.
        let oldest = commits.iter().map(|c| c.date).min().unwrap_or(0);
        let window = |r: &str| -> Result<HashMap<String, String>> {
            Ok(self
                .patch_ids(&[
                    "--max-count=5000",
                    &format!("--since={}", oldest - 7 * 86_400),
                    r,
                ])?
                .into_iter()
                .collect())
        };
        let master_ids = window(&master_ref)?;
        let develop_ids = match &develop_ref {
            Some(d) => window(d)?,
            None => HashMap::new(),
        };

        let mut results: Vec<CommitPlace> = commits
            .into_par_iter()
            .map(|commit| -> Result<CommitPlace> {
                let parents = self.run(&["rev-list", "--parents", "-n", "1", &commit.hash])?;
                let is_merge = parents.split_whitespace().count() > 2;
                let own_pid = if is_merge {
                    None
                } else {
                    self.patch_ids(&["-1", &commit.hash])?
                        .into_iter()
                        .next()
                        .map(|(p, _)| p)
                };
                let quick = |base: &str, ids: &HashMap<String, String>| -> Result<Presence> {
                    if self.is_ancestor(&commit.hash, base)? {
                        return Ok(Presence::Itself);
                    }
                    Ok(match own_pid.as_ref().and_then(|p| ids.get(p)) {
                        Some(h) => Presence::Copy {
                            short: h[..7.min(h.len())].to_string(),
                            hash: h.clone(),
                        },
                        None => Presence::Missing,
                    })
                };
                let master = quick(&master_ref, &master_ids)?;
                let develop = match &develop_ref {
                    Some(d) => Some(quick(d, &develop_ids)?),
                    None => None,
                };
                let first_tag = if master == Presence::Itself {
                    self.run(&[
                        "tag",
                        "--contains",
                        &commit.hash,
                        "--sort=creatordate",
                        "--format=%(refname:short)",
                    ])?
                    .lines()
                    .next()
                    .map(str::to_string)
                } else {
                    None
                };
                let branches = self.branches_containing(&commit.hash)?;
                let cherry_picks = self.log(&[
                    "--all",
                    "--fixed-strings",
                    &format!("--grep=cherry picked from commit {}", commit.hash),
                    "--max-count=20",
                ])?;
                Ok(CommitPlace {
                    commit,
                    is_merge,
                    master,
                    develop,
                    first_tag,
                    branches,
                    cherry_picks,
                    verdict: Verdict::Nowhere,
                })
            })
            .collect::<Result<_>>()?;

        // Content checks for whatever is still missing; one scratch index per base.
        let master_probe = if results
            .iter()
            .any(|r| r.master == Presence::Missing && !r.is_merge)
        {
            Some(ContentProbe::new(self, &master_ref)?)
        } else {
            None
        };
        let develop_probe = match &develop_ref {
            Some(d)
                if results
                    .iter()
                    .any(|r| r.develop == Some(Presence::Missing) && !r.is_merge) =>
            {
                Some(ContentProbe::new(self, d)?)
            }
            _ => None,
        };
        for r in &mut results {
            if r.is_merge {
                continue;
            }
            if let (Presence::Missing, Some(p)) = (&r.master, &master_probe) {
                if p.has(&r.commit.hash)? {
                    r.master = Presence::Content;
                }
            }
            if let (Some(Presence::Missing), Some(p)) = (&r.develop, &develop_probe) {
                if p.has(&r.commit.hash)? {
                    r.develop = Some(Presence::Content);
                }
            }
        }

        for r in &mut results {
            let in_master = r.master != Presence::Missing;
            let in_develop = r.develop.as_ref().is_some_and(|d| *d != Presence::Missing);
            r.verdict = match (in_master, in_develop, develop_ref.is_some()) {
                (true, true, _) | (true, false, false) => Verdict::Everywhere,
                (true, false, true) => Verdict::MasterOnly,
                (false, true, _) => Verdict::DevelopOnly,
                (false, false, _) if !r.branches.is_empty() => Verdict::BranchesOnly,
                _ => Verdict::Nowhere,
            };
        }

        Ok(CommitSearch {
            query: q.to_string(),
            results,
            truncated,
            master: flow.master.clone(),
            develop: refs
                .contains_key(&flow.develop)
                .then(|| flow.develop.clone()),
            develop_stale_days,
        })
    }

    /// Commits matching the query: a hash, or message text and branch names (a ticket key is
    /// often only in the branch name).
    fn find_commits(
        &self,
        q: &str,
        refs: &std::collections::BTreeMap<String, crate::repo::Refs>,
    ) -> Result<(Vec<Commit>, bool)> {
        if q.len() >= 4 && q.chars().all(|c| c.is_ascii_hexdigit()) {
            let out = self.git.output(
                &self.root,
                &["rev-parse", "--verify", "-q", &format!("{q}^{{commit}}")],
            )?;
            if out.status.success() {
                let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
                return Ok((self.log(&["-1", &hash])?, false));
            }
        }
        let mut found: Vec<Commit> = self.log(&[
            "--all",
            "--no-merges",
            "--regexp-ignore-case",
            "--fixed-strings",
            &format!("--grep={q}"),
            &format!("--max-count={}", MAX_RESULTS + 1),
        ])?;
        let ql = q.to_lowercase();
        // A branch whose name matches contributes its own commits, also after it was merged.
        let named = self.branches_where(&self.flow.develop, |n| n.to_lowercase().contains(&ql))?;
        for b in named.iter().take(10) {
            let Some(target_ref) = self.base_ref(&b.target, refs) else {
                continue;
            };
            if let (Some(range), _) = self.own_range(b, &target_ref)? {
                found.extend(self.log(&["--no-merges", "--max-count=20", &range])?);
            }
        }
        let mut seen = BTreeSet::new();
        found.retain(|c| seen.insert(c.hash.clone()));
        let truncated = found.len() > MAX_RESULTS;
        found.truncate(MAX_RESULTS);
        Ok((found, truncated))
    }

    /// Work branches holding the commit; `origin/x` and `x` count once.
    fn branches_containing(&self, hash: &str) -> Result<Vec<String>> {
        let mut args = vec![
            "for-each-ref",
            "--contains",
            hash,
            "--format=%(refname)",
            "refs/heads",
        ];
        let remote_prefix = self.remote.as_ref().map(|r| format!("refs/remotes/{r}/"));
        if let Some(p) = &remote_prefix {
            args.push(p);
        }
        let out = self.run(&args)?;
        let mut names = BTreeSet::new();
        for line in out.lines() {
            let name = line
                .strip_prefix("refs/heads/")
                .or_else(|| remote_prefix.as_deref().and_then(|p| line.strip_prefix(p)));
            if let Some(n) = name.filter(|n| *n != "HEAD" && !self.flow.is_base(n)) {
                names.insert(n.to_string());
            }
        }
        Ok(names.into_iter().collect())
    }
}
