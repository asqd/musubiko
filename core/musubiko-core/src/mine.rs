//! "My branches": the branches someone works on, what stands in the way of merging each one, and
//! which are merged and can go.
//!
//! A branch is someone's when they wrote its first own commit (git does not record who created
//! a branch); a merged branch has no own commits left, so its tip's author counts.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use serde::Serialize;

use crate::{Branch, Commit, Location, Person, Repo, Result, Status};

/// The base being this many commits ahead is worth pulling in before merging.
const BEHIND_ALERT: u32 = 20;
/// No commits for this long on an unmerged branch: forgotten, or waiting for review.
const QUIET_DAYS: i64 = 7;
/// Own commits listed on each card.
const RECENT: usize = 3;

/// Why a branch needs attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Attention {
    /// Merging into the target would conflict in these files.
    Conflict { files: Vec<String> },
    /// The target moved on by this many commits.
    Behind { count: u32 },
    /// Local commits the server does not have.
    Unpushed { count: u32 },
    /// The branch exists only here: never pushed.
    NeverPushed,
    /// Deleted on the server while the local branch still has work of its own.
    GoneWithWork,
    /// No commits for this many days.
    Quiet { days: i64 },
    /// The status could not be computed.
    Unknown { error: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkBranch {
    pub branch: Branch,
    /// Own commits against the target.
    pub commit_count: u32,
    /// Date of the oldest own commit.
    pub started: Option<i64>,
    /// The newest own commits, at most [`RECENT`].
    pub recent: Vec<Commit>,
    pub files: u32,
    pub added: u32,
    pub removed: u32,
    /// Empty when nothing stands in the way.
    pub attention: Vec<Attention>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    pub person: Person,
    /// Unmerged branches of this person.
    pub active: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MyBranches {
    /// Whose branches these are.
    pub author: Person,
    /// Everyone who has branches, most active first, to switch to a colleague.
    pub owners: Vec<Owner>,
    pub attention: Vec<WorkBranch>,
    pub active: Vec<WorkBranch>,
    /// Merged into their target (also by squash): safe to delete.
    pub merged: Vec<Branch>,
    pub behind_alert: u32,
    pub quiet_days: i64,
}

fn same_person(a: &Person, b: &Person) -> bool {
    if !a.email.is_empty() && !b.email.is_empty() {
        return a.email.eq_ignore_ascii_case(&b.email);
    }
    !a.name.is_empty() && a.name == b.name
}

fn owner_of(b: &Branch) -> &Person {
    if b.owner.email.is_empty() && b.owner.name.is_empty() {
        &b.tip.author
    } else {
        &b.owner
    }
}

fn merged(b: &Branch) -> bool {
    matches!(b.status, Status::Merged | Status::Squashed)
}

impl Repo {
    /// Branches of `author` (by default the repository's `user.email`), compared like the branch
    /// list: with `base`, hotfix halves with their fixed base.
    pub fn my_branches(&self, base: &str, author: Option<&str>) -> Result<MyBranches> {
        let me = self.info()?.user;
        let list = self.branches(base)?;

        let mut owners: Vec<Owner> = Vec::new();
        for b in &list {
            let p = owner_of(b);
            match owners.iter_mut().find(|o| same_person(&o.person, p)) {
                Some(o) => o.active += u32::from(!merged(b)),
                None => owners.push(Owner {
                    person: p.clone(),
                    active: u32::from(!merged(b)),
                }),
            }
        }
        owners.sort_by(|a, b| {
            b.active
                .cmp(&a.active)
                .then(a.person.name.cmp(&b.person.name))
        });

        let author = match author {
            Some(email) => owners
                .iter()
                .map(|o| o.person.clone())
                .find(|p| p.email.eq_ignore_ascii_case(email) || p.name == email)
                .unwrap_or(Person {
                    name: String::new(),
                    email: email.to_string(),
                }),
            None => me,
        };
        let theirs: Vec<Branch> = list
            .into_iter()
            .filter(|b| same_person(owner_of(b), &author))
            .collect();
        let (done, work): (Vec<Branch>, Vec<Branch>) = theirs.into_iter().partition(merged);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let details: Vec<WorkBranch> = work
            .into_par_iter()
            .map(|b| -> Result<WorkBranch> {
                let (branch_ref, _, target_ref) = self.resolve(&b.name, &b.target)?;
                let range = format!("{target_ref}..{branch_ref}");
                let commit_count: u32 = self
                    .run(&["rev-list", "--count", "--no-merges", &range])?
                    .trim()
                    .parse()
                    .unwrap_or(0);
                let recent =
                    self.log(&["--no-merges", &format!("--max-count={RECENT}"), &range])?;
                let started = if commit_count as usize <= RECENT {
                    recent.last().map(|c| c.date)
                } else {
                    self.run(&["log", "--no-merges", "--reverse", "--format=%ct", &range])?
                        .lines()
                        .next()
                        .and_then(|l| l.trim().parse().ok())
                };
                let (files, added, removed) = self.shortstat(&target_ref, &branch_ref)?;

                let mut attention = Vec::new();
                match &b.status {
                    Status::Conflict { files } => attention.push(Attention::Conflict {
                        files: files.clone(),
                    }),
                    Status::Unknown { error } => attention.push(Attention::Unknown {
                        error: error.clone(),
                    }),
                    _ => {}
                }
                // A new branch has nothing to fall behind with yet, and its date is the base's.
                let new = b.status == Status::New;
                if b.behind >= BEHIND_ALERT && !new {
                    attention.push(Attention::Behind { count: b.behind });
                }
                if let Some(s) = b.sync {
                    if s.ahead > 0 {
                        attention.push(Attention::Unpushed { count: s.ahead });
                    }
                }
                if b.location == Location::Local && !b.upstream_gone && self.remote.is_some() {
                    attention.push(Attention::NeverPushed);
                }
                if b.upstream_gone && commit_count > 0 {
                    attention.push(Attention::GoneWithWork);
                }
                let days = (now - b.tip.date) / 86_400;
                if days >= QUIET_DAYS && !new {
                    attention.push(Attention::Quiet { days });
                }
                Ok(WorkBranch {
                    branch: b,
                    commit_count,
                    started,
                    recent,
                    files,
                    added,
                    removed,
                    attention,
                })
            })
            .collect::<Result<_>>()?;
        let (attention, active): (Vec<WorkBranch>, Vec<WorkBranch>) =
            details.into_iter().partition(|w| !w.attention.is_empty());

        Ok(MyBranches {
            author,
            owners,
            attention,
            active,
            merged: done,
            behind_alert: BEHIND_ALERT,
            quiet_days: QUIET_DAYS,
        })
    }

    /// Files changed and lines added and removed since `branch` left `target`.
    fn shortstat(&self, target: &str, branch: &str) -> Result<(u32, u32, u32)> {
        let mb = self
            .git
            .output(&self.root, &["merge-base", target, branch])?;
        if !mb.status.success() {
            return Ok((0, 0, 0));
        }
        let mb = String::from_utf8_lossy(&mb.stdout).trim().to_string();
        let out = self.run(&["diff", "--shortstat", &mb, branch])?;
        // " 3 files changed, 10 insertions(+), 2 deletions(-)"
        let mut n: HashMap<&str, u32> = HashMap::new();
        for part in out.split(',') {
            let mut w = part.split_whitespace();
            if let (Some(v), Some(k)) = (w.next(), w.next()) {
                let key = if k.starts_with("file") {
                    "files"
                } else if k.starts_with("insertion") {
                    "added"
                } else {
                    "removed"
                };
                n.insert(key, v.parse().unwrap_or(0));
            }
        }
        Ok((
            n.get("files").copied().unwrap_or(0),
            n.get("added").copied().unwrap_or(0),
            n.get("removed").copied().unwrap_or(0),
        ))
    }
}
