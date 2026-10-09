import { useEffect, useRef, useState } from "react";
import { api, Commit, CommitPlace, CommitSearch, errorText, Presence, Radar, RepoInfo, Verdict } from "../api";
import { ago, copyText, fullDate, plural } from "../format";
import { SearchIcon } from "./Icons";
import { PortDialog } from "./OpDialogs";

interface Props {
  info: RepoInfo;
  /** A query to run right away, e.g. a hash clicked elsewhere; changes trigger a new search. */
  request: { query: string; n: number } | null;
  /** Known hotfix pairs, to send a master-only commit to its existing -dev half. */
  radar: Radar | null;
  active: boolean;
  onChanged: () => void;
}

const VERDICT_CLS: Record<Verdict, string> = {
  everywhere: "ok",
  masterOnly: "bad",
  developOnly: "ok",
  branchesOnly: "warn",
  nowhere: "warn",
};

function presence(p: Presence | null, tag?: string | null) {
  if (!p) return { text: "—", cls: "" };
  switch (p.kind) {
    case "itself":
      return { text: tag ? `✓ есть, с тега ${tag}` : "✓ есть", cls: "ok" };
    case "copy":
      return { text: `✓ как cherry-pick ${p.short}`, cls: "ok" };
    case "content":
      return { text: "✓ по содержимому (squash)", cls: "ok" };
    case "missing":
      return { text: "✗ нет", cls: "no" };
  }
}

function verdictText(r: CommitPlace, s: CommitSearch): string {
  const dev = s.develop ?? "develop";
  switch (r.verdict) {
    case "everywhere":
      return s.develop && r.develop ? `Есть и в ${s.master}, и в ${dev}.` : `Есть в ${s.master}.`;
    case "masterOnly":
      return `Есть в ${s.master}, но нет в ${dev}: при следующем релизе из ${dev} исправление пропадёт.`;
    case "developOnly":
      return `В ${dev}, ждёт релиза в ${s.master}.`;
    case "branchesOnly":
      return `Пока только в ${r.branches.length === 1 ? "ветке" : "ветках"}, ни в ${s.master}, ни в ${dev} его нет.`;
    case "nowhere":
      return "Не лежит ни в одной ветке: возможно, ветку удалили. Найти его можно только по хэшу.";
  }
}

export default function Locate({ info, request, radar, active, onChanged }: Props) {
  const [q, setQ] = useState("");
  const [search, setSearch] = useState<CommitSearch | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [port, setPort] = useState<{ commit: Commit; target: string; create: boolean } | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const seq = useRef(0);

  const run = async (query: string) => {
    if (!query.trim()) return;
    const id = ++seq.current;
    setLoading(true);
    setError(null);
    try {
      const s = await api.locate(query);
      if (id === seq.current) setSearch(s);
    } catch (e) {
      if (id === seq.current) setError(errorText(e));
    } finally {
      if (id === seq.current) setLoading(false);
    }
  };

  useEffect(() => {
    if (!request) return;
    setQ(request.query);
    void run(request.query);
  }, [request]);

  useEffect(() => {
    if (active) inputRef.current?.focus();
  }, [active]);

  // Where a master-only commit should go: its hotfix pair's -dev half when there is one.
  const portTarget = (r: CommitPlace) => {
    const { hotfixPrefix, devSuffix } = info.flow;
    const hotfix = r.branches.find((b) => b.startsWith(hotfixPrefix) && !b.endsWith(devSuffix));
    if (hotfix) {
      const dev = hotfix + devSuffix;
      const exists = radar?.pairs.some((p) => p.devBranch === dev && p.dev) ?? false;
      return { target: dev, create: !exists };
    }
    return { target: `${hotfixPrefix}port-${r.commit.short}${devSuffix}`, create: true };
  };

  return (
    <div className="page">
      <div className="ph">
        <h1>Где коммит?</h1>
      </div>
      <form
        className="findbox"
        onSubmit={(e) => {
          e.preventDefault();
          void run(q);
        }}
      >
        <label className="search big">
          <SearchIcon />
          <input
            ref={inputRef}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="хэш, номер задачи (LT-4888) или текст из сообщения"
            aria-label="Что искать"
            spellCheck={false}
          />
        </label>
        <button className="btn primary" type="submit" disabled={loading || !q.trim()}>
          {loading ? <span className="spin" /> : null}
          Найти
        </button>
      </form>
      <p className="legend">
        Номер задачи ищется и в сообщениях коммитов, и в названиях веток. Присутствие в {info.flow.master} и{" "}
        {info.flow.develop} проверяется по самому коммиту, по его cherry-pick и по содержимому (после squash).
      </p>
      {error && <div className="banner bad">{error}</div>}
      {search?.developStaleDays != null && (
        <div className="banner">
          {search.develop} отстаёт от {search.master} на {search.developStaleDays} дней: присутствие в {search.develop} не
          показываю.
        </div>
      )}
      {search && !loading && search.results.length === 0 && (
        <div className="tablewrap">
          <div className="empty">
            <h3>Ничего не нашлось</h3>
            Попробуйте часть хэша (от 4 символов), номер задачи или слово из сообщения.
          </div>
        </div>
      )}
      <div className={loading ? "stale-data" : ""}>
        {search?.results.map((r) => {
          const m = presence(r.master, r.firstTag);
          const d = presence(r.develop);
          const cls = VERDICT_CLS[r.verdict];
          return (
            <article key={r.commit.hash} className="fc">
              <div className="top1">
                <button className="hash" onClick={() => copyText(r.commit.hash)} title="Скопировать хэш">
                  {r.commit.short}
                </button>
                <span className="m">{r.commit.subject}</span>
                {r.isMerge && <span className="tag">merge</span>}
                <span className="sp" />
                <span className="muted small nowrap" title={fullDate(r.commit.date)}>
                  {r.commit.author.name} · {ago(r.commit.date)}
                </span>
              </div>
              <div className="where">
                <div>
                  <div className="k">{search.master}</div>
                  <div className={`v ${m.cls}`}>{m.text}</div>
                </div>
                {search.develop && search.developStaleDays == null && (
                  <div>
                    <div className="k">{search.develop}</div>
                    <div className={`v ${d.cls}`}>{d.text}</div>
                  </div>
                )}
                <div>
                  <div className="k">Ветки</div>
                  <div className="v">
                    {r.branches.length === 0 ? (
                      <span className="faint">нет</span>
                    ) : (
                      <>
                        {r.branches.slice(0, 5).map((b) => (
                          <div key={b} className="ell" title={b}>
                            {b}
                          </div>
                        ))}
                        {r.branches.length > 5 && (
                          <div className="faint">
                            и ещё {r.branches.length - 5} {plural(r.branches.length - 5, "ветка", "ветки", "веток")}
                          </div>
                        )}
                      </>
                    )}
                  </div>
                </div>
                {r.cherryPicks.length > 0 && (
                  <div>
                    <div className="k">Cherry-pick</div>
                    <div className="v">
                      {r.cherryPicks.map((c) => (
                        <div key={c.hash}>
                          <button className="hash" onClick={() => copyText(c.hash)} title={c.subject}>
                            {c.short}
                          </button>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
              <div className={`verdict ${cls}`}>
                <span>{verdictText(r, search)}</span>
                <span className="sp" />
                {r.verdict === "masterOnly" && !r.isMerge && (
                  <button className="btn sm hot" onClick={() => setPort({ commit: r.commit, ...portTarget(r) })}>
                    Перенести…
                  </button>
                )}
              </div>
            </article>
          );
        })}
        {search?.truncated && <div className="faint small">Показаны первые {search.results.length}: уточните запрос.</div>}
      </div>
      {port && (
        <PortDialog
          info={info}
          commits={[port.commit]}
          target={port.target}
          create={port.create}
          onClose={() => setPort(null)}
          onChanged={() => {
            onChanged();
            void run(q);
          }}
        />
      )}
    </div>
  );
}
