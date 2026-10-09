// Types mirror the serde output of musubiko-core; commands are defined in src-tauri/src/main.rs.
import { invoke } from "@tauri-apps/api/core";

export interface Person {
  name: string;
  email: string;
}

export interface Commit {
  hash: string;
  short: string;
  subject: string;
  author: Person;
  /** Unix seconds */
  date: number;
}

export interface Sync {
  /** Local commits not pushed */
  ahead: number;
  /** Remote commits not pulled */
  behind: number;
}

export type Status =
  | { kind: "merged" }
  | { kind: "squashed" }
  | { kind: "conflict"; files: string[] }
  | { kind: "open" }
  | { kind: "unknown"; error: string };

export interface Branch {
  name: string;
  tip: Commit;
  location: "both" | "local" | "remote";
  sync: Sync | null;
  target: string;
  status: Status;
  ahead: number;
  behind: number;
  owner: Person;
  isHead: boolean;
  /** The upstream was deleted on the server, usually after the merge request. */
  upstreamGone: boolean;
}

export interface Change {
  code: string;
  path: string;
}

export interface BaseTip {
  role: "master" | "develop";
  name: string;
  refName: string;
  tip: Commit;
  sync: Sync | null;
}

export interface Flow {
  master: string;
  develop: string;
  hotfixPrefix: string;
  devSuffix: string;
}

export interface RepoInfo {
  root: string;
  name: string;
  head: string | null;
  headShort: string;
  changes: Change[];
  user: Person;
  remote: string | null;
  flow: Flow;
  bases: BaseTip[];
  git: string;
  /** Unix seconds of the last fetch, from FETCH_HEAD. */
  lastFetch: number | null;
}

export type FileChange = "added" | "deleted" | "modified" | "renamed" | "copied";
export type LineKind = "context" | "add" | "del";

export interface Line {
  kind: LineKind;
  text: string;
}

export interface Hunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  section: string;
  lines: Line[];
}

export interface FileDiff {
  path: string;
  oldPath: string | null;
  change: FileChange;
  binary: boolean;
  modeChange: string | null;
  additions: number;
  deletions: number;
  hunks: Hunk[];
  truncated: boolean;
}

export interface BranchDiff {
  branch: string;
  target: string;
  mergeBase: Commit;
  tip: string;
  commits: Commit[];
  files: FileDiff[];
}

export interface ConflictRegion {
  line: number;
  before: string[];
  base: string[];
  branch: string[];
  after: string[];
}

export interface ConflictFile {
  path: string;
  regions: ConflictRegion[];
  note: string | null;
}

export type After = "worktree" | "switch" | "stay";

export interface HotfixInput {
  name: string;
  push: boolean;
  after: After;
}

export interface Step {
  title: string;
  args: string[];
}

export interface HotfixPlan {
  slug: string;
  masterBranch: string;
  devBranch: string;
  masterFrom: string;
  devFrom: string;
  worktreePath: string | null;
  canPush: boolean;
  steps: Step[];
  commands: string[];
  problems: string[];
}

export type StepState = { kind: "pending" } | { kind: "done"; output: string } | { kind: "failed"; error: string };
export type OpStatus = "running" | "done" | "failed" | "partial" | "rolledBack" | "undone";

export interface Operation {
  id: string;
  title: string;
  repo: string;
  started: number;
  steps: Step[];
  states: StepState[];
  undo: Step[];
  status: OpStatus;
  undoError: string | null;
}

export interface OpPlan {
  title: string;
  steps: Step[];
  commands: string[];
  problems: string[];
  warnings: string[];
}

export interface PortInput {
  commits: string[];
  target: string;
  create: boolean;
  push: boolean;
}

export interface DeleteInput {
  names: string[];
  local: boolean;
  remote: boolean;
}

export interface Ported {
  hash: string;
  short: string;
  place: string;
  byContent: boolean;
}

export interface PairCommit {
  commit: Commit;
  ported: Ported | null;
}

export type PairState = "empty" | "work" | "unsynced" | "noDev" | "lost" | "released" | "done";

export interface HotfixPair {
  slug: string;
  masterBranch: string;
  devBranch: string;
  master: Branch | null;
  dev: Branch | null;
  commits: PairCommit[];
  unsynced: number;
  state: PairState;
  note: string | null;
}

export interface Radar {
  pairs: HotfixPair[];
  lost: Commit[];
  olderHidden: number;
  developStaleDays: number | null;
  recentDays: number;
  master: string;
  develop: string;
}

export type Presence =
  | { kind: "itself" }
  | { kind: "copy"; hash: string; short: string }
  | { kind: "content" }
  | { kind: "missing" };

export type Verdict = "everywhere" | "masterOnly" | "developOnly" | "branchesOnly" | "nowhere";

export interface CommitPlace {
  commit: Commit;
  isMerge: boolean;
  master: Presence;
  develop: Presence | null;
  firstTag: string | null;
  branches: string[];
  cherryPicks: Commit[];
  verdict: Verdict;
}

export interface CommitSearch {
  query: string;
  results: CommitPlace[];
  truncated: boolean;
  master: string;
  develop: string | null;
  developStaleDays: number | null;
}

/** The command line a step runs, as shown to the user. */
export function commandOf(step: Step): string {
  return ["git", ...step.args.map((a) => (/^[\w\-./:=@^~+,%]+$/.test(a) ? a : `'${a.replace(/'/g, "'\\''")}'`))].join(" ");
}

export const api = {
  openRepo: (path: string) => invoke<RepoInfo>("open_repo", { path }),
  repoInfo: () => invoke<RepoInfo>("repo_info"),
  listBranches: (base: string) => invoke<Branch[]>("list_branches", { base }),
  branchDiff: (branch: string, base: string, ignoreWhitespace: boolean) =>
    invoke<BranchDiff>("branch_diff", { branch, base, ignoreWhitespace }),
  commitDiff: (hash: string, ignoreWhitespace: boolean) => invoke<FileDiff[]>("commit_diff", { hash, ignoreWhitespace }),
  fileLines: (rev: string, path: string) => invoke<string[]>("file_lines", { rev, path }),
  conflicts: (branch: string, base: string) => invoke<ConflictFile[]>("conflicts", { branch, base }),
  planHotfix: (input: HotfixInput) => invoke<HotfixPlan>("plan_hotfix", { input }),
  startHotfix: (input: HotfixInput) => invoke<Operation>("start_hotfix", { input }),
  radar: () => invoke<Radar>("hotfix_radar"),
  planPort: (input: PortInput) => invoke<OpPlan>("plan_port", { input }),
  startPort: (input: PortInput) => invoke<Operation>("start_port", { input }),
  planDelete: (input: DeleteInput) => invoke<OpPlan>("plan_delete", { input }),
  startDelete: (input: DeleteInput) => invoke<Operation>("start_delete", { input }),
  startFetch: () => invoke<Operation>("start_fetch"),
  locate: (query: string) => invoke<CommitSearch>("locate", { query }),
  opStep: () => invoke<Operation>("op_step"),
  opRollback: () => invoke<Operation>("op_rollback"),
  opLeave: () => invoke<Operation>("op_leave"),
  opClose: () => invoke<void>("op_close"),
  journal: () => invoke<Operation[]>("journal_list"),
  journalUndo: (id: string) => invoke<Operation>("journal_undo", { id }),
  recentRepos: () => invoke<string[]>("recent_repos"),
  forgetRepo: (path: string) => invoke<string[]>("forget_repo", { path }),
};

/** Tauri rejects with the Rust error string; anything else is a bug worth showing as is. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
