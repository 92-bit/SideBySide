// Code colours. A small, fast colouring tool written for SideBySide: it looks at one line at a time
// (only the lines on screen are ever coloured), so big files stay fast and nothing extra is downloaded.
// It is a quick guess, not a full parser: a comment or string that spans several lines may only be
// coloured on its first line. The colours come from the theme (--t-k, --t-s, --t-num, --t-sec, --t-cmc).

/** k = keyword, s = text in quotes, num = number / true / false / null, sec = names that stand out
 *  (tags, keys, headings, decorators, variables), cmc = comment, '' = plain text. */
export type Tok = '' | 'k' | 's' | 'num' | 'sec' | 'cmc';
export interface Piece {
  t: string;
  c: Tok;
}

type Kind = 'code' | 'markup' | 'css' | 'json' | 'yaml' | 'ini' | 'md' | 'bat';
interface Lang {
  kind: Kind;
  kw?: Set<string>;
  /** keywords are matched without caring about upper/lower case (SQL, PowerShell...) */
  anyCase?: boolean;
  line?: string[];
  block?: [string, string][];
  quotes?: string;
  /** the escape character inside quotes ('' = none) */
  esc?: string;
  /** $name is a variable (shells, PHP, PowerShell) */
  dollar?: boolean;
  /** @name / #name at the start of a word stands out (decorators, attributes, preprocessor) */
  at?: boolean;
  hashDirective?: boolean;
}

const words = (s: string) => new Set(s.split(' '));
const CONST = words('true false null undefined None True False nil NULL TRUE FALSE NaN Infinity');

const JS = words(
  'abstract any as async await boolean break case catch class const constructor continue debugger declare default delete do else enum export extends finally for from function get if implements import in infer instanceof interface keyof let module namespace never new number of package private protected public readonly return satisfies set static string super switch symbol this throw try type typeof unknown var void while with yield',
);
const PY = words(
  'and as assert async await break class continue def del elif else except finally for from global if import in is lambda match case nonlocal not or pass raise return self try while with yield print',
);
const C_LIKE = words(
  'auto bool break case catch char class const constexpr continue default delete do double else enum explicit extern final float for friend goto if inline int long mutable namespace new noexcept operator override private protected public register return short signed sizeof static struct switch template this throw try typedef typename union unsigned using virtual void volatile while',
);
const CS = words(
  'abstract as async await base bool break byte case catch char checked class const continue decimal default delegate do double else enum event explicit extern finally fixed float for foreach get goto if implicit in init int interface internal is lock long namespace new object operator out override params partial private protected public readonly record ref return sbyte sealed set short sizeof static string struct switch this throw try typeof uint ulong unchecked unsafe ushort using var virtual void volatile when where while yield',
);
const JAVA = words(
  'abstract assert boolean break byte case catch char class const continue data default do double else enum extends final finally float for fun if implements import in instanceof int interface is long native new object open override package private protected public return sealed short static super switch synchronized this throw throws transient try val var void volatile when while',
);
const GO = words(
  'break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var bool byte error float32 float64 int int8 int16 int32 int64 rune string uint uint8 uint16 uint32 uint64',
);
const RUST = words(
  'as async await break const continue crate dyn else enum extern fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait type unsafe use where while bool char f32 f64 i8 i16 i32 i64 i128 isize str u8 u16 u32 u64 u128 usize String Vec Option Some Result Ok Err Box',
);
const SWIFT = words(
  'as associatedtype break case catch class continue default defer do else enum extension fallthrough fileprivate for func guard if import in init inout internal is let open operator private protocol public repeat rethrows return self Self static struct subscript super switch throw throws try typealias var where while',
);
const PHP = words(
  'abstract and array as break callable case catch class clone const continue declare default do echo else elseif empty enddeclare endfor endforeach endif endswitch endwhile extends final finally fn for foreach function global goto if implements include include_once instanceof insteadof interface isset list match namespace new or print private protected public readonly require require_once return static switch throw trait try unset use var while yield',
);
const RUBY = words(
  'alias and begin break case class def defined? do else elsif end ensure for if in module next not or redo rescue retry return self super then undef unless until when while yield require attr_accessor attr_reader puts',
);
const SQL = words(
  'add all alter and as asc begin between by case check column commit constraint create cross database default delete desc distinct drop else end exec exists foreign from full group having if in index inner insert into is join key left like limit merge not of offset on or order outer primary procedure references replace returns right rollback select set table then top transaction trigger truncate union unique update values view when where with int integer varchar nvarchar char text date datetime bigint decimal float bit boolean count sum avg min max',
);
const PS = words(
  'begin break catch class continue data do dynamicparam else elseif end enum exit filter finally for foreach from function if in param process return switch throw trap try until using while -eq -ne -gt -ge -lt -le -like -notlike -match -notmatch -and -or -not -is -isnot -contains',
);
const SH = words(
  'if then else elif fi case esac for select while until do done in function return exit export local readonly declare unset shift break continue source alias echo cd set test',
);
const BAT = words(
  'if else not exist defined errorlevel goto call exit set setlocal endlocal echo for in do shift pushd popd start cd equ neq lss leq gtr geq',
);
const CSS_AT = words('important');

const code = (kw: Set<string>, extra: Partial<Lang> = {}): Lang => ({
  kind: 'code',
  kw,
  line: ['//'],
  block: [['/*', '*/']],
  quotes: '"\'`',
  ...extra,
});

const LANGS: Record<string, Lang> = {
  js: code(JS, { at: true }),
  py: { kind: 'code', kw: PY, line: ['#'], quotes: '"\'', at: true },
  c: code(C_LIKE, { quotes: '"\'', hashDirective: true }),
  cs: code(CS, { quotes: '"\'', hashDirective: true }),
  java: code(JAVA, { quotes: '"\'', at: true }),
  go: code(GO),
  rust: code(RUST, { quotes: '"' }),
  swift: code(SWIFT, { quotes: '"', at: true }),
  php: code(PHP, { line: ['//', '#'], dollar: true }),
  ruby: { kind: 'code', kw: RUBY, line: ['#'], quotes: '"\'', at: true },
  sql: { kind: 'code', kw: SQL, anyCase: true, line: ['--'], block: [['/*', '*/']], quotes: '\'"', esc: '' },
  ps: { kind: 'code', kw: PS, anyCase: true, line: ['#'], block: [['<#', '#>']], quotes: '"\'', esc: '`', dollar: true },
  sh: { kind: 'code', kw: SH, line: ['#'], quotes: '"\'', dollar: true },
  bat: { kind: 'bat', kw: BAT, anyCase: true, quotes: '"' },
  markup: { kind: 'markup' },
  css: { kind: 'css', block: [['/*', '*/']], line: [], quotes: '"\'' },
  json: { kind: 'json', line: ['//'], block: [['/*', '*/']], quotes: '"' },
  yaml: { kind: 'yaml', line: ['#'], quotes: '"\'' },
  ini: { kind: 'ini', line: [';', '#'], quotes: '"\'', esc: '' },
  md: { kind: 'md' },
};

const BY_EXT: Record<string, string> = {
  js: 'js', mjs: 'js', cjs: 'js', jsx: 'js', ts: 'js', mts: 'js', cts: 'js', tsx: 'js',
  py: 'py', pyw: 'py', pyi: 'py',
  c: 'c', h: 'c', cpp: 'c', cc: 'c', cxx: 'c', hpp: 'c', hh: 'c', hxx: 'c', ino: 'c',
  cs: 'cs', java: 'java', kt: 'java', kts: 'java', scala: 'java', groovy: 'java', gradle: 'java',
  go: 'go', rs: 'rust', swift: 'swift', dart: 'java', php: 'php', rb: 'ruby',
  sql: 'sql', ps1: 'ps', psm1: 'ps', psd1: 'ps', sh: 'sh', bash: 'sh', zsh: 'sh',
  bat: 'bat', cmd: 'bat',
  html: 'markup', htm: 'markup', xhtml: 'markup', xml: 'markup', svg: 'markup', xaml: 'markup',
  csproj: 'markup', vbproj: 'markup', props: 'markup', targets: 'markup', resx: 'markup', config: 'markup',
  plist: 'markup', vue: 'markup', svelte: 'markup',
  css: 'css', scss: 'css', less: 'css',
  json: 'json', jsonc: 'json', json5: 'json', geojson: 'json', ipynb: 'json',
  yaml: 'yaml', yml: 'yaml',
  ini: 'ini', cfg: 'ini', conf: 'ini', toml: 'ini', properties: 'ini', env: 'ini', reg: 'ini', editorconfig: 'ini',
  md: 'md', markdown: 'md',
};
const BY_NAME: Record<string, string> = {
  dockerfile: 'sh', makefile: 'sh', '.bashrc': 'sh', '.profile': 'sh', '.gitignore': 'ini', '.gitattributes': 'ini',
  '.env': 'ini', '.editorconfig': 'ini', '.npmrc': 'ini',
};

/** The language for a file name, or null for plain text (.txt, .log, unknown). */
export function langFor(name: string): string | null {
  const n = name.toLowerCase();
  if (BY_NAME[n]) return BY_NAME[n];
  const dot = n.lastIndexOf('.');
  return dot < 0 ? null : (BY_EXT[n.slice(dot + 1)] ?? null);
}

// Lines longer than this are shown plain: they are almost always machine-made (squashed code),
// and colouring a screen full of them would make scrolling stutter.
const MAX = 1000;
// Lines coloured recently, so scrolling back or typing in Find doesn't colour them again.
const cache = new Map<string, Piece[]>();
const isWord = (ch: string) => /[\p{L}\p{N}_]/u.test(ch);
const NUM = /^(?:0[xX][\da-fA-F_]+|0[bB][01_]+|(?:\d[\d_]*\.?[\d_]*|\.\d[\d_]*)(?:[eE][+-]?\d+)?)[a-zA-Z%]*/;

/** Splits one line into coloured pieces. */
export function colorLine(line: string, langId: string | null): Piece[] {
  const lang = langId ? LANGS[langId] : undefined;
  if (!lang || !line || line.length > MAX) return [{ t: line, c: '' }];
  const key = langId + ' ' + line;
  const known = cache.get(key);
  if (known) return known;
  if (cache.size > 4000) cache.clear();
  const out: Piece[] = [];
  cache.set(key, out);
  const push = (t: string, c: Tok) => {
    if (!t) return;
    const last = out[out.length - 1];
    if (last && last.c === c) last.t += t;
    else out.push({ t, c });
  };
  switch (lang.kind) {
    case 'markup':
      markup(line, push);
      break;
    case 'md':
      markdown(line, push);
      break;
    case 'bat':
      batch(line, lang, push);
      break;
    default:
      scan(line, lang, push);
  }
  return out;
}

type Push = (t: string, c: Tok) => void;

/** Reads a quoted string starting at i (line[i] is the quote). Returns the index after it. */
function quoted(line: string, i: number, esc = '\\'): number {
  const q = line[i];
  let j = i + 1;
  while (j < line.length && line[j] !== q) j += esc && line[j] === esc && q !== '`' ? 2 : 1;
  return Math.min(line.length, j + 1);
}

/** Code, JSON, YAML, INI and CSS. */
function scan(line: string, lang: Lang, push: Push) {
  const kind = lang.kind;
  let i = 0;
  const trimmed = line.trimStart();
  const lead = line.length - trimmed.length;

  // The middle or end of a /* ... */ comment that started on an earlier line.
  if (lang.block?.some(([a]) => a === '/*') && /^\*(?!\w)/.test(trimmed) && !trimmed.startsWith('*=')) {
    const end = line.indexOf('*/');
    if (end < 0 || !line.slice(0, end).includes('/*')) {
      const stop = end < 0 ? line.length : end + 2;
      push(line.slice(0, stop), 'cmc');
      i = stop;
    }
  }
  if (i === 0 && lang.hashDirective && trimmed.startsWith('#')) {
    // #include, #region, #if ... (C, C++, C#)
    const m = /^#\s*\w+/.exec(trimmed);
    if (m) {
      push(line.slice(0, lead + m[0].length), 'sec');
      i = lead + m[0].length;
    }
  }
  if (i === 0 && kind === 'ini') {
    if (/^\[.*\]\s*$/.test(trimmed)) {
      push(line, 'sec'); // [section]
      return;
    }
    const m = /^([^=:;#\s][^=:]*?)(\s*[=:])/.exec(trimmed);
    if (m) {
      push(line.slice(0, lead + m[1].length), 'sec');
      push(m[2], '');
      i = lead + m[0].length;
    }
  }
  if (i === 0 && kind === 'yaml') {
    const m = /^(-\s+)?([^\s#:'"][^:#]*?|"[^"]*"|'[^']*')(:)(?=\s|$)/.exec(trimmed);
    if (m) {
      push(line.slice(0, lead + (m[1]?.length ?? 0)), '');
      push(m[2], 'sec');
      push(m[3], '');
      i = lead + m[0].length;
    }
  }

  // CSS: is this line a selector (like ".a, .b {"), and where does its { start?
  const brace = kind === 'css' ? line.indexOf('{') : -1;
  const colon = kind === 'css' ? line.indexOf(':') : -1;
  const endsOpen = kind === 'css' && /[{,]\s*$/.test(line);
  const selLine = endsOpen && !/:\s*[^\s{]/.test(line.replace(/:[\w-]+(\(|\s*[,{]|\s*$)/g, ''));
  const inSelector = (at: number) => selLine && (brace < 0 || at < brace);

  while (i < line.length) {
    const ch = line[i];

    const lc = lang.line?.find((p) => line.startsWith(p, i));
    // In YAML/INI/shells a # only starts a comment at the start of a word.
    if (lc && (lc !== '#' || i === 0 || /\s/.test(line[i - 1]) || kind === 'code')) {
      if (!(lc === '#' && kind === 'code' && lang.dollar && line[i - 1] === '$')) {
        push(line.slice(i), 'cmc');
        return;
      }
    }
    const bc = lang.block?.find(([a]) => line.startsWith(a, i));
    if (bc) {
      const end = line.indexOf(bc[1], i + bc[0].length);
      const stop = end < 0 ? line.length : end + bc[1].length;
      push(line.slice(i, stop), 'cmc');
      i = stop;
      continue;
    }
    if (lang.quotes?.includes(ch)) {
      const stop = quoted(line, i, lang.esc);
      let c: Tok = 's';
      // A JSON key ("name": ...) stands out from the values.
      if (kind === 'json' && at(/\s*:/, line, stop)) c = 'sec';
      push(line.slice(i, stop), c);
      i = stop;
      continue;
    }
    if (lang.dollar && ch === '$') {
      const m = at(/\$(?:\{[^}]*\}|[\w:]+|[?!#@*$0-9])/, line, i);
      if (m) {
        push(m[0], 'sec');
        i += m[0].length;
        continue;
      }
    }
    if (lang.at && ch === '@' && (i === 0 || !isWord(line[i - 1]))) {
      const m = at(/@[\w.]+/, line, i);
      if (m) {
        push(m[0], 'sec');
        i += m[0].length;
        continue;
      }
    }
    if (kind === 'css') {
      const hex = ch === '#' && colon >= 0 && colon < i ? at(/#[\da-fA-F]{3,8}\b/, line, i) : null;
      if (hex) {
        push(hex[0], 'num');
        i += hex[0].length;
        continue;
      }
      if (ch === '@') {
        const m = at(/@[\w-]+/, line, i);
        if (m) {
          push(m[0], 'k');
          i += m[0].length;
          continue;
        }
      }
      // .class and #id in a selector line (before the {)
      if ((ch === '.' || ch === '#') && isWord(line[i + 1] ?? '') && inSelector(i)) {
        const m = at(/[.#][\w-]+/, line, i)!;
        push(m[0], 'sec');
        i += m[0].length;
        continue;
      }
      if (ch === '!' && at(/!important/i, line, i)) {
        push(line.slice(i, i + 10), 'k');
        i += 10;
        continue;
      }
    }
    if (/\d/.test(ch) || (ch === '.' && /\d/.test(line[i + 1] ?? ''))) {
      if (i === 0 || !isWord(line[i - 1])) {
        const m = at(NUM, line, i);
        if (m) {
          push(m[0], 'num');
          i += m[0].length;
          continue;
        }
      }
    }
    if (isWord(ch) || (ch === '-' && (kind === 'css' || lang.kw === PS))) {
      let j = i + 1;
      while (j < line.length && (isWord(line[j]) || ((kind === 'css' || lang.kw === PS) && line[j] === '-'))) j++;
      if (lang.kw === RUBY && line[j] === '?') j++;
      const w = line.slice(i, j);
      let c: Tok;
      if (kind === 'css') {
        // property: value / a selector / anything else
        c = CONST.has(w) ? 'num' : CSS_AT.has(w) ? 'k' : at(/\s*:/, line, j) && !endsOpen ? 'k' : inSelector(i) ? 'sec' : '';
      } else c = wordKind(w, lang);
      push(w, c);
      i = j;
      continue;
    }
    push(ch, '');
    i++;
  }
}

function wordKind(w: string, lang: Lang): Tok {
  if (CONST.has(w) || (lang.anyCase && CONST.has(w.toLowerCase()))) return 'num';
  if (lang.kind !== 'code') return '';
  const kw = lang.kw;
  if (!kw) return '';
  if (kw.has(w) || (lang.anyCase && kw.has(w.toLowerCase()))) return 'k';
  return '';
}

// Matching a pattern at a position, without copying the rest of the line each time
// (copying made long, busy lines slow).
const sticky = new Map<string, RegExp>();
function at(re: RegExp, line: string, i: number): RegExpExecArray | null {
  const key = re.source + '/' + re.flags;
  let s = sticky.get(key);
  if (!s) {
    s = new RegExp(re.source.replace(/^\^/, ''), re.flags.replace('y', '') + 'y');
    sticky.set(key, s);
  }
  s.lastIndex = i;
  return s.exec(line);
}

/** HTML, XML, SVG, XAML, project files. */
function markup(line: string, push: Push) {
  let i = 0;
  // The middle or end of a <!-- ... --> comment that started on an earlier line.
  const firstOpen = line.indexOf('<!--');
  const firstClose = line.indexOf('-->');
  if (firstClose >= 0 && (firstOpen < 0 || firstClose < firstOpen)) {
    push(line.slice(0, firstClose + 3), 'cmc');
    i = firstClose + 3;
  }
  while (i < line.length) {
    if (line.startsWith('<!--', i)) {
      const end = line.indexOf('-->', i + 4);
      const stop = end < 0 ? line.length : end + 3;
      push(line.slice(i, stop), 'cmc');
      i = stop;
      continue;
    }
    const tag = at(/<\/?[!?]?[\w:.-]+/, line, i);
    if (tag) {
      push(tag[0].slice(0, tag[0].search(/[\w!?]/)), '');
      push(tag[0].slice(tag[0].search(/[\w!?]/)), 'sec');
      i += tag[0].length;
      // Attributes until the end of the tag (or the line).
      while (i < line.length && line[i] !== '>') {
        const ch = line[i];
        if (ch === '"' || ch === "'") {
          const stop = quoted(line, i);
          push(line.slice(i, stop), 's');
          i = stop;
        } else if (/[\w:@.-]/.test(ch)) {
          let j = i + 1;
          while (j < line.length && /[\w:@.-]/.test(line[j])) j++;
          push(line.slice(i, j), 'k');
          i = j;
        } else {
          push(ch, '');
          i++;
        }
      }
      continue;
    }
    if (line[i] === '&') {
      const m = at(/&#?\w+;/, line, i);
      if (m) {
        push(m[0], 'num');
        i += m[0].length;
        continue;
      }
    }
    // Plain text up to the next < or &.
    let j = i + 1;
    while (j < line.length && line[j] !== '<' && line[j] !== '&') j++;
    push(line.slice(i, j), '');
    i = j;
  }
}

/** Markdown: headings, lists, quotes, code and links. */
function markdown(line: string, push: Push) {
  const h = /^#{1,6}\s/.exec(line);
  if (h) {
    push(line, 'sec');
    return;
  }
  if (/^\s*(```|~~~)/.test(line)) {
    push(line, 'cmc');
    return;
  }
  if (/^\s*>/.test(line)) {
    push(line, 'cmc');
    return;
  }
  let i = 0;
  const list = /^\s*(?:[-*+]|\d+[.)])\s/.exec(line);
  if (list) {
    push(list[0], 'k');
    i = list[0].length;
  }
  while (i < line.length) {
    const m = at(/`[^`]*`/, line, i) ?? at(/\*\*[^*]+\*\*/, line, i) ?? at(/\[[^\]]*\]\([^)]*\)/, line, i);
    if (m) {
      push(m[0], m[0][0] === '`' ? 's' : m[0][0] === '[' ? 'k' : 'sec');
      i += m[0].length;
      continue;
    }
    let j = i + 1;
    while (j < line.length && !'`*['.includes(line[j])) j++;
    push(line.slice(i, j), '');
    i = j;
  }
}

/** Windows .bat / .cmd files. */
function batch(line: string, lang: Lang, push: Push) {
  const t = line.trimStart();
  if (/^(rem(\s|$)|::)/i.test(t)) {
    push(line, 'cmc');
    return;
  }
  if (/^:\w/.test(t)) {
    push(line, 'sec'); // a label
    return;
  }
  let i = 0;
  while (i < line.length) {
    const ch = line[i];
    if (ch === '"') {
      const stop = quoted(line, i);
      push(line.slice(i, stop), 's');
      i = stop;
      continue;
    }
    const v = at(/(%~?\w+%?|%%\w|![\w]+!)/, line, i);
    if (ch === '%' || ch === '!') {
      if (v) {
        push(v[0], 'sec');
        i += v[0].length;
        continue;
      }
    }
    if (/\d/.test(ch) && (i === 0 || !isWord(line[i - 1]))) {
      const m = at(/\d+/, line, i)!;
      push(m[0], 'num');
      i += m[0].length;
      continue;
    }
    if (isWord(ch)) {
      let j = i + 1;
      while (j < line.length && isWord(line[j])) j++;
      const w = line.slice(i, j);
      push(w, lang.kw?.has(w.toLowerCase()) ? 'k' : '');
      i = j;
      continue;
    }
    push(ch, '');
    i++;
  }
}
