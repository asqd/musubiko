import { useEffect, useRef, useState } from "react";
import { After, api, errorText, HotfixPlan, RepoInfo } from "../api";
import { ago } from "../format";
import { FlameIcon } from "./Icons";
import { Dialog, OpDone, RunFooter, StepsView, useOpRunner } from "./OpRun";
import DirtyChoice, { useFreshInfo, useWorkMode } from "./DirtyChoice";
import { ErrorBanner } from "./ErrorBanner";

interface Props {
  info: RepoInfo;
  onClose: () => void;
  /** Something in the repository changed: re-read it. */
  onChanged: () => void;
}

export default function HotfixDialog({ info: screenInfo, onClose, onChanged }: Props) {
  const info = useFreshInfo(screenInfo);
  const [name, setName] = useState("");
  const [push, setPush] = useState(true);
  const [mode, setMode] = useWorkMode(info);
  const [stay, setStay] = useState(false);
  const [plan, setPlan] = useState<HotfixPlan | null>(null);
  const runner = useOpRunner(onChanged);
  const op = runner.op;
  const [error, setError] = useState<string | null>(null);
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

  const ready = !!plan && plan.slug !== "" && plan.problems.length === 0 && !runner.busy;
  const create = () => ready && runner.run(() => api.startHotfix({ name, push, after }));
  const close = () => {
    if (!runner.canClose) return;
    if (op) api.opClose().catch(() => undefined);
    onClose();
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && !op && document.activeElement === inputRef.current) void create();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const master = info.bases.find((b) => b.role === "master");
  const develop = info.bases.find((b) => b.role === "develop");

  return (
    <Dialog
      title={op ? op.title : "Новый hotfix"}
      icon={<FlameIcon />}
      canClose={runner.canClose}
      onClose={close}
      footer={
        op ? (
          <RunFooter runner={runner} onClose={close} />
        ) : (
          <>
            <span className="faint small hint-left">
              <kbd>Enter</kbd> создать
            </span>
            <button className="btn" onClick={close}>
              Отмена
            </button>
            <button className="btn hot" onClick={create} disabled={!ready}>
              {plan && !plan.withDev ? "Создать ветку" : "Создать ветки"}
            </button>
          </>
        )
      }
    >
        {!op ? (
          <>
            <div className="field">
              <label className="l" htmlFor="hf-name">
                Название
              </label>
              <input
                id="hf-name"
                ref={inputRef}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Project-4900 ошибка в документах"
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
              {(plan ? plan.withDev : !!develop) && (
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
              )}
            </div>

            {plan && plan.warnings.length > 0 && (
              <div className="warnbox">
                {plan.warnings.map((w) => (
                  <div key={w}>{w}</div>
                ))}
              </div>
            )}

            {plan?.canPush && (
              <label className="opt">
                <input type="checkbox" checked={push} onChange={(e) => setPush(e.target.checked)} />
                <span>{plan.withDev ? "Запушить обе ветки и настроить upstream" : "Запушить ветку и настроить upstream"}</span>
              </label>
            )}

            <div className={`after ${stay ? "off" : ""}`}>
              <div className="cmdhead">После создания</div>
              <DirtyChoice
                info={info}
                mode={mode}
                onMode={setMode}
                target={plan?.slug ? plan.masterBranch : `${info.flow.hotfixPrefix}…`}
                worktreePath={plan?.worktreePath ?? null}
                subject="hotfix откроется"
              />
              <label className="opt">
                <input type="checkbox" checked={stay} onChange={(e) => setStay(e.target.checked)} />
                <span>{plan && !plan.withDev ? "Только создать ветку, никуда не переключаться" : "Только создать ветки, никуда не переключаться"}</span>
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
            {error && <ErrorBanner error={error} />}
          </>
        ) : (
          <>
            <StepsView op={op} />
            {op.status === "done" && <OpDone op={op} />}
          </>
        )}
        {runner.error && <ErrorBanner error={runner.error} />}
    </Dialog>
  );
}
