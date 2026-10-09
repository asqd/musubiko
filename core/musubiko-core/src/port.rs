//! Carrying hotfix commits over to the develop side, and deleting branches.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ops::{Effect, Guard, Operation, Step};
use crate::{Error, Repo, Result, Status};

/// A planned operation: what it will run and why it cannot, if it cannot.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpPlan {
    pub title: String,
    pub steps: Vec<Step>,
    pub commands: Vec<String>,
    /// Reasons the plan cannot run.
    pub problems: Vec<String>,
    /// Things worth knowing that do not stop it.
    pub warnings: Vec<String>,
}

impl OpPlan {
    fn new(
        title: String,
        steps: Vec<Step>,
        problems: Vec<String>,
        warnings: Vec<String>,
    ) -> OpPlan {
        OpPlan {
            commands: steps.iter().map(Step::command).collect(),
            title,
            steps,
            problems,
            warnings,
        }
    }

    fn into_operation(self, repo: &Repo) -> Result<Operation> {
        if let Some(p) = self.problems.first() {
            return Err(Error::Refused(p.clone()));
        }
        Ok(Operation::new(self.title, repo, self.steps))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInput {
    /// Commits to cherry-pick; applied oldest first whatever the order here.
    pub commits: Vec<String>,
    /// Branch that receives them, e.g. `hotfix/LT-1-dev`.
    pub target: String,
    /// Create `target` from fresh develop first.
    pub create: bool,
    pub push: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteInput {
    pub names: Vec<String>,
    pub local: bool,
    pub remote: bool,
}

impl Repo {
    /// Cherry-picks commits onto a branch in a scratch worktree, so the user's folder and any
    /// uncommitted work stay untouched. The branch moves only after every pick succeeded.
    pub fn plan_port(&self, input: &PortInput) -> Result<OpPlan> {
        let refs = self.refs()?;
        let target = input.target.trim().to_string();
        let mut problems = Vec::new();
        let mut warnings = Vec::new();
        let existing = refs.get(&target);
        let remote = self.remote.clone();
        let on_remote = existing.is_some_and(|r| r.remote.is_some());

        if input.create && existing.is_some() {
            problems.push(format!("Ветка {target} уже есть"));
        }
        if !input.create && existing.is_none() {
            problems.push(format!("Нет ветки {target}"));
        }
        if input.commits.is_empty() {
            problems.push("Нечего переносить".into());
        }
        let commits = self.oldest_first(&input.commits)?;
        if let Some(path) = self.checked_out_in(&target)? {
            problems.push(format!(
                "{target} открыта в папке {}: перенесите коммиты там или переключитесь на другую ветку",
                path.display()
            ));
        }

        let dev = &self.flow.develop;
        let develop_from = match (&remote, refs.get(dev)) {
            (Some(rm), Some(r)) if r.remote.is_some() => format!("{rm}/{dev}"),
            _ => dev.clone(),
        };

        // Where the scratch worktree starts: the local branch, or the remote one when the local
        // copy is missing or strictly behind it.
        let mut start = target.clone();
        if let Some(r) = existing {
            if let (Some(rm), Some(remote_tip)) = (&remote, &r.remote) {
                match &r.local {
                    None => start = format!("{rm}/{target}"),
                    Some(l) if l.hash != remote_tip.hash => {
                        let (ahead, behind) = self.left_right(&l.hash, &remote_tip.hash)?;
                        if ahead > 0 && behind > 0 {
                            problems.push(format!(
                                "Локальная {target} разошлась с {rm}: сначала сведите их (git pull)"
                            ));
                        } else if behind > 0 {
                            start = format!("{rm}/{target}");
                        } else if input.push {
                            warnings.push(format!(
                                "В {target} есть {ahead} незапушенных коммитов: push отправит и их"
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }

        let tmp = std::env::temp_dir().join(format!(
            "musubiko-{}-{}",
            self.repo_name(),
            target.replace('/', "-")
        ));
        if tmp.exists() {
            problems.push(format!(
                "Временная папка {} осталась от прошлого раза: удалите её",
                tmp.display()
            ));
        }
        let tmp = tmp.to_string_lossy().into_owned();

        let mut steps = Vec::new();
        if let Some(rm) = &remote {
            let mut fetch = vec!["fetch", rm.as_str()];
            if input.create && develop_from.contains('/') {
                fetch.push(dev);
            }
            if on_remote {
                fetch.push(&target);
            }
            if fetch.len() > 2 {
                steps.push(Step::new("Получить свежие ветки", &fetch));
            }
        }
        if input.create {
            if let Some(rm) = &remote {
                steps.push(Step {
                    fail_if_output: Some(format!("{target} уже есть на {rm}")),
                    ..Step::new(
                        format!("Проверить, что {target} свободна на {rm}"),
                        &["ls-remote", "--heads", rm, &format!("refs/heads/{target}")],
                    )
                });
            }
            steps.push(
                Step::new(
                    format!("Создать {target} от {develop_from}"),
                    &["branch", "--no-track", &target, &develop_from],
                )
                .with_effect(Effect::Branch {
                    name: target.clone(),
                }),
            );
        }
        steps.push(
            Step::new(
                "Создать временную папку",
                &["worktree", "add", "--detach", &tmp, &start],
            )
            .with_effect(Effect::TempWorktree { path: tmp.clone() }),
        );
        let mut pick = vec!["-C", tmp.as_str(), "cherry-pick", "-x"];
        pick.extend(commits.iter().map(String::as_str));
        steps.push(Step::new(
            format!(
                "Перенести {} {}",
                commits.len(),
                plural_commits(commits.len())
            ),
            &pick,
        ));
        steps.push(
            Step::new(
                format!("Передвинуть {target} на результат"),
                &[
                    "-C",
                    &tmp,
                    "update-ref",
                    &format!("refs/heads/{target}"),
                    "HEAD",
                ],
            )
            .with_effect(Effect::MoveBranch {
                name: target.clone(),
            }),
        );
        steps.push(
            Step::new(
                "Удалить временную папку",
                &["worktree", "remove", "--force", &tmp],
            )
            .with_effect(Effect::WorktreeRemoved { path: tmp.clone() }),
        );
        if let (true, Some(rm)) = (input.push, &remote) {
            if input.create {
                steps.push(
                    Step::new(format!("Запушить {target}"), &["push", "-u", rm, &target])
                        .with_effect(Effect::Push {
                            remote: rm.clone(),
                            names: vec![target.clone()],
                        }),
                );
            } else if on_remote {
                steps.push(
                    Step::new(format!("Запушить {target}"), &["push", rm, &target]).with_effect(
                        Effect::PushUpdate {
                            remote: rm.clone(),
                            name: target.clone(),
                        },
                    ),
                );
            }
        }

        let title = if input.create {
            format!(
                "Создать {target} и перенести {} {}",
                commits.len(),
                plural_commits(commits.len())
            )
        } else {
            format!(
                "Перенести {} {} в {target}",
                commits.len(),
                plural_commits(commits.len())
            )
        };
        Ok(OpPlan::new(title, steps, problems, warnings))
    }

    pub fn start_port(&self, input: &PortInput) -> Result<Operation> {
        self.plan_port(input)?.into_operation(self)
    }

    /// Deletes branches locally and/or on the remote. Each deletion is guarded by the hash the
    /// plan saw, and undo recreates the branch at that hash.
    pub fn plan_delete(&self, input: &DeleteInput) -> Result<OpPlan> {
        let refs = self.refs()?;
        let head = self
            .run(&["symbolic-ref", "--short", "-q", "HEAD"])
            .unwrap_or_default()
            .trim()
            .to_string();
        let statuses =
            self.branches_where(&self.flow.develop, |n| input.names.iter().any(|x| x == n))?;
        let mut problems = Vec::new();
        let mut warnings = Vec::new();
        let mut steps = Vec::new();
        let mut remote_items: Vec<(String, String)> = Vec::new();
        let unmerged: Vec<&str> = statuses
            .iter()
            .filter(|b| !matches!(b.status, Status::Merged | Status::Squashed))
            .map(|b| b.name.as_str())
            .collect();
        if !unmerged.is_empty() {
            warnings.push(format!(
                "Не слиты: {}. Их коммиты останутся только в журнале",
                unmerged.join(", ")
            ));
        }
        for name in &input.names {
            let Some(r) = refs.get(name) else {
                problems.push(format!("Нет ветки {name}"));
                continue;
            };
            if self.flow.is_base(name) {
                problems.push(format!("{name} — базовая ветка, её не удаляю"));
                continue;
            }
            if let (true, Some(l)) = (input.local, &r.local) {
                if *name == head {
                    problems.push(format!("{name} — текущая ветка: сначала переключитесь"));
                } else if let Some(p) = self.checked_out_in(name)? {
                    problems.push(format!("{name} открыта в папке {}", p.display()));
                }
                let refname = format!("refs/heads/{name}");
                steps.push(
                    Step {
                        guard: Some(Guard {
                            refname,
                            hash: l.hash.clone(),
                            message: format!(
                                "{name} изменилась после того, как вы её смотрели: не удаляю"
                            ),
                        }),
                        ..Step::new(format!("Удалить {name}"), &["branch", "-D", name])
                    }
                    .with_effect(Effect::DeleteLocal {
                        name: name.clone(),
                        hash: l.hash.clone(),
                    }),
                );
            }
            if let (true, Some(_), Some(t)) = (input.remote, &self.remote, &r.remote) {
                remote_items.push((name.clone(), t.hash.clone()));
            }
        }
        // One atomic push for all remote deletions: either all go or none, and one network trip.
        if let (false, Some(rm)) = (remote_items.is_empty(), &self.remote) {
            let mut args: Vec<String> = vec!["push".into(), "--atomic".into()];
            args.extend(
                remote_items
                    .iter()
                    .map(|(n, h)| format!("--force-with-lease=refs/heads/{n}:{h}")),
            );
            args.extend([rm.clone(), "--delete".into()]);
            args.extend(remote_items.iter().map(|(n, _)| n.clone()));
            let title = if remote_items.len() == 1 {
                format!("Удалить {} на {rm}", remote_items[0].0)
            } else {
                format!(
                    "Удалить {} {} на {rm}",
                    remote_items.len(),
                    plural(remote_items.len(), "ветку", "ветки", "веток")
                )
            };
            steps.push(
                Step {
                    args,
                    ..Step::new(title, &[])
                }
                .with_effect(Effect::DeleteRemoteMany {
                    remote: rm.clone(),
                    items: remote_items.clone(),
                }),
            );
            let me = self
                .run(&["config", "user.email"])
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            let others: Vec<String> = statuses
                .iter()
                .filter(|b| remote_items.iter().any(|(n, _)| *n == b.name))
                .filter(|b| me.is_empty() || b.owner.email.to_lowercase() != me)
                .map(|b| format!("{} ({})", b.name, b.owner.name))
                .collect();
            if !others.is_empty() {
                warnings.push(format!(
                    "На {rm} удалятся ветки других авторов: {}",
                    others.join(", ")
                ));
            }
        }
        if steps.is_empty() && problems.is_empty() {
            problems.push("Удалять нечего: отметьте, где удалять".into());
        }
        let n = input.names.len();
        let title = if n == 1 {
            format!("Удалить {}", input.names[0])
        } else {
            format!("Удалить {n} {}", plural(n, "ветку", "ветки", "веток"))
        };
        Ok(OpPlan::new(title, steps, problems, warnings))
    }

    pub fn start_delete(&self, input: &DeleteInput) -> Result<Operation> {
        self.plan_delete(input)?.into_operation(self)
    }

    /// Fetches everything and drops remote-tracking branches deleted on the server, so merged
    /// statuses and "deleted on the server" are current. Nothing to undo: it only reads.
    pub fn plan_fetch(&self) -> Result<OpPlan> {
        let Some(rm) = self.remote.clone() else {
            return Ok(OpPlan::new(
                "Получить с сервера".into(),
                Vec::new(),
                vec!["У репозитория нет remote".into()],
                Vec::new(),
            ));
        };
        let steps = vec![Step::new(
            format!("Получить изменения с {rm} и убрать удалённые там ветки"),
            &["fetch", "--prune", &rm],
        )];
        Ok(OpPlan::new(
            format!("Получить с {rm}"),
            steps,
            Vec::new(),
            Vec::new(),
        ))
    }

    pub fn start_fetch(&self) -> Result<Operation> {
        self.plan_fetch()?.into_operation(self)
    }

    /// The folder where `branch` is checked out, if any (this one or another worktree).
    pub(crate) fn checked_out_in(&self, branch: &str) -> Result<Option<PathBuf>> {
        let out = self.run(&["worktree", "list", "--porcelain"])?;
        let mut path = None;
        for line in out.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(p));
            } else if line.strip_prefix("branch refs/heads/") == Some(branch) {
                return Ok(path);
            }
        }
        Ok(None)
    }

    /// Full hashes, oldest commit first.
    fn oldest_first(&self, commits: &[String]) -> Result<Vec<String>> {
        if commits.is_empty() {
            return Ok(Vec::new());
        }
        let mut args = vec!["log", "--no-walk=unsorted", "--format=%ct %H"];
        args.extend(commits.iter().map(String::as_str));
        let mut list: Vec<(i64, String)> = self
            .run(&args)?
            .lines()
            .filter_map(|l| {
                l.split_once(' ')
                    .map(|(t, h)| (t.parse().unwrap_or(0), h.to_string()))
            })
            .collect();
        list.sort_by_key(|(t, _)| *t);
        Ok(list.into_iter().map(|(_, h)| h).collect())
    }

    pub(crate) fn repo_name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".into())
    }
}

fn plural(n: usize, one: &'static str, few: &'static str, many: &'static str) -> &'static str {
    let (m10, m100) = (n % 10, n % 100);
    if m10 == 1 && m100 != 11 {
        one
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        few
    } else {
        many
    }
}

fn plural_commits(n: usize) -> &'static str {
    plural(n, "коммит", "коммита", "коммитов")
}
