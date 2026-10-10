import { useEffect, useRef, useState } from "react";
import { RepoInfo, Settings } from "../api";
import { ago, fullDate, plural } from "../format";
import { BranchIcon, DownloadIcon, FlameIcon, Logo, RefreshIcon, SearchIcon } from "./Icons";
import { key } from "../platform";
import { hasDevelop } from "./Radar";

interface Props {
  info: RepoInfo;
  onRefresh: () => Promise<void>;
  onHotfix: () => void;
  onFetch: () => void;
  fetching: boolean;
  onPalette: () => void;
  onSwitch: () => void;
  settings: Settings;
  onSettings: (s: Settings) => void;
  /** The last background fetch, when one ran. */
  bgFetch: { ok: boolean; at: number; error?: string } | null;
}

const INTERVALS = [5, 15, 30, 60];

const agoText = (t: number) => {
  const a = ago(t);
  return a === "только что" ? a : `${a} назад`;
};

export default function TopBar({
  info,
  onRefresh,
  onHotfix,
  onFetch,
  fetching,
  onPalette,
  onSwitch,
  settings,
  onSettings,
  bgFetch,
}: Props) {
  const [menu, setMenu] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    const onDown = (e: MouseEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) setMenu(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setMenu(false);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  const [busy, setBusy] = useState(false);
  const n = info.changes.length;
  const changesTitle = info.changes.map((c) => `${c.code.padEnd(2)} ${c.path}`).join("\n");

  const refresh = async () => {
    setBusy(true);
    await onRefresh();
    setBusy(false);
  };

  return (
    // On a Mac the window has no title bar: this bar drags it and leaves room for the traffic lights.
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
        <div className="fetch-split" ref={menuRef}>
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
            {settings.backgroundFetch && !bgFetch?.error && (
              <span className="tag" title={`Автоматически каждые ${settings.fetchIntervalMin} мин`}>
                авто
              </span>
            )}
            {bgFetch && !bgFetch.ok && (
              <span className="tag warn" title={`Фоновое получение не удалось (${fullDate(bgFetch.at)}):\n${bgFetch.error ?? ""}`}>
                ошибка
              </span>
            )}
          </button>
          <button
            className="btn ghost caret"
            onClick={() => setMenu((v) => !v)}
            aria-label="Автоматическое получение"
            aria-expanded={menu}
            title="Автоматическое получение"
          >
            <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="m4 6 4 4 4-4" />
            </svg>
          </button>
          {menu && (
            <div className="menu" role="menu">
              <div className="menu-h">Получать автоматически</div>
              <label className="menu-item">
                <input
                  type="radio"
                  name="bg-fetch"
                  checked={!settings.backgroundFetch}
                  onChange={() => onSettings({ ...settings, backgroundFetch: false })}
                />
                Выключено
              </label>
              {INTERVALS.map((m) => (
                <label key={m} className="menu-item">
                  <input
                    type="radio"
                    name="bg-fetch"
                    checked={settings.backgroundFetch && settings.fetchIntervalMin === m}
                    onChange={() => onSettings({ ...settings, backgroundFetch: true, fetchIntervalMin: m })}
                  />
                  Каждые {m} мин
                </label>
              ))}
              <div className="menu-note">
                Для открытого репозитория, тихо: без журнала и уведомлений. Fetch из терминала или по кнопке тоже
                считается, лишний раз не запускаю.
              </div>
              {bgFetch && !bgFetch.ok && <div className="menu-note bad">Последняя попытка: {bgFetch.error}</div>}
            </div>
          )}
        </div>
      )}
      <button className="btn ghost" onClick={refresh} disabled={busy} title={`Перечитать репозиторий (${key("R")})`}>
        {busy ? <span className="spin" /> : <RefreshIcon />}
        Обновить
      </button>
      <button className="btn ghost" onClick={onPalette} title="Палитра команд">
        <SearchIcon />
        <kbd>{key("K")}</kbd>
      </button>
      <button className="btn hot" onClick={onHotfix} title={
          hasDevelop(info)
            ? `${info.flow.hotfixPrefix}… от ${info.flow.master} и ${info.flow.hotfixPrefix}…${info.flow.devSuffix} от ${info.flow.develop}`
            : `${info.flow.hotfixPrefix}… от ${info.flow.master}`
        }>
        <FlameIcon />
        Новый hotfix
      </button>
    </header>
  );
}
