// What differs between macOS, Windows and Linux in the UI: shortcut names, home folders, paths.

export const isMac = /Mac/.test(navigator.userAgent);
const isWindows = /Windows/.test(navigator.userAgent);

/** A shortcut as the system names it: "⌘K" on a Mac, "Ctrl+K" elsewhere. */
export function key(k: string): string {
  return isMac ? `⌘${k}` : `Ctrl+${k}`;
}

/** "Показать в …": Finder, Проводник or whatever the desktop has. */
export const inFileManager = isMac ? "в Finder" : isWindows ? "в Проводнике" : "в файловом менеджере";

/** The home folder as "~": /Users/me, /home/me, C:\Users\me or C:/Users/me. */
export function home(p: string): string {
  return p.replace(/^(\/Users\/[^/]+|\/home\/[^/]+|[A-Za-z]:[\\/]Users[\\/][^\\/]+)/, "~");
}

/** The last part of a path, with either separator. */
export function baseName(p: string): string {
  return p.split(/[\\/]/).filter(Boolean).pop() ?? p;
}
