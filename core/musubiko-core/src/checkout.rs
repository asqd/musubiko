//! Switching branches without losing uncommitted work: in a sibling worktree, or here after
//! putting the changes in the stash, and getting them back when coming back.

use serde::{Deserialize, Serialize};

use crate::ops::{Effect, Guard, Operation, Step};
use crate::{Error, Repo, Result};

/// Prefix of stash messages Musubiko writes: `musubiko: <branch> перед <target>`.
const STASH_TAG: &str = "musubiko: ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckoutMode {
    /// Switch in this folder; uncommitted changes go to the stash first.
    Switch,
    /// Open the branch in a sibling folder; this one stays as it is.
    Worktree,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutInput {
    pub name: String,
    pub mode: CheckoutMode,
    /// Pop the stash Musubiko made when leaving this branch, if it is still on top.
    pub restore_stash: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutPlan {
    pub title: String,
    pub steps: Vec<Step>,
    pub commands: Vec<String>,
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
    pub worktree_path: String,
    /// Changes Musubiko stashed when leaving this branch, waiting on top of the stash.
    pub saved_stash: Option<String>,
}

impl Repo {
    /// Stash uncommitted changes (if any), then switch; both undoable.
    pub(crate) fn stash_and_switch(&self, target: &str) -> Result<Vec<Step>> {
        let info = self.info()?;
        let mut steps = Vec::new();
        let from_label = info.head.clone().unwrap_or_else(|| info.head_short.clone());
        if !info.changes.is_empty() {
            steps.push(
                Step::new(
                    format!("Убрать {} изменений в stash", info.changes.len()),
                    &[
                        "stash",
                        "push",
                        "-u",
                        "-m",
                        &format!("{STASH_TAG}{from_label} перед {target}"),
                    ],
                )
                .with_effect(Effect::Stash),
            );
        }
        let (from, detached) = match &info.head {
            Some(h) => (h.clone(), false),
            None => (self.run(&["rev-parse", "HEAD"])?.trim().to_string(), true),
        };
        steps.push(
            Step::new(format!("Переключиться на {target}"), &["switch", target])
                .with_effect(Effect::Switch { from, detached }),
        );
        Ok(steps)
    }

    pub fn plan_checkout(&self, input: &CheckoutInput) -> Result<CheckoutPlan> {
        let name = input.name.trim().to_string();
        let refs = self.refs()?;
        let info = self.info()?;
        let mut problems = Vec::new();
        let mut warnings = Vec::new();
        let mut steps = Vec::new();

        let Some(r) = refs.get(&name) else {
            return Err(Error::NoBranch(name));
        };
        if info.head.as_deref() == Some(name.as_str()) {
            problems.push(format!("Вы уже на {name}"));
        }
        if let Some(p) = self.checked_out_in(&name)? {
            if p != self.root {
                problems.push(format!(
                    "{name} уже открыта в папке {}: работайте там",
                    p.display()
                ));
            }
        }

        // A branch that is only on the server gets a local copy that tracks it.
        if r.local.is_none() {
            let remote = self.remote.clone().unwrap_or_else(|| "origin".into());
            let upstream = format!("{remote}/{name}");
            steps.push(
                Step::new(
                    format!("Создать локальную {name} от {upstream}"),
                    &["branch", "--track", &name, &upstream],
                )
                .with_effect(Effect::Branch { name: name.clone() }),
            );
        } else if let Some(sync) = self.sync(r)? {
            if sync.behind > 0 && sync.ahead == 0 {
                warnings.push(format!(
                    "Локальная {name} отстаёт от сервера на {}: после переключения сделайте git pull",
                    sync.behind
                ));
            } else if sync.behind > 0 {
                warnings.push(format!(
                    "Локальная {name} разошлась с сервером: {} своих и {} чужих коммитов",
                    sync.ahead, sync.behind
                ));
            }
        }

        let slug = name.replace('/', "-");
        let path = self.worktree_path(&slug);
        let path_str = path.to_string_lossy().into_owned();

        // The stash entry left when we last went away from this branch.
        let top = self
            .run(&["stash", "list", "-1", "--format=%H%x00%gs"])
            .unwrap_or_default();
        let saved = top
            .lines()
            .next()
            .and_then(|l| l.split_once('\0'))
            .and_then(|(hash, subject)| {
                subject
                    .contains(&format!("{STASH_TAG}{name} перед "))
                    .then(|| {
                        (
                            hash.to_string(),
                            subject
                                .split_once(": ")
                                .map(|(_, m)| m)
                                .unwrap_or(subject)
                                .to_string(),
                        )
                    })
            });

        match input.mode {
            CheckoutMode::Switch => {
                let dirty = !info.changes.is_empty();
                steps.extend(self.stash_and_switch(&name)?);
                if let (true, Some((hash, _))) = (input.restore_stash, &saved) {
                    // Our own stash went on top just now when the folder was dirty.
                    let entry = if dirty { "stash@{1}" } else { "stash@{0}" };
                    steps.push(Step {
                        guard: Some(Guard {
                            refname: entry.into(),
                            hash: hash.clone(),
                            message: "Нужный stash уже не на месте: верните изменения вручную (git stash list)".into(),
                        }),
                        ..Step::new(format!("Вернуть изменения {name} из stash"), &["stash", "pop", entry])
                    });
                }
            }
            CheckoutMode::Worktree => {
                if path.exists() {
                    problems.push(format!("Папка {} уже есть", path.display()));
                }
                steps.push(
                    Step::new(
                        format!("Открыть {name} в отдельной папке"),
                        &["worktree", "add", &path_str, &name],
                    )
                    .with_effect(Effect::Worktree {
                        path: path_str.clone(),
                    }),
                );
                if let (true, Some((hash, _))) = (input.restore_stash, &saved) {
                    steps.push(Step {
                        guard: Some(Guard {
                            refname: "stash@{0}".into(),
                            hash: hash.clone(),
                            message: "Нужный stash уже не на месте: верните изменения вручную (git stash list)".into(),
                        }),
                        ..Step::new(format!("Вернуть изменения {name} из stash в новой папке"), &["-C", &path_str, "stash", "pop"])
                    });
                }
            }
        }

        let title = match input.mode {
            CheckoutMode::Switch => format!("Переключиться на {name}"),
            CheckoutMode::Worktree => format!("Открыть {name} в отдельной папке"),
        };
        Ok(CheckoutPlan {
            commands: steps.iter().map(Step::command).collect(),
            title,
            steps,
            problems,
            warnings,
            worktree_path: path_str,
            saved_stash: saved.map(|(_, m)| m),
        })
    }

    pub fn start_checkout(&self, input: &CheckoutInput) -> Result<Operation> {
        let plan = self.plan_checkout(input)?;
        if let Some(p) = plan.problems.first() {
            return Err(Error::Refused(p.clone()));
        }
        Ok(Operation::new(plan.title, self, plan.steps))
    }
}
