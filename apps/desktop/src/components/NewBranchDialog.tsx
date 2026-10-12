import { useEffect, useMemo, useRef, useState } from "react";
import { After, api, Branch, errorText, NewBranchPlan, RepoInfo } from "../api";
import { BranchIcon } from "./Icons";
import { Dialog, OpDone, RunFooter, StepsView, useOpRunner } from "./OpRun";
import DirtyChoice, { useFreshInfo, useWorkMode } from "./DirtyChoice";
import BasePicker from "./BasePicker";
import { ErrorBanner } from "./ErrorBanner";

interface Props {
  info: RepoInfo;
  /** The base chosen on the branch list: the new branch starts from it unless changed here. */
  base: string;
  /** Known branches, for the base picker and the usual prefixes. */
  branches: Branch[];
  onClose: () => void;
  /** Something in the repository changed: re-read it. */
  onChanged: () => void;
}

/** The prefixes this repository's branches use most, e.g. feature/, bugfix/. */
function usualPrefixes(branches: Branch[], hotfixPrefix: string): string[] {
  const count = new Map<string, number>();
  for (const b of branches) {
    const i = b.name.indexOf("/");
    if (i <= 0) continue;
    const p = b.name.slice(0, i + 1);
    if (p === hotfixPrefix) continue;
    count.set(p, (count.get(p) ?? 0) + 1);
  }
  const found = [...count.entries()].sort((a, b) => b[1] - a[1]).map(([p]) => p);
  return (found.length ? found : ["feature/", "bugfix/"]).slice(0, 4);
}

export default function NewBranchDialog({ info: screenInfo, base: initialBase, branches, onClose, onChanged }: Props) {
  const info = useFreshInfo(screenInfo);
  const prefixes = useMemo(() => usualPrefixes(branches, info.flow.hotfixPrefix), [branches, info.flow.hotfixPrefix]);
  const [name, setName] = useState(prefixes[0] ?? "");
  const [base, setBase] = useState(initialBase);
  const [fresh, setFresh] = useState(true);
  const [push, setPush] = useState(false);
  const [mode, setMode] = useWorkMode(info);
  const [stay, setStay] = useState(false);
  const [plan, setPlan] = useState<NewBranchPlan | null>(null);
  const runner = useOpRunner(onChanged);
  const op = runner.op;
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const after: After = stay ? "stay" : mode;
  const input = { name, base, fresh, push, after };

  useEffect(() => {
    const el = inputRef.current;
    el?.focus();
    el?.setSelectionRange(el.value.length, el.value.length);
  }, []);

  // The core plans the steps: the name, where it starts, checks and commands.
  useEffect(() => {
    if (op) return;
    let live = true;
    const t = window.setTimeout(() => {
      api
        .planNewBranch(input)
        .then((p) => {
          if (!live) return;
          setPlan(p);
          setError(null);
        })
        .catch((e) => live && setError(errorText(e)));
    }, 120);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [name, base, fresh, push, after, op]);

  const typed = name.trim() !== "" && !prefixes.includes(name.trim());
  const ready = !!plan && typed && plan.problems.length === 0 && !runner.busy;
  const create = () => ready && runner.run(() => api.startNewBranch(input));
  const close = () => {
    if (!runner.canClose) return;
    if (op) api.opClose().catch(() => undefined);
    onClose();
  };

  /** Swaps the prefix, keeping what was typed after it. */
  const usePrefix = (p: string) => {
    const rest = prefixes.reduce((n, q) => (n.startsWith(q) ? n.slice(q.length) : n), name);
    setName(p + rest);
    inputRef.current?.focus();
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && !op && document.activeElement === inputRef.current) void create();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const baseNames = info.bases.map((b) => b.name);
  const shown = plan && typed ? plan.name : null;

  return (
    <Dialog
      title={op ? op.title : "Новая ветка"}
      icon={<BranchIcon />}
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
            <button className="btn primary" onClick={create} disabled={!ready}>
              Создать ветку
            </button>
          </>
        )
      }
    >
      {!op ? (
        <>
          <div className="field">
            <label className="l" htmlFor="nb-name">
              Название
            </label>
            <input
              id="nb-name"
              ref={inputRef}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="feature/Project-12 новая форма"
              autoComplete="off"
              spellCheck={false}
            />
            <div className="nb-prefixes">
              {prefixes.map((p) => (
                <button key={p} className={`chip ${name.startsWith(p) ? "on" : ""}`} onClick={() => usePrefix(p)}>
                  {p}
                </button>
              ))}
              <span className="hint">Можно с пробелами и по-русски: номер задачи сохранится, остальное станет латиницей.</span>
            </div>
          </div>

          <div className="field">
            <span className="l">От ветки</span>
            <div className="nb-base">
              <BasePicker value={base} bases={baseNames} branches={branches.map((b) => b.name)} root={info.root} onChange={setBase} align="left" title="Ветка, от которой начнётся новая" />
              {plan?.canFresh && (
                <label className="opt">
                  <input type="checkbox" checked={fresh} onChange={(e) => setFresh(e.target.checked)} />
                  <span>
                    от свежей <span className="mono">{info.remote}/{base}</span>, получив её с сервера
                  </span>
                </label>
              )}
            </div>
          </div>

          <div className="newb">
            <div className="nb">
              <span className="ic n">
                <BranchIcon />
              </span>
              <span className="n">{shown ?? "…"}</span>
              <span className="from">
                от <b>{plan?.from ?? base}</b>
              </span>
            </div>
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
              <span>Запушить ветку и настроить upstream</span>
            </label>
          )}

          <div className={`after ${stay ? "off" : ""}`}>
            <div className="cmdhead">После создания</div>
            <DirtyChoice
              info={info}
              mode={mode}
              onMode={setMode}
              target={shown ?? "новую ветку"}
              worktreePath={plan?.worktreePath ?? null}
              subject="ветка откроется"
            />
            <label className="opt">
              <input type="checkbox" checked={stay} onChange={(e) => setStay(e.target.checked)} />
              <span>Только создать ветку, никуда не переключаться</span>
            </label>
          </div>

          {plan && typed && plan.problems.length > 0 && (
            <div className="banner bad col">
              {plan.problems.map((p) => (
                <div key={p}>{p}</div>
              ))}
            </div>
          )}

          <div>
            <div className="cmdhead">Будет выполнено</div>
            <pre className="cmds">
              {plan && typed
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
