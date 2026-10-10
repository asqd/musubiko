import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, Branch, copyReport, errorText, Radar as RadarData, RepoInfo, Settings } from "./api";
import Welcome from "./components/Welcome";
import Branches from "./components/Branches";
import TopBar from "./components/TopBar";
import DiffView from "./diff/DiffView";
import HotfixDialog from "./components/HotfixDialog";
import Journal from "./components/Journal";
import Radar, { needsAttention, pairState } from "./components/Radar";
import Locate from "./components/Locate";
import CheckoutDialog from "./components/CheckoutDialog";
import Palette, { Command } from "./palette/Palette";
import { BranchIcon, FlameIcon, LogIcon, SearchIcon } from "./components/Icons";
import { ErrorBanner } from "./components/ErrorBanner";
import { baseName, home, inFileManager, key } from "./platform";

export default function App() {
  const [info, setInfo] = useState<RepoInfo | null>(null);
  const [recent, setRecent] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  // Bumped to make every view reload: refresh button, ⌘R, window focus.
  const [tick, setTick] = useState(0);
  const lastRefresh = useRef(0);
  const [diffOf, setDiffOf] = useState<{ branch: Branch; base: string } | null>(null);
  const mainRef = useRef<HTMLElement>(null);
  const [view, setView] = useState<"branches" | "hotfixes" | "find" | "journal">("branches");
  const [findRequest, setFindRequest] = useState<{ query: string; n: number } | null>(null);
  const [radar, setRadar] = useState<RadarData | null>(null);
  const [radarError, setRadarError] = useState<string | null>(null);
  const [radarLoading, setRadarLoading] = useState(false);
  const [hotfixOpen, setHotfixOpen] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [checkoutOf, setCheckoutOf] = useState<Branch | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [settings, setSettings] = useState<Settings>({ backgroundFetch: false, fetchIntervalMin: 15, compactSidebar: false });
  // Outcome of the last background fetch; a failure is shown quietly next to the fetch button.
  const [bgFetch, setBgFetch] = useState<{ ok: boolean; at: number; error?: string } | null>(null);
  const bgBusy = useRef(false);
  const bgLastTry = useRef(0);
  const [branchList, setBranchList] = useState<Branch[]>([]);
  const listScroll = useRef(0);

  useEffect(() => {
    api.recentRepos().then(setRecent).catch(() => setRecent([]));
    api.getSettings().then(setSettings).catch(() => undefined);
  }, []);

  const compact = settings.compactSidebar;

  const saveSettings = async (next: Settings) => {
    setSettings(next);
    try {
      setSettings(await api.setSettings(next));
    } catch (e) {
      setError(errorText(e));
    }
  };

  const openPath = useCallback(async (path: string) => {
    setOpening(true);
    setError(null);
    lastRefresh.current = Date.now();
    try {
      setInfo(await api.openRepo(path));
      setDiffOf(null);
      setView("branches");
      setRecent(await api.recentRepos());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setOpening(false);
    }
  }, []);

  const pickFolder = useCallback(async () => {
    const path = await open({ directory: true, multiple: false, title: "Открыть репозиторий" });
    if (typeof path === "string") await openPath(path);
  }, [openPath]);

  const refresh = useCallback(async () => {
    if (!info) return;
    lastRefresh.current = Date.now();
    try {
      setInfo(await api.repoInfo());
      setTick((t) => t + 1);
    } catch (e) {
      setError(errorText(e));
    }
  }, [info]);

  // Branches change outside the app (terminal, IDE), so re-read when the window comes back.
  useEffect(() => {
    // Status of hundreds of branches takes seconds to compute: at most once per 15 s on focus.
    const onFocus = () => {
      if (Date.now() - lastRefresh.current > 15_000) void refresh();
    };
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        // Not over a dialog that is asking something.
        if (!document.querySelector(".backdrop:not(.pal-back)")) setPaletteOpen((v) => !v);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "b") {
        e.preventDefault();
        toggleCompactRef.current();
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r") {
        e.preventDefault();
        void refresh();
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "o") {
        e.preventDefault();
        void pickFolder();
      }
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("keydown", onKey);
    };
  }, [refresh, pickFolder]);

  // The radar is cheap enough to keep fresh for the sidebar badge.
  useEffect(() => {
    if (!info) return;
    let live = true;
    setRadarLoading(true);
    api
      .radar()
      .then((r) => {
        if (!live) return;
        setRadar(r);
        setRadarError(null);
      })
      .catch((e) => {
        if (!live) return;
        setRadar(null);
        setRadarError(errorText(e));
      })
      .finally(() => live && setRadarLoading(false));
    return () => {
      live = false;
    };
  }, [info?.root, tick]);

  const toggleCompact = () => void saveSettings({ ...settings, compactSidebar: !settings.compactSidebar });
  // The key handler is registered once; it calls whatever the latest toggle is.
  const toggleCompactRef = useRef(toggleCompact);
  toggleCompactRef.current = toggleCompact;

  // Background fetch, only when turned on. It counts any fetch (button, terminal) via FETCH_HEAD,
  // waits a full interval after a failed try, and stays out of the way of running operations.
  useEffect(() => {
    if (!info?.remote || !settings.backgroundFetch) return;
    const interval = settings.fetchIntervalMin * 60;
    const check = async () => {
      const now = Date.now() / 1000;
      const last = Math.max(info.lastFetch ?? 0, bgLastTry.current);
      if (bgBusy.current || fetching || now - last < interval) return;
      bgBusy.current = true;
      bgLastTry.current = now;
      try {
        if (await api.fetchQuietly()) {
          setBgFetch({ ok: true, at: now });
          await refresh();
        }
      } catch (e) {
        setBgFetch({ ok: false, at: now, error: errorText(e) });
      } finally {
        bgBusy.current = false;
      }
    };
    void check();
    const t = window.setInterval(check, 60_000);
    return () => window.clearInterval(t);
  }, [info?.root, info?.remote, info?.lastFetch, settings, fetching, refresh]);

  // git fetch --prune as a journaled operation; a failure is shown and the operation closed.
  const fetchNow = async () => {
    if (fetching) return;
    setFetching(true);
    setError(null);
    try {
      let op = await api.startFetch();
      while (op.status === "running") op = await api.opStep();
      bgLastTry.current = Date.now() / 1000;
      if (op.status === "failed") {
        const failed = op.states.find((s) => s.kind === "failed");
        setError(`Не получилось обновиться с сервера: ${failed && failed.kind === "failed" ? failed.error : ""}`);
        await api.opLeave();
      } else {
        await api.opClose();
        setBgFetch(null);
      }
      await refresh();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setFetching(false);
    }
  };

  const findCommit = (query: string) => {
    setFindRequest({ query, n: Date.now() });
    setView("find");
    setDiffOf(null);
  };

  const openDiff = (branch: Branch, base: string) => {
    listScroll.current = mainRef.current?.scrollTop ?? 0;
    setDiffOf({ branch, base });
    mainRef.current?.scrollTo(0, 0);
  };
  const closeDiff = useCallback(() => {
    setDiffOf(null);
    requestAnimationFrame(() => mainRef.current?.scrollTo(0, listScroll.current));
  }, []);

  const go = (v: typeof view) => {
    setView(v);
    setDiffOf(null);
  };
  const commands: Command[] = !info
    ? []
    : [
        { id: "act:hotfix", group: "Действия", label: "Новый hotfix", hint: `${info.flow.master} + ${info.flow.develop}`, run: () => setHotfixOpen(true) },
        ...(info.remote
          ? [{ id: "act:fetch", group: "Действия", label: "Получить с сервера", hint: "git fetch --prune", run: () => void fetchNow() }]
          : []),
        { id: "act:refresh", group: "Действия", label: "Обновить", hint: <kbd>{key("R")}</kbd>, run: () => void refresh() },
        ...(info.remote
          ? [
              settings.backgroundFetch
                ? {
                    id: "act:bg-off",
                    group: "Действия",
                    label: "Выключить автоматическое получение",
                    hint: `сейчас каждые ${settings.fetchIntervalMin} мин`,
                    run: () => void saveSettings({ ...settings, backgroundFetch: false }),
                  }
                : {
                    id: "act:bg-on",
                    group: "Действия",
                    label: "Включить автоматическое получение",
                    hint: `каждые ${settings.fetchIntervalMin} мин`,
                    run: () => void saveSettings({ ...settings, backgroundFetch: true }),
                  },
            ]
          : []),
        {
          id: "act:compact",
          group: "Действия",
          label: compact ? "Развернуть левую панель" : "Свернуть левую панель до иконок",
          hint: <kbd>{key("B")}</kbd>,
          run: toggleCompact,
        },
        {
          id: "act:recompute",
          group: "Действия",
          label: "Пересчитать статусы веток с нуля",
          hint: "сбросить кэш",
          run: () => void api.clearStatusCache().then(refresh),
        },
        { id: "act:open", group: "Действия", label: "Открыть репозиторий…", hint: <kbd>{key("O")}</kbd>, run: () => void pickFolder() },
        {
          id: "act:report",
          group: "Действия",
          label: "Скопировать отчёт о проблеме",
          hint: "версии и последние записи лога",
          run: () => void copyReport("(без ошибки: отчёт из палитры команд)").catch(() => undefined),
        },
        {
          id: "act:log",
          group: "Действия",
          label: `Показать лог ${inFileManager}`,
          run: () => void api.logPath().then((p) => (p ? revealItemInDir(p) : undefined)).catch(() => undefined),
        },
        { id: "view:branches", group: "Разделы", label: "Ветки", run: () => go("branches") },
        {
          id: "view:hotfixes",
          group: "Разделы",
          label: "Хотфиксы",
          hint: needsAttention(radar) ? `требуют внимания: ${needsAttention(radar)}` : undefined,
          run: () => go("hotfixes"),
        },
        { id: "view:find", group: "Разделы", label: "Где коммит?", run: () => go("find") },
        { id: "view:journal", group: "Разделы", label: "Журнал", run: () => go("journal") },
        ...(radar?.pairs ?? []).map((p) => ({
          id: "hotfix:" + p.slug,
          group: "Хотфиксы",
          label: p.slug,
          mono: true,
          hint: pairState(p.state, info.flow)[0],
          run: () => go("hotfixes"),
        })),
        ...recent
          .filter((p) => p !== info.root)
          .map((p) => ({ id: "repo:" + p, group: "Репозитории", label: baseName(p), hint: home(p), run: () => void openPath(p) })),
        ...branchList.flatMap((b) => [
          { id: "diff:" + b.name, group: "Ветки", kind: "Дифф", label: b.name, mono: true, run: () => openDiff(b, b.target) },
          ...(b.isHead
            ? []
            : [{ id: "co:" + b.name, group: "Ветки", kind: "Checkout", label: b.name, mono: true, run: () => setCheckoutOf(b) }]),
        ]),
      ];
  const dynamicCommands = (q: string): Command[] => [
    { id: "dyn:find", group: "Поиск", label: `Где коммит «${q.trim()}»?`, run: () => findCommit(q.trim()) },
  ];

  if (!info) {
    return (
      <Welcome
        recent={recent}
        error={error}
        opening={opening}
        onPick={pickFolder}
        onOpen={openPath}
        onForget={async (p) => setRecent(await api.forgetRepo(p))}
      />
    );
  }

  return (
    <div className={`app ${compact ? "compact" : ""}`}>
      <TopBar
        info={info}
        onRefresh={refresh}
        onHotfix={() => setHotfixOpen(true)}
        onFetch={fetchNow}
        fetching={fetching}
        onPalette={() => setPaletteOpen(true)}
        settings={settings}
        onSettings={saveSettings}
        bgFetch={bgFetch}
        onSwitch={() => {
          setInfo(null);
          setDiffOf(null);
          setBranchList([]);
        }}
      />
      <nav className="side" aria-label="Разделы">
        {(
          [
            ["branches", <BranchIcon />, "Ветки"],
            ["hotfixes", <FlameIcon />, "Хотфиксы"],
            ["find", <SearchIcon />, "Где коммит?"],
            ["journal", <LogIcon />, "Журнал"],
          ] as const
        ).map(([v, icon, label]) => {
          const badge = v === "hotfixes" ? needsAttention(radar) : 0;
          return (
            <button
              key={v}
              className={`nav ${view === v ? "on" : ""}`}
              data-tip={compact ? (badge ? `${label} · требуют внимания: ${badge}` : label) : undefined}
              aria-label={label}
              onClick={() => {
                setView(v);
                if (v === "branches" && diffOf) closeDiff();
                else setDiffOf(null);
              }}
            >
              <span className="nav-ic">
                {icon}
                {badge > 0 && compact && <span className="dot-badge">{badge}</span>}
              </span>
              <span className="nav-label">{label}</span>
              {badge > 0 && !compact && (
                <span className="badge" title="Требуют внимания">
                  {badge}
                </span>
              )}
            </button>
          );
        })}
        <div className="side-foot">
          {!compact && (
            <>
              <div className="faint" title={info.root}>
                {home(info.root)}
              </div>
              <div className="faint">git {info.git.split(" ")[0]}</div>
            </>
          )}
          <button
            className="nav collapse"
            onClick={toggleCompact}
            data-tip={compact ? `Развернуть панель (${key("B")})` : undefined}
            aria-label={compact ? "Развернуть панель" : "Свернуть панель"}
            title={compact ? undefined : `Оставить одни иконки (${key("B")})`}
          >
            <span className="nav-ic">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6">
                <rect x="2" y="3" width="12" height="10" rx="1.5" />
                <path d="M6 3v10" />
                <path d={compact ? "m9 6.5 1.5 1.5L9 9.5" : "m10.5 6.5L9 8l1.5 1.5"} />
              </svg>
            </span>
            <span className="nav-label">Свернуть панель</span>
          </button>
        </div>
      </nav>
      <main className="main" ref={mainRef}>
        {error && <ErrorBanner error={error} onClose={() => setError(null)} />}
        {/* The list stays mounted under the diff so going back keeps filters and scroll. */}
        <div hidden={!!diffOf || view !== "branches"}>
          <Branches
            info={info}
            tick={tick}
            active={!diffOf && view === "branches" && !hotfixOpen && !checkoutOf && !paletteOpen}
            onOpen={openDiff}
            onChanged={refresh}
            onCheckout={setCheckoutOf}
            onList={setBranchList}
          />
        </div>
        <div hidden={!!diffOf || view !== "hotfixes"}>
          <Radar
            info={info}
            radar={radar}
            error={radarError}
            loading={radarLoading}
            onHotfix={() => setHotfixOpen(true)}
            onDiff={(b) => openDiff(b, b.target)}
            onFind={findCommit}
            onChanged={refresh}
          />
        </div>
        <div hidden={!!diffOf || view !== "find"}>
          <Locate info={info} request={findRequest} radar={radar} active={view === "find" && !diffOf} onChanged={refresh} />
        </div>
        {diffOf && (
          <DiffView key={diffOf.branch.name} info={info} branch={diffOf.branch} base={diffOf.base} tick={tick} onBack={closeDiff} onCheckout={setCheckoutOf} />
        )}
        {view === "journal" && !diffOf && <Journal tick={tick} onChanged={refresh} />}
      </main>
      {paletteOpen && <Palette commands={commands} dynamic={dynamicCommands} onClose={() => setPaletteOpen(false)} />}
      {checkoutOf && (
        <CheckoutDialog info={info} branch={checkoutOf} onClose={() => setCheckoutOf(null)} onChanged={refresh} />
      )}
      {hotfixOpen && <HotfixDialog info={info} onClose={() => setHotfixOpen(false)} onChanged={refresh} />}
    </div>
  );
}
