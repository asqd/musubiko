//! Musubiko core: reads branches and decides how each one relates to its base branch.
//!
//! Every git operation goes through the git CLI, so the user's config, hooks and credentials
//! apply as they would in a terminal. Reads never take the index lock (`GIT_OPTIONAL_LOCKS=0`).

mod branches;
mod error;
mod flow;
mod git;
mod repo;

pub use branches::{Branch, Location, Status, Sync};
pub use error::{Error, Result};
pub use flow::Flow;
pub use git::{Git, MIN_VERSION};
pub use repo::{BaseTip, Change, Commit, Person, Repo, RepoInfo};
