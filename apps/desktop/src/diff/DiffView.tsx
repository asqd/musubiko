import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, Branch, BranchDiff, Commit, ConflictFile, errorText, FileDiff, RepoInfo } from "../api";
import { ago, copyText, fullDate, plural } from "../format";
import { StatusPill } from "../components/Branches";
import FileTree from "./FileTree";
import FileView, { Mode } from "./FileView";
import Conflicts from "./Conflicts";
import { fingerprint } from "./model";
import { byFile, findMatches, MATCH_LIMIT } from "./search";
import { key } from "../platform";
import { ErrorBanner } from "../components/ErrorBanner";

type Tab = "files" | "commits" | "conflicts";

interface Props {
  info: RepoInfo;
  branch: Branch;
  base: string;
  tick: number;
  onBack: () => void;
  onCheckout: (b: Branch) => void;
}

const MODE_KEY = "musubiko:diff-mode";
const viewedKey = (root: string, branch: string) => `musubiko:viewed:${root}:${branch}`;

function loadJson<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v ? (JSON.parse(v) as T) : fallback;
  } catch {
    return fallback;
  }
}

export default function DiffView({ info, branch, base, tick, onBack, onCheckout }: Props) {
  const [diff, setDiff] = useState<BranchDiff | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [ws, setWs] = useState(false);
  const [mode, setMode] = useState<Mode>(() => (loadJson<string>(MODE_KEY, "unified") === "split" ? "split" : "unified"));
  const [tab, setTab] = useState<Tab>("files");
  const [commit, setCommit] = useState<Commit | null>(null);
  const [commitFiles, setCommitFiles] = useState<FileDiff[] | null>(null);
  const [conflicts, setConflicts] = useState<ConflictFile[] | null>(null);
  // path → fingerprint of the patch the user marked as viewed.
  const [viewedMap, setViewedMap] = useState<Record<string, string>>(() => loadJson(viewedKey(info.root, branch.name), {}));
  // Files whose collapsed state the user flipped from the default (viewed = collapsed).
  const [flipped, setFlipped] = useState<Set<string>>(new Set());
  const [active, setActive] = useState(0);
  const [toast, setToast] = useState<string | null>(null);
  // Find in the diff (⌘F).
  const [findOpen, setFindOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [typed, setTyped] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [cur, setCur] = useState(0);
  const findRef = useRef<HTMLInputElement>(null);
  const request = useRef(0);

  const hasConflicts = branch.status.kind === "conflict";

  useEffect(() => {
    const id = ++request.current;
    setLoading(true);
    setError(null);
    api
      .branchDiff(branch.name, base, ws)
      .then((d) => id === request.current && setDiff(d))
      .catch((e) => id === request.current && setError(errorText(e)))
      .finally(() => id === request.current && setLoading(false));
  }, [branch.name, base, ws, tick]);

  useEffect(() => {
    if (!commit) {
      setCommitFiles(null);
      return;
    }
    let live = true;
    api
      .commitDiff(commit.hash, ws)
      .then((f) => live && setCommitFiles(f))
      .catch((e) => live && setError(errorText(e)));
    return () => {
      live = false;
    };
  }, [commit, ws]);

  useEffect(() => {
    if (tab !== "conflicts" || conflicts) return;
    api
      .conflicts(branch.name, base)
      .then(setConflicts)
      .catch((e) => setError(errorText(e)));
  }, [tab, conflicts, branch.name, base]);

  useEffect(() => {
    try {
      localStorage.setItem(MODE_KEY, JSON.stringify(mode));
    } catch {
      /* storage may be unavailable; the mode just won't stick */
    }
  }, [mode]);

  const files = commit ? (commitFiles ?? []) : (diff?.files ?? []);
  const rev = commit ? commit.hash : (diff?.tip ?? "");
  const prints = useMemo(() => files.map(fingerprint), [files]);
  const isViewed = useCallback((i: number) => !!files[i] && viewedMap[files[i].path] === prints[i], [files, prints, viewedMap]);
  const isCollapsed = (i: number) => isViewed(i) !== flipped.has(files[i].path);

  const saveViewed = (next: Record<string, string>) => {
    setViewedMap(next);
    try {
      localStorage.setItem(viewedKey(info.root, branch.name), JSON.stringify(next));
    } catch {
      /* not persisted, still works for this session */
    }
  };

  const toggleViewed = useCallback(
    (i: number) => {
      const f = files[i];
      const next = { ...viewedMap };
      if (isViewed(i)) delete next[f.path];
      else next[f.path] = prints[i];
      saveViewed(next);
      setFlipped((s) => {
        const n = new Set(s);
        n.delete(f.path);
        return n;
      });
    },
    [files, prints, viewedMap, isViewed],
  );

  const toggleCollapse = useCallback(
    (i: number) =>
      setFlipped((s) => {
        const n = new Set(s);
        const p = files[i].path;
        if (n.has(p)) n.delete(p);
        else n.add(p);
        return n;
      }),
    [files],
  );

  // Typing redraws every file with a match, so wait for a pause.
  useEffect(() => {
    const t = window.setTimeout(() => setQuery(typed), 120);
    return () => window.clearTimeout(t);
  }, [typed]);
  const matches = useMemo(() => (findOpen ? findMatches(files, query, caseSensitive) : []), [findOpen, files, query, caseSensitive]);
  const perFile = useMemo(() => byFile(matches, files.length), [matches, files.length]);
  useEffect(() => setCur(0), [matches]);
  const curMatch = matches[cur];

  // Bring the current match into view: unfold its file, then scroll to its mark.
  useEffect(() => {
    if (!curMatch) return;
    const i = curMatch.file;
    if (isCollapsed(i)) toggleCollapse(i);
    setActive(i);
    // A file that was folded or big draws its rows a few frames later: wait for the mark.
    let frame = 0;
    let tries = 0;
    const scroll = () => {
      const mark = document.querySelector(".flist mark.sm.cur");
      if (mark) mark.scrollIntoView({ block: "center" });
      else if (tries++ < 120) frame = requestAnimationFrame(scroll);
    };
    frame = requestAnimationFrame(scroll);
    return () => cancelAnimationFrame(frame);
  }, [curMatch]);

  const openFind = useCallback(() => {
    setTab("files");
    setFindOpen(true);
    requestAnimationFrame(() => {
      findRef.current?.focus();
      findRef.current?.select();
    });
  }, []);
  const closeFind = () => {
    setFindOpen(false);
    findRef.current?.blur();
  };
  const step = useCallback(
    (by: number) => {
      if (matches.length === 0) return;
      setCur((c) => (c + by + matches.length) % matches.length);
    },
    [matches.length],
  );

  const jump = useCallback((i: number) => {
    setActive(i);
    document.getElementById(`file-${i}`)?.scrollIntoView({ block: "start" });
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const k = e.key.toLowerCase();
      if ((e.metaKey || e.ctrlKey) && !e.altKey && k === "f") {
        e.preventDefault();
        openFind();
        return;
      }
      if ((e.metaKey || e.ctrlKey) && !e.altKey && k === "g" && findOpen) {
        e.preventDefault();
        step(e.shiftKey ? -1 : 1);
        return;
      }
      if (document.activeElement instanceof HTMLInputElement && document.activeElement.type !== "checkbox") return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key === "Escape") onBack();
      if (tab !== "files" || files.length === 0) return;
      if (e.key === "j") jump(Math.min(active + 1, files.length - 1));
      if (e.key === "k") jump(Math.max(active - 1, 0));
      if (e.key === "v") {
        const wasViewed = isViewed(active);
        toggleViewed(active);
        if (!wasViewed && active < files.length - 1) jump(active + 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tab, files.length, active, jump, toggleViewed, isViewed, onBack, openFind, step, findOpen]);

  const copy = async (text: string) => {
    setToast((await copyText(text)) ? `Скопировано: ${text}` : "Не удалось скопировать");
    window.setTimeout(() => setToast(null), 2500);
  };

  const totals = useMemo(
    () => (diff?.files ?? []).reduce((t, f) => ({ a: t.a + f.additions, d: t.d + f.deletions }), { a: 0, d: 0 }),
    [diff],
  );
  const viewedCount = files.filter((_, i) => isViewed(i)).length;

  return (
    <div className="page">
      <div className="dhead">
        <button className="btn ghost" onClick={onBack} title="Назад к веткам (Esc)">
          ← Назад
        </button>
        <div className="refs">
          <button className="ref" onClick={() => copy(branch.name)} title="Скопировать имя ветки">
            {branch.name}
          </button>
          <span className="arrow">→</span>
          <span>{diff?.target ?? branch.target}</span>
        </div>
        <StatusPill status={branch.status} target={branch.target} />
        {loading && <span className="spin muted" />}
        <span className="sp" />
        {branch.isHead ? (
          <span className="tag headtag">HEAD</span>
        ) : (
          <button className="btn sm" onClick={() => onCheckout(branch)}>
            Checkout
          </button>
        )}
      </div>

      {diff && (
        <div className="dmeta">
          <span>
            {diff.commits.length} {plural(diff.commits.length, "коммит", "коммита", "коммитов")}
          </span>
          <span>
            {diff.files.length} {plural(diff.files.length, "файл", "файла", "файлов")}
          </span>
          <span className="num">
            <span className="add-n">+{totals.a}</span> <span className="del-n">−{totals.d}</span>
          </span>
          <span title={`${diff.mergeBase.subject}\n${fullDate(diff.mergeBase.date)}`}>
            от merge-base{" "}
            <button className="hash" onClick={() => copy(diff.mergeBase.hash)}>
              {diff.mergeBase.short}
            </button>{" "}
            · {ago(diff.mergeBase.date)} назад
          </span>
          <span>
            ↑{branch.ahead} ↓{branch.behind} относительно {branch.target}
          </span>
        </div>
      )}

      {error && <ErrorBanner error={error} />}
      {(branch.status.kind === "merged" || branch.status.kind === "squashed") && diff && diff.files.length > 0 && (
        <div className="banner ok">
          Ветка уже слита в {diff.target}
          {branch.status.kind === "squashed" ? " через squash или rebase" : ""}: всё, что ниже, там уже есть.
        </div>
      )}

      <div className="tabs">
        <button className={`tab ${tab === "files" ? "on" : ""}`} onClick={() => setTab("files")}>
          Файлы<span className="n">{diff?.files.length ?? "…"}</span>
        </button>
        <button className={`tab ${tab === "commits" ? "on" : ""}`} onClick={() => setTab("commits")}>
          Коммиты<span className="n">{diff?.commits.length ?? "…"}</span>
        </button>
        {hasConflicts && (
          <button className={`tab badtab ${tab === "conflicts" ? "on" : ""}`} onClick={() => setTab("conflicts")}>
            Конфликты<span className="n">{branch.status.kind === "conflict" ? branch.status.files.length : 0}</span>
          </button>
        )}
        {tab === "files" && (
          <div className="tools">
            <div className="seg" role="group" aria-label="Вид диффа">
              <button className={mode === "unified" ? "on" : ""} onClick={() => setMode("unified")}>
                Unified
              </button>
              <button className={mode === "split" ? "on" : ""} onClick={() => setMode("split")}>
                Split
              </button>
            </div>
            <label className="toggle" title="git diff -w">
              <input type="checkbox" checked={ws} onChange={(e) => setWs(e.target.checked)} />
              Скрыть пробельные
            </label>
            <div className="prog" title="Отметка снимается, если файл снова изменится">
              <span className="num">
                {viewedCount} / {files.length} просмотрено
              </span>
              <span className="bar">
                <i style={{ width: `${files.length ? (viewedCount / files.length) * 100 : 0}%` }} />
              </span>
            </div>
          </div>
        )}
      </div>

      {tab === "commits" && diff && (
        <div className="clist">
          <button
            className="crow"
            onClick={() => {
              setCommit(null);
              setTab("files");
            }}
          >
            <span className="faint mono">все</span>
            <span className="m">
              <b>Все изменения ветки</b> <span className="muted">от merge-base {diff.mergeBase.short}</span>
            </span>
            <span />
            <span />
          </button>
          {diff.commits.map((c) => (
            <button
              key={c.hash}
              className={`crow ${commit?.hash === c.hash ? "on" : ""}`}
              onClick={() => {
                setCommit(c);
                setActive(0);
                setTab("files");
              }}
            >
              <span className="hash">{c.short}</span>
              <span className="m" title={c.subject}>
                {c.subject}
              </span>
              <span className="muted">{c.author.name}</span>
              <span className="faint num" title={fullDate(c.date)}>
                {ago(c.date)}
              </span>
            </button>
          ))}
          {diff.commits.length === 0 && <div className="empty">Своих коммитов у ветки нет: всё уже в {diff.target}.</div>}
        </div>
      )}

      {tab === "conflicts" && <Conflicts files={conflicts} branch={branch.name} target={branch.target} />}

      {tab === "files" && (
        <>
          {commit && (
            <div className="cfilter">
              Показан один коммит: <span className="hash">{commit.short}</span> {commit.subject}
              <span className="sp" />
              <button className="btn sm" onClick={() => setCommit(null)}>
                Все изменения
              </button>
            </div>
          )}
          {diff && files.length === 0 && !loading && (commit ? commitFiles : true) && (
            <div className="tablewrap">
              <div className="empty">
                <h3>Изменений нет</h3>
                {branch.status.kind === "squashed"
                  ? `Содержимое ветки уже в ${diff.target} (слита через squash или rebase), но от merge-base у неё свои коммиты.`
                  : commit
                    ? "В этом коммите нет изменений в файлах."
                    : `Ветка указывает туда же, куда ${diff.target}, или все её изменения уже там.`}
              </div>
            </div>
          )}
          {files.length > 0 && (
            <div className="dgrid">
              <FileTree files={files} viewed={isViewed} active={active} onJump={jump} />
              <div className="flist">
                {findOpen && (
                  <div className="dfind" role="search">
                    <input
                      ref={findRef}
                      value={typed}
                      onChange={(e) => setTyped(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          if (typed !== query) setQuery(typed);
                          else step(e.shiftKey ? -1 : 1);
                        } else if (e.key === "Escape") {
                          e.preventDefault();
                          e.stopPropagation();
                          closeFind();
                        }
                      }}
                      placeholder="Найти в диффе"
                      aria-label="Найти в диффе"
                      spellCheck={false}
                      title="Ищет в строках диффа, в том числе в свёрнутых и больших файлах. Скрытые строки между фрагментами не ищутся, пока их не развернуть."
                    />
                    <span className={`count num ${query && matches.length === 0 ? "none" : ""}`}>
                      {!query
                        ? ""
                        : matches.length === 0
                          ? "нет совпадений"
                          : `${cur + 1} из ${matches.length}${matches.length >= MATCH_LIMIT ? "+" : ""}`}
                    </span>
                    <button
                      className={`ib case ${caseSensitive ? "on" : ""}`}
                      onClick={() => setCaseSensitive((v) => !v)}
                      title="Учитывать регистр"
                      aria-pressed={caseSensitive}
                    >
                      Aa
                    </button>
                    <button className="ib" onClick={() => step(-1)} disabled={matches.length === 0} title={`Предыдущее (⇧Enter, ⇧${key("G")})`} aria-label="Предыдущее совпадение">
                      ↑
                    </button>
                    <button className="ib" onClick={() => step(1)} disabled={matches.length === 0} title={`Следующее (Enter, ${key("G")})`} aria-label="Следующее совпадение">
                      ↓
                    </button>
                    <button className="ib" onClick={closeFind} title="Закрыть (Esc)" aria-label="Закрыть поиск">
                      ✕
                    </button>
                  </div>
                )}
                {files.map((f, i) => (
                  <FileView
                    key={(commit?.hash ?? "") + f.path}
                    file={f}
                    index={i}
                    rev={rev}
                    mode={mode}
                    viewed={isViewed(i)}
                    collapsed={isCollapsed(i)}
                    active={active === i}
                    onViewed={toggleViewed}
                    onCollapse={toggleCollapse}
                    matches={perFile[i]}
                    current={curMatch?.file === i ? cur : -1}
                  />
                ))}
              </div>
            </div>
          )}
        </>
      )}
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
