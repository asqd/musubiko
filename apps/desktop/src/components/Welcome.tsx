import { CloseIcon, FolderIcon, Logo } from "./Icons";

interface Props {
  recent: string[];
  error: string | null;
  opening: boolean;
  onPick: () => void;
  onOpen: (path: string) => void;
  onForget: (path: string) => void;
}

const home = (p: string) => p.replace(/^\/Users\/[^/]+/, "~");

export default function Welcome({ recent, error, opening, onPick, onOpen, onForget }: Props) {
  return (
    <div className="welcome">
      <div className="drag" data-tauri-drag-region />
      <div className="welcome-card">
        <div className="welcome-head">
          <Logo size={44} />
          <div>
            <h1>Musubiko</h1>
            <p className="muted">Ветки, их статусы и hotfix flow в одном окне</p>
          </div>
        </div>
        <button className="btn primary big" onClick={onPick} disabled={opening}>
          <FolderIcon />
          {opening ? "Открываю…" : "Открыть репозиторий…"}
          <kbd>⌘O</kbd>
        </button>
        {error && <div className="banner bad">{error}</div>}
        {recent.length > 0 && (
          <div className="recent">
            <h6>Недавние</h6>
            <ul>
              {recent.map((p) => (
                <li key={p}>
                  <button className="recent-item" onClick={() => onOpen(p)} disabled={opening}>
                    <b>{p.split("/").pop()}</b>
                    <span className="faint mono">{home(p)}</span>
                  </button>
                  <button className="ib" title="Убрать из списка" aria-label="Убрать из списка" onClick={() => onForget(p)}>
                    <CloseIcon />
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
