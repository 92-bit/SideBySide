// Preferences that several screens read and that update everywhere at once when changed.
import { FONT_DEFAULT, FONT_MAX, FONT_MIN, loadFontSize, saveFontSize } from '#lib/settings.ts';

const CODE_COLORS_KEY = 'sbs-code-colors';
const CODE_FONT_KEY = 'sbs-code-font';
const SPACING_KEY = 'sbs-line-spacing';
const TIDY_KEY = 'sbs-tidy';

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // not saved this time
  }
}

/** The fonts for the files' text: the built-in one, and ones every Windows 11 PC has. Each falls
 *  back to the next one if it is missing (e.g. on Linux). */
export const CODE_FONTS = [
  { id: 'geist', name: 'Geist Mono', note: 'built in', css: "'Geist Mono Variable', 'Cascadia Mono', Consolas, ui-monospace, monospace" },
  { id: 'cascadia', name: 'Cascadia', note: 'Cascadia Mono, from Windows Terminal', css: "'Cascadia Mono', 'Cascadia Code', Consolas, ui-monospace, monospace" },
  { id: 'consolas', name: 'Consolas', note: 'classic Windows', css: "Consolas, 'Cascadia Mono', ui-monospace, monospace" },
  { id: 'courier', name: 'Courier New', note: 'typewriter', css: "'Courier New', Courier, ui-monospace, monospace" },
] as const;
export type CodeFont = (typeof CODE_FONTS)[number]['id'];

/** Space between lines: the row height is the text size times this. */
export const SPACINGS = [
  { id: 'compact', name: 'Compact', factor: 1.32 },
  { id: 'normal', name: 'Normal', factor: 1.54 },
  { id: 'roomy', name: 'Roomy', factor: 1.8 },
] as const;
export type Spacing = (typeof SPACINGS)[number]['id'];

const savedFont = read(CODE_FONT_KEY);
const savedSpacing = read(SPACING_KEY);

export const prefs = $state({
  /** Colour the code in code files (on unless switched off). */
  codeColors: read(CODE_COLORS_KEY) !== 'off',
  /** Text size of the files, in pixels (the same as zoom: Ctrl +/-). */
  fontSize: loadFontSize(),
  codeFont: (CODE_FONTS.some((f) => f.id === savedFont) ? savedFont : 'geist') as CodeFont,
  spacing: (SPACINGS.some((s) => s.id === savedSpacing) ? savedSpacing : 'normal') as Spacing,
  /** Tidy up JSON and XML files before comparing (one item per line). */
  tidy: read(TIDY_KEY) === 'on',
});

export function setTidy(on: boolean) {
  prefs.tidy = on;
  write(TIDY_KEY, on ? 'on' : 'off');
}

/** JSON or XML, by the file name (the same list as tidy.rs). */
export function tidyKind(name: string): 'JSON' | 'XML' | null {
  const ext = name.toLowerCase().split('.').pop() ?? '';
  if (['json', 'jsonc', 'json5', 'geojson', 'ipynb', 'har', 'webmanifest'].includes(ext)) return 'JSON';
  const xml = ['xml', 'xsd', 'xsl', 'xslt', 'svg', 'xaml', 'csproj', 'vbproj', 'fsproj', 'props', 'targets', 'resx', 'config', 'plist', 'nuspec', 'wsdl', 'kml', 'gpx', 'rss', 'atom'];
  return xml.includes(ext) ? 'XML' : null;
}

export function setCodeColors(on: boolean) {
  prefs.codeColors = on;
  write(CODE_COLORS_KEY, on ? 'on' : 'off');
}

/** Sets the text size (kept between the smallest and biggest allowed). */
export function setFontSize(v: number) {
  prefs.fontSize = Math.max(FONT_MIN, Math.min(FONT_MAX, Math.round(v)));
  saveFontSize(prefs.fontSize);
}
/** step 0 = back to normal size. */
export function zoomStep(step: number) {
  setFontSize(step === 0 ? FONT_DEFAULT : prefs.fontSize + step);
}

export function setCodeFont(id: CodeFont) {
  prefs.codeFont = id;
  write(CODE_FONT_KEY, id);
  applyCodeFont();
}
/** Puts the chosen font in --font-code. (On <body>: the theme rewrites the styles on <html>.) */
export function applyCodeFont() {
  if (typeof document === 'undefined') return;
  const f = CODE_FONTS.find((x) => x.id === prefs.codeFont) ?? CODE_FONTS[0];
  document.body.style.setProperty('--font-code', f.css);
}

export function setSpacing(id: Spacing) {
  prefs.spacing = id;
  write(SPACING_KEY, id);
}
export const spacingFactor = () => SPACINGS.find((s) => s.id === prefs.spacing)?.factor ?? 1.54;
