import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, Branch, errorText, Radar as RadarData, RepoInfo } from "./api";
import Welcome from "./components/Welcome";
import Branches from "./components/Branches";
import TopBar from "./components/TopBar";
import DiffView from "./diff/DiffView";
import HotfixDialog from "./components/HotfixDialog";
import Journal from "./components/Journal";
import Radar, { needsAttention } from "./components/Radar";
import Locate from "./components/Locate";
import CheckoutDialog from "./components/CheckoutDialog";
import { BranchIcon, FlameIcon, LogIcon, SearchIcon } from "./components/Icons";

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
  const listScroll = useRef(0);

  useEffect(() => {
    api.recentRepos().then(setRecent).catch(() => setRecent([]));
  }, []);

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

  // git fetch --prune as a journaled operation; a failure is shown and the operation closed.
  const fetchNow = async () => {
    if (fetching) return;
    setFetching(true);
    setError(null);
    try {
      let op = await api.startFetch();
      while (op.status === "running") op = await api.opStep();
      if (op.status === "failed") {
        const failed = op.states.find((s) => s.kind === "failed");
        setError(`Не получилось обновиться с сервера: ${failed && failed.kind === "failed" ? failed.error : ""}`);
        await api.opLeave();
      } else {
        await api.opClose();
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
    <div className="app">
      <TopBar
        info={info}
        onRefresh={refresh}
        onHotfix={() => setHotfixOpen(true)}
        onFetch={fetchNow}
        fetching={fetching}
        onSwitch={() => {
          setInfo(null);
          setDiffOf(null);
        }}
      />
      <nav className="side" aria-label="Разделы">
        <button
          className={`nav ${view === "branches" ? "on" : ""}`}
          onClick={() => {
            setView("branches");
            if (diffOf) closeDiff();
          }}
        >
          <BranchIcon />
          Ветки
        </button>
        <button
          className={`nav ${view === "hotfixes" ? "on" : ""}`}
          onClick={() => {
            setView("hotfixes");
            setDiffOf(null);
          }}
        >
          <FlameIcon />
          Хотфиксы
          {needsAttention(radar) > 0 && (
            <span className="badge" title="Требуют внимания">
              {needsAttention(radar)}
            </span>
          )}
        </button>
        <button
          className={`nav ${view === "find" ? "on" : ""}`}
          onClick={() => {
            setView("find");
            setDiffOf(null);
          }}
        >
          <SearchIcon />
          Где коммит?
        </button>
        <button
          className={`nav ${view === "journal" ? "on" : ""}`}
          onClick={() => {
            setView("journal");
            setDiffOf(null);
          }}
        >
          <LogIcon />
          Журнал
        </button>
        <div className="side-foot">
          <div className="faint" title={info.root}>{info.root.replace(/^\/Users\/[^/]+/, "~")}</div>
          <div className="faint">git {info.git.split(" ")[0]}</div>
        </div>
      </nav>
      <main className="main" ref={mainRef}>
        {error && (
          <div className="banner bad" role="alert">
            {error}
            <button className="btn sm ghost" onClick={() => setError(null)}>Скрыть</button>
          </div>
        )}
        {/* The list stays mounted under the diff so going back keeps filters and scroll. */}
        <div hidden={!!diffOf || view !== "branches"}>
          <Branches
            info={info}
            tick={tick}
            active={!diffOf && view === "branches" && !hotfixOpen && !checkoutOf}
            onOpen={openDiff}
            onChanged={refresh}
            onCheckout={setCheckoutOf}
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
      {checkoutOf && (
        <CheckoutDialog info={info} branch={checkoutOf} onClose={() => setCheckoutOf(null)} onChanged={refresh} />
      )}
      {hotfixOpen && <HotfixDialog info={info} onClose={() => setHotfixOpen(false)} onChanged={refresh} />}
    </div>
  );
}
