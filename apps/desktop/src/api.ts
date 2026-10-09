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

export const api = {
  openRepo: (path: string) => invoke<RepoInfo>("open_repo", { path }),
  repoInfo: () => invoke<RepoInfo>("repo_info"),
  listBranches: (base: string) => invoke<Branch[]>("list_branches", { base }),
  recentRepos: () => invoke<string[]>("recent_repos"),
  forgetRepo: (path: string) => invoke<string[]>("forget_repo", { path }),
};

/** Tauri rejects with the Rust error string; anything else is a bug worth showing as is. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
