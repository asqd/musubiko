//! A cherry-pick stopped on a conflict in a scratch worktree: what is conflicting, taking one
//! side of a file, and carrying on once the user has resolved it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Commit, Error, Repo, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPath {
    pub path: String,
    /// `UU` both changed, `AA` both added, `UD`/`DU` changed on one side and deleted on the other.
    pub code: String,
    /// No conflict markers left in the file (or it is a delete/modify conflict with nothing to edit).
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickState {
    pub dir: String,
    /// The commit being picked when it stopped.
    pub current: Option<Commit>,
    /// Commits still waiting after the current one.
    pub remaining: u32,
    pub files: Vec<ConflictPath>,
}

/// Which version of a conflicting file to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    /// The branch receiving the commits.
    Branch,
    /// The commit being picked.
    Commit,
}

const UNMERGED: [&str; 7] = ["DD", "AU", "UD", "UA", "DU", "AA", "UU"];

fn has_markers(text: &str) -> bool {
    text.lines()
        .any(|l| l.starts_with("<<<<<<< ") || l.starts_with(">>>>>>> ") || l == "=======")
}

impl Repo {
    fn in_dir(&self, dir: &str, args: &[&str]) -> Result<std::process::Output> {
        let mut all = vec!["-C", dir];
        all.extend_from_slice(args);
        self.git.output(&self.root, &all)
    }

    fn unmerged(&self, dir: &str) -> Result<Vec<(String, String)>> {
        let out = self.in_dir(
            dir,
            &["status", "--porcelain=v1", "-z", "--untracked-files=no"],
        )?;
        Ok(String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|e| e.len() > 3)
            .filter_map(|e| {
                let (xy, path) = e.split_at(3);
                let xy = &xy[..2];
                UNMERGED
                    .contains(&xy)
                    .then(|| (xy.to_string(), path.to_string()))
            })
            .collect())
    }

    pub fn pick_state(&self, dir: &str) -> Result<PickState> {
        let files = self
            .unmerged(dir)?
            .into_iter()
            .map(|(code, path)| {
                let text = std::fs::read_to_string(Path::new(dir).join(&path)).unwrap_or_default();
                ConflictPath {
                    resolved: !has_markers(&text),
                    code,
                    path,
                }
            })
            .collect();
        let head = self.in_dir(dir, &["rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"])?;
        let current = if head.status.success() {
            let h = String::from_utf8_lossy(&head.stdout).trim().to_string();
            self.log(&["-1", &h])?.pop()
        } else {
            None
        };
        let todo = self.in_dir(dir, &["rev-parse", "--git-path", "sequencer/todo"])?;
        let todo = PathBuf::from(dir).join(String::from_utf8_lossy(&todo.stdout).trim());
        // The todo list still starts with the commit that stopped.
        let listed = std::fs::read_to_string(todo)
            .map(|t| t.lines().filter(|l| l.starts_with("pick ")).count() as u32)
            .unwrap_or(0);
        let remaining = listed.saturating_sub(u32::from(current.is_some()));
        Ok(PickState {
            dir: dir.to_string(),
            current,
            remaining,
            files,
        })
    }

    /// Keeps one side of a conflicting file and marks it resolved. The scratch worktree is
    /// thrown away on rollback, so this needs no undo.
    pub fn pick_take(&self, dir: &str, path: &str, side: Side) -> Result<PickState> {
        let flag = match side {
            Side::Branch => "--ours",
            Side::Commit => "--theirs",
        };
        let out = self.in_dir(dir, &["checkout", flag, "--", path])?;
        if out.status.success() {
            let add = self.in_dir(dir, &["add", "--", path])?;
            if !add.status.success() {
                return Err(Error::Command {
                    args: format!("add {path}"),
                    stderr: String::from_utf8_lossy(&add.stderr).trim().into(),
                });
            }
        } else {
            // The chosen side deleted the file.
            let rm = self.in_dir(dir, &["rm", "-q", "--", path])?;
            if !rm.status.success() {
                return Err(Error::Command {
                    args: format!("rm {path}"),
                    stderr: String::from_utf8_lossy(&rm.stderr).trim().into(),
                });
            }
        }
        self.pick_state(dir)
    }

    /// Stages what the user resolved and continues the cherry-pick, or skips the current commit.
    pub(crate) fn continue_pick(&self, dir: &str, skip: bool) -> Result<String> {
        let editor: [(&str, &Path); 1] = [("GIT_EDITOR", Path::new("true"))];
        let run = |args: &[&str]| -> Result<String> {
            let mut all = vec!["-C", dir];
            all.extend_from_slice(args);
            let out = self
                .git
                .output_with(&self.root, &all, &editor, Vec::new())?;
            let text = [
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            ]
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
            if out.status.success() {
                Ok(text)
            } else if text.contains("empty") || text.contains("nothing to commit") {
                Err(Error::Refused(
                    "После разрешения коммит не меняет ничего: пропустите его".into(),
                ))
            } else {
                Err(Error::Command {
                    args: args.join(" "),
                    stderr: text,
                })
            }
        };
        if skip {
            return run(&["cherry-pick", "--skip"]);
        }
        let state = self.pick_state(dir)?;
        if state.current.is_none() && state.files.is_empty() && state.remaining == 0 {
            // Finished by hand in a terminal already.
            return Ok(String::new());
        }
        let left: Vec<&str> = state
            .files
            .iter()
            .filter(|f| !f.resolved)
            .map(|f| f.path.as_str())
            .collect();
        if !left.is_empty() {
            return Err(Error::Refused(format!(
                "В файлах ещё есть маркеры конфликта: {}",
                left.join(", ")
            )));
        }
        if !state.files.is_empty() {
            let mut add = vec!["add", "-A", "--"];
            add.extend(state.files.iter().map(|f| f.path.as_str()));
            run(&add)?;
        }
        run(&["cherry-pick", "--continue"])
    }
}
