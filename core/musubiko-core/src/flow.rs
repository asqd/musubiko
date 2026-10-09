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

    /// The base a branch is compared with: hotfix halves have a fixed one, the rest use `chosen`.
    pub fn target_for<'a>(&'a self, name: &str, chosen: &'a str) -> &'a str {
        if !self.is_hotfix(name) {
            chosen
        } else if name.ends_with(&self.dev_suffix) {
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
        assert_eq!(f.target_for("hotfix/LT-1-fix", "develop"), "master");
        assert_eq!(f.target_for("hotfix/LT-1-fix-dev", "master"), "develop");
        assert_eq!(f.target_for("feature/LT-2", "master"), "master");
        assert_eq!(f.target_for("feature/LT-2", "develop"), "develop");
    }
}
