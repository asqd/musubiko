// Carrying commits over to the develop side, and deleting branches.
import { useEffect, useState } from "react";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, Branch, Commit, errorText, OpPlan, PickState, RepoInfo } from "../api";
import { StatusPill } from "./Branches";
import { copyText, plural } from "../format";
import { Dialog, OpDone, PlanView, RunFooter, StepsView, useOpRunner } from "./OpRun";
import { ErrorBanner } from "./ErrorBanner";
import { home, inFileManager } from "../platform";

interface PortProps {
  info: RepoInfo;
  commits: Commit[];
  /** Branch that receives the commits; editable when it has to be created. */
  target: string;
  create: boolean;
  onClose: () => void;
  onChanged: () => void;
}

export function PortDialog({ info, commits, target: initialTarget, create, onClose, onChanged }: PortProps) {
  const [target, setTarget] = useState(initialTarget);
  const [push, setPush] = useState(true);
  const [plan, setPlan] = useState<OpPlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const runner = useOpRunner(onChanged);
  const input = { commits: commits.map((c) => c.hash), target, create, push };

  useEffect(() => {
    if (runner.op) return;
    let live = true;
    const t = window.setTimeout(() => {
      setPlan(null);
      api
        .planPort(input)
        .then((p) => live && setPlan(p))
        .catch((e) => live && setPlanError(errorText(e)));
    }, 150);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [target, push, runner.op]);

  const n = commits.length;
  const ready = !!plan && plan.problems.length === 0 && !runner.busy;
  return (
    <Dialog
      title={runner.op?.title ?? plan?.title ?? (create ? "Создать -dev и перенести" : "Перенести в -dev")}
      wide
      canClose={runner.canClose}
      onClose={onClose}
      footer={
        runner.op ? (
          <RunFooter runner={runner} onClose={onClose} />
        ) : (
          <>
            <button className="btn" onClick={onClose}>
              Отмена
            </button>
            <button className="btn hot" disabled={!ready} onClick={() => runner.run(() => api.startPort(input))}>
              Перенести
            </button>
          </>
        )
      }
    >
      {runner.op ? (
        <>
          <StepsView op={runner.op} />
          {runner.op.status === "failed" && runner.op.steps.some((s, i) => s.resume && runner.op!.states[i].kind === "failed") && (
            <ConflictPanel runner={runner} target={target} />
          )}
          {runner.op.status === "done" && <OpDone op={runner.op} />}
        </>
      ) : (
        <>
          <div className="small">
            {create ? "Создам ветку от свежего " + info.flow.develop + " и перенесу" : "Перенесу"} {n}{" "}
            {plural(n, "коммит", "коммита", "коммитов")} через cherry-pick:
          </div>
          <ul className="clist-plain">
            {commits.map((c) => (
              <li key={c.hash}>
                <span className="hash">{c.short}</span> {c.subject}
              </li>
            ))}
          </ul>
          {create ? (
            <div className="field">
              <label className="l" htmlFor="port-target">
                Новая ветка
              </label>
              <input id="port-target" value={target} onChange={(e) => setTarget(e.target.value)} spellCheck={false} />
            </div>
          ) : (
            <div className="small">
              В ветку <span className="mono">{target}</span>
            </div>
          )}
          {info.remote && (
            <label className="opt">
              <input type="checkbox" checked={push} onChange={(e) => setPush(e.target.checked)} />
              <span>Запушить на {info.remote}</span>
            </label>
          )}
          <div className="hint">
            Работа идёт во временной папке рядом с репозиторием: ваша текущая ветка и незакоммиченные изменения не
            затрагиваются. Ветка передвинется, только когда все коммиты перенесутся; конфликт можно разрешить по ходу.
          </div>
          <PlanView plan={plan} />
          {planError && <ErrorBanner error={planError} />}
        </>
      )}
      {runner.error && <ErrorBanner error={runner.error} />}
    </Dialog>
  );
}

interface DeleteProps {
  info: RepoInfo;
  names: string[];
  /** Details for the list: status and where each branch lives. */
  branches?: Branch[];
  onClose: () => void;
  onChanged: () => void;
}

export function DeleteDialog({ info, names, branches, onClose, onChanged }: DeleteProps) {
  const [local, setLocal] = useState(true);
  const [remote, setRemote] = useState(true);
  const [ack, setAck] = useState(false);
  const [plan, setPlan] = useState<OpPlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const runner = useOpRunner(onChanged);
  const input = { names, local, remote };

  useEffect(() => {
    if (runner.op) return;
    let live = true;
    setPlan(null);
    api
      .planDelete(input)
      .then((p) => live && setPlan(p))
      .catch((e) => live && setPlanError(errorText(e)));
    return () => {
      live = false;
    };
  }, [local, remote, runner.op]);

  const needsAck = !!plan && plan.warnings.length > 0;
  const ready = !!plan && plan.problems.length === 0 && (!needsAck || ack) && !runner.busy;
  return (
    <Dialog
      title={runner.op?.title ?? plan?.title ?? "Удалить ветки"}
      canClose={runner.canClose}
      onClose={onClose}
      footer={
        runner.op ? (
          <RunFooter runner={runner} onClose={onClose} />
        ) : (
          <>
            <button className="btn" onClick={onClose}>
              Отмена
            </button>
            <button className="btn danger solid" disabled={!ready} onClick={() => runner.run(() => api.startDelete(input))}>
              Удалить
            </button>
          </>
        )
      }
    >
      {runner.op ? (
        <StepsView op={runner.op} />
      ) : (
        <>
          <ul className="clist-plain dl">
            {names.map((n) => {
              const b = branches?.find((x) => x.name === n);
              return (
                <li key={n}>
                  <span className="mono ell">{n}</span>
                  {b && <StatusPill status={b.status} target={b.target} />}
                  {b && (
                    <span className="faint small nowrap">
                      {b.upstreamGone
                        ? "только локально, на сервере удалена"
                        : b.location === "both"
                          ? `локально и на ${info.remote}`
                          : b.location === "local"
                            ? "только локально"
                            : `только на ${info.remote}`}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
          <div className="opts-row">
            <label className="opt">
              <input type="checkbox" checked={local} onChange={(e) => setLocal(e.target.checked)} />
              <span>Локальные</span>
            </label>
            {info.remote && (
              <label className="opt">
                <input type="checkbox" checked={remote} onChange={(e) => setRemote(e.target.checked)} />
                <span>На {info.remote}</span>
              </label>
            )}
          </div>
          <PlanView plan={plan} />
          {needsAck && (
            <label className="opt">
              <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} />
              <span>Понимаю, удалить всё равно</span>
            </label>
          )}
          <div className="hint">Хэши сохраняются в журнале: оттуда ветки можно вернуть.</div>
          {planError && <ErrorBanner error={planError} />}
        </>
      )}
      {runner.error && <ErrorBanner error={runner.error} />}
    </Dialog>
  );
}


/** A cherry-pick stopped on a conflict: resolve it in the scratch folder and carry on. */
function ConflictPanel({ runner, target }: { runner: ReturnType<typeof useOpRunner>; target: string }) {
  const [state, setState] = useState<PickState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const load = () =>
    api
      .opPickState()
      .then((s) => {
        setState(s);
        setError(null);
      })
      .catch((e) => setError(errorText(e)));

  // Re-read when coming back from the editor.
  useEffect(() => {
    void load();
    const onFocus = () => void load();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [runner.op]);

  const take = async (path: string, side: "branch" | "commit") => {
    try {
      setState(await api.opPickTake(path, side));
    } catch (e) {
      setError(errorText(e));
    }
  };

  if (!state) return error ? <ErrorBanner error={error} /> : null;
  const left = state.files.filter((f) => !f.resolved).length;
  return (
    <section className="pick">
      <div className="pick-h">
        <b>Конфликт при переносе</b>
        {state.current && (
          <span className="small">
            на коммите <span className="hash">{state.current.short}</span> {state.current.subject}
          </span>
        )}
        {state.remaining > 0 && (
          <span className="faint small">
            после него ещё {state.remaining} {plural(state.remaining, "коммит", "коммита", "коммитов")}
          </span>
        )}
      </div>
      <div className="pick-dir">
        <span className="mono ell" title={state.dir}>
          {home(state.dir)}
        </span>
        <button className="btn sm" onClick={() => revealItemInDir(state.dir).catch(() => undefined)}>
          Показать {inFileManager}
        </button>
        <button className="btn sm" onClick={async () => setCopied(await copyText(state.dir))}>
          {copied ? "Скопировано" : "Скопировать путь"}
        </button>
      </div>
      {state.files.length > 0 && (
        <ul className="pick-files">
          {state.files.map((f) => (
            <li key={f.path}>
              <span className="tag">{f.code}</span>
              <button className="linklike mono ell" title="Открыть файл" onClick={() => openPath(`${state.dir}/${f.path}`).catch(() => undefined)}>
                {f.path}
              </button>
              <span className={`pill ${f.resolved ? "ok" : "bad"}`}>{f.resolved ? "разрешён" : "есть маркеры"}</span>
              <button className="btn sm" onClick={() => take(f.path, "branch")} title={`Оставить файл как в ${target}`}>
                Взять из ветки
              </button>
              <button className="btn sm" onClick={() => take(f.path, "commit")} title="Оставить файл как в переносимом коммите">
                Взять из коммита
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="hint">
        Откройте файлы в редакторе, уберите маркеры <span className="mono">{"<<<<<<<"}</span>, <span className="mono">=======</span>,{" "}
        <span className="mono">{">>>>>>>"}</span> и вернитесь сюда. <span className="mono">git add</span> Musubiko сделает сам.
      </div>
      {error && <ErrorBanner error={error} />}
      <div className="pick-actions">
        <button className="btn ghost" onClick={() => void load()}>
          Обновить
        </button>
        <span className="sp" />
        <button className="btn" disabled={runner.busy} onClick={() => runner.run(() => api.opResume(true))} title="git cherry-pick --skip">
          Пропустить коммит
        </button>
        <button className="btn primary" disabled={runner.busy || left > 0} onClick={() => runner.run(() => api.opResume(false))}>
          {left > 0 ? `Осталось разрешить: ${left}` : "Проверить и продолжить"}
        </button>
      </div>
    </section>
  );
}
