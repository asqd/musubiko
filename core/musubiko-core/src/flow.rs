use serde::Serialize;

/// The team's branch model: two long-lived bases and paired hotfix branches.
///
/// A hotfix is `hotfix/<name>` cut from fresh master plus `hotfix/<name>-dev` cut from fresh
/// develop. Each half is always compared with its own base, whatever base the user picked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    pub master: String,
    pub develop: String,
    pub hotfix_prefix: String,
    pub dev_suffix: String,
}

impl Default for Flow {
    fn default() -> Self {
        Flow {
            master: "master".into(),
            develop: "develop".into(),
            hotfix_prefix: "hotfix/".into(),
            dev_suffix: "-dev".into(),
        }
    }
}

impl Flow {
    pub fn is_base(&self, name: &str) -> bool {
        name == self.master || name == self.develop
    }

    pub fn is_hotfix(&self, name: &str) -> bool {
        name.starts_with(&self.hotfix_prefix)
    }

    /// Whether a hotfix branch is a develop half. The suffix alone does not say: a task that
    /// ends in "dev" makes a master half `hotfix/x-on-dev` whose pair is `hotfix/x-on-dev-dev`.
    /// A `-dev` branch is a develop half when its master half exists, or when nothing pairs it
    /// as a master half (a develop half whose master half is gone). `exists` tells which
    /// branches there are.
    pub fn is_dev_half(&self, name: &str, exists: impl Fn(&str) -> bool) -> bool {
        let Some(master) = name.strip_suffix(&self.dev_suffix) else {
            return false;
        };
        self.is_hotfix(name) && (exists(master) || !exists(&format!("{name}{}", self.dev_suffix)))
    }

    /// The hotfix's name without the prefix and, for a develop half, the suffix.
    pub fn slug_of(&self, name: &str, exists: impl Fn(&str) -> bool) -> String {
        let s = name.strip_prefix(&self.hotfix_prefix).unwrap_or(name);
        if self.is_dev_half(name, exists) {
            s.strip_suffix(&self.dev_suffix).unwrap_or(s).to_string()
        } else {
            s.to_string()
        }
    }

    /// The base a branch is compared with: hotfix halves have a fixed one, the rest use `chosen`.
    pub fn target_for<'a>(
        &'a self,
        name: &str,
        chosen: &'a str,
        exists: impl Fn(&str) -> bool,
    ) -> &'a str {
        if !self.is_hotfix(name) {
            chosen
        } else if self.is_dev_half(name, exists) {
            &self.develop
        } else {
            &self.master
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotfix_halves_have_fixed_targets() {
        let f = Flow::default();
        let pair = |n: &str| ["hotfix/LT-1-fix", "hotfix/LT-1-fix-dev"].contains(&n);
        assert_eq!(f.target_for("hotfix/LT-1-fix", "develop", pair), "master");
        assert_eq!(
            f.target_for("hotfix/LT-1-fix-dev", "master", pair),
            "develop"
        );
        assert_eq!(f.target_for("feature/LT-2", "master", pair), "master");
        assert_eq!(f.target_for("feature/LT-2", "develop", pair), "develop");
    }

    #[test]
    fn a_master_half_may_end_in_dev() {
        let f = Flow::default();
        let names = ["hotfix/x-on-dev", "hotfix/x-on-dev-dev"];
        let exists = |n: &str| names.contains(&n);
        assert_eq!(f.target_for("hotfix/x-on-dev", "develop", exists), "master");
        assert_eq!(
            f.target_for("hotfix/x-on-dev-dev", "master", exists),
            "develop"
        );
        assert_eq!(f.slug_of("hotfix/x-on-dev", exists), "x-on-dev");
        assert_eq!(f.slug_of("hotfix/x-on-dev-dev", exists), "x-on-dev");
        // Alone, a -dev branch is a develop half whose master half is gone.
        let alone = |n: &str| n == "hotfix/LT-3-dev";
        assert_eq!(f.target_for("hotfix/LT-3-dev", "master", alone), "develop");
        assert_eq!(f.slug_of("hotfix/LT-3-dev", alone), "LT-3");
    }
}
