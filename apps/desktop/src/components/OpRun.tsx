// Running an operation step by step, shared by every dialog that changes the repository.
import { ReactNode, useEffect, useState } from "react";
import { api, commandOf, errorText, Operation, OpPlan } from "../api";

/** Drives the current operation: runs steps until it finishes or stops, then reports a change. */
export function useOpRunner(onChanged: () => void) {
  const [op, setOp] = useState<Operation | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const drive = async (start: Operation) => {
    let cur = start;
    setOp(cur);
    while (cur.status === "running") {
      cur = await api.opStep();
      setOp(cur);
    }
  };

  const run = async (f: () => Promise<Operation>, after?: () => void) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await f();
      if (next.status === "running") await drive(next);
      else setOp(next);
      onChanged();
      after?.();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const running = op?.status === "running";
  const canClose = !op || !["running", "failed"].includes(op.status);
  return { op, busy, error, run, running, canClose };
}

export function StepsView({ op }: { op: Operation }) {
  const running = op.status === "running";
  const next = op.states.findIndex((x) => x.kind !== "done");
  const failedAt = op.states.findIndex((s) => s.kind === "failed");
  return (
    <>
      <div className="steps">
        {op.steps.map((s, i) => {
          const st = op.states[i];
          const cls = st.kind === "done" ? "ok" : st.kind === "failed" ? "fail" : running && i === next ? "run" : "wait";
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
      {op.status === "rolledBack" && <div className="banner ok">Откатил: всё как было до операции.</div>}
      {op.undoError && <div className="banner bad">Откат остановился: {op.undoError}</div>}
    </>
  );
}

/** Footer buttons while and after an operation runs. */
export function RunFooter({
  runner,
  onClose,
}: {
  runner: ReturnType<typeof useOpRunner>;
  onClose: () => void;
}) {
  const { op, busy, run, running } = runner;
  if (!op) return null;
  const failedAt = op.states.findIndex((s) => s.kind === "failed");
  return (
    <>
      {running && <span className="faint small">Выполняю…</span>}
      {op.status === "failed" && (
        <>
          <button className="btn" disabled={busy} onClick={() => run(api.opLeave, onClose)}>
            Оставить как есть
          </button>
          {op.undo.length > 0 && (
            <button className="btn danger" disabled={busy} onClick={() => run(api.opRollback)}>
              {op.undoError ? "Повторить откат" : "Откатить"}
            </button>
          )}
          <button className="btn primary" disabled={busy} onClick={() => run(api.opStep)}>
            Повторить шаг {failedAt + 1}
          </button>
        </>
      )}
      {["done", "rolledBack"].includes(op.status) && (
        <button className="btn primary" onClick={onClose}>
          Закрыть
        </button>
      )}
    </>
  );
}

/** The plan as it will run: problems that block it, warnings, and the exact commands. */
export function PlanView({ plan }: { plan: OpPlan | null }) {
  if (!plan) {
    return (
      <div className="faint small">
        <span className="spin sm" /> Считаю план…
      </div>
    );
  }
  return (
    <>
      {plan.problems.length > 0 && (
        <div className="banner bad col">
          {plan.problems.map((p) => (
            <div key={p}>{p}</div>
          ))}
        </div>
      )}
      {plan.warnings.length > 0 && (
        <div className="warnbox">
          {plan.warnings.map((w) => (
            <div key={w}>{w}</div>
          ))}
        </div>
      )}
      <div>
        <div className="cmdhead">Будет выполнено</div>
        <pre className="cmds">
          {plan.commands.map((c, i) => (
            <span key={i}>
              <span className="p">$ </span>
              {c}
              {"\n"}
            </span>
          ))}
        </pre>
      </div>
    </>
  );
}

/** Dialog chrome with Escape and backdrop click handled. */
export function Dialog({
  title,
  icon,
  wide,
  canClose,
  onClose,
  children,
  footer,
}: {
  title: string;
  icon?: ReactNode;
  wide?: boolean;
  canClose: boolean;
  onClose: () => void;
  children: ReactNode;
  footer: ReactNode;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && canClose && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [canClose, onClose]);
  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && canClose && onClose()}>
      <div className={`dlg ${wide ? "wide" : ""}`} role="dialog" aria-modal="true" aria-label={title} tabIndex={-1}>
        <header>
          {icon && <span className="hot-ic">{icon}</span>}
          <h2>{title}</h2>
          <button className="ib x" onClick={onClose} disabled={!canClose} aria-label="Закрыть">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8">
              <path d="m4 4 8 8M12 4l-8 8" />
            </svg>
          </button>
        </header>
        <div className="body">{children}</div>
        <footer>{footer}</footer>
      </div>
    </div>
  );
}
