import { useEffect, useRef, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { After, api, commandOf, errorText, HotfixPlan, Operation, RepoInfo } from "../api";
import { ago, copyText, plural } from "../format";
import { CloseIcon, FlameIcon } from "./Icons";

interface Props {
  info: RepoInfo;
  onClose: () => void;
  /** Something in the repository changed: re-read it. */
  onChanged: () => void;
}

type Mode = "worktree" | "switch";

export default function HotfixDialog({ info, onClose, onChanged }: Props) {
  const dirty = info.changes.length;
  const [name, setName] = useState("");
  const [push, setPush] = useState(true);
  const [mode, setMode] = useState<Mode>(dirty ? "worktree" : "switch");
  const [stay, setStay] = useState(false);
  const [plan, setPlan] = useState<HotfixPlan | null>(null);
  const [op, setOp] = useState<Operation | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const after: After = stay ? "stay" : mode;

  useEffect(() => inputRef.current?.focus(), []);

  // The core plans the steps: names, checks, commands. Re-plan as the user types.
  useEffect(() => {
    if (op) return;
    let live = true;
    const t = window.setTimeout(() => {
      api
        .planHotfix({ name, push, after })
        .then((p) => live && setPlan(p))
        .catch((e) => live && setError(errorText(e)));
    }, 120);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [name, push, after, op]);

  const ready = !!plan && plan.slug !== "" && plan.problems.length === 0;
  const running = op?.status === "running";
  const canClose = !op || !["running", "failed"].includes(op.status);

  // Runs steps until the operation finishes or stops on a failure.
  const drive = async (start: Operation) => {
    let cur = start;
    setOp(cur);
    while (cur.status === "running") {
      cur = await api.opStep();
      setOp(cur);
    }
    onChanged();
  };

  const create = async () => {
    if (!ready || busy) return;
    setBusy(true);
    setError(null);
    try {
      await drive(await api.startHotfix({ name, push, after }));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const act = async (f: () => Promise<Operation>, thenClose = false) => {
    setBusy(true);
    setError(null);
    try {
      const next = await f();
      if (next.status === "running") await drive(next);
      else setOp(next);
      onChanged();
      if (thenClose) onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const close = async () => {
    if (!canClose) return;
    if (op) await api.opClose().catch(() => undefined);
    onClose();
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void close();
      if (e.key === "Enter" && !op && document.activeElement === inputRef.current) void create();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const master = info.bases.find((b) => b.role === "master");
  const develop = info.bases.find((b) => b.role === "develop");
  const failedAt = op ? op.states.findIndex((s) => s.kind === "failed") : -1;

  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="dlg" role="dialog" aria-modal="true" aria-labelledby="hf-title">
        <header>
          <span className="hot-ic">
            <FlameIcon />
          </span>
          <h2 id="hf-title">{op ? op.title : "Новый hotfix"}</h2>
          <button className="ib x" onClick={close} disabled={!canClose} aria-label="Закрыть">
            <CloseIcon />
          </button>
        </header>

        {!op ? (
          <div className="body">
            <div className="field">
              <label className="l" htmlFor="hf-name">
                Название
              </label>
              <input
                id="hf-name"
                ref={inputRef}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="LT-4900 ошибка в ваучере"
                autoComplete="off"
                spellCheck={false}
              />
              <div className="hint">Можно с пробелами и по-русски: номер задачи сохранится, остальное станет латиницей.</div>
            </div>

            <div className="newb">
              <div className="nb">
                <span className="ic m">M</span>
                <span className="n">{plan?.slug ? plan.masterBranch : `${info.flow.hotfixPrefix}…`}</span>
                <span className="from">
                  от свежего <b>{plan?.masterFrom ?? info.flow.master}</b>
                  {master && (
                    <>
                      {" "}
                      · <span className="mono">{master.tip.short}</span> · {ago(master.tip.date)}
                    </>
                  )}
                </span>
              </div>
              <div className="nb">
                <span className="ic d">D</span>
                <span className="n">{plan?.slug ? plan.devBranch : `${info.flow.hotfixPrefix}…${info.flow.devSuffix}`}</span>
                <span className="from">
                  от свежего <b>{plan?.devFrom ?? info.flow.develop}</b>
                  {develop && (
                    <>
                      {" "}
                      · <span className="mono">{develop.tip.short}</span> · {ago(develop.tip.date)}
                    </>
                  )}
                </span>
              </div>
            </div>

            {plan?.canPush && (
              <label className="opt">
                <input type="checkbox" checked={push} onChange={(e) => setPush(e.target.checked)} />
                <span>Запушить обе ветки и настроить upstream</span>
              </label>
            )}

            <div className={`after ${stay ? "off" : ""}`}>
              <div className="cmdhead">После создания</div>
              {dirty > 0 && (
                <div className="warnbox">
                  <b>
                    В <span className="mono">{info.head ?? info.headShort}</span> {dirty}{" "}
                    {plural(dirty, "незакоммиченное изменение", "незакоммиченных изменения", "незакоммиченных изменений")}:
                  </b>
                  <ul className="dfiles">
                    {info.changes.slice(0, 6).map((c) => (
                      <li key={c.path}>
                        <span className="s">{c.code}</span>
                        {c.path}
                      </li>
                    ))}
                    {dirty > 6 && <li className="faint">и ещё {dirty - 6}</li>}
                  </ul>
                  Чтобы их не задеть, hotfix откроется в отдельной папке (worktree). Если удобнее работать здесь, могу убрать
                  изменения в stash и переключиться.
                </div>
              )}
              <div className="modepick" role="group" aria-label="Как открыть hotfix">
                {(dirty
                  ? ([
                      ["worktree", "Worktree", "отдельная папка, здесь ничего не трогаю", true],
                      ["switch", "Stash и переключиться", "изменения уйдут в stash, переключусь в этой папке", false],
                    ] as const)
                  : ([
                      ["switch", "Переключиться здесь", "рабочая копия чистая, stash не нужен", false],
                      ["worktree", "Worktree", "отдельная папка", false],
                    ] as const)
                ).map(([k, title, sub, rec]) => (
                  <button key={k} type="button" className={`mode ${mode === k ? "on" : ""}`} aria-pressed={mode === k} onClick={() => setMode(k)}>
                    <b>
                      {title}
                      {rec && <span className="rec">рекомендую</span>}
                    </b>
                    <small>{sub}</small>
                  </button>
                ))}
              </div>
              <div className="hint">
                {mode === "worktree" ? (
                  <>
                    Создам папку <span className="mono">{plan?.worktreePath?.replace(/^\/Users\/[^/]+/, "~") ?? "…"}</span> с веткой{" "}
                    <span className="mono">{plan?.slug ? plan.masterBranch : "hotfix/…"}</span>.
                    {dirty > 0 && " Здесь всё останется как есть."}
                  </>
                ) : dirty > 0 ? (
                  <>
                    Все изменения, включая новые файлы, уйдут в stash. Чтобы вернуть: переключитесь обратно на{" "}
                    <span className="mono">{info.head}</span> и сделайте <span className="mono">git stash pop</span>, или отмените
                    операцию в журнале.
                  </>
                ) : (
                  <>
                    Переключусь на <span className="mono">{plan?.slug ? plan.masterBranch : "hotfix/…"}</span> в этой папке.
                  </>
                )}
              </div>
              <label className="opt">
                <input type="checkbox" checked={stay} onChange={(e) => setStay(e.target.checked)} />
                <span>Только создать ветки, никуда не переключаться</span>
              </label>
            </div>

            {plan && plan.slug && plan.problems.length > 0 && (
              <div className="banner bad">
                {plan.problems.map((p) => (
                  <div key={p}>{p}</div>
                ))}
              </div>
            )}

            <div>
              <div className="cmdhead">Будет выполнено</div>
              <pre className="cmds">
                {plan?.slug
                  ? plan.commands.map((c) => (
                      <span key={c}>
                        <span className="p">$ </span>
                        {c}
                        {"\n"}
                      </span>
                    ))
                  : "Введите название, чтобы увидеть команды"}
              </pre>
            </div>
            {error && <div className="banner bad">{error}</div>}
          </div>
        ) : (
          <div className="body">
            <div className="steps">
              {op.steps.map((s, i) => {
                const st = op.states[i];
                const isNext = running && i === op.states.findIndex((x) => x.kind !== "done");
                const cls = st.kind === "done" ? "ok" : st.kind === "failed" ? "fail" : isNext ? "run" : "wait";
                return (
                  <div key={i} className={`stp ${cls}`}>
                    <span className="i">{cls === "ok" ? "✓" : cls === "fail" ? "✗" : cls === "run" ? <span className="spin sm" /> : "·"}</span>
                    <span className="c">
                      <span className="t">{s.title}</span>
                      <span className="mono">$ {commandOf(s)}</span>
                      {st.kind === "failed" && <span className="out">{st.error}</span>}
                    </span>
                  </div>
                );
              })}
            </div>
            {op.status === "failed" && (
              <div className="warnbox">
                Остановился на шаге {failedAt + 1} из {op.steps.length}. Предыдущие шаги выполнены, дальше ничего не запускалось.
              </div>
            )}
            {op.status === "done" && <Done op={op} />}
            {op.status === "rolledBack" && <div className="banner ok">Откатил: всё как было до операции.</div>}
            {op.undoError && <div className="banner bad">Откат остановился: {op.undoError}</div>}
            {error && <div className="banner bad">{error}</div>}
          </div>
        )}

        <footer>
          {!op && (
            <>
              <span className="faint small hint-left">
                <kbd>Enter</kbd> создать
              </span>
              <button className="btn" onClick={close}>
                Отмена
              </button>
              <button className="btn hot" onClick={create} disabled={!ready || busy}>
                Создать ветки
              </button>
            </>
          )}
          {running && <span className="faint small">Выполняю…</span>}
          {op?.status === "failed" && (
            <>
              <button className="btn" disabled={busy} onClick={() => act(api.opLeave, true)}>
                Оставить как есть
              </button>
              {op.undo.length > 0 && (
                <button className="btn danger" disabled={busy} onClick={() => act(api.opRollback)}>
                  {op.undoError ? "Повторить откат" : "Откатить"}
                </button>
              )}
              <button className="btn primary" disabled={busy} onClick={() => act(api.opStep)}>
                Повторить шаг {failedAt + 1}
              </button>
            </>
          )}
          {op && ["done", "rolledBack"].includes(op.status) && (
            <button className="btn primary" onClick={close}>
              Закрыть
            </button>
          )}
        </footer>
      </div>
    </div>
  );
}

function Done({ op }: { op: Operation }) {
  const [copied, setCopied] = useState(false);
  const wt = op.steps.find((s) => s.args[0] === "worktree")?.args[2];
  return (
    <div className="banner ok col">
      <b>Готово.</b>
      {wt ? (
        <div className="wt">
          <span>
            Папка <span className="mono">{wt.replace(/^\/Users\/[^/]+/, "~")}</span>
          </span>
          <button className="btn sm" onClick={() => revealItemInDir(wt).catch(() => undefined)}>
            Показать в Finder
          </button>
          <button
            className="btn sm"
            onClick={async () => {
              setCopied(await copyText(wt));
            }}
          >
            {copied ? "Скопировано" : "Скопировать путь"}
          </button>
        </div>
      ) : (
        <span>Отменить можно в журнале.</span>
      )}
    </div>
  );
}
