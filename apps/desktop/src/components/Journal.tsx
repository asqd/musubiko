import { useEffect, useState } from "react";
import { api, commandOf, errorText, Operation, OpStatus, withoutHints } from "../api";
import { fullDate } from "../format";
import { ErrorBanner } from "./ErrorBanner";

const STATUS: Record<OpStatus, [string, string]> = {
  running: ["идёт", "open"],
  done: ["выполнено", "ok"],
  failed: ["остановилась", "bad"],
  partial: ["частично", "warn"],
  rolledBack: ["откачено", "mute"],
  undone: ["отменено", "mute"],
};

export default function Journal({ tick, onChanged }: { tick: number; onChanged: () => void }) {
  const [ops, setOps] = useState<Operation[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () =>
    api
      .journal()
      .then(setOps)
      .catch((e) => setError(errorText(e)));

  useEffect(() => {
    void load();
  }, [tick]);

  const undo = async (op: Operation) => {
    setBusy(true);
    setError(null);
    try {
      const done = await api.journalUndo(op.id);
      if (done.undoError) setError(done.undoError);
      setConfirm(null);
      await load();
      onChanged();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="page">
      <div className="ph">
        <h1>Журнал</h1>
        <span className="muted small">Всё, что Musubiko делал с этим репозиторием, с точными командами.</span>
      </div>
      {error && <ErrorBanner error={error} />}
      {ops && ops.length === 0 && (
        <div className="tablewrap">
          <div className="empty">
            <h3>Пока пусто</h3>
            Здесь появятся созданные хотфиксы и другие операции.
          </div>
        </div>
      )}
      {ops?.map((op) => {
        const [label, cls] = STATUS[op.status];
        const done = op.states.filter((s) => s.kind === "done").length;
        const canUndo = op.undo.length > 0 && ["done", "partial"].includes(op.status);
        return (
          <div key={op.id} className="logi">
            <div className="r1">
              <span className="faint small num">{fullDate(op.started)}</span>
              <b>{op.title}</b>
              <span className={`pill ${cls}`}>{label}</span>
              <span className="sp" />
              {canUndo && confirm !== op.id && (
                <button className="btn sm" onClick={() => setConfirm(op.id)}>
                  Отменить…
                </button>
              )}
            </div>
            {confirm === op.id && (
              <div className="confirm">
                <div>Будет сделано, по порядку:</div>
                <ol>
                  {[...op.undo].reverse().map((u, i) => (
                    <li key={i}>
                      {u.title} <span className="mono faint">$ {commandOf(u)}</span>
                    </li>
                  ))}
                </ol>
                <div className="hint">Ветку с новыми коммитами и чужой stash Musubiko не тронет и остановится.</div>
                <div className="row">
                  <button className="btn" onClick={() => setConfirm(null)} disabled={busy}>
                    Не отменять
                  </button>
                  <button className="btn danger" onClick={() => undo(op)} disabled={busy}>
                    {busy ? "Отменяю…" : "Отменить операцию"}
                  </button>
                </div>
              </div>
            )}
            {op.undoError && <ErrorBanner prefix="Отмена остановилась: " error={op.undoError} />}
            <details>
              <summary>
                {done} из {op.steps.length} шагов
              </summary>
              <pre className="cmds">
                {op.steps.map((s, i) => {
                  const st = op.states[i];
                  return (
                    <span key={i} className={st.kind}>
                      <span className="p">{st.kind === "done" ? "✓" : st.kind === "failed" ? "✗" : "·"} $ </span>
                      {commandOf(s)}
                      {"\n"}
                      {st.kind === "failed" && <span className="err">  {withoutHints(st.error)}{"\n"}</span>}
                    </span>
                  );
                })}
              </pre>
            </details>
          </div>
        );
      })}
    </div>
  );
}
