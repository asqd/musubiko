//! Musubiko core: reads branches and decides how each one relates to its base branch.
//!
//! Every git operation goes through the git CLI, so the user's config, hooks and credentials
//! apply as they would in a terminal. Reads never take the index lock (`GIT_OPTIONAL_LOCKS=0`).

mod branches;
mod cache;
mod card;
mod checkout;
mod diff;
mod error;
mod flow;
mod git;
mod hotfix;
mod journal;
mod locate;
mod mine;
mod newbranch;
mod ops;
mod pick;
mod port;
mod radar;
mod repo;

pub use branches::{Branch, Location, Status, Sync};
pub use cache::CacheStats;
pub use card::{BranchCard, FileStat};
pub use checkout::{CheckoutInput, CheckoutMode, CheckoutPlan};
pub use diff::{
    BranchDiff, ConflictFile, ConflictRegion, FileChange, FileDiff, Hunk, Line, LineKind,
};
pub use error::{Error, Result};
pub use flow::Flow;
pub use git::{Finished, Git, MIN_VERSION, set_command_hook};
pub use hotfix::{After, HotfixInput, HotfixPlan, slugify};
pub use journal::Journal;
pub use locate::{CommitPlace, CommitSearch, Presence, Verdict};
pub use mine::{Attention, MyBranches, Owner, WorkBranch};
pub use newbranch::{NewBranchInput, NewBranchPlan, branch_name};
pub use ops::{Effect, Guard, OpStatus, Operation, Resume, Step, StepState};
pub use pick::{ConflictPath, PickState, Side};
pub use port::{DeleteInput, OpPlan, PortInput};
pub use radar::{HotfixPair, PairCommit, PairState, Ported, Radar};
pub use repo::{BaseTip, Change, Commit, Person, Repo, RepoInfo};
