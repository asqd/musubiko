// Find in the diff (⌘F): matches in the diff's own lines, collapsed and big files included.
// Hidden context between hunks is not searched until it is expanded: it is not part of the diff.
import type { FileDiff } from "../api";

export interface Match {
  file: number;
  /** `hunk:line` in the file's hunks, the same id rows carry. */
  line: string;
  start: number;
  end: number;
}

/** Ranges of one line, with the global index of each match so the current one can stand out. */
export type LineMatches = Map<string, { start: number; end: number; index: number }[]>;

/** More than this is not worth counting one by one: the query is too short to be useful. */
export const MATCH_LIMIT = 5000;

export function findMatches(files: FileDiff[], query: string, caseSensitive: boolean): Match[] {
  if (!query) return [];
  const q = caseSensitive ? query : query.toLowerCase();
  const out: Match[] = [];
  files.forEach((f, fi) => {
    if (f.binary) return;
    f.hunks.forEach((h, hi) => {
      h.lines.forEach((l, li) => {
        const text = caseSensitive ? l.text : l.text.toLowerCase();
        let at = text.indexOf(q);
        while (at >= 0 && out.length < MATCH_LIMIT) {
          out.push({ file: fi, line: `${hi}:${li}`, start: at, end: at + q.length });
          at = text.indexOf(q, at + q.length);
        }
      });
    });
  });
  return out;
}

/** Shared by every file without matches, so those files do not redraw as the query changes. */
const NONE: LineMatches = new Map();

/** Matches grouped per file and line, for the rows to mark them. */
export function byFile(matches: Match[], files: number): LineMatches[] {
  const out: LineMatches[] = Array.from({ length: files }, () => NONE);
  matches.forEach((m, index) => {
    if (out[m.file] === NONE) out[m.file] = new Map();
    const map = out[m.file];
    const list = map.get(m.line);
    const r = { start: m.start, end: m.end, index };
    if (list) list.push(r);
    else map.set(m.line, [r]);
  });
  return out;
}
