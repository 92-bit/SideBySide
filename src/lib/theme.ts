// Colour theme + light/dark mode. The choice is remembered for next time.
import { DEFAULT_THEME, THEMES, type ThemeInfo } from './themes';

/** What the user picked. "auto" follows the Windows light/dark setting. */
export type Mode = 'dark' | 'light' | 'auto';
/** What is on screen right now. */
export type Shade = 'dark' | 'light';
export type Look = { theme: string; mode: Mode };

const KEY = 'sbs-look';
const OLD_KEY = 'sbs-theme'; // used up to v0.1.1: just "dark" or "light"
/** Ready-made CSS for app.html, so the saved colours are on screen before anything is drawn. */
const CSS_KEY = 'sbs-look-css';

const systemLight = () => window.matchMedia('(prefers-color-scheme: light)').matches;

export function themeInfo(id: string): ThemeInfo {
  return THEMES.find((t) => t.id === id) ?? THEMES.find((t) => t.id === DEFAULT_THEME)!;
}

export function shadeOf(mode: Mode): Shade {
  return mode === 'auto' ? (systemLight() ? 'light' : 'dark') : mode;
}

export function loadLook(): Look {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (saved && typeof saved.theme === 'string' && ['dark', 'light', 'auto'].includes(saved.mode)) {
      return { theme: themeInfo(saved.theme).id, mode: saved.mode };
    }
    const old = localStorage.getItem(OLD_KEY);
    if (old === 'dark' || old === 'light') return { theme: DEFAULT_THEME, mode: old };
  } catch {
    // Unreadable settings: use the default look.
  }
  return { theme: DEFAULT_THEME, mode: 'auto' };
}

/** CSS text that sets every theme colour as a --t-* variable. */
function cssFor(id: string, shade: Shade): string {
  const p = themeInfo(id)[shade];
  return Object.entries(p)
    .map(([k, v]) => `--t-${k}:${v};`)
    .join('');
}

/** Put a look on screen (without saving it). */
export function showLook(look: Look): void {
  const root = document.documentElement;
  const shade = shadeOf(look.mode);
  root.style.cssText = cssFor(look.theme, shade) + `color-scheme:${shade};`;
  root.dataset.theme = shade;
  root.dataset.look = themeInfo(look.theme).id;
  window.dispatchEvent(new Event('sbs-look'));
}

/** Put a look on screen and remember it. */
export function setLook(look: Look): void {
  showLook(look);
  try {
    localStorage.setItem(KEY, JSON.stringify(look));
    localStorage.setItem(
      CSS_KEY,
      JSON.stringify({ mode: look.mode, dark: cssFor(look.theme, 'dark'), light: cssFor(look.theme, 'light') }),
    );
  } catch {
    // Saving can fail in rare cases; the look still works for this session.
  }
}

/** Change the look with a short reveal: the new colours grow as a circle from the click.
 *  Theme changes are rare, so a little motion is fine here (skipped when "reduce motion" is on). */
export function changeLook(look: Look, from?: { x: number; y: number }): void {
  const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const doc = document as Document & { startViewTransition?: (cb: () => void) => { ready: Promise<void> } };
  if (!doc.startViewTransition || reduce) {
    setLook(look);
    return;
  }
  const x = from?.x ?? innerWidth / 2;
  const y = from?.y ?? innerHeight / 2;
  const r = Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y));
  const vt = doc.startViewTransition(() => setLook(look));
  vt.ready
    .then(() => {
      document.documentElement.animate(
        { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${r}px at ${x}px ${y}px)`] },
        { duration: 480, easing: 'cubic-bezier(.77,0,.175,1)', pseudoElement: '::view-transition-new(root)' },
      );
    })
    .catch(() => {});
}

/** Follow Windows when the mode is "auto". Returns a function that stops listening. */
export function followSystem(getLook: () => Look): () => void {
  const mq = window.matchMedia('(prefers-color-scheme: light)');
  const onChange = () => {
    const look = getLook();
    if (look.mode === 'auto') showLook(look);
  };
  mq.addEventListener('change', onChange);
  return () => mq.removeEventListener('change', onChange);
}

/** Read one theme colour as it is on screen right now (for drawing on a canvas). */
export function themeColor(key: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(`--t-${key}`).trim();
}
