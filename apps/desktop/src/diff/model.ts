// Turns hunks from the core into numbered rows, pairs changed lines and finds what changed inside them.
import type { FileDiff, Hunk } from "../api";

export interface Row {
  kind: "context" | "add" | "del";
  /** `hunk:line` for rows from the diff, so search matches find their row; none for expanded context. */
  id?: string;
  text: string;
  old?: number;
  new?: number;
  /** Changed part of the line, [start, end), when it has a counterpart on the other side. */
  mark?: [number, number];
  /** The file ends here without a newline. */
  noEol?: boolean;
}

export type Block = { type: "ctx"; row: Row } | { type: "chg"; dels: Row[]; adds: Row[] };

/** Numbers the hunk's lines and groups each run of deletions with the additions after it. */
export function blocks(h: Hunk, hi = 0): Block[] {
  let o = h.oldStart;
  let n = h.newStart;
  const out: Block[] = [];
  let i = 0;
  while (i < h.lines.length) {
    const l = h.lines[i];
    if (l.kind === "context") {
      out.push({ type: "ctx", row: { kind: "context", id: `${hi}:${i}`, text: l.text, old: o++, new: n++, noEol: l.noEol } });
      i++;
      continue;
    }
    const dels: Row[] = [];
    const adds: Row[] = [];
    while (i < h.lines.length && h.lines[i].kind === "del") {
      const id = `${hi}:${i}`;
      const l = h.lines[i++];
      dels.push({ kind: "del", id, text: l.text, old: o++, noEol: l.noEol });
    }
    while (i < h.lines.length && h.lines[i].kind === "add") {
      const id = `${hi}:${i}`;
      const l = h.lines[i++];
      adds.push({ kind: "add", id, text: l.text, new: n++, noEol: l.noEol });
    }
    if (dels.length === adds.length) {
      dels.forEach((d, k) => {
        const m = intraline(d.text, adds[k].text);
        if (m) {
          d.mark = m[0];
          adds[k].mark = m[1];
        }
      });
    }
    out.push({ type: "chg", dels, adds });
  }
  return out;
}

/** Common prefix and suffix; marks the middle only when enough of the line stayed the same. */
export function intraline(a: string, b: string): [[number, number], [number, number]] | null {
  let p = 0;
  while (p < a.length && p < b.length && a[p] === b[p]) p++;
  let q = 0;
  while (q < a.length - p && q < b.length - p && a[a.length - 1 - q] === b[b.length - 1 - q]) q++;
  const same = p + q;
  if (same === 0 || same < 0.3 * Math.max(a.length, b.length)) return null;
  return [
    [p, a.length - q],
    [p, b.length - q],
  ];
}

/** Hidden unchanged lines before a hunk, in new-file numbering; `delta` converts to old numbers. */
export interface Gap {
  from: number;
  /** Inclusive; `null` means to the end of the file. */
  to: number | null;
  delta: number;
}

export function gapBefore(file: FileDiff, i: number): Gap | null {
  const h = file.hunks[i];
  const prev = file.hunks[i - 1];
  const from = prev ? prev.newStart + prev.newLines : 1;
  const to = h.newStart - 1;
  return to >= from ? { from, to, delta: h.oldStart - h.newStart } : null;
}

export function gapAfter(file: FileDiff): Gap | null {
  if (file.change === "added" || file.change === "deleted" || file.truncated) return null;
  const last = file.hunks[file.hunks.length - 1];
  if (!last) return null;
  return { from: last.newStart + last.newLines, to: null, delta: last.oldStart - last.newStart };
}

/** Changes when the file's patch changes, so "viewed" resets after new commits. */
export function fingerprint(file: FileDiff): string {
  let h = 0x811c9dc5;
  const feed = (s: string) => {
    for (let i = 0; i < s.length; i++) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 0x01000193);
    }
  };
  feed(file.path);
  for (const hunk of file.hunks) for (const l of hunk.lines) feed(l.kind[0] + l.text);
  return (h >>> 0).toString(16);
}

const GENERATED = /(^|\/)(package-lock\.json|yarn\.lock|pnpm-lock\.yaml|Gemfile\.lock|go\.sum|Cargo\.lock|composer\.lock|poetry\.lock)$|\.min\.(js|css)$/;
const MIGRATION = /(^|\/)(db\/migrate|migrations?)\//;

export const isGenerated = (path: string) => GENERATED.test(path);
export const isMigration = (path: string) => MIGRATION.test(path);

/** Files this long start collapsed; reviewing them line by line is rarely the point. */
export const BIG_FILE_LINES = 800;
export const lineCount = (f: FileDiff) => f.hunks.reduce((n, h) => n + h.lines.length, 0);
