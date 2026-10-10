import { useEffect, useRef, useState } from "react";
import { api, Branch, BranchCard as Card, errorText, RepoInfo } from "../api";
import { ago, fullDate, plural } from "../format";
import BasePicker from "./BasePicker";
import { StatusPill } from "./Branches";
import { DeleteDialog } from "./OpDialogs";
import { ErrorBanner } from "./ErrorBanner";
import { CardIcon } from "./Icons";

interface Props {
  info: RepoInfo;
  /** The branch shown, or null before one is picked. */
  name: string | null;
  /** The base chosen on the branch list: commits and files are counted against it. */
  base: string;
  /** Every branch, for the picker. */
  branches: Branch[];
  tick: number;
  onPick: (name: string) => void;
  onDiff: (branch: Branch, base: string) => void;
  onCheckout: (branch: Branch) => void;
  onFind: (hash: string) => void;
  onNewBranch: (base: string) => void;
  onChanged: () => void;
}

const FILES_SHOWN = 100;

export default function BranchCard({ info, name, base, branches, tick, onPick, onDiff, onCheckout, onFind, onNewBranch, onChanged }: Props) {
  const [card, setCard] = useState<Card | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [allFiles, setAllFiles] = useState(false);
  const request = useRef(0);

  useEffect(() => {
    setAllFiles(false);
    if (!name) {
      setCard(null);
      return;
    }
    const id = ++request.current;
    setLoading(true);
    setError(null);
    api
      .branchCard(name, base)
      .then((c) => id === request.current && setCard(c))
      .catch((e) => {
        if (id !== request.current) return;
        setCard(null);
        setError(errorText(e));
      })
      .finally(() => id === request.current && setLoading(false));
  }, [name, base, tick, info.root]);

  const baseNames = new Set(info.bases.map((b) => b.name));
  const names = branches.map((b) => b.name).filter((n) => !baseNames.has(n));
  const mine = branches.find((b) => b.isHead && !baseNames.has(b.name));

  const b = card?.branch;
  const added = card?.files.reduce((s, f) => s + (f.added ?? 0), 0) ?? 0;
  const removed = card?.files.reduce((s, f) => s + (f.removed ?? 0), 0) ?? 0;

  return (
    <div className="page">
      <div className="ph">
        <h1>Ветка</h1>
        <BasePicker
          value={name ?? "выбрать…"}
          bases={[]}
          branches={names}
          root={info.root}
          memory="recentCards"
          onChange={onPick}
          align="left"
          title="Какую ветку показать"
        />
        {loading && <span className="spin muted" />}
      </div>

      {!name && (
        <div className="tablewrap">
          <div className="empty">
            <h3>Выберите ветку</h3>
            Найдите её по имени выше или откройте из списка веток.
            {mine && (
              <div style={{ marginTop: 12 }}>
                <button className="btn" onClick={() => onPick(mine.name)}>
                  Текущая: <span className="mono">{mine.name}</span>
                </button>
              </div>
            )}
          </div>
        </div>
      )}

      {error && <ErrorBanner error={error} />}

      {card && b && (
        <article className="hf card">
          <div className="hh">
            <CardIcon />
            <h3>{b.name}</h3>
            {b.isHead && <span className="tag headtag">HEAD</span>}
            {b.location === "remote" && <span className="tag">только на {info.remote ?? "сервере"}</span>}
            {b.location === "local" && !b.upstreamGone && <span className="tag">не запушена</span>}
            {b.upstreamGone && <span className="tag warn">удалена на сервере</span>}
            {b.sync && b.sync.ahead > 0 && <span className="tag warn">↑{b.sync.ahead} не запушено</span>}
            {b.sync && b.sync.behind > 0 && <span className="tag">↓{b.sync.behind} на сервере</span>}
            <span className="sp" />
            <span className="muted small">
              {b.owner.name || b.tip.author.name}
              {card.started && (
                <span title={fullDate(card.started)}> · начата {ago(card.started)} назад</span>
              )}
              <span title={fullDate(b.tip.date)}> · последний коммит {ago(b.tip.date)} назад</span>
            </span>
          </div>

          <div className="lanes">
            {card.against.map((x) => (
              <div key={x.target} className="lane">
                <div className="lt">
                  <span className="to">против</span>
                  <b>{x.target}</b>
                  {x.target === b.target && <span className="tag">основная база</span>}
                  {x.status.kind === "conflict" && (
                    <span className="muted small" title={x.status.files.join("\n")}>
                      конфликт в {x.status.files.slice(0, 3).join(", ")}
                      {x.status.files.length > 3 && ` и ещё ${x.status.files.length - 3}`}
                    </span>
                  )}
                </div>
                <span className="lb">
                  <StatusPill status={x.status} target={x.target} />
                  <span className="ab num">
                    <span className="a">↑{x.ahead}</span>
                    <span className="b">↓{x.behind}</span>
                  </span>
                </span>
                <button className="btn sm" onClick={() => onDiff(x, x.target)}>
                  Дифф
                </button>
              </div>
            ))}
          </div>

          <div className="sync">
            <h4>
              Свои коммиты против {b.target}: {card.commitCount}
            </h4>
            {card.commitCount === 0 && <div className="muted small">Своих коммитов нет: всё уже в {b.target}.</div>}
            {card.commits.map((c) => (
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
              </div>
            ))}
            {card.commitCount > card.commits.length && (
              <div className="faint small">
                Показаны {card.commits.length} последних из {card.commitCount}.
              </div>
            )}
          </div>

          {card.files.length > 0 && (
            <div className="sync">
              <h4>
                {card.files.length} {plural(card.files.length, "файл", "файла", "файлов")} ·{" "}
                <span className="add">+{added}</span> <span className="del">−{removed}</span>
              </h4>
              {(allFiles ? card.files : card.files.slice(0, FILES_SHOWN)).map((f) => (
                <div key={f.path} className="frow">
                  <span className="mono ell" title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path}>
                    {f.oldPath && <span className="faint">{f.oldPath} → </span>}
                    {f.path}
                  </span>
                  {f.added === null ? (
                    <span className="faint small">бинарный</span>
                  ) : (
                    <span className="num small">
                      <span className="add">+{f.added}</span> <span className="del">−{f.removed}</span>
                    </span>
                  )}
                </div>
              ))}
              {!allFiles && card.files.length > FILES_SHOWN && (
                <button className="linklike small" onClick={() => setAllFiles(true)}>
                  Показать все {card.files.length}
                </button>
              )}
            </div>
          )}

          <div className="foot">
            <span className="muted small">
              {card.upstream
                ? `Следит за ${card.upstream}`
                : b.location === "remote"
                  ? "Локальной копии нет"
                  : "Upstream не настроен"}
            </span>
            <span className="sp" />
            {!b.isHead && (
              <button className="btn danger" onClick={() => setDeleting(true)}>
                Удалить…
              </button>
            )}
            <button className="btn" onClick={() => onNewBranch(b.name)}>
              Новая ветка от неё
            </button>
            {!b.isHead && (
              <button className="btn" onClick={() => onCheckout(b)}>
                Checkout
              </button>
            )}
            <button className="btn primary" onClick={() => onDiff(b, b.target)}>
              Дифф против {b.target}
            </button>
          </div>
        </article>
      )}

      {deleting && card && (
        <DeleteDialog info={info} names={[card.branch.name]} branches={[card.branch]} onClose={() => setDeleting(false)} onChanged={onChanged} />
      )}
    </div>
  );
}
