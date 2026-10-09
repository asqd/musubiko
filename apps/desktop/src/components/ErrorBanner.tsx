import { Component, ReactNode, useEffect, useState } from "react";
import { copyReport, logEvent } from "../api";

/** Copies a report for `error`: versions, the error itself and the recent log. */
export function CopyReport({ error, className = "btn sm ghost" }: { error: string; className?: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  useEffect(() => {
    if (state === "idle") return;
    const t = window.setTimeout(() => setState("idle"), 2500);
    return () => window.clearTimeout(t);
  }, [state]);
  const copy = () =>
    copyReport(error)
      .then(() => setState("copied"))
      .catch(() => setState("failed"));
  return (
    <button className={className} onClick={copy} title="Версии, текст ошибки и последние записи лога — чтобы переслать">
      {state === "copied" ? "Скопировано" : state === "failed" ? "Не удалось скопировать" : "Скопировать отчёт"}
    </button>
  );
}

/** An error box with a report button, and a close button when `onClose` is given. */
export function ErrorBanner({ error, onClose, prefix }: { error: string; onClose?: () => void; prefix?: string }) {
  return (
    <div className="banner bad" role="alert">
      <span className="banner-text">{prefix}{error}</span>
      <span className="banner-actions">
        <CopyReport error={prefix ? prefix + error : error} />
        {onClose && <button className="btn sm ghost" onClick={onClose}>Скрыть</button>}
      </span>
    </div>
  );
}

/** Script errors outside React go to the log; a render error replaces the window with this. */
export class Crash extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null };

  static getDerivedStateFromError(e: unknown) {
    return { error: e instanceof Error ? `${e.message}\n${e.stack ?? ""}` : String(e) };
  }

  componentDidCatch(e: unknown, info: { componentStack?: string | null }) {
    const text = e instanceof Error ? `${e.message}\n${e.stack ?? ""}` : String(e);
    logEvent("UI", `Окно упало: ${text}${info.componentStack ?? ""}`);
  }

  render() {
    if (this.state.error === null) return this.props.children;
    return (
      <div className="crash">
        <div className="drag" data-tauri-drag-region />
        <div className="crash-card">
          <h1>Что-то сломалось</h1>
          <p className="muted">Окно Musubiko упало из-за ошибки в программе. Операции git не прерываются: всё, что уже сделано, есть в журнале.</p>
          <pre className="crash-error">{this.state.error}</pre>
          <div className="crash-actions">
            <CopyReport error={this.state.error} className="btn primary" />
            <button className="btn" onClick={() => window.location.reload()}>Перезапустить окно</button>
          </div>
        </div>
      </div>
    );
  }
}

let installed = false;

/** Sends uncaught script errors and rejected promises to the log. */
export function logScriptErrors() {
  if (installed) return;
  installed = true;
  window.addEventListener("error", (e) => {
    logEvent("UI", `${e.message} (${e.filename}:${e.lineno}:${e.colno})${e.error?.stack ? "\n" + e.error.stack : ""}`);
  });
  window.addEventListener("unhandledrejection", (e) => {
    const r = e.reason;
    logEvent("UI", `Необработанный отказ: ${r instanceof Error ? `${r.message}\n${r.stack ?? ""}` : typeof r === "string" ? r : JSON.stringify(r)}`);
  });
}
