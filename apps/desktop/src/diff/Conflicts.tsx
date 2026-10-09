import { ConflictFile } from "../api";
import { key } from "../platform";

interface Props {
  files: ConflictFile[] | null;
  branch: string;
  target: string;
}

/** What `git merge` would stop on, taken from an in-memory `git merge-tree`. */
export default function Conflicts({ files, branch, target }: Props) {
  if (!files) {
    return (
      <div className="tablewrap">
        <div className="empty">
          <span className="spin" /> Сливаю в памяти…
        </div>
      </div>
    );
  }
  if (files.length === 0) {
    return (
      <div className="tablewrap">
        <div className="empty">
          <h3>Конфликтов нет</h3>
          Похоже, {target} изменился и ветка теперь сливается чисто. Обновите список веток ({key("R")}).
        </div>
      </div>
    );
  }
  return (
    <>
      <p className="legend">
        Так выглядел бы <span className="mono">git merge {branch}</span> в {target}. Проверено через{" "}
        <span className="mono">git merge-tree</span>: рабочая копия и индекс не менялись. Чтобы разобраться заранее, сделайте в
        ветке <span className="mono">git merge {target}</span> или <span className="mono">git rebase {target}</span>.
      </p>
      {files.map((f) => (
        <section key={f.path} className="conf">
          <div className="fh">
            <span className="path">
              <b>{f.path}</b>
            </span>
            <span className="pill bad">
              {f.regions.length
                ? `${f.regions.length} ${f.regions.length === 1 ? "место" : f.regions.length < 5 ? "места" : "мест"}`
                : "конфликт"}
            </span>
          </div>
          {f.note && <div className="note">{f.note}</div>}
          {f.regions.map((r, i) => (
            <div key={i} className="region">
              <div className="rline faint small">строка {r.line}</div>
              <pre>
                {r.before.map((l, k) => (
                  <span key={`b${k}`} className="ctx">
                    {l}
                    {"\n"}
                  </span>
                ))}
                <span className="side base-side">
                  <span className="lab">в {target}</span>
                  {r.base.length ? r.base.join("\n") : "(пусто)"}
                  {"\n"}
                </span>
                <span className="side branch-side">
                  <span className="lab">в {branch}</span>
                  {r.branch.length ? r.branch.join("\n") : "(пусто)"}
                  {"\n"}
                </span>
                {r.after.map((l, k) => (
                  <span key={`a${k}`} className="ctx">
                    {l}
                    {"\n"}
                  </span>
                ))}
              </pre>
            </div>
          ))}
        </section>
      ))}
    </>
  );
}
