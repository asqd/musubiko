import { useEffect, useState } from "react";
import { api, Branch, CheckoutPlan, errorText, RepoInfo } from "../api";
import { StatusPill } from "./Branches";
import DirtyChoice, { useFreshInfo, useWorkMode } from "./DirtyChoice";
import { Dialog, OpDone, PlanView, RunFooter, StepsView, useOpRunner } from "./OpRun";
import { ErrorBanner } from "./ErrorBanner";

interface Props {
  info: RepoInfo;
  branch: Branch;
  onClose: () => void;
  onChanged: () => void;
}

export default function CheckoutDialog({ info: screenInfo, branch, onClose, onChanged }: Props) {
  const info = useFreshInfo(screenInfo);
  const [mode, setMode] = useWorkMode(info);
  const [restore, setRestore] = useState(true);
  const [plan, setPlan] = useState<CheckoutPlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const runner = useOpRunner(onChanged);
  const input = { name: branch.name, mode, restoreStash: restore };

  useEffect(() => {
    if (runner.op) return;
    let live = true;
    setPlan(null);
    api
      .planCheckout(input)
      .then((p) => live && setPlan(p))
      .catch((e) => live && setPlanError(errorText(e)));
    return () => {
      live = false;
    };
  }, [mode, restore, runner.op]);

  const ready = !!plan && plan.problems.length === 0 && !runner.busy;
  const close = () => {
    if (runner.op) api.opClose().catch(() => undefined);
    onClose();
  };

  return (
    <Dialog
      title={runner.op?.title ?? (mode === "worktree" ? `Открыть ${branch.name}` : `Переключиться на ${branch.name}`)}
      canClose={runner.canClose}
      onClose={close}
      footer={
        runner.op ? (
          <RunFooter runner={runner} onClose={close} />
        ) : (
          <>
            <button className="btn" onClick={close}>
              Отмена
            </button>
            <button className="btn primary" disabled={!ready} onClick={() => runner.run(() => api.startCheckout(input))}>
              {mode === "worktree" ? "Открыть в папке" : "Переключиться"}
            </button>
          </>
        )
      }
    >
      {runner.op ? (
        <>
          <StepsView op={runner.op} />
          {runner.op.status === "done" && <OpDone op={runner.op} />}
        </>
      ) : (
        <>
          <div className="nb">
            <span className="ic d">→</span>
            <span className="n">{branch.name}</span>
            <span className="from">
              <StatusPill status={branch.status} target={branch.target} />{" "}
              {branch.location === "remote" ? `есть только на ${info.remote}: создам локальную копию, которая его отслеживает` : ""}
            </span>
          </div>
          <DirtyChoice
            info={info}
            mode={mode}
            onMode={setMode}
            target={branch.name}
            worktreePath={plan?.worktreePath ?? null}
            subject="ветка откроется"
          />
          {plan?.savedStash && (
            <label className="opt">
              <input type="checkbox" checked={restore} onChange={(e) => setRestore(e.target.checked)} />
              <span>
                Вернуть изменения, которые Musubiko убрал в stash, когда вы уходили с {branch.name}
                <small>{plan.savedStash}</small>
              </span>
            </label>
          )}
          <PlanView plan={plan} />
          {planError && <ErrorBanner error={planError} />}
        </>
      )}
      {runner.error && <ErrorBanner error={runner.error} />}
    </Dialog>
  );
}
