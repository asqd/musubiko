import { useMemo, useState } from "react";
import { FileDiff } from "../api";
import { SearchIcon } from "../components/Icons";

interface Node {
  dirs: Map<string, Node>;
  files: { name: string; file: FileDiff; index: number }[];
}

interface Props {
  files: FileDiff[];
  viewed: (index: number) => boolean;
  active: number;
  onJump: (index: number) => void;
}

const DOT: Record<FileDiff["change"], string> = { added: "new", deleted: "gone", modified: "", renamed: "", copied: "new" };

export default function FileTree({ files, viewed, active, onJump }: Props) {
  const [q, setQ] = useState("");
  const [closed, setClosed] = useState<Set<string>>(new Set());

  const root = useMemo(() => {
    const ql = q.trim().toLowerCase();
    const root: Node = { dirs: new Map(), files: [] };
    files.forEach((file, index) => {
      if (ql && !file.path.toLowerCase().includes(ql)) return;
      const parts = file.path.split("/");
      let node = root;
      for (const p of parts.slice(0, -1)) {
        if (!node.dirs.has(p)) node.dirs.set(p, { dirs: new Map(), files: [] });
        node = node.dirs.get(p)!;
      }
      node.files.push({ name: parts[parts.length - 1], file, index });
    });
    return root;
  }, [files, q]);

  const toggle = (path: string) =>
    setClosed((s) => {
      const n = new Set(s);
      if (n.has(path)) n.delete(path);
      else n.add(path);
      return n;
    });

  const walk = (node: Node, prefix: string): JSX.Element[] => {
    const out: JSX.Element[] = [];
    for (const [dirName, dirNode] of node.dirs) {
      // Folders with a single subfolder collapse into one line: app/services/pricing.
      let name = dirName;
      let child = dirNode;
      while (child.dirs.size === 1 && child.files.length === 0) {
        const [n2, c2] = [...child.dirs][0];
        name += "/" + n2;
        child = c2;
      }
      const full = prefix + name;
      const isClosed = closed.has(full);
      out.push(
        <li key={"d:" + full}>
          <button className="tn dir" onClick={() => toggle(full)}>
            <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2">
              <path d={isClosed ? "m6 4 4 4-4 4" : "m4 6 4 4 4-4"} />
            </svg>
            <span className="nm">{name}</span>
          </button>
          {!isClosed && <ul>{walk(child, full + "/")}</ul>}
        </li>,
      );
    }
    for (const f of node.files) {
      const seen = viewed(f.index);
      out.push(
        <li key={"f:" + f.file.path}>
          <button
            className={`tn ${active === f.index ? "act" : ""} ${seen ? "seenf" : ""}`}
            onClick={() => onJump(f.index)}
            title={f.file.path}
          >
            <span className={`dot ${seen ? "seen" : DOT[f.file.change]}`} />
            <span className="nm">{f.name}</span>
            <span className="st num">
              <span className="add-n">+{f.file.additions}</span>
              <span className="del-n">−{f.file.deletions}</span>
            </span>
          </button>
        </li>,
      );
    }
    return out;
  };

  return (
    <aside className="tree">
      <label className="search">
        <SearchIcon />
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Фильтр файлов" aria-label="Фильтр файлов" spellCheck={false} />
      </label>
      <ul>{walk(root, "")}</ul>
    </aside>
  );
}
