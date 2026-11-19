// Light / dark switch. The choice is remembered for next time.
export type Theme = 'dark' | 'light';

const KEY = 'sbs-theme';

export function currentTheme(): Theme {
  return document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
}

export function setTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // Saving can fail in rare cases; the switch still works for this session.
  }
}
