//! A new work branch from a chosen base: fresh from the server or as the base is locally,
//! optionally pushed, then opened here or in a sibling folder.

use serde::{Deserialize, Serialize};

use crate::hotfix::{After, slugify};
use crate::ops::{Effect, Operation, Step};
use crate::port::plural_commits;
use crate::{Error, Repo, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBranchInput {
    /// As typed: "feature/Project-12 новая форма" becomes `feature/PROJECT-12-novaya-forma`.
    pub name: String,
    /// The branch to start from.
    pub base: String,
    /// Start from the server's copy of the base, fetched just before; otherwise from the local
    /// base. Ignored when the base exists on one side only.
    pub fresh: bool,
    pub push: bool,
    pub after: After,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBranchPlan {
    /// The branch name that will be created.
    pub name: String,
    /// Where it starts, e.g. `origin/develop` or `develop`.
    pub from: String,
    /// The base is both local and on the server, so "fresh" is a real choice.
    pub can_fresh: bool,
    pub worktree_path: Option<String>,
    pub can_push: bool,
    pub steps: Vec<Step>,
    pub commands: Vec<String>,
    /// Reasons the plan cannot run; empty when it can.
    pub problems: Vec<String>,
    /// Worth knowing, but nothing stops the plan.
    pub warnings: Vec<String>,
}

/// A branch name from what was typed. A name that is already a plain ref name stays as it is;
/// otherwise the part after the prefix (`feature/`, `bugfix/team/`…) is turned into a slug the
/// way hotfix names are.
pub fn branch_name(raw: &str) -> String {
    let raw = raw.trim();
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/');
    if raw.chars().all(plain) {
        return raw.to_string();
    }
    // Leading `segment/` parts made of plain characters are the prefix.
    let mut cut = 0;
    for (i, c) in raw.char_indices() {
        if c == '/' {
            cut = i + 1;
        } else if !plain(c) {
            break;
        }
    }
    let (prefix, rest) = raw.split_at(cut);
    let slug = slugify(rest, "", "");
    if slug.is_empty() {
        return prefix.trim_end_matches('/').to_string();
    }
    format!("{prefix}{slug}")
}

impl Repo {
    pub fn plan_new_branch(&self, input: &NewBranchInput) -> Result<NewBranchPlan> {
        let name = branch_name(&input.name);
        let base = input.base.trim().to_string();
        let refs = self.refs()?;
        let mut problems = Vec::new();
        let mut warnings = Vec::new();

        if name.is_empty() {
            problems.push("Введите название ветки".to_string());
        } else if !self
            .git
            .output(&self.root, &["check-ref-format", "--branch", &name])?
            .status
            .success()
        {
            problems.push(format!("«{name}» не годится как имя ветки"));
        } else if refs.contains_key(&name) {
            problems.push(format!("Ветка {name} уже есть"));
        }

        let remote = self.remote.clone();
        let b = refs.get(&base);
        let (local, on_server) = (
            b.is_some_and(|r| r.local.is_some()),
            b.is_some_and(|r| r.remote.is_some()) && remote.is_some(),
        );
        if b.is_none() {
            problems.push(format!("Нет ветки {base}"));
        }
        let can_fresh = local && on_server;
        let use_server = on_server && (input.fresh || !local);
        let from = match (&remote, use_server) {
            (Some(rm), true) => format!("{rm}/{base}"),
            _ => base.clone(),
        };
        if let (true, true, Some(r)) = (can_fresh, use_server, b) {
            if let Some(s) = self.sync(r)? {
                if s.ahead > 0 {
                    warnings.push(format!(
                        "В локальной {base} {} {} не запушено: в новую ветку они не попадут. \
                         Снимите «от свежей», чтобы начать с локальной.",
                        s.ahead,
                        plural_commits(s.ahead as usize)
                    ));
                }
            }
        }
        if let (true, false, Some(r)) = (can_fresh, use_server, b) {
            if let Some(s) = self.sync(r)? {
                if s.behind > 0 {
                    warnings.push(format!(
                        "Локальная {base} отстаёт от сервера на {} {}: новая ветка начнётся со старой версии.",
                        s.behind,
                        plural_commits(s.behind as usize)
                    ));
                }
            }
        }

        let mut steps = Vec::new();
        if let (Some(rm), true) = (&remote, use_server && input.fresh) {
            steps.push(Step::new(
                format!("Получить свежую {base}"),
                &["fetch", rm, &base],
            ));
        }
        let can_push = remote.is_some();
        let push = input.push && can_push;
        if let (Some(rm), true) = (&remote, push) {
            steps.push(Step {
                fail_if_output: Some(format!("{name} уже есть на {rm}: выберите другое название")),
                ..Step::new(
                    format!("Проверить, что название свободно на {rm}"),
                    &["ls-remote", "--heads", rm, &format!("refs/heads/{name}")],
                )
            });
        }
        steps.push(
            // --no-track: the new branch must not push into its base by accident.
            Step::new(
                format!("Создать {name} от {from}"),
                &["branch", "--no-track", &name, &from],
            )
            .with_effect(Effect::Branch { name: name.clone() }),
        );
        if let (Some(rm), true) = (&remote, push) {
            steps.push(
                Step::new(
                    format!("Запушить {name} на {rm}"),
                    &["push", "-u", rm, &name],
                )
                .with_effect(Effect::Push {
                    remote: rm.clone(),
                    names: vec![name.clone()],
                }),
            );
        }

        let mut worktree_path = None;
        match input.after {
            After::Stay => {}
            After::Worktree => {
                let path = self.worktree_path(&name.replace('/', "-"));
                if !name.is_empty() && path.exists() {
                    problems.push(format!("Папка {} уже есть", path.display()));
                }
                let p = path.to_string_lossy().into_owned();
                steps.push(
                    Step::new(
                        format!("Открыть {name} в отдельной папке"),
                        &["worktree", "add", &p, &name],
                    )
                    .with_effect(Effect::Worktree { path: p.clone() }),
                );
                worktree_path = Some(p);
            }
            After::Switch => steps.extend(self.stash_and_switch(&name)?),
        }

        Ok(NewBranchPlan {
            commands: steps.iter().map(Step::command).collect(),
            name,
            from,
            can_fresh,
            worktree_path,
            can_push,
            steps,
            problems,
            warnings,
        })
    }

    /// Plans again from the same input and returns an operation ready to run.
    pub fn start_new_branch(&self, input: &NewBranchInput) -> Result<Operation> {
        let plan = self.plan_new_branch(input)?;
        if let Some(p) = plan.problems.first() {
            return Err(Error::Refused(p.clone()));
        }
        Ok(Operation::new(
            format!("Новая ветка {}", plan.name),
            self,
            plan.steps,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::branch_name;

    #[test]
    fn names_from_what_was_typed() {
        assert_eq!(
            branch_name("feature/LT-1-fix_login.v2"),
            "feature/LT-1-fix_login.v2"
        );
        assert_eq!(
            branch_name("feature/Project-12 новая форма"),
            "feature/PROJECT-12-novaya-forma"
        );
        assert_eq!(
            branch_name("bugfix/team/опечатка в футере"),
            "bugfix/team/opechatka-v-futere"
        );
        assert_eq!(branch_name("  новая форма  "), "novaya-forma");
        assert_eq!(branch_name("feature/ ошибка "), "feature/oshibka");
    }
}
