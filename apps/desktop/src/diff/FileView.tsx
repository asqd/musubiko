import { memo, useEffect, useMemo, useState } from "react";
import { api, errorText, FileDiff } from "../api";
import { blocks, BIG_FILE_LINES, Gap, gapAfter, gapBefore, isGenerated, isMigration, lineCount, Row } from "./model";
import { highlight, highlightHits, Hit, languageOf } from "./highlight";
import type { LineMatches } from "./search";
import { plural } from "../format";

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
  /** Search matches in this file, by row id. */
  matches?: LineMatches;
  /** Global index of the match the search is on, or -1 when it is in another file. */
  current?: number;
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

function FileView({ file, index, rev, mode, viewed, collapsed, active, onViewed, onCollapse, matches, current = -1 }: Props) {
  const lang = useMemo(() => languageOf(file.path), [file.path]);
  const big = lineCount(file) > BIG_FILE_LINES || isGenerated(file.path);
  const [forceOpen, setForceOpen] = useState(false);
  const [source, setSource] = useState<string[] | null>(null);
  const [opened, setOpened] = useState<Set<string>>(new Set());
  const [loadError, setLoadError] = useState<string | null>(null);

  const hidden = collapsed;
  const showBody = !hidden && (!big || forceOpen);
  // The search moved to a match in this big file: show it, and keep it shown.
  useEffect(() => {
    if (current >= 0 && big) setForceOpen(true);
  }, [current, big]);

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
                  : file.change === "added"
                    ? "Новый пустой файл."
                    : file.change === "deleted"
                      ? "Удалён пустой файл."
                      : "Изменения только в пробелах."}
            </div>
          ) : !showBody ? (
            <div className="note">
              {isGenerated(file.path) ? "Сгенерированный файл" : "Большой дифф"}: {lineCount(file)}{" "}
              {plural(lineCount(file), "строка", "строки", "строк")}.{" "}
              <button className="btn sm" onClick={() => setForceOpen(true)}>
                Показать
              </button>
            </div>
          ) : (
            <Table file={file} lang={lang} mode={mode} source={source} opened={opened} onExpand={expand} matches={matches} current={current} />
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
  matches?: LineMatches;
  current: number;
}

function Table({ file, lang, mode, source, opened, onExpand, matches, current }: TableProps) {
  const split = mode === "split";
  const hitsOf = (row?: Row): Hit[] =>
    (row?.id && matches?.get(row.id)?.map((m) => ({ start: m.start, end: m.end, current: m.index === current }))) || [];
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
            {count === null ? "⋯ Показать до конца файла" : `⋯ Показать ${count} ${plural(count, "строку", "строки", "строк")} (${gap.from}–${gap.to})`}
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
    blocks(h, hi).forEach((b, bi) => {
      const k = `${hi}-${bi}`;
      if (b.type === "ctx") {
        const hits = hitsOf(b.row);
        rows.push(split ? <SplitCtx key={k} row={b.row} lang={lang} hits={hits} /> : <UniRow key={k} row={b.row} lang={lang} hits={hits} />);
        return;
      }
      if (!split) {
        b.dels.forEach((r, j) => rows.push(<UniRow key={`${k}d${j}`} row={r} lang={lang} hits={hitsOf(r)} />));
        b.adds.forEach((r, j) => rows.push(<UniRow key={`${k}a${j}`} row={r} lang={lang} hits={hitsOf(r)} />));
        return;
      }
      const n = Math.max(b.dels.length, b.adds.length);
      for (let j = 0; j < n; j++)
        rows.push(<SplitChg key={`${k}s${j}`} del={b.dels[j]} add={b.adds[j]} lang={lang} delHits={hitsOf(b.dels[j])} addHits={hitsOf(b.adds[j])} />);
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

/** After a line the file ends on without a newline: otherwise "-x" and "+x" look the same. */
const NO_EOL = '<span class="noeol" title="Нет перевода строки в конце файла">нет ⏎</span>';
const withEol = (html: string, row: Row) => (row.noEol ? html + NO_EOL : html);

const NO_HITS: Hit[] = [];

function UniRow({ row, lang, hits = NO_HITS }: { row: Row; lang: string | null; hits?: Hit[] }) {
  const html = withEol(highlightHits(row.text, lang, row.mark, row.kind === "add" ? "wa" : "wd", hits), row);
  return (
    <tr className={row.kind === "context" ? "" : row.kind}>
      <td className="no">{row.old ?? ""}</td>
      <td className="no">{row.new ?? ""}</td>
      <td className="sg">{sign[row.kind]}</td>
      <td className="cd" dangerouslySetInnerHTML={{ __html: html }} />
    </tr>
  );
}

function SplitCtx({ row, lang, hits = NO_HITS }: { row: Row; lang: string | null; hits?: Hit[] }) {
  const html = withEol(hits.length ? highlightHits(row.text, lang, undefined, "", hits) : highlight(row.text, lang), row);
  return (
    <tr>
      <td className="no">{row.old}</td>
      <td className="cd" dangerouslySetInnerHTML={{ __html: html }} />
      <td className="no sep">{row.new}</td>
      <td className="cd" dangerouslySetInnerHTML={{ __html: html }} />
    </tr>
  );
}

function SplitChg({
  del,
  add,
  lang,
  delHits = NO_HITS,
  addHits = NO_HITS,
}: {
  del?: Row;
  add?: Row;
  lang: string | null;
  delHits?: Hit[];
  addHits?: Hit[];
}) {
  return (
    <tr>
      {del ? (
        <>
          <td className="no del">{del.old}</td>
          <td className="cd del" dangerouslySetInnerHTML={{ __html: withEol(highlightHits(del.text, lang, del.mark, "wd", delHits), del) }} />
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
          <td className="cd add" dangerouslySetInnerHTML={{ __html: withEol(highlightHits(add.text, lang, add.mark, "wa", addHits), add) }} />
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
