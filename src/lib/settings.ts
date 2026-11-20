// Small preferences remembered between launches.
const FONT_KEY = 'sbs-font-size';
export const FONT_DEFAULT = 13;
export const FONT_MIN = 9;
export const FONT_MAX = 28;

export function loadFontSize(): number {
  try {
    const v = Number(localStorage.getItem(FONT_KEY));
    if (v >= FONT_MIN && v <= FONT_MAX) return v;
  } catch {
    // fall through to the default
  }
  return FONT_DEFAULT;
}

const AUTO_KEY = 'sbs-auto-update';
const LAST_CHECK_KEY = 'sbs-last-update-check';

/** Check for updates automatically, once a day. On unless switched off. */
export function loadAutoUpdate(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== 'off';
  } catch {
    return true;
  }
}
export function saveAutoUpdate(on: boolean) {
  try {
    localStorage.setItem(AUTO_KEY, on ? 'on' : 'off');
  } catch {
    // not saved this time
  }
}
/** True if the last automatic check was more than a day ago. */
export function updateCheckDue(): boolean {
  try {
    return Date.now() - Number(localStorage.getItem(LAST_CHECK_KEY) ?? 0) > 24 * 3600 * 1000;
  } catch {
    return true;
  }
}
export function markUpdateChecked() {
  try {
    localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
  } catch {
    // not saved this time
  }
}

export function saveFontSize(v: number) {
  try {
    localStorage.setItem(FONT_KEY, String(v));
  } catch {
    // not saved this time
  }
}

const LIVE_KEY = 'sbs-live';
/** Live reload: off unless switched on. */
export function loadLive(): boolean {
  try {
    return localStorage.getItem(LIVE_KEY) === 'on';
  } catch {
    return false;
  }
}
export function saveLive(on: boolean) {
  try {
    localStorage.setItem(LIVE_KEY, on ? 'on' : 'off');
  } catch {
    // not saved this time
  }
}

const REOPEN_KEY = 'sbs-reopen';
const SESSION_KEY = 'sbs-last-session';
/** Reopen the last comparison when SideBySide starts: off unless switched on. */
export function loadReopen(): boolean {
  try {
    return localStorage.getItem(REOPEN_KEY) === 'on';
  } catch {
    return false;
  }
}
export function saveReopen(on: boolean) {
  try {
    localStorage.setItem(REOPEN_KEY, on ? 'on' : 'off');
  } catch {
    // not saved this time
  }
}

/** The comparison on screen, remembered so the next start can reopen it. */
export interface Session {
  left: string;
  right: string;
  ignore: { spaces: boolean; blank: boolean; caseLetters: boolean; rules?: boolean };
  place: { top: number; x: number; atEnd: boolean } | null;
  bookmarks: number[];
  /** File sizes when it was saved: bookmarks only come back if the files are unchanged. */
  sizes: [number, number];
  /** "Lines…": only part of a file was compared. */
  ranges?: { left: [number, number] | null; right: [number, number] | null };
}
let lastSaved = '';
export function saveSession(s: Session | null) {
  const text = s ? JSON.stringify(s) : '';
  if (text === lastSaved) return; // nothing new (this is called every couple of seconds)
  lastSaved = text;
  try {
    if (s) localStorage.setItem(SESSION_KEY, text);
    else localStorage.removeItem(SESSION_KEY);
  } catch {
    // not saved this time
  }
}
export function loadSession(): Session | null {
  try {
    const s = JSON.parse(localStorage.getItem(SESSION_KEY) ?? 'null');
    return s && typeof s.left === 'string' && typeof s.right === 'string' ? (s as Session) : null;
  } catch {
    return null;
  }
}
