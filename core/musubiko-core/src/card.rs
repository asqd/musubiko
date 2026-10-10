//! Everything about one branch on one page: how it stands against each base, its own commits,
//! the files it changes and how it is kept on the server.

use serde::Serialize;

use crate::{Branch, Commit, Error, Repo, Result};

/// Own commits shown at most; the count says how many there are in all.
const COMMITS_SHOWN: usize = 200;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub path: String,
    /// The old name when the file was renamed.
    pub old_path: Option<String>,
    /// `None` for binary files.
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCard {
    /// Compared with its target: the chosen base, or the fixed one for hotfix halves.
    pub branch: Branch,
    /// The same branch compared with each base the repository has (master/main, develop),
    /// and with the chosen base when it is another branch.
    pub against: Vec<Branch>,
    /// Own commits against the target, newest first, at most [`COMMITS_SHOWN`].
    pub commits: Vec<Commit>,
    pub commit_count: u32,
    /// Date of the oldest own commit: roughly when work on the branch started.
    pub started: Option<i64>,
    /// Files changed since the branch left its target, as a pull request would show them.
    pub files: Vec<FileStat>,
    /// `origin/feature/x`, when the local branch tracks one.
    pub upstream: Option<String>,
}

impl Repo {
    pub fn branch_card(&self, name: &str, base: &str) -> Result<BranchCard> {
        let branch = self
            .branches_where(base, |n| n == name)?
            .pop()
            .ok_or_else(|| Error::NoBranch(name.into()))?;

        let refs = self.refs()?;
        let mut against = Vec::new();
        let mut bases: Vec<&str> = vec![&self.flow.master, &self.flow.develop];
        if !self.flow.is_base(base) {
            bases.push(base);
        }
        for b in bases {
            if refs.contains_key(b) {
                if let Some(x) = self.branch_against(name, b)? {
                    against.push(x);
                }
            }
        }

        let (branch_ref, _, target_ref) = self.resolve(name, base)?;
        let range = format!("{target_ref}..{branch_ref}");
        let commit_count: u32 = self
            .run(&["rev-list", "--count", "--no-merges", &range])?
            .trim()
            .parse()
            .unwrap_or(0);
        let commits = self.log(&[
            "--no-merges",
            &format!("--max-count={COMMITS_SHOWN}"),
            &range,
        ])?;
        let started = if commits.len() as u32 == commit_count {
            commits.last().map(|c| c.date)
        } else {
            self.log(&["--no-merges", "--reverse", &range])?
                .first()
                .map(|c| c.date)
        };

        let files = self.file_stats(&target_ref, &branch_ref)?;
        let upstream = match branch.location {
            crate::Location::Remote => None,
            _ => {
                let u = self.run(&[
                    "for-each-ref",
                    "--format=%(upstream:short)",
                    &format!("refs/heads/{name}"),
                ])?;
                Some(u.trim().to_string()).filter(|u| !u.is_empty())
            }
        };

        Ok(BranchCard {
            branch,
            against,
            commits,
            commit_count,
            started,
            files,
            upstream,
        })
    }

    /// Lines added and removed per file, from where the branch left `target` to its tip.
    fn file_stats(&self, target: &str, branch: &str) -> Result<Vec<FileStat>> {
        let mb = self
            .git
            .output(&self.root, &["merge-base", target, branch])?;
        if !mb.status.success() {
            return Ok(Vec::new());
        }
        let mb = String::from_utf8_lossy(&mb.stdout).trim().to_string();
        let out = self.run(&["diff", "--numstat", "-z", "-M", &mb, branch])?;
        // `added\tremoved\tpath\0`, or for a rename `added\tremoved\t\0old\0new\0`.
        let mut parts = out.split('\0');
        let mut files = Vec::new();
        while let Some(head) = parts.next() {
            if head.is_empty() {
                continue;
            }
            let mut f = head.splitn(3, '\t');
            let (Some(a), Some(r), Some(path)) = (f.next(), f.next(), f.next()) else {
                continue;
            };
            let (path, old_path) = if path.is_empty() {
                let old = parts.next().unwrap_or_default().to_string();
                (parts.next().unwrap_or_default().to_string(), Some(old))
            } else {
                (path.to_string(), None)
            };
            files.push(FileStat {
                path,
                old_path,
                added: a.parse().ok(),
                removed: r.parse().ok(),
            });
        }
        Ok(files)
    }
}
