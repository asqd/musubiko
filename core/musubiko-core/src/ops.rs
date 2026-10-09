//! Operations that change the repository, run one step at a time.
//!
//! Each finished step leaves an undo step behind, built from what actually happened (the hash a
//! new branch got, the stash entry that was made). Undo steps carry guards, so undoing never
//! deletes a branch that gained commits since or pops someone else's stash.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{Error, Repo, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub title: String,
    /// Arguments after `git`.
    pub args: Vec<String>,
    /// The step fails with this message when the command prints anything (availability checks).
    #[serde(default)]
    pub fail_if_output: Option<String>,
    #[serde(default)]
    pub guard: Option<Guard>,
    #[serde(default)]
    pub effect: Option<Effect>,
}

/// Run the step only while `refname` still points at `hash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guard {
    pub refname: String,
    pub hash: String,
    pub message: String,
}

/// What a step did, so the matching undo step can be built after it succeeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Effect {
    Branch {
        name: String,
    },
    Push {
        remote: String,
        names: Vec<String>,
    },
    Stash,
    Switch {
        from: String,
        detached: bool,
    },
    Worktree {
        path: String,
    },
    /// A scratch worktree Musubiko removes itself when done; undo removes it by force.
    TempWorktree {
        path: String,
    },
    /// The scratch worktree was removed: its undo step is no longer needed.
    WorktreeRemoved {
        path: String,
    },
    /// The branch moved (or appeared); undo puts it back where it was before the step.
    MoveBranch {
        name: String,
    },
    /// The branch was pushed over an existing remote branch; undo restores the remote one.
    PushUpdate {
        remote: String,
        name: String,
    },
    /// A local branch at `hash` was deleted; undo recreates it.
    DeleteLocal {
        name: String,
        hash: String,
    },
    /// A remote branch at `hash` was deleted; undo pushes it back.
    DeleteRemote {
        remote: String,
        name: String,
        hash: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StepState {
    Pending,
    Done { output: String },
    Failed { error: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpStatus {
    Running,
    Done,
    /// Stopped on a failed step; earlier steps stay done.
    Failed,
    /// Stopped on a failure and the user chose to keep what was done.
    Partial,
    /// Undone right after a failure.
    RolledBack,
    /// Undone later, from the journal.
    Undone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub id: String,
    pub title: String,
    pub repo: PathBuf,
    /// Unix seconds.
    pub started: i64,
    pub steps: Vec<Step>,
    pub states: Vec<StepState>,
    /// Undo steps for what is done, last one on top.
    pub undo: Vec<Step>,
    pub status: OpStatus,
    /// Set when undoing stopped part-way.
    #[serde(default)]
    pub undo_error: Option<String>,
}

impl Step {
    pub fn new(title: impl Into<String>, args: &[&str]) -> Step {
        Step {
            title: title.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            fail_if_output: None,
            guard: None,
            effect: None,
        }
    }

    pub fn with_effect(mut self, effect: Effect) -> Step {
        self.effect = Some(effect);
        self
    }

    /// The command as the user would type it, for previews and the journal.
    pub fn command(&self) -> String {
        let mut s = String::from("git");
        for a in &self.args {
            s.push(' ');
            s.push_str(&shell_quote(a));
        }
        s
    }
}

fn shell_quote(a: &str) -> String {
    if !a.is_empty()
        && a.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@^~+,%".contains(c))
    {
        a.to_string()
    } else {
        format!("'{}'", a.replace('\'', r"'\''"))
    }
}

impl Operation {
    pub fn new(title: impl Into<String>, repo: &Repo, steps: Vec<Step>) -> Operation {
        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        Operation {
            id: format!("{started}-{nanos:09}"),
            title: title.into(),
            repo: repo.root().to_path_buf(),
            started,
            states: vec![StepState::Pending; steps.len()],
            steps,
            undo: Vec::new(),
            status: OpStatus::Running,
            undo_error: None,
        }
    }

    /// Index of the next step to run.
    pub fn cursor(&self) -> usize {
        self.states
            .iter()
            .position(|s| !matches!(s, StepState::Done { .. }))
            .unwrap_or(self.states.len())
    }

    /// Runs the next pending (or the failed) step. A failure is recorded in the step's state
    /// and stops the operation; it is not returned as an error.
    pub fn step(&mut self, repo: &Repo) {
        if !matches!(self.status, OpStatus::Running | OpStatus::Failed) {
            return;
        }
        let i = self.cursor();
        if i >= self.steps.len() {
            self.status = OpStatus::Done;
            return;
        }
        self.status = OpStatus::Running;
        match repo.run_step(&self.steps[i]) {
            Ok((output, undo)) => {
                self.states[i] = StepState::Done { output };
                if let Some(Effect::WorktreeRemoved { path }) = &self.steps[i].effect {
                    self.undo.retain(|u| !u.args.contains(path));
                }
                if let Some(u) = undo {
                    self.undo.push(u);
                }
                if i + 1 == self.steps.len() {
                    self.status = OpStatus::Done;
                }
            }
            Err(e) => {
                self.states[i] = StepState::Failed {
                    error: e.to_string(),
                };
                self.status = OpStatus::Failed;
            }
        }
    }

    /// Undoes finished steps, newest first. Stops at the first undo step that fails and leaves
    /// it and the older ones on the stack.
    pub fn undo_all(&mut self, repo: &Repo, final_status: OpStatus) {
        self.undo_error = None;
        while let Some(u) = self.undo.pop() {
            if let Err(e) = repo.run_step(&u) {
                self.undo_error = Some(format!("{}: {e}", u.title));
                self.undo.push(u);
                return;
            }
        }
        self.status = final_status;
    }
}

impl Repo {
    /// Runs one step; returns git's output and the undo step for what it did.
    pub fn run_step(&self, step: &Step) -> Result<(String, Option<Step>)> {
        if let Some(g) = &step.guard {
            let out = self
                .git
                .output(&self.root, &["rev-parse", "--verify", "-q", &g.refname])?;
            let now = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if now != g.hash {
                return Err(Error::Refused(g.message.clone()));
            }
        }
        // Where things stood before, for effects whose undo needs it.
        let before = match &step.effect {
            Some(Effect::MoveBranch { name }) => self.rev_opt(&format!("refs/heads/{name}"))?,
            Some(Effect::PushUpdate { remote, name }) => {
                self.rev_opt(&format!("refs/remotes/{remote}/{name}"))?
            }
            _ => None,
        };
        let args: Vec<&str> = step.args.iter().map(String::as_str).collect();
        let out = self.git.output(&self.root, &args)?;
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if !out.status.success() {
            return Err(Error::Command {
                args: step.args.join(" "),
                stderr: if stderr.is_empty() { stdout } else { stderr },
            });
        }
        if let Some(msg) = &step.fail_if_output {
            if !stdout.is_empty() {
                return Err(Error::Refused(msg.clone()));
            }
        }
        let undo = match &step.effect {
            None | Some(Effect::WorktreeRemoved { .. }) => None,
            Some(effect) => Some(self.undo_for(effect, before)?),
        };
        // Push and fetch report progress on stderr; keep it for the journal.
        let output = [stdout, stderr]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        Ok((output, undo))
    }

    fn rev(&self, refname: &str) -> Result<String> {
        Ok(self
            .run(&["rev-parse", "--verify", refname])?
            .trim()
            .to_string())
    }

    fn rev_opt(&self, refname: &str) -> Result<Option<String>> {
        let out = self
            .git
            .output(&self.root, &["rev-parse", "--verify", "-q", refname])?;
        Ok(out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()))
    }

    fn undo_for(&self, effect: &Effect, before: Option<String>) -> Result<Step> {
        Ok(match effect {
            Effect::Branch { name } => {
                let refname = format!("refs/heads/{name}");
                Step {
                    guard: Some(Guard {
                        hash: self.rev(&refname)?,
                        refname,
                        message: format!("В {name} появились новые коммиты, удалять её не буду"),
                    }),
                    ..Step::new(format!("Удалить ветку {name}"), &["branch", "-D", name])
                }
            }
            Effect::Push { remote, names } => {
                // --force-with-lease: delete only if the server still has what we pushed.
                let mut args = vec!["push".to_string()];
                for n in names {
                    args.push(format!(
                        "--force-with-lease=refs/heads/{n}:{}",
                        self.rev(&format!("refs/heads/{n}"))?
                    ));
                }
                args.push(remote.clone());
                args.push("--delete".into());
                args.extend(names.iter().cloned());
                Step {
                    args,
                    ..Step::new(format!("Удалить {} на {remote}", names.join(" и ")), &[])
                }
            }
            Effect::Stash => Step {
                guard: Some(Guard {
                    refname: "refs/stash".into(),
                    hash: self.rev("refs/stash")?,
                    message: "Сверху stash уже не наши изменения, возвращать не буду".into(),
                }),
                ..Step::new("Вернуть изменения из stash", &["stash", "pop"])
            },
            Effect::Switch { from, detached } => {
                let args: Vec<&str> = if *detached {
                    vec!["switch", "--detach", from]
                } else {
                    vec!["switch", from]
                };
                Step::new(format!("Вернуться на {from}"), &args)
            }
            Effect::Worktree { path } => Step::new(
                format!("Удалить папку {path}"),
                &["worktree", "remove", path],
            ),
            Effect::TempWorktree { path } => Step::new(
                "Удалить временную папку",
                &["worktree", "remove", "--force", path],
            ),
            Effect::WorktreeRemoved { .. } => unreachable!("has no undo"),
            Effect::MoveBranch { name } => {
                // update-ref with the expected current value: refuses if the branch moved since.
                let refname = format!("refs/heads/{name}");
                let now = self.rev(&refname)?;
                match before {
                    Some(old) => Step::new(
                        format!("Вернуть {name} на {}", &old[..7.min(old.len())]),
                        &["update-ref", &refname, &old, &now],
                    ),
                    None => Step::new(
                        format!("Удалить ветку {name}"),
                        &["update-ref", "-d", &refname, &now],
                    ),
                }
            }
            Effect::PushUpdate { remote, name } => {
                let now = self.rev(&format!("refs/heads/{name}"))?;
                let lease = format!("--force-with-lease=refs/heads/{name}:{now}");
                match before {
                    Some(old) => Step::new(
                        format!("Вернуть {name} на {remote} к {}", &old[..7.min(old.len())]),
                        &["push", &lease, remote, &format!("{old}:refs/heads/{name}")],
                    ),
                    None => Step::new(
                        format!("Удалить {name} на {remote}"),
                        &["push", &lease, remote, "--delete", name],
                    ),
                }
            }
            Effect::DeleteLocal { name, hash } => Step::new(
                format!("Восстановить ветку {name}"),
                &["branch", name, hash],
            ),
            Effect::DeleteRemote { remote, name, hash } => Step::new(
                format!("Восстановить {name} на {remote}"),
                &["push", remote, &format!("{hash}:refs/heads/{name}")],
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_what_the_shell_needs() {
        let s = Step::new(
            "x",
            &["stash", "push", "-m", "musubiko: before hotfix/LT-1"],
        );
        assert_eq!(
            s.command(),
            "git stash push -m 'musubiko: before hotfix/LT-1'"
        );
        let s = Step::new(
            "x",
            &["branch", "--no-track", "hotfix/LT-1-fix", "origin/master"],
        );
        assert_eq!(
            s.command(),
            "git branch --no-track hotfix/LT-1-fix origin/master"
        );
    }
}
