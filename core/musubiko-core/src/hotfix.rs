//! Starting a hotfix: `hotfix/<name>` from fresh master and `hotfix/<name>-dev` from fresh develop.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ops::{Effect, Operation, Step};
use crate::{Error, Repo, Result};

/// What to do with the working copy once the branches exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum After {
    /// Check the master half out in a sibling folder; the current folder stays as it is.
    Worktree,
    /// Switch here; uncommitted changes go to the stash first.
    Switch,
    /// Only create the branches.
    Stay,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotfixInput {
    /// As typed: "LT-4900 ошибка в ваучере" works.
    pub name: String,
    pub push: bool,
    pub after: After,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotfixPlan {
    pub slug: String,
    pub master_branch: String,
    pub dev_branch: String,
    /// Where each half starts, e.g. `origin/master`.
    pub master_from: String,
    pub dev_from: String,
    pub worktree_path: Option<String>,
    pub can_push: bool,
    pub steps: Vec<Step>,
    pub commands: Vec<String>,
    /// Reasons the plan cannot run; empty when it can.
    pub problems: Vec<String>,
}

impl Repo {
    pub fn plan_hotfix(&self, input: &HotfixInput) -> Result<HotfixPlan> {
        let flow = &self.flow;
        let slug = slugify(&input.name, &flow.hotfix_prefix, &flow.dev_suffix);
        let master_branch = format!("{}{slug}", flow.hotfix_prefix);
        let dev_branch = format!("{master_branch}{}", flow.dev_suffix);
        let refs = self.refs()?;
        let mut problems = Vec::new();
        if slug.is_empty() {
            problems.push("Введите название: номер задачи или пару слов".to_string());
        }
        for b in [&master_branch, &dev_branch] {
            if !slug.is_empty() && refs.contains_key(b.as_str()) {
                problems.push(format!("Ветка {b} уже есть"));
            }
        }

        let remote = self.remote.clone();
        let on_remote = |name: &str| refs.get(name).is_some_and(|r| r.remote.is_some());
        let from = |name: &str| -> Option<String> {
            match &remote {
                Some(rm) if on_remote(name) => Some(format!("{rm}/{name}")),
                _ => refs
                    .get(name)
                    .and_then(|r| r.local.as_ref())
                    .map(|_| name.to_string()),
            }
        };
        let master_from = from(&flow.master);
        let dev_from = from(&flow.develop);
        if master_from.is_none() {
            problems.push(format!("Нет ветки {}", flow.master));
        }
        if dev_from.is_none() {
            problems.push(format!("Нет ветки {}", flow.develop));
        }
        let master_from = master_from.unwrap_or_else(|| flow.master.clone());
        let dev_from = dev_from.unwrap_or_else(|| flow.develop.clone());

        let mut steps = Vec::new();
        if let Some(rm) = &remote {
            let fetch: Vec<&str> = [flow.master.as_str(), flow.develop.as_str()]
                .into_iter()
                .filter(|b| on_remote(b))
                .collect();
            if !fetch.is_empty() {
                let mut args = vec!["fetch", rm.as_str()];
                args.extend(fetch);
                steps.push(Step::new(
                    format!("Получить свежие {} и {}", flow.master, flow.develop),
                    &args,
                ));
            }
            steps.push(Step {
                fail_if_output: Some(format!(
                    "{master_branch} или {dev_branch} уже есть на {rm}: выберите другое название"
                )),
                ..Step::new(
                    format!("Проверить, что названия свободны на {rm}"),
                    &[
                        "ls-remote",
                        "--heads",
                        rm,
                        &format!("refs/heads/{master_branch}"),
                        &format!("refs/heads/{dev_branch}"),
                    ],
                )
            });
        }
        steps.push(
            Step::new(
                format!("Создать {master_branch} от {master_from}"),
                &["branch", "--no-track", &master_branch, &master_from],
            )
            .with_effect(Effect::Branch {
                name: master_branch.clone(),
            }),
        );
        steps.push(
            Step::new(
                format!("Создать {dev_branch} от {dev_from}"),
                &["branch", "--no-track", &dev_branch, &dev_from],
            )
            .with_effect(Effect::Branch {
                name: dev_branch.clone(),
            }),
        );
        let can_push = remote.is_some();
        if let (true, Some(rm)) = (input.push, &remote) {
            steps.push(
                Step::new(
                    format!("Запушить обе ветки на {rm}"),
                    &["push", "-u", rm, &master_branch, &dev_branch],
                )
                .with_effect(Effect::Push {
                    remote: rm.clone(),
                    names: vec![master_branch.clone(), dev_branch.clone()],
                }),
            );
        }

        let mut worktree_path = None;
        match input.after {
            After::Stay => {}
            After::Worktree => {
                let path = self.worktree_path(&slug);
                if !slug.is_empty() && path.exists() {
                    problems.push(format!("Папка {} уже есть", path.display()));
                }
                let p = path.to_string_lossy().into_owned();
                steps.push(
                    Step::new(
                        format!("Открыть {master_branch} в отдельной папке"),
                        &["worktree", "add", &p, &master_branch],
                    )
                    .with_effect(Effect::Worktree { path: p.clone() }),
                );
                worktree_path = Some(p);
            }
            After::Switch => steps.extend(self.stash_and_switch(&master_branch)?),
        }

        Ok(HotfixPlan {
            commands: steps.iter().map(Step::command).collect(),
            slug,
            master_branch,
            dev_branch,
            master_from,
            dev_from,
            worktree_path,
            can_push,
            steps,
            problems,
        })
    }

    /// Plans again from the same input and returns an operation ready to run.
    pub fn start_hotfix(&self, input: &HotfixInput) -> Result<Operation> {
        let plan = self.plan_hotfix(input)?;
        if let Some(p) = plan.problems.first() {
            return Err(Error::Refused(p.clone()));
        }
        Ok(Operation::new(
            format!("Новый hotfix {}", plan.slug),
            self,
            plan.steps,
        ))
    }

    /// `~/work/booking-api` → `~/work/booking-api@LT-4900-voucher`.
    pub(crate) fn worktree_path(&self, slug: &str) -> PathBuf {
        let name = self
            .root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".into());
        self.root.with_file_name(format!("{name}@{slug}"))
    }
}

/// "LT-4900 ошибка в ваучере" → "LT-4900-oshibka-v-vauchere". The ticket key keeps its case,
/// the rest is transliterated, lowercased and joined with dashes.
pub fn slugify(raw: &str, prefix: &str, dev_suffix: &str) -> String {
    let mut s = raw.trim();
    s = s.strip_prefix(prefix).unwrap_or(s);
    let ticket_len = s
        .find(|c: char| !(c.is_ascii_alphabetic()))
        .filter(|&i| i > 0 && s[i..].starts_with('-'))
        .and_then(|i| {
            let digits = s[i + 1..]
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(s.len() - i - 1);
            (digits > 0).then_some(i + 1 + digits)
        });
    let (ticket, rest) = match ticket_len {
        Some(n) => (s[..n].to_ascii_uppercase(), &s[n..]),
        None => (String::new(), s),
    };
    let mut words = String::new();
    for c in rest.chars().flat_map(char::to_lowercase) {
        match translit(c) {
            Some(t) => words.push_str(t),
            None if c.is_ascii_alphanumeric() => words.push(c),
            None => words.push('-'),
        }
    }
    let words: Vec<&str> = words.split('-').filter(|w| !w.is_empty()).collect();
    let mut out = ticket;
    for w in words {
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(w);
    }
    let mut out: String = out.chars().take(60).collect();
    while out.ends_with('-') {
        out.pop();
    }
    let suffix = dev_suffix.trim_start_matches('-');
    if let Some(stripped) = out.strip_suffix(&format!("-{suffix}")) {
        out = stripped.to_string();
    }
    out
}

fn translit(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "g",
        'д' => "d",
        'е' | 'ё' | 'э' => "e",
        'ж' => "zh",
        'з' => "z",
        'и' => "i",
        'й' | 'ы' => "y",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "h",
        'ц' => "ts",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "sch",
        'ъ' | 'ь' => "",
        'ю' => "yu",
        'я' => "ya",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slug(s: &str) -> String {
        slugify(s, "hotfix/", "-dev")
    }

    #[test]
    fn makes_branch_names_from_what_people_type() {
        assert_eq!(
            slug("LT-4900 ошибка в ваучере"),
            "LT-4900-oshibka-v-vauchere"
        );
        assert_eq!(slug("lt-4900"), "LT-4900");
        assert_eq!(slug("  hotfix/LT-12 Fix  NPE!! "), "LT-12-fix-npe");
        assert_eq!(slug("LT-12-npe-dev"), "LT-12-npe");
        assert_eq!(
            slug("Съешь ещё этих мягких булок"),
            "sesh-esche-etih-myagkih-bulok"
        );
        assert_eq!(slug("LT-x"), "lt-x");
        assert_eq!(slug("   "), "");
        assert!(slug(&"очень ".repeat(30)).len() <= 60);
    }
}
