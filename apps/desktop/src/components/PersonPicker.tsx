import { useEffect, useMemo, useRef, useState } from "react";
import { Person } from "../api";
import { match, pieces } from "../palette/fuzzy";

export interface PersonItem {
  person: Person;
  /** Shown on the right, e.g. "3 в работе". */
  hint?: string;
}

interface Props {
  /** null is "me". */
  value: string | null;
  me: Person;
  people: PersonItem[];
  onChange: (email: string | null) => void;
}

const key = (p: Person) => p.email || p.name;

/** A stable colour per person, so the same colleague always looks the same. */
function hue(p: Person): number {
  let h = 0;
  for (const c of key(p)) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}

export function Avatar({ person }: { person: Person }) {
  const letters = (person.name || person.email)
    .split(/[\s.@_-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join("");
  return (
    <span className="ava" style={{ background: `hsl(${hue(person)} 45% 45%)` }} aria-hidden="true">
      {letters || "?"}
    </span>
  );
}

/** Whose branches to show: me first, then everyone else, found by name or address. */
export default function PersonPicker({ value, me, people, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [cursor, setCursor] = useState(0);
  const boxRef = useRef<HTMLDivElement>(null);

  const others = people.filter((x) => !(me.email && x.person.email.toLowerCase() === me.email.toLowerCase()));
  const mine = people.find((x) => me.email && x.person.email.toLowerCase() === me.email.toLowerCase());
  const current = value ? people.find((x) => key(x.person) === value)?.person : me;

  const items = useMemo(() => {
    const all: { value: string | null; person: Person; hint?: string; label: string; ranges: [number, number][] }[] = [
      { value: null, person: me, hint: mine?.hint, label: "Мои", ranges: [] },
      ...others.map((x) => ({ value: key(x.person), person: x.person, hint: x.hint, label: x.person.name || x.person.email, ranges: [] as [number, number][] })),
    ];
    if (!q.trim()) return all;
    return all
      .map((it) => {
        const m = match(`${it.label} ${it.person.email}`, q);
        return m && { ...it, score: m.score, ranges: m.ranges.filter(([a]) => a < it.label.length) };
      })
      .filter((x): x is NonNullable<typeof x> => !!x)
      .sort((a, b) => b.score - a.score);
  }, [q, people, me]);

  useEffect(() => setCursor(0), [q, open]);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!boxRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const pick = (v: string | null) => {
    onChange(v);
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
      if (items[cursor]) pick(items[cursor].value);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
    }
  };

  return (
    <div className="basepick" ref={boxRef}>
      <button
        className={`btn sm basepick-btn person-btn ${value ? "custom" : ""}`}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="listbox"
        aria-expanded={open}
        title="Чьи ветки показать"
      >
        {current && <Avatar person={current} />}
        <span className="ell">{value ? current?.name || current?.email || value : "Мои"}</span>
        {!value && me.email && <span className="faint small ell">{me.email}</span>}
        <span className="caret">▾</span>
      </button>
      {open && (
        <div className="basepick-pop left" role="dialog" aria-label="Чьи ветки показать">
          <input autoFocus value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={onKey} placeholder="Найти по имени или почте…" spellCheck={false} aria-label="Найти автора" />
          <ul role="listbox">
            {items.length === 0 && <li className="pal-empty">Такого автора нет</li>}
            {items.map((it, i) => (
              <li
                key={it.value ?? "me"}
                className={`person ${i === cursor ? "on" : ""}`}
                role="option"
                aria-selected={it.value === value}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => pick(it.value)}
                onMouseMove={() => setCursor(i)}
              >
                <span className="mark">{it.value === value ? "✓" : ""}</span>
                <Avatar person={it.person} />
                <span className="who-text">
                  <span className="pal-label">
                    {pieces(it.label, it.ranges).map((p, k) => (p.hit ? <mark key={k}>{p.text}</mark> : <span key={k}>{p.text}</span>))}
                  </span>
                  {it.person.email && <span className="faint small ell">{it.person.email}</span>}
                </span>
                {it.hint && <span className="faint small nowrap">{it.hint}</span>}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
