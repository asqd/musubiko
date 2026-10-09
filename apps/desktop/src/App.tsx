import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, Branch, errorText, RepoInfo } from "./api";
import Welcome from "./components/Welcome";
import Branches from "./components/Branches";
import TopBar from "./components/TopBar";
import DiffView from "./diff/DiffView";
import { BranchIcon } from "./components/Icons";

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
        onSwitch={() => {
          setInfo(null);
          setDiffOf(null);
        }}
      />
      <nav className="side" aria-label="Разделы">
        <button className="nav on" onClick={() => diffOf && closeDiff()}>
          <BranchIcon />
          Ветки
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
        <div hidden={!!diffOf}>
          <Branches info={info} tick={tick} active={!diffOf} onOpen={openDiff} />
        </div>
        {diffOf && <DiffView key={diffOf.branch.name} info={info} branch={diffOf.branch} base={diffOf.base} tick={tick} onBack={closeDiff} />}
      </main>
    </div>
  );
}
