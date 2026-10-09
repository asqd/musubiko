import { useEffect, useMemo, useRef, useState } from "react";
import { api, Branch, errorText, RepoInfo, Status } from "../api";
import { ago, copyText, fullDate, plural } from "../format";
import { CopyIcon, SearchIcon } from "./Icons";

type Filter = "all" | "open" | "merged" | "conflict" | "stale" | "unpushed";

const STALE_DAYS = 30;
const isMerged = (s: Status) => s.kind === "merged" || s.kind === "squashed";
const isStale = (b: Branch, now: number) => !isMerged(b.status) && now - b.tip.date > STALE_DAYS * 86400;

function matches(b: Branch, f: Filter, now: number): boolean {
  switch (f) {
    case "open":
      return b.status.kind === "open" || b.status.kind === "conflict";
    case "merged":
      return isMerged(b.status);
    case "conflict":
      return b.status.kind === "conflict";
    case "stale":
      return isStale(b, now);
    case "unpushed":
      return b.location === "local" || (b.sync?.ahead ?? 0) > 0;
    default:
      return true;
  }
}

/** Hotfix `-dev` halves go right under their master half; everything else stays newest first. */
function grouped(list: Branch[], info: RepoInfo): Branch[] {
  const { hotfixPrefix, devSuffix } = info.flow;
  const isDev = (n: string) => n.startsWith(hotfixPrefix) && n.endsWith(devSuffix);
  const names = new Set(list.map((b) => b.name));
  const byName = new Map(list.map((b) => [b.name, b]));
  const out: Branch[] = [];
  for (const b of list) {
    if (isDev(b.name) && names.has(b.name.slice(0, -devSuffix.length))) continue;
    out.push(b);
    const dev = byName.get(b.name + devSuffix);
    if (b.name.startsWith(hotfixPrefix) && !isDev(b.name) && dev) out.push(dev);
  }
  return out;
}

const PILL: Record<Status["kind"], [string, string]> = {
  open: ["не слита", "open"],
  merged: ["слита", "ok"],
  squashed: ["слита · squash", "sq"],
  conflict: ["конфликт", "bad"],
  unknown: ["?", "mute"],
};

function StatusPill({ status, target }: { status: Status; target: string }) {
  const [label, cls] = PILL[status.kind];
  let title = "";
  if (status.kind === "merged") title = `Все коммиты ветки уже в ${target}`;
  if (status.kind === "squashed")
    title = `Коммиты другие (squash или rebase), но всё содержимое уже в ${target}. Проверено через git merge-tree.`;
  if (status.kind === "conflict")
    title = `Слияние с ${target} даст конфликт в ${status.files.length} ${plural(status.files.length, "файле", "файлах", "файлах")}:\n${status.files.join("\n")}`;
  if (status.kind === "unknown") title = status.error;
  return (
    <span className={`pill ${cls}`} title={title}>
      {status.kind === "unknown" ? status.error : label}
      {status.kind === "conflict" && <span className="pill-n">{status.files.length}</span>}
    </span>
  );
}

export default function Branches({ info, tick }: { info: RepoInfo; tick: number }) {
  const baseNames = info.bases.map((b) => b.name);
  const defaultBase = info.bases.find((b) => b.role === "develop")?.name ?? baseNames[0] ?? info.flow.master;
  const [base, setBase] = useState(defaultBase);
  const [list, setList] = useState<Branch[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [took, setTook] = useState<number | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [mine, setMine] = useState(false);
  const [q, setQ] = useState("");
  const [toast, setToast] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const request = useRef(0);

  useEffect(() => {
    if (!baseNames.includes(base)) setBase(defaultBase);
  }, [info.root]);

  useEffect(() => {
    const id = ++request.current;
    const started = performance.now();
    setLoading(true);
    setError(null);
    api
      .listBranches(base)
      .then((l) => {
        if (id !== request.current) return;
        setList(l);
        setTook(performance.now() - started);
      })
      .catch((e) => id === request.current && setError(errorText(e)))
      .finally(() => id === request.current && setLoading(false));
  }, [base, info.root, tick]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = document.activeElement instanceof HTMLInputElement;
      if (e.key === "/" && !typing) {
        e.preventDefault();
        searchRef.current?.focus();
      }
      if (e.key === "Escape" && document.activeElement === searchRef.current) {
        setQ("");
        searchRef.current?.blur();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const now = Date.now() / 1000;
  const myEmail = info.user.email.toLowerCase();
  const isMine = (b: Branch) => !!myEmail && (b.owner.email.toLowerCase() === myEmail || b.tip.author.email.toLowerCase() === myEmail);
  const scoped = useMemo(() => (list ?? []).filter((b) => !mine || isMine(b)), [list, mine, myEmail]);
  const rows = useMemo(() => {
    const ql = q.trim().toLowerCase();
    const filtered = scoped.filter(
      (b) =>
        matches(b, filter, now) &&
        (!ql ||
          b.name.toLowerCase().includes(ql) ||
          b.owner.name.toLowerCase().includes(ql) ||
          b.tip.hash.startsWith(ql) ||
          b.tip.subject.toLowerCase().includes(ql)),
    );
    return grouped(filtered, info);
  }, [scoped, filter, q, info]);

  const count = (f: Filter) => scoped.filter((b) => matches(b, f, now)).length;

  const copy = async (text: string) => {
    setToast((await copyText(text)) ? `Скопировано: ${text}` : "Не удалось скопировать");
    window.setTimeout(() => setToast(null), 2500);
  };

  const chips: [Filter, string][] = [
    ["all", "Все"],
    ["open", "Не слиты"],
    ["merged", "Слиты"],
    ["conflict", "Конфликт"],
    ["stale", "Старые"],
    ["unpushed", "Не запушены"],
  ];

  return (
    <div className="page">
      <div className="ph">
        <h1>Ветки</h1>
        {loading && (
          <span className="muted small">
            <span className="spin" /> считаю статусы…
          </span>
        )}
        {!loading && took !== null && list && (
          <span className="faint small">
            {list.length} {plural(list.length, "ветка", "ветки", "веток")} · {(took / 1000).toFixed(1)} с
          </span>
        )}
        <span className="sp" />
        {baseNames.length > 1 && (
          <>
            <span className="lbl">Сравнивать с</span>
            <div className="seg" role="group" aria-label="Базовая ветка">
              {baseNames.map((n) => (
                <button key={n} className={base === n ? "on" : ""} onClick={() => setBase(n)}>
                  {n}
                </button>
              ))}
            </div>
          </>
        )}
        <label className="search">
          <SearchIcon />
          <input
            ref={searchRef}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="ветка, автор, хэш…"
            aria-label="Поиск веток"
            spellCheck={false}
          />
          <kbd>/</kbd>
        </label>
      </div>

      <div className="bases">
        {info.bases.map((b) => (
          <div key={b.name} className={`base ${b.name === base ? "cur" : ""}`}>
            <b>{b.refName}</b>
            <button className="hash" onClick={() => copy(b.tip.hash)} title="Скопировать хэш">
              {b.tip.short}
            </button>
            <span className="muted ell">{b.tip.subject}</span>
            <span className="faint small" title={fullDate(b.tip.date)}>{ago(b.tip.date)}</span>
            {b.sync && b.sync.behind > 0 && (
              <span className="pill warn" title={`Локальный ${b.name} отстаёт от ${b.refName}`}>
                локальный отстаёт на {b.sync.behind}
              </span>
            )}
          </div>
        ))}
        {info.bases.length === 0 && <div className="banner">Не нашёл ни master/main, ни develop: статусы считать не с чем.</div>}
      </div>

      <div className="filters">
        <div className="seg" role="group" aria-label="Автор">
          <button className={!mine ? "on" : ""} onClick={() => setMine(false)}>Все авторы</button>
          <button
            className={mine ? "on" : ""}
            onClick={() => setMine(true)}
            disabled={!myEmail}
            title={myEmail ? `Ветки, где автор ${info.user.email}` : "В git config не задан user.email"}
          >
            Мои
          </button>
        </div>
        {chips.map(([k, l]) => (
          <button key={k} className={`fchip ${filter === k ? "on" : ""}`} onClick={() => setFilter(k)}>
            {l}
            <span className="n">{list ? count(k) : "…"}</span>
          </button>
        ))}
      </div>

      {error && <div className="banner bad">{error}</div>}

      <div className="tablewrap">
        {list === null ? (
          <div className="empty">
            <span className="spin" /> Читаю ветки…
          </div>
        ) : rows.length === 0 ? (
          <div className="empty">
            <h3>{list.length === 0 ? "Веток, кроме базовых, нет" : "Таких веток нет"}</h3>
            {list.length > 0 && "Поменяйте фильтр или очистите поиск."}
          </div>
        ) : (
          <table className={`br ${loading ? "stale-data" : ""}`}>
            <thead>
              <tr>
                <th>Ветка</th>
                <th>Статус</th>
                <th title="Коммитов впереди / позади целевой ветки">↑ ↓</th>
                <th>Автор</th>
                <th>Последний коммит</th>
                <th>Обновлена</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((b) => (
                <Row key={b.name} b={b} info={info} base={base} now={now} onCopy={copy} />
              ))}
            </tbody>
          </table>
        )}
      </div>
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}

function Row({ b, info, base, now, onCopy }: { b: Branch; info: RepoInfo; base: string; now: number; onCopy: (t: string) => void }) {
  const slash = b.name.indexOf("/");
  const pre = slash > 0 ? b.name.slice(0, slash + 1) : "";
  const rest = slash > 0 ? b.name.slice(slash + 1) : b.name;
  const { hotfixPrefix, devSuffix } = info.flow;
  const pair = b.name.startsWith(hotfixPrefix) && b.name.endsWith(devSuffix);
  const owner = b.owner.name || b.tip.author.name;
  return (
    <tr className={`${pair ? "pair" : ""} ${b.isHead ? "headrow" : ""}`}>
      <td>
        <div className="bname">
          {b.isHead ? <span className="hd" title="HEAD" /> : <span className="nohd" />}
          <span>
            <span className="pre">{pre}</span>
            {rest}
          </span>
          {b.isHead && <span className="tag headtag">HEAD</span>}
          {b.location === "remote" && <span className="tag" title="Есть только на сервере">{info.remote ?? "remote"}</span>}
          {b.location === "local" && <span className="tag" title="Не запушена">локальная</span>}
          {b.sync && b.sync.ahead > 0 && <span className="tag warn" title="Локальные коммиты, которых нет на сервере">↑{b.sync.ahead} не запушено</span>}
          {b.sync && b.sync.behind > 0 && <span className="tag" title="На сервере есть коммиты, которых нет локально">↓{b.sync.behind} на сервере</span>}
        </div>
      </td>
      <td>
        <StatusPill status={b.status} target={b.target} />
        {b.target !== base && <span className="tgt">→ {b.target}</span>}
        {isStale(b, now) && <span className="pill mute" title={`Нет коммитов больше ${STALE_DAYS} дней`}>{ago(b.tip.date)} без движения</span>}
      </td>
      <td className="ab num">
        <span className="a">↑{b.ahead}</span>
        <span className="b">↓{b.behind}</span>
      </td>
      <td title={b.owner.email}>{owner}</td>
      <td>
        <div className="commit">
          <button className="hash" onClick={() => onCopy(b.tip.hash)} title="Скопировать хэш">
            {b.tip.short}
          </button>
          <span className="msg" title={b.tip.subject}>{b.tip.subject}</span>
        </div>
      </td>
      <td className="muted num" title={fullDate(b.tip.date)}>{ago(b.tip.date, now)}</td>
      <td className="acts">
        <button className="ib" onClick={() => onCopy(b.name)} title="Скопировать имя ветки" aria-label="Скопировать имя ветки">
          <CopyIcon />
        </button>
      </td>
    </tr>
  );
}
