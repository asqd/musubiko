//! Refs under `refs/musubiko/deleted` keep the commits of deleted branches from garbage
//! collection, so a delete can be undone from the journal however late. They live exactly as
//! long as that is needed:
//!
//! - a tip already in master or develop needs none: those bases are not deleted, so its
//!   commits stay reachable;
//! - undoing the delete removes them (the operation's own undo step);
//! - an operation that leaves the journal releases them: nothing can undo it any more;
//! - on opening a repository, refs whose commits have since reached a base are pruned.

use std::collections::BTreeMap;

use crate::repo::Refs;
use crate::{Operation, Repo, Result};

pub(crate) const KEPT: &str = "refs/musubiko/deleted";

impl Repo {
    /// Whether the commit is in master or develop (local or on the server), where deleting
    /// branches cannot make it unreachable.
    pub(crate) fn in_a_base(&self, hash: &str, refs: &BTreeMap<String, Refs>) -> Result<bool> {
        for base in [&self.flow.master, &self.flow.develop] {
            let Some(r) = refs.get(base.as_str()) else {
                continue;
            };
            for tip in r.local.iter().chain(r.remote.iter()) {
                if self.is_ancestor(hash, &tip.hash)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Removes kept refs whose commits have reached master or develop since. Returns how many.
    pub fn prune_kept(&self) -> Result<usize> {
        let refs = self.refs()?;
        let mut gone = Vec::new();
        for (name, hash) in self.kept()? {
            if self.in_a_base(&hash, &refs)? {
                gone.push((name, hash));
            }
        }
        self.drop_kept(&gone)?;
        Ok(gone.len())
    }

    /// Removes these kept refs, those that still point where they did; others are left alone.
    pub fn release_kept(&self, refs: &[(String, String)]) -> Result<()> {
        let now = self.kept()?;
        let still: Vec<(String, String)> =
            refs.iter().filter(|r| now.contains(r)).cloned().collect();
        self.drop_kept(&still)
    }

    fn kept(&self) -> Result<Vec<(String, String)>> {
        Ok(self
            .run(&["for-each-ref", "--format=%(refname) %(objectname)", KEPT])?
            .lines()
            .filter_map(|l| l.split_once(' '))
            .map(|(n, h)| (n.to_string(), h.to_string()))
            .collect())
    }

    fn drop_kept(&self, refs: &[(String, String)]) -> Result<()> {
        if refs.is_empty() {
            return Ok(());
        }
        let input: String = refs
            .iter()
            .map(|(n, h)| format!("delete {n} {h}\n"))
            .collect();
        self.git
            .run_with_stdin(&self.root, &["update-ref", "--stdin"], input.into_bytes())?;
        Ok(())
    }
}

impl Operation {
    /// The refs this operation keeps for its undo: those its finished steps created.
    pub fn kept_refs(&self) -> Vec<(String, String)> {
        self.steps
            .iter()
            .zip(&self.states)
            .filter(|(_, st)| matches!(st, crate::StepState::Done { .. }))
            .filter_map(|(s, _)| match &s.effect {
                Some(crate::Effect::Keep { refs }) => Some(refs.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }
}
