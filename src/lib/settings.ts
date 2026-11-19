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
