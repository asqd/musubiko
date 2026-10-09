import { Fragment, ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { match, pieces } from "./fuzzy";

export interface Command {
  /** Stable id, used to remember recently run commands. */
  id: string;
  group: string;
  label: string;
  /** Right-aligned hint: a shortcut or a detail. */
  hint?: ReactNode;
  /** Branch names and paths read better in monospace. */
  mono?: boolean;
  /** Prefix shown before the label, not matched: "Дифф", "Checkout". */
  kind?: string;
  run: () => void;
}

interface Props {
  commands: Command[];
  /** Commands built from what the user typed, e.g. "where is «abc123»?". */
  dynamic: (query: string) => Command[];
  onClose: () => void;
}

const RECENT_KEY = "musubiko:palette-recent";
const LIMIT = 60;

function loadRecent(): string[] {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
  } catch {
    return [];
  }
}

function remember(id: string) {
  try {
    const list = [id, ...loadRecent().filter((x) => x !== id)].slice(0, 6);
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    /* recents are a convenience */
  }
}

export default function Palette({ commands, dynamic, onClose }: Props) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);

  const items = useMemo(() => {
    if (!q.trim()) {
      const recent = loadRecent()
        .map((id) => commands.find((c) => c.id === id))
        .filter((c): c is Command => !!c)
        .map((c) => ({ c: { ...c, group: "Недавние" }, ranges: [] as [number, number][] }));
      const rest = commands.filter((c) => c.group !== "Ветки" && c.group !== "Репозитории").map((c) => ({ c, ranges: [] as [number, number][] }));
      return [...recent, ...rest].slice(0, LIMIT);
    }
    // The kind ("Дифф", "Checkout") is matched too, so "checkout lt-48" works; only the label
    // part of a match is highlighted.
    const scored = commands
      .map((c) => {
        const offset = c.kind ? c.kind.length + 1 : 0;
        const m = match((c.kind ? c.kind + " " : "") + c.label, q);
        if (!m) return null;
        const ranges = m.ranges
          .map(([a, b]) => [Math.max(a - offset, 0), b - offset] as [number, number])
          .filter(([a, b]) => b > a);
        return { c, score: m.score, ranges };
      })
      .filter((x): x is { c: Command; score: number; ranges: [number, number][] } => !!x)
      .sort((a, b) => b.score - a.score)
      .slice(0, LIMIT)
      .map(({ c, ranges }) => ({ c, ranges }));
    const dyn = dynamic(q).map((c) => ({ c, ranges: [] as [number, number][] }));
    // Something that looks like a hash or a ticket key is most likely a commit search.
    const commitLike = /^[0-9a-f]{4,40}$/i.test(q.trim()) || /^[a-z]+-\d+$/i.test(q.trim());
    return commitLike ? [...dyn, ...scored] : [...scored, ...dyn];
  }, [q, commands, dynamic]);

  useEffect(() => setSel(0), [q]);
  useEffect(() => {
    listRef.current?.querySelector(".on")?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const runAt = (i: number) => {
    const it = items[i];
    if (!it) return;
    if (!it.c.id.startsWith("dyn:")) remember(it.c.id);
    onClose();
    it.c.run();
  };

  // Group headers appear where the group changes, so ordering by score keeps groups mixed.
  let lastGroup = "";
  return (
    <div className="backdrop pal-back" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="pal" role="dialog" aria-modal="true" aria-label="Палитра команд">
        <input
          autoFocus
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Команда, ветка, хэш или номер задачи…"
          aria-label="Команда"
          spellCheck={false}
          onKeyDown={(e) => {
            // Keep these keys away from the views underneath (Esc goes back in the diff, j/k move).
            e.stopPropagation();
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSel((s) => Math.min(s + 1, items.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setSel((s) => Math.max(s - 1, 0));
            } else if (e.key === "Enter") {
              e.preventDefault();
              runAt(sel);
            } else if (e.key === "Escape") {
              e.preventDefault();
              onClose();
            }
          }}
        />
        <ul ref={listRef} role="listbox">
          {items.length === 0 && <li className="pal-empty">Ничего не найдено</li>}
          {items.map(({ c, ranges }, i) => {
            const header = c.group !== lastGroup ? c.group : null;
            lastGroup = c.group;
            return (
              <Fragment key={c.group + c.id}>
                {header && (
                  <li className="pal-group" role="presentation">
                    {header}
                  </li>
                )}
                <li
                  role="option"
                  aria-selected={i === sel}
                  className={i === sel ? "on" : ""}
                  onMouseMove={() => i !== sel && setSel(i)}
                  onClick={() => runAt(i)}
                >
                  {c.kind && <span className="pal-kind">{c.kind}</span>}
                  <span className={`pal-label ${c.mono ? "mono" : ""}`}>
                    {pieces(c.label, ranges).map((p, k) => (p.hit ? <mark key={k}>{p.text}</mark> : <span key={k}>{p.text}</span>))}
                  </span>
                  {c.hint && <span className="pal-hint">{c.hint}</span>}
                </li>
              </Fragment>
            );
          })}
        </ul>
        <div className="pal-foot">
          <span>
            <kbd>↑</kbd> <kbd>↓</kbd> выбор
          </span>
          <span>
            <kbd>Enter</kbd> выполнить
          </span>
          <span>
            <kbd>Esc</kbd> закрыть
          </span>
        </div>
      </div>
    </div>
  );
}
