// Matching for the command palette: whole substring first, then every word, then letters in
// order ("lt48np" finds "hotfix/LT-4888-npe-payment"). Returns the matched ranges for highlighting.

export interface Match {
  score: number;
  ranges: [number, number][];
}

const SEP = /[\s/:._-]/;

function atWordStart(s: string, i: number): boolean {
  return i === 0 || SEP.test(s[i - 1]);
}

export function match(label: string, query: string): Match | null {
  const q = query.trim().toLowerCase();
  if (!q) return { score: 0, ranges: [] };
  const l = label.toLowerCase();

  const idx = l.indexOf(q);
  if (idx >= 0) {
    return { score: 1000 - idx - label.length / 20 + (atWordStart(l, idx) ? 200 : 0), ranges: [[idx, idx + q.length]] };
  }

  const words = q.split(/\s+/);
  if (words.length > 1) {
    const ranges: [number, number][] = [];
    for (const w of words) {
      const i = l.indexOf(w);
      if (i < 0) break;
      ranges.push([i, i + w.length]);
    }
    if (ranges.length === words.length) {
      return { score: 600 - label.length / 20, ranges: ranges.sort((a, b) => a[0] - b[0]) };
    }
  }

  // Letters in order; contiguous runs and word starts score higher.
  const ranges: [number, number][] = [];
  let score = 300;
  let pos = 0;
  for (const ch of q.replace(/\s+/g, "")) {
    const i = l.indexOf(ch, pos);
    if (i < 0) return null;
    const last = ranges[ranges.length - 1];
    if (last && last[1] === i) {
      last[1] = i + 1;
      score += 5;
    } else {
      ranges.push([i, i + 1]);
      score -= Math.min(i - pos, 20);
      if (atWordStart(l, i)) score += 8;
    }
    pos = i + 1;
  }
  return { score: score - label.length / 20, ranges };
}

/** Splits `label` into plain and matched pieces. */
export function pieces(label: string, ranges: [number, number][]): { text: string; hit: boolean }[] {
  const out: { text: string; hit: boolean }[] = [];
  let at = 0;
  for (const [a, b] of ranges) {
    if (a > at) out.push({ text: label.slice(at, a), hit: false });
    out.push({ text: label.slice(a, b), hit: true });
    at = b;
  }
  if (at < label.length) out.push({ text: label.slice(at), hit: false });
  return out;
}
