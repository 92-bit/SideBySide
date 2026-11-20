// Preferences that several screens read and that update everywhere at once when changed.
const CODE_COLORS_KEY = 'sbs-code-colors';

function load(key: string, fallback: boolean): boolean {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === 'on';
  } catch {
    return fallback;
  }
}
function save(key: string, on: boolean) {
  try {
    localStorage.setItem(key, on ? 'on' : 'off');
  } catch {
    // not saved this time
  }
}

export const prefs = $state({
  /** Colour the code in code files (on unless switched off). */
  codeColors: load(CODE_COLORS_KEY, true),
});

export function setCodeColors(on: boolean) {
  prefs.codeColors = on;
  save(CODE_COLORS_KEY, on);
}
