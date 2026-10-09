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

export const api = {
  openRepo: (path: string) => invoke<RepoInfo>("open_repo", { path }),
  repoInfo: () => invoke<RepoInfo>("repo_info"),
  listBranches: (base: string) => invoke<Branch[]>("list_branches", { base }),
  branchDiff: (branch: string, base: string, ignoreWhitespace: boolean) =>
    invoke<BranchDiff>("branch_diff", { branch, base, ignoreWhitespace }),
  commitDiff: (hash: string, ignoreWhitespace: boolean) => invoke<FileDiff[]>("commit_diff", { hash, ignoreWhitespace }),
  fileLines: (rev: string, path: string) => invoke<string[]>("file_lines", { rev, path }),
  conflicts: (branch: string, base: string) => invoke<ConflictFile[]>("conflicts", { branch, base }),
  recentRepos: () => invoke<string[]>("recent_repos"),
  forgetRepo: (path: string) => invoke<string[]>("forget_repo", { path }),
};

/** Tauri rejects with the Rust error string; anything else is a bug worth showing as is. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
