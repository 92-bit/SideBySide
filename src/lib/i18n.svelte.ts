// The app's languages. The English text is the key: t("Find") gives "Buscar" in Spanish. Text with
// changing parts uses {placeholders}: t("{n} differences", { n: 3 }). Missing translations show
// the English text, so nothing is ever blank.
import es from '#lib/i18n/es.ts';
import fr from '#lib/i18n/fr.ts';
import de from '#lib/i18n/de.ts';
import zh from '#lib/i18n/zh.ts';

export const LANGS = [
  { id: 'en', name: 'English' },
  { id: 'es', name: 'Español' },
  { id: 'fr', name: 'Français' },
  { id: 'de', name: 'Deutsch' },
  { id: 'zh', name: '中文（简体）' },
] as const;
export type Lang = (typeof LANGS)[number]['id'];

const DICTS: Record<Lang, Record<string, string>> = { en: {}, es, fr, de, zh };
const KEY = 'sbs-lang';

/** The language: as chosen, or else Windows' own (if the app has it), or else English. */
function startLang(): Lang {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved && saved in DICTS) return saved as Lang;
  } catch {
    // fall through
  }
  const sys = (typeof navigator !== 'undefined' ? navigator.language : 'en').slice(0, 2).toLowerCase();
  return (sys in DICTS ? sys : 'en') as Lang;
}

export const lang = $state({ id: startLang() });

export function setLang(id: Lang) {
  lang.id = id;
  try {
    localStorage.setItem(KEY, id);
  } catch {
    // not saved this time
  }
  if (typeof document !== 'undefined') document.documentElement.lang = id;
}

/** The text in the chosen language, with {placeholders} filled in. */
export function t(key: string, params?: Record<string, string | number>): string {
  const s = DICTS[lang.id][key] ?? key;
  return params ? s.replace(/\{(\w+)\}/g, (m, k) => (k in params ? String(params[k]) : m)) : s;
}

const escapeHtml = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c] ?? c);

/** For a sentence with a word in bold or code inside: the translation, made safe for HTML, with
 *  each {placeholder} replaced by the given HTML (for {@html …}). Plain values are made safe too. */
export function tHtml(key: string, html: Record<string, string>, plain: Record<string, string | number> = {}): string {
  const tpl = escapeHtml(DICTS[lang.id][key] ?? key);
  return tpl.replace(/\{(\w+)\}/g, (m, k) => (k in html ? html[k] : k in plain ? escapeHtml(String(plain[k])) : m));
}
/** Text made safe to put into tHtml's HTML parts. */
export const safe = escapeHtml;

// (screen readers and spell checks should know the language too)
if (typeof document !== 'undefined') document.documentElement.lang = lang.id;
