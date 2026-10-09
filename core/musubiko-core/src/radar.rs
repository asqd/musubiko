//! The hotfix radar: for each `hotfix/X` + `hotfix/X-dev` pair, which commits of the master half
//! already have an equivalent on the develop side, and which commits reached master but not
//! develop at all.
//!
//! Equivalence is by `git patch-id`: a cherry-picked commit has a different hash but the same
//! patch, so it counts as carried over. A cherry-pick whose conflicts were resolved by hand
//! changes the patch and shows up as missing.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use serde::Serialize;

use crate::{Branch, Commit, Error, Location, Repo, Result, Status};

/// Pairs whose newest tip is older than this are left out.
const RECENT_DAYS: i64 = 120;
/// How far back to look for commits that are in master and not in develop.
const LOST_DAYS: u32 = 120;
/// develop this much older than master means the repository does not use develop.
const STALE_DEVELOP_DAYS: i64 = 90;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ported {
    pub hash: String,
    pub short: String,
    /// The `-dev` branch or the develop branch where the equivalent commit is.
    pub place: String,
    /// No single equivalent commit, but the changes are already there (squash merges).
    pub by_content: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairCommit {
    pub commit: Commit,
    pub ported: Option<Ported>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PairState {
    /// No commits in the master half yet.
    Empty,
    /// Commits are carried over; nothing is merged yet.
    Work,
    /// Some commits of the master half are not on the develop side.
    Unsynced,
    /// There is no `-dev` branch and the fix is not in develop.
    NoDev,
    /// Merged into master, no `-dev` branch, not in develop: the fix will be lost on release.
    Lost,
    /// Merged into master and carried over; the `-dev` half waits for its merge.
    Released,
    Done,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotfixPair {
    pub slug: String,
    pub master_branch: String,
    pub dev_branch: String,
    pub master: Option<Branch>,
    pub dev: Option<Branch>,
    /// Own commits of the master half, oldest first.
    pub commits: Vec<PairCommit>,
    pub unsynced: u32,
    pub state: PairState,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Radar {
    pub pairs: Vec<HotfixPair>,
    /// Commits in master with no equivalent in develop that belong to none of the pairs shown.
    pub lost: Vec<Commit>,
    /// Pairs left out as older than [`RECENT_DAYS`].
    pub older_hidden: u32,
    /// Days develop's last commit lags behind master's, when it is long enough to mean the
    /// repository lives without develop; the master-only check is skipped then.
    pub develop_stale_days: Option<i64>,
    pub recent_days: i64,
    pub master: String,
    pub develop: String,
}

impl Repo {
    pub fn hotfix_radar(&self) -> Result<Radar> {
        let flow = self.flow.clone();
        let refs = self.refs()?;
        let master_ref = self
            .base_ref(&flow.master, &refs)
            .ok_or_else(|| Error::NoBranch(flow.master.clone()))?;
        let develop_ref = self
            .base_ref(&flow.develop, &refs)
            .ok_or_else(|| Error::NoBranch(flow.develop.clone()))?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let slug_of = |name: &str| {
            let s = &name[flow.hotfix_prefix.len()..];
            s.strip_suffix(&flow.dev_suffix).unwrap_or(s).to_string()
        };
        // Newest tip per pair straight from the refs, so statuses are computed only for pairs
        // recent enough to show.
        let mut newest: HashMap<String, i64> = HashMap::new();
        for (name, r) in refs.iter().filter(|(n, _)| flow.is_hotfix(n)) {
            let date = r
                .local
                .iter()
                .chain(r.remote.iter())
                .map(|c| c.date)
                .max()
                .unwrap_or(0);
            let e = newest.entry(slug_of(name)).or_insert(0);
            *e = (*e).max(date);
        }
        newest.retain(|slug, _| !slug.is_empty());
        let recent: HashSet<&str> = newest
            .iter()
            .filter(|(_, d)| now - **d <= RECENT_DAYS * 86_400)
            .map(|(s, _)| s.as_str())
            .collect();
        let older_hidden = (newest.len() - recent.len()) as u32;
        let branches = self.branches_where(&flow.develop, |n| {
            flow.is_hotfix(n) && recent.contains(slug_of(n).as_str())
        })?;
        let by_name: HashMap<&str, &Branch> =
            branches.iter().map(|b| (b.name.as_str(), b)).collect();
        let slugs: BTreeSet<String> = recent.iter().map(|s| s.to_string()).collect();

        let mut found: Vec<(String, Option<Branch>, Option<Branch>)> = Vec::new();
        for slug in slugs {
            let m = by_name
                .get(format!("{}{slug}", flow.hotfix_prefix).as_str())
                .map(|b| (*b).clone());
            let d = by_name
                .get(format!("{}{slug}{}", flow.hotfix_prefix, flow.dev_suffix).as_str())
                .map(|b| (*b).clone());
            if m.is_some() || d.is_some() {
                found.push((slug, m, d));
            }
        }

        // Own commits of each half: the commits it adds to its base.
        let ranges: Vec<_> = found
            .par_iter()
            .map(|(_, m, d)| -> Result<_> {
                let m_range = match m {
                    Some(b) => self.own_range(b, &master_ref)?,
                    None => (None, None),
                };
                let d_range = match d {
                    Some(b) => self.own_range(b, &develop_ref)?.0,
                    None => None,
                };
                Ok((m_range, d_range))
            })
            .collect::<Result<_>>()?;

        // Develop's recent history, once for all pairs.
        let oldest = ranges
            .iter()
            .filter_map(|((r, _), _)| r.as_ref())
            .filter_map(|r| {
                self.log(&["--no-merges", "--reverse", r])
                    .ok()
                    .and_then(|c| c.first().map(|c| c.date))
            })
            .min();
        let develop_ids: HashMap<String, String> = match oldest {
            Some(t) => self
                .patch_ids(&[
                    "--max-count=5000",
                    &format!("--since={}", t - 7 * 86_400),
                    &develop_ref,
                ])?
                .into_iter()
                .collect(),
            None => HashMap::new(),
        };

        let mut pairs: Vec<HotfixPair> = found
            .into_par_iter()
            .zip(ranges)
            .map(
                |((slug, master, dev), ((m_range, note), d_range))| -> Result<HotfixPair> {
                    let commits = match &m_range {
                        Some(r) => self.log(&["--no-merges", "--reverse", r])?,
                        None => Vec::new(),
                    };
                    let own_ids: HashMap<String, String> = match &m_range {
                        Some(r) => self
                            .patch_ids(&[r])?
                            .into_iter()
                            .map(|(p, c)| (c, p))
                            .collect(),
                        None => HashMap::new(),
                    };
                    let dev_ids: HashMap<String, String> = match &d_range {
                        Some(r) => self.patch_ids(&[r])?.into_iter().collect(),
                        None => HashMap::new(),
                    };
                    let dev_branch =
                        format!("{}{slug}{}", self.flow.hotfix_prefix, self.flow.dev_suffix);
                    let commits: Vec<PairCommit> = commits
                        .into_iter()
                        .map(|commit| {
                            let pid = own_ids.get(&commit.hash);
                            let ported = pid.and_then(|p| {
                                dev_ids
                                    .get(p)
                                    .map(|h| (h, dev_branch.clone()))
                                    .or_else(|| {
                                        develop_ids.get(p).map(|h| (h, self.flow.develop.clone()))
                                    })
                                    .map(|(h, place)| Ported {
                                        short: h[..7.min(h.len())].to_string(),
                                        hash: h.clone(),
                                        place,
                                        by_content: false,
                                    })
                            });
                            PairCommit { commit, ported }
                        })
                        .collect();
                    // No twin by patch: look at content, which survives squash merges.
                    let mut commits = commits;
                    if commits.iter().any(|c| c.ported.is_none()) {
                        let mut probes = Vec::new();
                        if let Some(d) = &dev {
                            probes.push((
                                ContentProbe::new(self, &self.ref_of(d))?,
                                dev_branch.clone(),
                            ));
                        }
                        probes.push((
                            ContentProbe::new(self, &develop_ref)?,
                            self.flow.develop.clone(),
                        ));
                        for c in commits.iter_mut().filter(|c| c.ported.is_none()) {
                            for (probe, place) in &probes {
                                if probe.has(&c.commit.hash)? {
                                    c.ported = Some(Ported {
                                        hash: String::new(),
                                        short: String::new(),
                                        place: place.clone(),
                                        by_content: true,
                                    });
                                    break;
                                }
                            }
                        }
                    }
                    let unsynced = commits.iter().filter(|c| c.ported.is_none()).count() as u32;
                    let merged = |b: &Option<Branch>| {
                        b.as_ref()
                            .is_some_and(|b| matches!(b.status, Status::Merged | Status::Squashed))
                    };
                    let (m_merged, d_merged) = (merged(&master), merged(&dev));
                    let state = if commits.is_empty() {
                        if m_merged && d_merged {
                            PairState::Done
                        } else {
                            PairState::Empty
                        }
                    } else if unsynced > 0 && dev.is_none() {
                        if m_merged {
                            PairState::Lost
                        } else {
                            PairState::NoDev
                        }
                    } else if unsynced > 0 {
                        PairState::Unsynced
                    } else if m_merged && (d_merged || dev.is_none()) {
                        PairState::Done
                    } else if m_merged {
                        PairState::Released
                    } else {
                        PairState::Work
                    };
                    let note = note.or_else(|| {
                        master
                            .is_none()
                            .then(|| "Master-часть удалена: сверять не с чем".to_string())
                    });
                    Ok(HotfixPair {
                        master_branch: format!("{}{slug}", self.flow.hotfix_prefix),
                        dev_branch,
                        slug,
                        master,
                        dev,
                        commits,
                        unsynced,
                        state,
                        note,
                    })
                },
            )
            .collect::<Result<_>>()?;
        pairs.sort_by_key(|p| {
            std::cmp::Reverse(
                p.master
                    .iter()
                    .chain(p.dev.iter())
                    .map(|b| b.tip.date)
                    .max()
                    .unwrap_or(0),
            )
        });

        let tip_date = |r: &str| -> Result<i64> {
            Ok(self
                .run(&["log", "-1", "--format=%ct", r])?
                .trim()
                .parse()
                .unwrap_or(0))
        };
        let lag = (tip_date(&master_ref)? - tip_date(&develop_ref)?) / 86_400;
        let develop_stale_days = (lag > STALE_DEVELOP_DAYS).then_some(lag);

        let in_pairs: HashSet<&str> = pairs
            .iter()
            .flat_map(|p| p.commits.iter().map(|c| c.commit.hash.as_str()))
            .collect();
        let candidates: Vec<Commit> = if develop_stale_days.is_some() {
            Vec::new()
        } else {
            self.log(&[
                "--cherry-pick",
                "--right-only",
                "--no-merges",
                "--max-count=300",
                &format!("--since={LOST_DAYS}.days"),
                &format!("{develop_ref}...{master_ref}"),
            ])?
            .into_iter()
            .filter(|c| !in_pairs.contains(c.hash.as_str()))
            .collect()
        };
        let lost = if candidates.is_empty() || develop_stale_days.is_some() {
            Vec::new()
        } else {
            let probe = ContentProbe::new(self, &develop_ref)?;
            let mut lost = Vec::new();
            for c in candidates {
                if !probe.has(&c.hash)? {
                    lost.push(c);
                }
            }
            lost
        };

        Ok(Radar {
            pairs,
            lost,
            older_hidden,
            develop_stale_days,
            recent_days: RECENT_DAYS,
            master: flow.master,
            develop: flow.develop,
        })
    }

    fn ref_of(&self, b: &Branch) -> String {
        match (b.location, &self.remote) {
            (Location::Remote, Some(r)) => format!("refs/remotes/{r}/{}", b.name),
            _ => format!("refs/heads/{}", b.name),
        }
    }

    /// The revision range of commits a branch adds to `base`, also after it was merged:
    /// before the merge it is `base..branch`; after a merge commit it is `M^1..branch`;
    /// after a fast-forward the branch's reflog tells where it was created.
    fn own_range(&self, b: &Branch, base: &str) -> Result<(Option<String>, Option<String>)> {
        let r = self.ref_of(b);
        if b.ahead > 0 {
            return Ok((Some(format!("{base}..{r}")), None));
        }
        let merges = self.run(&[
            "rev-list",
            "--ancestry-path",
            "--merges",
            "--reverse",
            "--max-count=200",
            &format!("{r}..{base}"),
        ])?;
        for m in merges.lines() {
            let p1 = format!("{m}^1");
            if !self.is_ancestor(&r, &p1)? {
                return Ok((Some(format!("{p1}..{r}")), None));
            }
        }
        if b.location != Location::Remote {
            let out = self.git.output(
                &self.root,
                &[
                    "reflog",
                    "show",
                    "--format=%H",
                    &format!("refs/heads/{}", b.name),
                ],
            )?;
            let created = String::from_utf8_lossy(&out.stdout)
                .lines()
                .last()
                .map(str::to_string);
            if let Some(created) = created.filter(|c| *c != b.tip.hash) {
                return Ok((Some(format!("{created}..{r}")), None));
            }
            if out.status.success() {
                return Ok((None, None));
            }
        }
        Ok((
            None,
            Some("Не удалось определить коммиты хотфикса: ветка влита без merge-коммита".into()),
        ))
    }

    pub(crate) fn is_ancestor(&self, a: &str, b: &str) -> Result<bool> {
        Ok(self
            .git
            .output(&self.root, &["merge-base", "--is-ancestor", a, b])?
            .status
            .success())
    }

    /// `(patch id, commit)` for each non-merge commit `git log` selects with `args`.
    pub(crate) fn patch_ids(&self, args: &[&str]) -> Result<Vec<(String, String)>> {
        let mut all = vec![
            "log",
            "-p",
            "--no-merges",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--format=commit %H",
        ];
        all.extend_from_slice(args);
        let patch = self.git.output(&self.root, &all)?;
        if !patch.status.success() || patch.stdout.is_empty() {
            return Ok(Vec::new());
        }
        let out = self
            .git
            .run_with_stdin(&self.root, &["patch-id", "--stable"], patch.stdout)?;
        Ok(out
            .lines()
            .filter_map(|l| {
                l.split_once(' ')
                    .map(|(p, c)| (p.to_string(), c.to_string()))
            })
            .collect())
    }
}

/// Answers "are this commit's changes already in that tree?" by reverse-applying the commit's
/// patch to the tree in a scratch index. The repository's own index is never touched.
struct ContentProbe<'a> {
    repo: &'a Repo,
    index: std::path::PathBuf,
}

impl<'a> ContentProbe<'a> {
    fn new(repo: &'a Repo, target: &str) -> Result<ContentProbe<'a>> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let index =
            std::env::temp_dir().join(format!("musubiko-probe-{}-{n}.index", std::process::id()));
        let probe = ContentProbe { repo, index };
        let out = repo.git.output_with(
            &repo.root,
            &["read-tree", target],
            &[("GIT_INDEX_FILE", &probe.index)],
            Vec::new(),
        )?;
        if !out.status.success() {
            return Err(Error::Command {
                args: format!("read-tree {target}"),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(probe)
    }

    fn has(&self, commit: &str) -> Result<bool> {
        let r = self.repo;
        let patch = r.git.output(
            &r.root,
            &[
                "diff",
                "--binary",
                "--full-index",
                "--no-color",
                "--no-ext-diff",
                &format!("{commit}^"),
                commit,
            ],
        )?;
        if !patch.status.success() || patch.stdout.is_empty() {
            return Ok(false);
        }
        let out = r.git.output_with(
            &r.root,
            &["apply", "--cached", "--check", "-R", "--whitespace=nowarn"],
            &[("GIT_INDEX_FILE", &self.index)],
            patch.stdout,
        )?;
        Ok(out.status.success())
    }
}

impl Drop for ContentProbe<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.index);
    }
}
