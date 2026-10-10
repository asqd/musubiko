import { useEffect, useMemo, useRef, useState } from "react";
import { match, pieces } from "../palette/fuzzy";

interface Props {
  /** The branch compared with now. */
  value: string;
  /** master/main and develop, shown first. */
  bases: string[];
  /** Every other branch name. */
  branches: string[];
  /** Folder of the repository, to remember recent picks per repository. */
  root: string;
  onChange: (name: string) => void;
  /** Which edge of the button the list lines up with. */
  align?: "left" | "right";
  /** Tooltip of the button. */
  title?: string;
}

interface Item {
  name: string;
  group: string;
  ranges: [number, number][];
}

const LIMIT = 60;
const RECENT_KEPT = 5;

function recentKey(root: string) {
  return `musubiko.recentBases:${root}`;
}

function loadRecent(root: string): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(recentKey(root)) ?? "[]");
    return Array.isArray(v) ? v.filter((x) => typeof x === "string") : [];
  } catch {
    return [];
  }
}

function remember(root: string, name: string) {
  try {
    const list = [name, ...loadRecent(root).filter((n) => n !== name)].slice(0, RECENT_KEPT);
    localStorage.setItem(recentKey(root), JSON.stringify(list));
  } catch {
    // Recent picks are a convenience; without storage the list just starts empty.
  }
}

/** "Compare with": the bases first, then any branch, found by typing part of its name. */
export default function BasePicker({ value, bases, branches, root, onChange, align = "right", title }: Props) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [cursor, setCursor] = useState(0);
  const boxRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const items: Item[] = useMemo(() => {
    if (!open) return [];
    const all = [...new Set([...bases, ...branches, value])];
    if (!q.trim()) {
      const recent = loadRecent(root).filter((n) => all.includes(n) && !bases.includes(n));
      const rest = branches.filter((n) => !bases.includes(n) && !recent.includes(n));
      return [
        ...bases.map((name) => ({ name, group: "Базовые", ranges: [] })),
        ...recent.map((name) => ({ name, group: "Недавние", ranges: [] })),
        ...rest.slice(0, LIMIT).map((name) => ({ name, group: "Ветки", ranges: [] })),
      ];
    }
    return all
      .map((name) => {
        const m = match(name, q);
        return m && { name, group: "Найдено", ranges: m.ranges, score: m.score };
      })
      .filter((x): x is Item & { score: number } => !!x)
      .sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
      .slice(0, LIMIT)
      .map(({ name, group, ranges }) => ({ name, group, ranges }));
  }, [open, q, bases, branches, value, root]);

  useEffect(() => setCursor(0), [q, open]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!boxRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  useEffect(() => {
    listRef.current?.querySelector("li.on")?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const pick = (name: string) => {
    if (!bases.includes(name)) remember(root, name);
    onChange(name);
    setOpen(false);
    setQ("");
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor((c) => Math.min(c + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor((c) => Math.max(c - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (items[cursor]) pick(items[cursor].name);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
    }
  };

  return (
    <div className="basepick" ref={boxRef}>
      <button
        className={`btn sm basepick-btn ${bases.includes(value) ? "" : "custom"}`}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="listbox"
        aria-expanded={open}
        title={title ?? `Сравнивать с ${value}. Можно выбрать любую ветку`}
      >
        <span className="mono ell">{value}</span>
        <span className="caret">▾</span>
      </button>
      {open && (
        <div className={`basepick-pop ${align}`} role="dialog" aria-label="Сравнивать с веткой">
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={onKey}
            placeholder="Найти ветку…"
            spellCheck={false}
            aria-label="Найти ветку"
          />
          <ul ref={listRef} role="listbox">
            {items.length === 0 && <li className="pal-empty">Такой ветки нет</li>}
            {items.map((it, i) => (
              <Row key={it.group + it.name} it={it} first={i === 0 || items[i - 1].group !== it.group} on={i === cursor} cur={it.name === value} onPick={pick} onHover={() => setCursor(i)} />
            ))}
          </ul>
          {!q.trim() && branches.length > LIMIT && (
            <div className="basepick-foot">Показаны {LIMIT} свежих веток из {branches.length}, остальные — поиском</div>
          )}
        </div>
      )}
    </div>
  );
}

function Row({ it, first, on, cur, onPick, onHover }: { it: Item; first: boolean; on: boolean; cur: boolean; onPick: (n: string) => void; onHover: () => void }) {
  return (
    <>
      {first && <li className="pal-group">{it.group}</li>}
      <li className={on ? "on" : ""} role="option" aria-selected={cur} onMouseDown={(e) => e.preventDefault()} onClick={() => onPick(it.name)} onMouseMove={onHover}>
        <span className="mark">{cur ? "✓" : ""}</span>
        <span className="mono pal-label">
          {pieces(it.name, it.ranges).map((p, k) => (p.hit ? <mark key={k}>{p.text}</mark> : <span key={k}>{p.text}</span>))}
        </span>
      </li>
    </>
  );
}
