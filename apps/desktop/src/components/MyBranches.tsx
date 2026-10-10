import { useEffect, useRef, useState } from "react";
import { api, Attention, Branch, errorText, MyBranches as Data, RepoInfo, WorkBranch } from "../api";
import { ago, fullDate, plural } from "../format";
import { StatusPill } from "./Branches";
import { DeleteDialog } from "./OpDialogs";
import { ErrorBanner } from "./ErrorBanner";
import { CardIcon, PlusIcon, SwitchIcon } from "./Icons";
import PersonPicker from "./PersonPicker";

type Filter = "all" | "attention" | "active" | "merged";

interface Props {
  info: RepoInfo;
  /** The current user's branches, loaded by the app (they also feed the sidebar badge). */
  mine: Data | null;
  loading: boolean;
  error: string | null;
  /** The base chosen on the branch list. */
  base: string;
  tick: number;
  onDiff: (branch: Branch, base: string) => void;
  onCard: (name: string) => void;
  onCheckout: (branch: Branch) => void;
  onFind: (hash: string) => void;
  onNewBranch: () => void;
  onChanged: () => void;
}

const EMPTY: Record<Filter, string> = {
  all: "Веток нет",
  attention: "Ничего не требует внимания",
  active: "Веток в работе без проблем нет",
  merged: "Слитых веток нет",
};

/** What stands in the way, as a chip: text and how bad it is. */
function reason(a: Attention, target: string): [string, "bad" | "warn"] {
  switch (a.kind) {
    case "conflict": {
      const files = a.files.slice(0, 2).join(", ") + (a.files.length > 2 ? ` и ещё ${a.files.length - 2}` : "");
      return [`конфликт с ${target}: ${files}`, "bad"];
    }
    case "behind":
      return [`${target} ушёл вперёд на ${a.count} ${plural(a.count, "коммит", "коммита", "коммитов")}`, "warn"];
    case "unpushed":
      return [`${a.count} ${plural(a.count, "коммит не запушен", "коммита не запушено", "коммитов не запушено")}`, "warn"];
    case "neverPushed":
      return ["ни разу не запушена", "warn"];
    case "goneWithWork":
      return ["удалена на сервере, а свои коммиты остались", "bad"];
    case "quiet":
      return [`${a.days} ${plural(a.days, "день", "дня", "дней")} без коммитов`, "warn"];
    case "unknown":
      return [a.error, "bad"];
  }
}

export default function MyBranches({ info, mine, loading, error, base, tick, onDiff, onCard, onCheckout, onFind, onNewBranch, onChanged }: Props) {
  // Someone else's branches, picked from the list of owners; null shows the user's own.
  const [who, setWho] = useState<string | null>(null);
  const [other, setOther] = useState<Data | null>(null);
  const [otherLoading, setOtherLoading] = useState(false);
  const [otherError, setOtherError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [deleting, setDeleting] = useState<Branch[] | null>(null);
  const request = useRef(0);

  useEffect(() => setWho(null), [info.root]);
  useEffect(() => {
    if (!who) return;
    const id = ++request.current;
    setOtherLoading(true);
    setOtherError(null);
    api
      .myBranches(base, who)
      .then((d) => id === request.current && setOther(d))
      .catch((e) => id === request.current && setOtherError(errorText(e)))
      .finally(() => id === request.current && setOtherLoading(false));
  }, [who, base, tick]);

  const data = who ? other : mine;
  const busy = who ? otherLoading : loading;
  const err = who ? otherError : error;
  const owners = mine?.owners ?? [];
  const isMe = !who;
  const show = (f: Filter) => filter === "all" || filter === f;
  const chips: [Filter, string, number][] = data
    ? [
        ["all", "Все", data.attention.length + data.active.length + data.merged.length],
        ["attention", "Требуют внимания", data.attention.length],
        ["active", "В работе", data.active.length],
        ["merged", "Слиты", data.merged.length],
      ]
    : [];
  const deletable = data?.merged.filter((b) => !b.isHead) ?? [];

  return (
    <div className="page">
      <div className="ph">
        <h1>{isMe ? "Мои ветки" : "Ветки коллеги"}</h1>
        {owners.length > 0 && (
          <PersonPicker
            value={who}
            me={info.user}
            people={owners.map((o) => ({ person: o.person, hint: `${o.active} в работе` }))}
            onChange={setWho}
          />
        )}
        {busy && <span className="spin muted" />}
        <span className="sp" />
        <button className="btn" onClick={onNewBranch}>
          <PlusIcon />
          Новая ветка
        </button>
      </div>
      <p className="legend">
        Ветки, где автор первого своего коммита — {isMe ? "вы" : data?.author.name || data?.author.email || "коллега"}. Сверху то,
        что мешает влить: конфликт, отставание от базы больше чем на {data?.behindAlert ?? 20} коммитов, незапушенное, затишье
        дольше {data?.quietDays ?? 7} дней. Слитые можно удалить разом.
      </p>
      {isMe && !info.user.email && (
        <div className="banner">
          В git config не задан user.email, поэтому «мои» ветки не определить. Задайте его: git config user.email you@example.com
        </div>
      )}
      {err && <ErrorBanner error={err} />}

      {data && (
        <>
          <div className="filters">
            {chips.map(([k, l, n]) => (
              <button key={k} className={`fchip ${filter === k ? "on" : ""} ${k === "attention" && n > 0 ? "alert" : ""}`} onClick={() => setFilter(k)}>
                {l}
                <span className="n">{n}</span>
              </button>
            ))}
          </div>

          {show("attention") && data.attention.length > 0 && (
            <>
              {filter === "all" && (
                <h2 className="group-h">
                  Требуют внимания <span className="faint">{data.attention.length}</span>
                </h2>
              )}
              {data.attention.map((w) => (
                <WorkCard key={w.branch.name} w={w} onDiff={onDiff} onCard={onCard} onCheckout={onCheckout} onFind={onFind} />
              ))}
            </>
          )}
          {show("active") && data.active.length > 0 && (
            <>
              {filter === "all" && (
                <h2 className="group-h">
                  В работе <span className="faint">{data.active.length}</span>
                </h2>
              )}
              {data.active.map((w) => (
                <WorkCard key={w.branch.name} w={w} onDiff={onDiff} onCard={onCard} onCheckout={onCheckout} onFind={onFind} />
              ))}
            </>
          )}
          {show("merged") && data.merged.length > 0 && (
            <>
              {filter === "all" && (
                <h2 className="group-h">
                  Слиты, можно удалить <span className="faint">{data.merged.length}</span>
                </h2>
              )}
              <div className="hf merged-list">
                {data.merged.map((b) => (
                  <div key={b.name} className="mrow">
                    <button className="linklike mono ell" onClick={() => onCard(b.name)} title="Карточка ветки">
                      {b.name}
                    </button>
                    <StatusPill status={b.status} target={b.target} />
                    <span className="faint small nowrap" title={fullDate(b.tip.date)}>
                      {b.upstreamGone ? "удалена на сервере · " : ""}
                      {ago(b.tip.date)} назад
                    </span>
                  </div>
                ))}
                <div className="foot">
                  <span className="muted small">Удалю локально и на сервере одним push, с возвратом из журнала.</span>
                  <span className="sp" />
                  <button className="btn danger" disabled={deletable.length === 0} onClick={() => setDeleting(deletable)}>
                    Удалить {deletable.length}…
                  </button>
                </div>
              </div>
            </>
          )}
          {(filter === "all" ? chips[0][2] : chips.find((c) => c[0] === filter)?.[2]) === 0 && (
            <div className="tablewrap">
              <div className="empty">
                <h3>{EMPTY[filter]}</h3>
                {filter === "all" && isMe && "Создайте ветку кнопкой «Новая ветка»."}
              </div>
            </div>
          )}
        </>
      )}

      {deleting && deleting.length > 0 && (
        <DeleteDialog info={info} names={deleting.map((b) => b.name)} branches={deleting} onClose={() => setDeleting(null)} onChanged={onChanged} />
      )}
    </div>
  );
}

function WorkCard({
  w,
  onDiff,
  onCard,
  onCheckout,
  onFind,
}: {
  w: WorkBranch;
  onDiff: (b: Branch, base: string) => void;
  onCard: (name: string) => void;
  onCheckout: (b: Branch) => void;
  onFind: (hash: string) => void;
}) {
  const b = w.branch;
  const reasons = w.attention.map((a) => reason(a, b.target));
  const cls = reasons.some((r) => r[1] === "bad") ? "badc" : reasons.length ? "warnc" : "";
  const slash = b.name.indexOf("/");
  return (
    <article className={`hf work ${cls}`}>
      <div className="hh">
        <h3>
          {slash > 0 && <span className="faint">{b.name.slice(0, slash + 1)}</span>}
          {slash > 0 ? b.name.slice(slash + 1) : b.name}
        </h3>
        {b.isHead && <span className="tag headtag">HEAD</span>}
        <StatusPill status={b.status} target={b.target} />
        <span className="to mono">→ {b.target}</span>
        <span className="ab num">
          <span className="a">↑{b.ahead}</span>
          <span className="b">↓{b.behind}</span>
        </span>
        <span className="sp" />
        <span className="muted small" title={fullDate(b.tip.date)}>
          последний коммит {ago(b.tip.date)} назад
        </span>
      </div>
      {reasons.length > 0 && (
        <div className="why">
          {reasons.map(([t, k]) => (
            <span key={t} className={`reason ${k}`}>
              {t}
            </span>
          ))}
        </div>
      )}
      <dl className="meta">
        <div>
          <dt>Начата</dt>
          <dd title={w.started ? fullDate(w.started) : undefined}>{w.started ? `${ago(w.started)} назад` : "—"}</dd>
        </div>
        <div>
          <dt>Свои коммиты</dt>
          <dd>{w.commitCount}</dd>
        </div>
        <div>
          <dt>Файлы</dt>
          <dd>
            {w.files} · <span className="add">+{w.added}</span> <span className="del">−{w.removed}</span>
          </dd>
        </div>
        <div>
          <dt>На сервере</dt>
          <dd>
            {b.location === "local"
              ? b.upstreamGone
                ? "удалена"
                : "нет"
              : b.sync && b.sync.ahead > 0
                ? `↑${b.sync.ahead} не запушено`
                : "всё запушено"}
          </dd>
        </div>
      </dl>
      {w.recent.length > 0 && (
        <div className="sync">
          {w.recent.map((c) => (
            <div key={c.hash} className="srow">
              <button className="hash" onClick={() => onFind(c.hash)} title="Где этот коммит?">
                {c.short}
              </button>
              <span className="m" title={c.subject}>
                {c.subject}
              </span>
              <span className="muted small nowrap" title={fullDate(c.date)}>
                {ago(c.date)}
              </span>
            </div>
          ))}
          {w.commitCount > w.recent.length && (
            <div className="faint small">и ещё {w.commitCount - w.recent.length} в карточке</div>
          )}
        </div>
      )}
      <div className="foot">
        <button className="btn sm" onClick={() => onCard(b.name)}>
          <CardIcon />
          Карточка
        </button>
        <button className="btn sm" onClick={() => onDiff(b, b.target)}>
          Дифф
        </button>
        {!b.isHead && (
          <button className="btn sm" onClick={() => onCheckout(b)}>
            <SwitchIcon />
            Checkout
          </button>
        )}
      </div>
    </article>
  );
}
