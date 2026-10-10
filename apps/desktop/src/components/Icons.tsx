// Small stroke icons at 14 px, drawn with currentColor.
const base = { width: 14, height: 14, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor", strokeWidth: 1.6 } as const;

export const BranchIcon = () => (
  <svg {...base}><circle cx="4" cy="3.5" r="1.8" /><circle cx="4" cy="12.5" r="1.8" /><circle cx="12" cy="5" r="1.8" /><path d="M4 5.3v5.4M12 6.8c0 3-3 3.2-6.4 4.6" /></svg>
);
export const PlusIcon = () => (
  <svg {...base} strokeWidth={1.8}><path d="M8 3v10M3 8h10" /></svg>
);
export const SearchIcon = () => (
  <svg {...base} strokeWidth={1.7}><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3.5 3.5" /></svg>
);
export const RefreshIcon = () => (
  <svg {...base}><path d="M13 8a5 5 0 1 1-1.5-3.6M13 2.5v2.8h-2.8" /></svg>
);
export const CopyIcon = () => (
  <svg {...base} strokeWidth={1.5}><rect x="5" y="5" width="8.5" height="8.5" rx="1.5" /><path d="M3 10.5V3.8C3 3.3 3.3 3 3.8 3h6.7" /></svg>
);
export const FolderIcon = () => (
  <svg {...base}><path d="M2 4.5c0-.6.4-1 1-1h3.2l1.5 1.6H13c.6 0 1 .4 1 1V12c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1z" /></svg>
);
export const CloseIcon = () => (
  <svg {...base} strokeWidth={1.8}><path d="m4 4 8 8M12 4l-8 8" /></svg>
);

export const DownloadIcon = () => (
  <svg {...base}><path d="M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10" /></svg>
);
export const SwitchIcon = () => (
  <svg {...base}><path d="M3 5h9l-2.5-2.5M13 11H4l2.5 2.5" /></svg>
);
export const TrashIcon = () => (
  <svg {...base} strokeWidth={1.5}><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5" /></svg>
);
export const FlameIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
    <path d="M8 15c-3 0-5-2.2-5-5.2 0-1.4.4-2.5 1-3.4C4 8.5 5.3 9.5 5.3 9.5S4.8 6.7 7 4.6C7.6 4 8 3.2 8 2.2c2.6 1.6 5 4.3 5 7.6C13 12.8 11 15 8 15z" />
  </svg>
);
export const LogIcon = () => (
  <svg {...base}><circle cx="8" cy="8" r="6" /><path d="M8 4.8V8l2.2 1.5" /></svg>
);

/** The knot: two branches tied together. */
export const Logo = ({ size = 22 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
    <rect width="24" height="24" rx="6" fill="var(--fg)" />
    <path d="M7 19V5" stroke="var(--bg)" strokeWidth="2" strokeLinecap="round" fill="none" />
    <path d="M7 9c0 4 10 2 10 7v3" stroke="var(--bg)" strokeWidth="2" strokeLinecap="round" fill="none" />
    <path d="M17 5v2.5c0 3-10 2-10 6" stroke="var(--hot)" strokeWidth="2" strokeLinecap="round" fill="none" />
  </svg>
);
