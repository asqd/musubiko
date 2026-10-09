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

/** Highlights a line with its changed part wrapped in a mark. */
export function highlightMarked(text: string, lang: string | null, mark: [number, number] | undefined, cls: string): string {
  if (!mark || mark[0] >= mark[1]) return highlight(text, lang);
  const [a, b] = mark;
  return highlight(text.slice(0, a), lang) + `<mark class="${cls}">` + highlight(text.slice(a, b), lang) + "</mark>" + highlight(text.slice(b), lang);
}
