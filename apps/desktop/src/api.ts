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
  /** Created and never committed to: nothing of its own yet. */
  | { kind: "new" }
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

export type Attention =
  | { kind: "conflict"; files: string[] }
  | { kind: "behind"; count: number }
  | { kind: "unpushed"; count: number }
  | { kind: "neverPushed" }
  | { kind: "goneWithWork" }
  | { kind: "quiet"; days: number }
  | { kind: "unknown"; error: string };

export interface WorkBranch {
  branch: Branch;
  commitCount: number;
  started: number | null;
  /** The newest own commits. */
  recent: Commit[];
  files: number;
  added: number;
  removed: number;
  /** Empty when nothing stands in the way. */
  attention: Attention[];
}

export interface MyBranches {
  author: Person;
  /** Everyone who has branches, most active first. */
  owners: { person: Person; active: number }[];
  attention: WorkBranch[];
  active: WorkBranch[];
  merged: Branch[];
  behindAlert: number;
  quietDays: number;
}

export interface FileStat {
  path: string;
  /** The old name when the file was renamed. */
  oldPath: string | null;
  /** null for binary files */
  added: number | null;
  removed: number | null;
}

export interface BranchCard {
  /** Compared with its target: the chosen base, or the fixed one for hotfix halves. */
  branch: Branch;
  /** The same branch against master/main, develop and a custom chosen base. */
  against: Branch[];
  /** Own commits against the target, newest first. */
  commits: Commit[];
  commitCount: number;
  /** Unix seconds of the oldest own commit. */
  started: number | null;
  files: FileStat[];
  upstream: string | null;
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

export interface NewBranchInput {
  name: string;
  base: string;
  /** From the server's copy of the base, fetched first; otherwise from the local base. */
  fresh: boolean;
  push: boolean;
  after: After;
}

export interface NewBranchPlan {
  name: string;
  from: string;
  /** The base is both local and on the server, so "fresh" is a real choice. */
  canFresh: boolean;
  worktreePath: string | null;
  canPush: boolean;
  steps: Step[];
  commands: string[];
  problems: string[];
  warnings: string[];
}

export interface Step {
  title: string;
  args: string[];
  /** Lines fed to git on stdin (`update-ref --stdin`). */
  stdin?: string[] | null;
  /** Set on steps that can stop half-way and be finished by hand (a cherry-pick). */
  resume?: { kind: "cherryPick"; dir: string } | null;
}

export interface ConflictPath {
  path: string;
  code: string;
  resolved: boolean;
}

export interface PickState {
  dir: string;
  current: Commit | null;
  remaining: number;
  files: ConflictPath[];
}

export interface HotfixPlan {
  slug: string;
  masterBranch: string;
  devBranch: string;
  masterFrom: string;
  devFrom: string;
  /** False when the repository has no develop: only the master half is made. */
  withDev: boolean;
  worktreePath: string | null;
  canPush: boolean;
  steps: Step[];
  commands: string[];
  problems: string[];
  warnings: string[];
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
  /** No develop: a hotfix is one branch and nothing is carried over. */
  hasDevelop: boolean;
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

export type CheckoutMode = "switch" | "worktree";

export interface CheckoutInput {
  name: string;
  mode: CheckoutMode;
  restoreStash: boolean;
}

export interface CheckoutPlan extends OpPlan {
  worktreePath: string;
  savedStash: string | null;
}

/** The command line a step runs, as shown to the user. */
export function commandOf(step: Step): string {
  const quote = (a: string) => (/^[\w\-./:=@^~+,%]+$/.test(a) ? a : `'${a.replace(/'/g, "'\\''")}'`);
  const git = ["git", ...step.args.map(quote)].join(" ");
  return step.stdin ? `${["printf", "'%s\\n'", ...step.stdin.map(quote)].join(" ")} | ${git}` : git;
}

export interface Settings {
  backgroundFetch: boolean;
  fetchIntervalMin: number;
  /** Left panel shows icons only, names on hover. */
  compactSidebar: boolean;
}

/** Invokes a command; an error it returns also goes to the log, so a report can show it. */
function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args).catch((e: unknown) => {
    logEvent("ERROR", `${cmd}: ${errorText(e)}`);
    throw e;
  });
}

/** Writes a line to the app log; never fails. */
export function logEvent(level: string, text: string): void {
  invoke("log_event", { level, text }).catch(() => undefined);
}

/** Puts the error report on the clipboard and returns it. */
export async function copyReport(error: string): Promise<string> {
  const report = await invoke<string>("error_report", { error });
  try {
    await invoke("copy_text", { text: report });
  } catch {
    await navigator.clipboard.writeText(report);
  }
  return report;
}

export const api = {
  logPath: () => invoke<string | null>("log_path"),
  openRepo: (path: string) => call<RepoInfo>("open_repo", { path }),
  repoInfo: () => call<RepoInfo>("repo_info"),
  listBranches: (base: string) => call<Branch[]>("list_branches", { base }),
  branchCard: (name: string, base: string) => call<BranchCard>("branch_card", { name, base }),
  myBranches: (base: string, author: string | null) => call<MyBranches>("my_branches", { base, author }),
  branchDiff: (branch: string, base: string, ignoreWhitespace: boolean) =>
    call<BranchDiff>("branch_diff", { branch, base, ignoreWhitespace }),
  commitDiff: (hash: string, ignoreWhitespace: boolean) => call<FileDiff[]>("commit_diff", { hash, ignoreWhitespace }),
  fileLines: (rev: string, path: string) => call<string[]>("file_lines", { rev, path }),
  conflicts: (branch: string, base: string) => call<ConflictFile[]>("conflicts", { branch, base }),
  planHotfix: (input: HotfixInput) => call<HotfixPlan>("plan_hotfix", { input }),
  startHotfix: (input: HotfixInput) => call<Operation>("start_hotfix", { input }),
  planNewBranch: (input: NewBranchInput) => call<NewBranchPlan>("plan_new_branch", { input }),
  startNewBranch: (input: NewBranchInput) => call<Operation>("start_new_branch", { input }),
  radar: () => call<Radar>("hotfix_radar"),
  planPort: (input: PortInput) => call<OpPlan>("plan_port", { input }),
  startPort: (input: PortInput) => call<Operation>("start_port", { input }),
  planDelete: (input: DeleteInput) => call<OpPlan>("plan_delete", { input }),
  startDelete: (input: DeleteInput) => call<Operation>("start_delete", { input }),
  startFetch: () => call<Operation>("start_fetch"),
  locate: (query: string) => call<CommitSearch>("locate", { query }),
  clearStatusCache: () => call<void>("clear_status_cache"),
  getSettings: () => call<Settings>("get_settings"),
  setSettings: (value: Settings) => call<Settings>("set_settings", { value }),
  fetchQuietly: () => call<boolean>("fetch_quietly"),
  planCheckout: (input: CheckoutInput) => call<CheckoutPlan>("plan_checkout", { input }),
  startCheckout: (input: CheckoutInput) => call<Operation>("start_checkout", { input }),
  opStep: () => call<Operation>("op_step"),
  opPickState: () => call<PickState | null>("op_pick_state"),
  opPickTake: (path: string, side: "branch" | "commit") => call<PickState>("op_pick_take", { path, side }),
  opResume: (skip: boolean) => call<Operation>("op_resume", { skip }),
  opRollback: () => call<Operation>("op_rollback"),
  opLeave: () => call<Operation>("op_leave"),
  opClose: () => call<void>("op_close"),
  journal: () => call<Operation[]>("journal_list"),
  journalUndo: (id: string) => call<Operation>("journal_undo", { id }),
  recentRepos: () => call<string[]>("recent_repos"),
  forgetRepo: (path: string) => call<string[]>("forget_repo", { path }),
};

/** Tauri rejects with the Rust error string; anything else is a bug worth showing as is. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
