// Syntax highlighting per line. Constructs spanning lines (block comments, heredocs) are not
// tracked across lines; for review that trade keeps it fast and simple.
import hljs from "highlight.js/lib/common";
import dockerfile from "highlight.js/lib/languages/dockerfile";

hljs.registerLanguage("dockerfile", dockerfile);

const BY_EXT: Record<string, string> = {
  rb: "ruby", rake: "ruby", gemspec: "ruby", ru: "ruby", erb: "xml", haml: "ruby",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript", mts: "typescript",
  go: "go", py: "python", rs: "rust", java: "java", kt: "kotlin", kts: "kotlin", swift: "swift",
  c: "c", h: "c", cc: "cpp", cpp: "cpp", hpp: "cpp", cs: "csharp", m: "objectivec", php: "php",
  json: "json", yml: "yaml", yaml: "yaml", toml: "ini", ini: "ini", env: "ini",
  md: "markdown", sh: "bash", bash: "bash", zsh: "bash", sql: "sql", graphql: "graphql", gql: "graphql",
  css: "css", scss: "scss", less: "less", html: "xml", xml: "xml", svg: "xml", vue: "xml", lua: "lua",
};
const BY_NAME: Record<string, string> = {
  Gemfile: "ruby", Rakefile: "ruby", Guardfile: "ruby", Podfile: "ruby", Fastfile: "ruby",
  Dockerfile: "dockerfile", Makefile: "makefile", Brewfile: "ruby",
};

export function languageOf(path: string): string | null {
  const name = path.split("/").pop() ?? path;
  if (BY_NAME[name]) return BY_NAME[name];
  if (name.startsWith("Dockerfile")) return "dockerfile";
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  const lang = BY_EXT[ext];
  return lang && hljs.getLanguage(lang) ? lang : null;
}

const escape = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

export function highlight(text: string, lang: string | null): string {
  if (!lang || text.length > 2000) return escape(text);
  try {
    return hljs.highlight(text, { language: lang, ignoreIllegals: true }).value;
  } catch {
    return escape(text);
  }
}

export interface Hit {
  start: number;
  end: number;
  /** The match the search is on now. */
  current: boolean;
}

/**
 * Highlights a line with its changed part and search hits marked. The line is cut where any mark
 * starts or ends and each piece is highlighted on its own, as for the changed part alone.
 */
export function highlightHits(text: string, lang: string | null, mark: [number, number] | undefined, cls: string, hits: Hit[]): string {
  if (hits.length === 0) return highlightMarked(text, lang, mark, cls);
  const cuts = new Set([0, text.length]);
  if (mark && mark[0] < mark[1]) {
    cuts.add(mark[0]);
    cuts.add(mark[1]);
  }
  for (const h of hits) {
    cuts.add(h.start);
    cuts.add(h.end);
  }
  const at = [...cuts].sort((a, b) => a - b);
  let out = "";
  for (let k = 0; k < at.length - 1; k++) {
    const [s, e] = [at[k], at[k + 1]];
    if (s === e) continue;
    let piece = highlight(text.slice(s, e), lang);
    const hit = hits.find((h) => s >= h.start && e <= h.end);
    if (hit) piece = `<mark class="sm${hit.current ? " cur" : ""}">${piece}</mark>`;
    if (mark && s >= mark[0] && e <= mark[1]) piece = `<mark class="${cls}">${piece}</mark>`;
    out += piece;
  }
  return out;
}

/** Highlights a line with its changed part wrapped in a mark. */
export function highlightMarked(text: string, lang: string | null, mark: [number, number] | undefined, cls: string): string {
  if (!mark || mark[0] >= mark[1]) return highlight(text, lang);
  const [a, b] = mark;
  return highlight(text.slice(0, a), lang) + `<mark class="${cls}">` + highlight(text.slice(a, b), lang) + "</mark>" + highlight(text.slice(b), lang);
}
