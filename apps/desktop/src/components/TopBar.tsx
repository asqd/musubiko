import { useState } from "react";
import { RepoInfo } from "../api";
import { ago, fullDate, plural } from "../format";
import { BranchIcon, DownloadIcon, FlameIcon, Logo, RefreshIcon } from "./Icons";

interface Props {
  info: RepoInfo;
  onRefresh: () => Promise<void>;
  onHotfix: () => void;
  onFetch: () => void;
  fetching: boolean;
  onSwitch: () => void;
}

const agoText = (t: number) => {
  const a = ago(t);
  return a === "только что" ? a : `${a} назад`;
};

export default function TopBar({ info, onRefresh, onHotfix, onFetch, fetching, onSwitch }: Props) {
  const [busy, setBusy] = useState(false);
  const n = info.changes.length;
  const changesTitle = info.changes.map((c) => `${c.code.padEnd(2)} ${c.path}`).join("\n");

  const refresh = async () => {
    setBusy(true);
    await onRefresh();
    setBusy(false);
  };

  return (
    // The window has no title bar: this bar drags it and leaves room for the traffic lights.
    <header className="top" data-tauri-drag-region>
      <div className="logo" data-tauri-drag-region>
        <Logo />
      </div>
      <button className="chip" onClick={onSwitch} title="Открыть другой репозиторий">
        <b>{info.name}</b>
      </button>
      <span className="chip head" title={info.head ? "Текущая ветка" : "HEAD отсоединён"}>
        <BranchIcon />
        <span className="mono ell">{info.head ?? `(detached) ${info.headShort}`}</span>
        {n > 0 && (
          <span className="dirty" title={changesTitle}>
            ● {n} {plural(n, "изменение", "изменения", "изменений")}
          </span>
        )}
      </span>
      <span className="sp" data-tauri-drag-region />
      {info.remote && (
        <button
          className="btn ghost"
          onClick={onFetch}
          disabled={fetching}
          title={`git fetch --prune ${info.remote}: свежие ветки и статусы, удалённые на сервере ветки уходят из списка${
            info.lastFetch ? `\nПоследний раз: ${fullDate(info.lastFetch)}` : ""
          }`}
        >
          {fetching ? <span className="spin" /> : <DownloadIcon />}
          {fetching ? "Получаю…" : "Получить"}
          {!fetching && info.lastFetch && <span className="faint fetch-chip">{agoText(info.lastFetch)}</span>}
        </button>
      )}
      <button className="btn ghost" onClick={refresh} disabled={busy} title="Перечитать репозиторий (⌘R)">
        {busy ? <span className="spin" /> : <RefreshIcon />}
        Обновить
      </button>
      <button className="btn hot" onClick={onHotfix} title="hotfix/… от master и hotfix/…-dev от develop">
        <FlameIcon />
        Новый hotfix
      </button>
    </header>
  );
}
