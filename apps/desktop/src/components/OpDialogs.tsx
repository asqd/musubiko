// Carrying commits over to the develop side, and deleting branches.
import { useEffect, useState } from "react";
import { api, Commit, errorText, OpPlan, RepoInfo } from "../api";
import { plural } from "../format";
import { Dialog, PlanView, RunFooter, StepsView, useOpRunner } from "./OpRun";

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
        <StepsView op={runner.op} />
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
            Работа идёт во временной папке: ваша текущая ветка и незакоммиченные изменения не затрагиваются. Ветка
            передвинется, только если все коммиты перенеслись без конфликтов.
          </div>
          <PlanView plan={plan} />
          {planError && <div className="banner bad">{planError}</div>}
        </>
      )}
      {runner.error && <div className="banner bad">{runner.error}</div>}
    </Dialog>
  );
}

interface DeleteProps {
  info: RepoInfo;
  names: string[];
  onClose: () => void;
  onChanged: () => void;
}

export function DeleteDialog({ info, names, onClose, onChanged }: DeleteProps) {
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
          <ul className="clist-plain">
            {names.map((n) => (
              <li key={n} className="mono">
                {n}
              </li>
            ))}
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
          {planError && <div className="banner bad">{planError}</div>}
        </>
      )}
      {runner.error && <div className="banner bad">{runner.error}</div>}
    </Dialog>
  );
}
