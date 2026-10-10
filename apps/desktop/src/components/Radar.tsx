import { useState } from "react";
import { Branch, Commit, Flow, HotfixPair, PairState, Radar as RadarData, RepoInfo } from "../api";
import { ago, fullDate, plural } from "../format";
import { StatusPill } from "./Branches";
import { DeleteDialog, PortDialog } from "./OpDialogs";
import { FlameIcon } from "./Icons";
import { ErrorBanner } from "./ErrorBanner";

/** A pair's state as a label and a pill class, with the repository's own base names. */
export function pairState(state: PairState, flow: Flow): [string, string] {
  const { master, develop, devSuffix } = flow;
  switch (state) {
    case "lost":
      return [`в ${master}, но не в ${develop}`, "bad"];
    case "noDev":
      return [`нет ${devSuffix} ветки`, "bad"];
    case "unsynced":
      return ["не всё перенесено", "warn"];
    case "work":
      return ["в работе", "hot"];
    case "released":
      return [`в ${master}, ждёт ${develop}`, "open"];
    case "empty":
      return ["коммитов пока нет", "mute"];
    case "done":
      return ["завершён", "ok"];
  }
}
const ATTENTION: PairState[] = ["lost", "noDev", "unsynced"];

export const needsAttention = (r: RadarData | null) =>
  r ? r.pairs.filter((p) => ATTENTION.includes(p.state)).length + r.lost.length : 0;

type Dialog =
  | { kind: "port"; commits: Commit[]; target: string; create: boolean }
  | { kind: "delete"; names: string[] }
  | null;

interface Props {
  info: RepoInfo;
  radar: RadarData | null;
  error: string | null;
  loading: boolean;
  onHotfix: () => void;
  onDiff: (b: Branch) => void;
  /** Open "Где коммит?" for a hash. */
  onFind: (hash: string) => void;
  onChanged: () => void;
}

export default function Radar({ info, radar, error, loading, onHotfix, onDiff, onFind, onChanged }: Props) {
  const [dialog, setDialog] = useState<Dialog>(null);
  const [showDone, setShowDone] = useState(false);
  const { hotfixPrefix, devSuffix } = info.flow;

  const groups = radar
    ? {
        attention: radar.pairs.filter((p) => ATTENTION.includes(p.state)),
        active: radar.pairs.filter((p) => ["work", "released", "empty"].includes(p.state)),
        done: radar.pairs.filter((p) => p.state === "done"),
      }
    : null;

  const portLost = (c: Commit) =>
    setDialog({ kind: "port", commits: [c], target: `${hotfixPrefix}port-${c.short}${devSuffix}`, create: true });

  return (
    <div className="page">
      <div className="ph">
        <h1>Хотфиксы</h1>
        {loading && <span className="spin muted" />}
        <span className="sp" />
        <button className="btn hot" onClick={onHotfix}>
          <FlameIcon />
          Новый hotfix
        </button>
      </div>
      <p className="legend">
        Хотфикс — пара веток: <span className="mono">{hotfixPrefix}имя</span> от {info.flow.master} и{" "}
        <span className="mono">
          {hotfixPrefix}имя{devSuffix}
        </span>{" "}
        от {info.flow.develop}. Пары сверяются по содержимому коммитов, поэтому cherry-pick и squash-слияние тоже считаются
        переносом.
      </p>
      {error && <ErrorBanner error={error} />}

      {radar?.developStaleDays != null && (
        <div className="banner">
          {radar.develop} отстаёт от {radar.master} на {radar.developStaleDays} дней: похоже, этот репозиторий живёт без{" "}
          {radar.develop}. Проверку «в {radar.master}, но не в {radar.develop}» пропускаю.
        </div>
      )}

      {radar && radar.lost.length > 0 && (
        <section className="alert-box">
          <div className="alert-h">
            <b>
              {radar.lost.length} {plural(radar.lost.length, "коммит есть", "коммита есть", "коммитов есть")} в {radar.master}, но
              не в {radar.develop}
            </b>
            <span className="muted small">
              Без переноса эти исправления пропадут при следующем релизе из {radar.develop}.
            </span>
          </div>
          {radar.lost.map((c) => (
            <div key={c.hash} className="srow">
              <button className="hash" onClick={() => onFind(c.hash)} title="Где этот коммит?">
                {c.short}
              </button>
              <span className="m" title={c.subject}>
                {c.subject}
              </span>
              <span className="muted small nowrap" title={fullDate(c.date)}>
                {c.author.name} · {ago(c.date)}
              </span>
              <button className="btn sm" onClick={() => portLost(c)}>
                Перенести…
              </button>
            </div>
          ))}
        </section>
      )}

      {radar && radar.pairs.length === 0 && (
        <div className="tablewrap">
          <div className="empty">
            <h3>Свежих хотфиксов нет</h3>
            {radar.olderHidden > 0
              ? `${radar.olderHidden} ${plural(radar.olderHidden, "пара старше", "пары старше", "пар старше")} ${radar.recentDays} дней не показаны.`
              : "Создайте первый кнопкой «Новый hotfix»."}
          </div>
        </div>
      )}

      {groups && (
        <>
          {groups.attention.length > 0 && <h2 className="group-h">Требуют внимания</h2>}
          {groups.attention.map((p) => (
            <PairCard key={p.slug} pair={p} info={info} onDiff={onDiff} onFind={onFind} onDialog={setDialog} />
          ))}
          {groups.active.length > 0 && <h2 className="group-h">В работе</h2>}
          {groups.active.map((p) => (
            <PairCard key={p.slug} pair={p} info={info} onDiff={onDiff} onFind={onFind} onDialog={setDialog} />
          ))}
          {groups.done.length > 0 && (
            <button className="group-h toggle-h" onClick={() => setShowDone((v) => !v)}>
              {showDone ? "▾" : "▸"} Завершённые <span className="faint">{groups.done.length}</span>
            </button>
          )}
          {showDone &&
            groups.done.map((p) => (
              <PairCard key={p.slug} pair={p} info={info} onDiff={onDiff} onFind={onFind} onDialog={setDialog} />
            ))}
          {radar && radar.olderHidden > 0 && radar.pairs.length > 0 && (
            <div className="faint small">
              Ещё {radar.olderHidden} {plural(radar.olderHidden, "пара старше", "пары старше", "пар старше")} {radar.recentDays} дней не
              показаны.
            </div>
          )}
        </>
      )}

      {dialog?.kind === "port" && (
        <PortDialog
          info={info}
          commits={dialog.commits}
          target={dialog.target}
          create={dialog.create}
          onClose={() => setDialog(null)}
          onChanged={onChanged}
        />
      )}
      {dialog?.kind === "delete" && (
        <DeleteDialog info={info} names={dialog.names} onClose={() => setDialog(null)} onChanged={onChanged} />
      )}
    </div>
  );
}

function Lane({ name, target, b, onDiff }: { name: string; target: string; b: Branch | null; onDiff: (b: Branch) => void }) {
  if (!b) {
    return (
      <div className="lane miss">
        <div className="lt">
          <b>{name}</b>
          <span className="to">→ {target}</span>
        </div>
        <span className="pill bad">ветки нет</span>
        <span />
      </div>
    );
  }
  return (
    <div className="lane">
      <div className="lt">
        <b>{b.name}</b>
        <span className="to">→ {b.target}</span>
        {b.location === "local" && <span className="tag">локальная</span>}
        {b.location === "remote" && <span className="tag">только на сервере</span>}
      </div>
      <span className="lb">
        <StatusPill status={b.status} target={b.target} />
        <span className="ab num">
          <span className="a">↑{b.ahead}</span>
        </span>
      </span>
      <button className="btn sm" onClick={() => onDiff(b)}>
        Дифф
      </button>
    </div>
  );
}

function PairCard({
  pair: p,
  info,
  onDiff,
  onFind,
  onDialog,
}: {
  pair: HotfixPair;
  info: RepoInfo;
  onDiff: (b: Branch) => void;
  onFind: (hash: string) => void;
  onDialog: (d: Dialog) => void;
}) {
  const [label, cls] = pairState(p.state, info.flow);
  const missing = p.commits.filter((c) => !c.ported).map((c) => c.commit);
  const newest = [p.master, p.dev].filter(Boolean).map((b) => b!.tip.date);
  const owner = p.master?.owner.name ?? p.dev?.owner.name;
  return (
    <article className={`hf ${cls === "bad" ? "badc" : cls === "warn" ? "warnc" : ""}`}>
      <div className="hh">
        <h3>{p.slug}</h3>
        <span className={`pill ${cls}`}>{label}</span>
        <span className="sp" />
        <span className="muted small">
          {owner}
          {newest.length > 0 && ` · ${ago(Math.max(...newest))}`}
        </span>
      </div>
      <div className="lanes">
        <Lane name={p.masterBranch} target={info.flow.master} b={p.master} onDiff={onDiff} />
        <Lane name={p.devBranch} target={info.flow.develop} b={p.dev} onDiff={onDiff} />
      </div>
      {p.commits.length > 0 && (
        <div className="sync">
          <h4>Коммиты {info.flow.master}-части → {info.flow.develop}-сторона</h4>
          {p.commits.map(({ commit: c, ported }) => (
            <div key={c.hash} className="srow">
              <button className="hash" onClick={() => onFind(c.hash)} title="Где этот коммит?">
                {c.short}
              </button>
              <span className="m" title={c.subject}>
                {c.subject}
              </span>
              {ported ? (
                <span className="res ok" title={ported.byContent ? "Отдельного коммита нет, но изменения уже там (например, squash)" : ""}>
                  ✓ {ported.byContent ? "уже" : ported.short} в {ported.place}
                </span>
              ) : (
                <span className="res no">✗ не перенесён</span>
              )}
            </div>
          ))}
        </div>
      )}
      {(p.note || missing.length > 0 || p.state === "done") && (
        <div className="foot">
          {p.note && <span className="muted small">{p.note}</span>}
          <span className="sp" />
          {missing.length > 0 && p.dev && (
            <button className="btn hot" onClick={() => onDialog({ kind: "port", commits: missing, target: p.devBranch, create: false })}>
              Перенести в {p.devBranch.slice(info.flow.hotfixPrefix.length)}: {missing.length}
            </button>
          )}
          {missing.length > 0 && !p.dev && (
            <button className="btn hot" onClick={() => onDialog({ kind: "port", commits: missing, target: p.devBranch, create: true })}>
              Создать {p.devBranch.slice(info.flow.hotfixPrefix.length)} и перенести {missing.length}
            </button>
          )}
          {p.state === "done" && (
            <button
              className="btn danger"
              onClick={() => onDialog({ kind: "delete", names: [p.master?.name, p.dev?.name].filter((n): n is string => !!n) })}
            >
              Удалить ветки…
            </button>
          )}
        </div>
      )}
    </article>
  );
}
