// Ignore rules: parts of lines that don't count when comparing, like the time at the start of
// each line of a log. Ready-made rules, plus the user's own (as regular expressions).

export interface Preset {
  id: string;
  name: string;
  example: string;
  re: string;
}

export const PRESETS: Preset[] = [
  {
    id: 'time',
    name: 'Dates and times',
    example: '2026-11-20 10:04:31.250',
    re: String.raw`\d{4}[-/.]\d{1,2}[-/.]\d{1,2}(?:[T ]\d{1,2}:\d{2}(?::\d{2})?(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?)?|\d{1,2}[-/.]\d{1,2}[-/.]\d{4}|\d{1,2}:\d{2}:\d{2}(?:[.,]\d+)?`,
  },
  {
    id: 'guid',
    name: 'IDs (GUIDs)',
    example: '3f2a8c1e-9b4d-4e7a-a1c2-5d6e7f809a1b',
    re: String.raw`\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b`,
  },
  {
    id: 'hex',
    name: 'Memory addresses',
    example: '0x7ffd4a2c',
    re: String.raw`\b0[xX][0-9a-fA-F]+\b`,
  },
  {
    id: 'numbers',
    name: 'All numbers',
    example: '42, 3.14',
    re: String.raw`\d+(?:[.,]\d+)?`,
  },
];

export interface RulesConfig {
  /** The ready-made rules that are ticked. */
  presets: string[];
  /** The user's own rules, one per line. */
  custom: string;
}

const KEY = 'sbs-ignore-rules';
export function loadRules(): RulesConfig {
  try {
    const c = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (c && Array.isArray(c.presets) && typeof c.custom === 'string') return c;
  } catch {
    // fall through
  }
  return { presets: ['time'], custom: '' };
}
export function saveRules(c: RulesConfig) {
  try {
    localStorage.setItem(KEY, JSON.stringify(c));
  } catch {
    // not saved this time
  }
}

/** The regular expressions to send to the compare engine. */
export function activeRules(c: RulesConfig): string[] {
  const own = c.custom
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);
  return [...PRESETS.filter((p) => c.presets.includes(p.id)).map((p) => p.re), ...own];
}
