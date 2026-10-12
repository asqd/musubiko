// Dev only, loaded by bridge.html: lets the UI run in a plain browser against the bridge
// (apps/desktop/bridge), which serves the app's commands over HTTP with the real core.

const BRIDGE = "http://127.0.0.1:1421";

type Internals = {
  invoke: (cmd: string, args?: unknown) => Promise<unknown>;
  transformCallback: (cb: (v: unknown) => void, once?: boolean) => number;
  convertFileSrc: (p: string) => string;
  metadata: unknown;
};

let callbacks = 0;

const internals: Internals = {
  async invoke(cmd, args = {}) {
    const r = await fetch(`${BRIDGE}/invoke`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ cmd, args }),
    });
    const body: unknown = await r.json();
    // Tauri rejects with the command's error value as is: here, a string.
    if (!r.ok) throw body;
    return body;
  },
  transformCallback(cb) {
    const id = ++callbacks;
    (window as unknown as Record<string, unknown>)[`_${id}`] = cb;
    return id;
  },
  convertFileSrc: (p) => p,
  metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
};

(window as unknown as { __TAURI_INTERNALS__: Internals }).__TAURI_INTERNALS__ = internals;
