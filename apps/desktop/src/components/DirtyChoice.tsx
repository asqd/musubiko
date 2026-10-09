// "You have uncommitted changes: open in a separate folder, or stash and switch here?"
// Shared by the hotfix and checkout dialogs so both ask the same way.
import { RepoInfo } from "../api";
import { plural } from "../format";
import { home } from "../platform";

export type WorkMode = "worktree" | "switch";

interface Props {
  info: RepoInfo;
  mode: WorkMode;
  onMode: (m: WorkMode) => void;
  /** Branch that will be opened, for the hint. */
  target: string;
  worktreePath: string | null;
  /** What the dialog is doing, for the warning: "hotfix откроется", "ветка откроется". */
  subject: string;
}


export default function DirtyChoice({ info, mode, onMode, target, worktreePath, subject }: Props) {
  const dirty = info.changes.length;
  const options = dirty
    ? ([
        ["worktree", "Worktree", "отдельная папка, здесь ничего не трогаю", true],
        ["switch", "Stash и переключиться", "изменения уйдут в stash, переключусь в этой папке", false],
      ] as const)
    : ([
        ["switch", "Переключиться здесь", "рабочая копия чистая, stash не нужен", false],
        ["worktree", "Worktree", "отдельная папка", false],
      ] as const);
  return (
    <>
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
          Чтобы их не задеть, {subject} в отдельной папке (worktree). Если удобнее работать здесь, могу убрать изменения в
          stash и переключиться.
        </div>
      )}
      <div className="modepick" role="group" aria-label="Где открыть">
        {options.map(([k, title, sub, rec]) => (
          <button key={k} type="button" className={`mode ${mode === k ? "on" : ""}`} aria-pressed={mode === k} onClick={() => onMode(k)}>
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
            Создам папку <span className="mono">{worktreePath ? home(worktreePath) : "…"}</span> с веткой{" "}
            <span className="mono">{target}</span>.{dirty > 0 && " Здесь всё останется как есть."}
          </>
        ) : dirty > 0 ? (
          <>
            Все изменения, включая новые файлы, уйдут в stash. Когда вернётесь на <span className="mono">{info.head}</span>{" "}
            через Musubiko, он предложит вернуть их; ещё их можно вернуть отменой операции в журнале.
          </>
        ) : (
          <>
            Переключусь на <span className="mono">{target}</span> в этой папке.
          </>
        )}
      </div>
    </>
  );
}
