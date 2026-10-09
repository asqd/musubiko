import { memo, useMemo, useState } from "react";
import { api, errorText, FileDiff } from "../api";
import { blocks, BIG_FILE_LINES, Gap, gapAfter, gapBefore, isGenerated, isMigration, lineCount, Row } from "./model";
import { highlight, highlightMarked, languageOf } from "./highlight";

export type Mode = "unified" | "split";

interface Props {
  file: FileDiff;
  index: number;
  /** Revision the new side comes from, for reading hidden context. */
  rev: string;
  mode: Mode;
  viewed: boolean;
  collapsed: boolean;
  active: boolean;
  onViewed: (index: number) => void;
  onCollapse: (index: number) => void;
}

const CHANGE_TAG: Partial<Record<FileDiff["change"], string>> = {
  added: "новый",
  deleted: "удалён",
  renamed: "переименован",
  copied: "копия",
};

function PathLabel({ path }: { path: string }) {
  const k = path.lastIndexOf("/");
  return k < 0 ? (
    <b>{path}</b>
  ) : (
    <>
      <span className="dir">{path.slice(0, k + 1)}</span>
      <b>{path.slice(k + 1)}</b>
    </>
  );
}

/** GitHub's five squares: share of additions and deletions. */
function Blocks({ a, d }: { a: number; d: number }) {
  const total = a + d;
  const green = total ? Math.round((a / total) * 5) : 0;
  return (
    <span className="blocks" aria-hidden="true">
      {[0, 1, 2, 3, 4].map((k) => (
        <i key={k} className={!total ? "" : k < green ? "a" : "d"} />
      ))}
    </span>
  );
}

function FileView({ file, index, rev, mode, viewed, collapsed, active, onViewed, onCollapse }: Props) {
  const lang = useMemo(() => languageOf(file.path), [file.path]);
  const big = lineCount(file) > BIG_FILE_LINES || isGenerated(file.path);
  const [forceOpen, setForceOpen] = useState(false);
  const [source, setSource] = useState<string[] | null>(null);
  const [opened, setOpened] = useState<Set<string>>(new Set());
  const [loadError, setLoadError] = useState<string | null>(null);

  const hidden = collapsed;
  const showBody = !hidden && (!big || forceOpen);

  const expand = async (key: string) => {
    try {
      if (!source) setSource(await api.fileLines(rev, file.path));
      setOpened((s) => new Set(s).add(key));
    } catch (e) {
      setLoadError(errorText(e));
    }
  };

  return (
    <section className={`file ${hidden ? "collapsed" : ""} ${active ? "act" : ""}`} id={`file-${index}`}>
      <div className="fh">
        <button className="chev" onClick={() => onCollapse(index)} aria-label={hidden ? "Развернуть" : "Свернуть"}>
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2">
            <path d={hidden ? "m6 4 4 4-4 4" : "m4 6 4 4 4-4"} />
          </svg>
        </button>
        <span className="path">
          {file.oldPath && (
            <>
              <span className="dir">{file.oldPath}</span>
              <span className="arrow"> → </span>
            </>
          )}
          <PathLabel path={file.path} />
        </span>
        {CHANGE_TAG[file.change] && <span className={`tag ${file.change === "deleted" ? "warn" : ""}`}>{CHANGE_TAG[file.change]}</span>}
        {file.modeChange && <span className="tag" title="Изменились права файла">{file.modeChange}</span>}
        {isMigration(file.path) && <span className="pill warn">миграция</span>}
        {isGenerated(file.path) && <span className="tag">сгенерированный</span>}
        <span className="right">
          <span className="num small">
            <span className="add-n">+{file.additions}</span> <span className="del-n">−{file.deletions}</span>
            <Blocks a={file.additions} d={file.deletions} />
          </span>
          <label className={`viewed ${viewed ? "on" : ""}`}>
            <input type="checkbox" checked={viewed} onChange={() => onViewed(index)} />
            Просмотрено
          </label>
        </span>
      </div>
      {!hidden && (
        <div className="code">
          {file.binary ? (
            <div className="note">Бинарный файл: содержимое не показывается.</div>
          ) : file.hunks.length === 0 ? (
            <div className="note">
              {file.change === "renamed" || file.change === "copied"
                ? "Содержимое не изменилось."
                : file.modeChange
                  ? "Изменились только права файла."
                  : "Изменения только в пробелах."}
            </div>
          ) : !showBody ? (
            <div className="note">
              {isGenerated(file.path) ? "Сгенерированный файл" : "Большой дифф"}: {lineCount(file)} строк.{" "}
              <button className="btn sm" onClick={() => setForceOpen(true)}>
                Показать
              </button>
            </div>
          ) : (
            <Table file={file} lang={lang} mode={mode} source={source} opened={opened} onExpand={expand} />
          )}
          {file.truncated && <div className="note">Файл слишком большой: показаны первые 5000 строк диффа.</div>}
          {loadError && <div className="note bad">{loadError}</div>}
        </div>
      )}
    </section>
  );
}

export default memo(FileView);

interface TableProps {
  file: FileDiff;
  lang: string | null;
  mode: Mode;
  source: string[] | null;
  opened: Set<string>;
  onExpand: (key: string) => void;
}

function Table({ file, lang, mode, source, opened, onExpand }: TableProps) {
  const split = mode === "split";
  const rows: JSX.Element[] = [];
  const cols = 4;

  const gapRows = (gap: Gap | null, key: string) => {
    if (!gap) return;
    if (opened.has(key) && source) {
      const to = gap.to ?? source.length;
      for (let n = gap.from; n <= to; n++) {
        const text = source[n - 1] ?? "";
        const row: Row = { kind: "context", text, old: n + gap.delta, new: n };
        rows.push(split ? <SplitCtx key={`${key}-${n}`} row={row} lang={lang} /> : <UniRow key={`${key}-${n}`} row={row} lang={lang} />);
      }
      return;
    }
    if (opened.has(key) && gap.to === null) return; // reached the end of the file
    const count = gap.to === null ? null : gap.to - gap.from + 1;
    rows.push(
      <tr key={key} className="gap">
        <td colSpan={cols}>
          <button onClick={() => onExpand(key)}>
            {count === null ? "⋯ Показать до конца файла" : `⋯ Показать ${count} ${count === 1 ? "строку" : count < 5 ? "строки" : "строк"} (${gap.from}–${gap.to})`}
          </button>
        </td>
      </tr>,
    );
  };

  file.hunks.forEach((h, hi) => {
    gapRows(gapBefore(file, hi), `g${hi}`);
    rows.push(
      <tr key={`h${hi}`} className="hunk">
        {split ? <td className="no" /> : <><td className="no" /><td className="no" /></>}
        <td colSpan={split ? 3 : 2} title={h.section || undefined}>
          @@ −{h.oldStart},{h.oldLines} +{h.newStart},{h.newLines} @@ {h.section}
        </td>
      </tr>,
    );
    blocks(h).forEach((b, bi) => {
      const k = `${hi}-${bi}`;
      if (b.type === "ctx") {
        rows.push(split ? <SplitCtx key={k} row={b.row} lang={lang} /> : <UniRow key={k} row={b.row} lang={lang} />);
        return;
      }
      if (!split) {
        b.dels.forEach((r, j) => rows.push(<UniRow key={`${k}d${j}`} row={r} lang={lang} />));
        b.adds.forEach((r, j) => rows.push(<UniRow key={`${k}a${j}`} row={r} lang={lang} />));
        return;
      }
      const n = Math.max(b.dels.length, b.adds.length);
      for (let j = 0; j < n; j++) rows.push(<SplitChg key={`${k}s${j}`} del={b.dels[j]} add={b.adds[j]} lang={lang} />);
    });
  });
  gapRows(gapAfter(file), "tail");

  return (
    <table className={`d ${split ? "split" : ""}`}>
      {split && (
        <colgroup>
          <col className="c-no" />
          <col />
          <col className="c-no" />
          <col />
        </colgroup>
      )}
      <tbody>{rows}</tbody>
    </table>
  );
}

const sign = { context: " ", add: "+", del: "−" } as const;

function UniRow({ row, lang }: { row: Row; lang: string | null }) {
  const html = highlightMarked(row.text, lang, row.mark, row.kind === "add" ? "wa" : "wd");
  return (
    <tr className={row.kind === "context" ? "" : row.kind}>
      <td className="no">{row.old ?? ""}</td>
      <td className="no">{row.new ?? ""}</td>
      <td className="sg">{sign[row.kind]}</td>
      <td className="cd" dangerouslySetInnerHTML={{ __html: html }} />
    </tr>
  );
}

function SplitCtx({ row, lang }: { row: Row; lang: string | null }) {
  const html = highlight(row.text, lang);
  return (
    <tr>
      <td className="no">{row.old}</td>
      <td className="cd" dangerouslySetInnerHTML={{ __html: html }} />
      <td className="no sep">{row.new}</td>
      <td className="cd" dangerouslySetInnerHTML={{ __html: html }} />
    </tr>
  );
}

function SplitChg({ del, add, lang }: { del?: Row; add?: Row; lang: string | null }) {
  return (
    <tr>
      {del ? (
        <>
          <td className="no del">{del.old}</td>
          <td className="cd del" dangerouslySetInnerHTML={{ __html: highlightMarked(del.text, lang, del.mark, "wd") }} />
        </>
      ) : (
        <>
          <td className="no blank" />
          <td className="cd blank" />
        </>
      )}
      {add ? (
        <>
          <td className="no add sep">{add.new}</td>
          <td className="cd add" dangerouslySetInnerHTML={{ __html: highlightMarked(add.text, lang, add.mark, "wa") }} />
        </>
      ) : (
        <>
          <td className="no blank sep" />
          <td className="cd blank" />
        </>
      )}
    </tr>
  );
}
