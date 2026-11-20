<script lang="ts">
  // Shows two files side by side. Only the rows on screen are ever loaded,
  // so this works the same for a 10-line file and a 10-million-line file.
  import { onMount, tick } from 'svelte';
  import { CHANGED, fetchRows, fmtNum, nav, overview, type Cell, type Hit, type Row, type Summary } from '#lib/api.ts';
  import { themeColor } from '#lib/theme.ts';
  import { colorLine, langFor, type Tok } from '#lib/syntax.ts';
  import { prefs, spacingFactor } from '#lib/prefs.svelte.ts';

  let {
    summary,
    fontSize = 13,
    inline = false,
    query = '',
    matchCase = false,
    onCurrent,
    onZoom,
    onFilesChanged,
    place = null,
    bookmarks = $bindable([]),
    onOpenFold,
    onGotoLine,
    canEdit = { left: false, right: false },
    onCopy,
    onEditLine,
  }: {
    summary: Summary;
    fontSize?: number;
    /** One column instead of two: removed words struck through, added words underlined, side by side in the line. */
    inline?: boolean;
    query?: string;
    matchCase?: boolean;
    onCurrent: (hit: Hit | null) => void;
    onZoom?: (step: number) => void;
    /** A file was changed on disk after it was compared, so its text can't be shown correctly. */
    onFilesChanged?: () => void;
    /** A "⋯ unchanged lines" row was clicked (see fold.rs). */
    onOpenFold?: (row: number) => void;
    /** "Moved to line 120" was clicked: go to that line (left = in the original file). */
    onGotoLine?: (left: boolean, line: number) => void;
    /** Which files can be edited (a converted, tidied or partial file can't). */
    canEdit?: { left: boolean; right: boolean };
    /** "Copy to the left / right" on the current difference. */
    onCopy?: (row: number, toLeft: boolean) => void;
    /** A line was edited (double-click, then Enter). */
    onEditLine?: (left: boolean, line: number, text: string) => void;
    /** After a live update: where the view was before (see getPlace). */
    place?: { top: number; x: number; atEnd: boolean } | null;
    /** Bookmarked screen rows (kept by the page, so they survive live updates). */
    bookmarks?: number[];
  } = $props();

  const MAP_W = 14;
  // Changed / added / removed, in the current theme's colours. Read again when the theme changes.
  const mapColors = () => ['', themeColor('chg-s'), themeColor('add-s'), themeColor('del-s'), themeColor('sec')];
  let lookChanges = $state(0);
  $effect(() => {
    const bump = () => lookChanges++;
    window.addEventListener('sbs-look', bump);
    return () => window.removeEventListener('sbs-look', bump);
  });

  let viewport: HTMLDivElement;
  let mapEl: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let height = $state(0);
  let width = $state(0);
  let top = $state(0);
  let rows = $state<Row[]>([]);
  let rowsTop = $state(0);
  let current = $state<Hit | null>(null);
  let flashRow = $state<number | null>(null);
  let scrollX = $state(0);
  let map = $state<number[]>([]);
  let charW = $state(7.8);

  const rowH = $derived(Math.round(fontSize * spacingFactor()));
  // Line-number column: wide enough for the biggest line number in either file.
  const digits = $derived(Math.max(3, String(Math.max(summary.left.lines, summary.right.lines)).length));
  const lnW = $derived(Math.round(charW * 0.92 * digits + 22));
  const total = $derived(summary.totalRows);
  const visible = $derived(Math.max(1, Math.floor(height / rowH)));
  // Word wrap: rows can be several lines tall, so fewer of them fit; their real sizes are measured.
  const wrap = $derived(prefs.wrap);
  let boxes = $state<{ top: number; h: number }[]>([]);
  $effect(() => {
    void rows, wrap, fontSize, width, height, inline;
    if (!wrap) return;
    tick().then(() => {
      if (!viewport) return;
      boxes = [...viewport.querySelectorAll<HTMLElement>('.row')].map((el) => ({ top: el.offsetTop, h: el.offsetHeight }));
    });
  });
  /** Rows that fit on the screen completely (with wrap, fewer than `visible`). */
  const fitting = $derived(wrap ? Math.max(1, boxes.filter((b) => b.top + b.h <= height).length) : visible);
  // With wrap, the last row can be scrolled to the top (the rows below it may be tall).
  const maxTop = $derived(Math.max(0, wrap ? total - 1 : total - visible));
  const thumbH = $derived(height ? Math.max(28, Math.min(height, (height * visible) / Math.max(total, 1))) : 0);
  const thumbY = $derived(maxTop ? (top / maxTop) * (height - thumbH) : 0);
  // How far the text can scroll sideways. The widths are measured on screen, so tabs and
  // wide characters (Chinese, Japanese, emoji...) are counted at their real size.
  let longestPx = $state(0);
  let paneW = $state(0);
  $effect(() => {
    void rows, fontSize, width, lnW, prefs.codeFont; // measure again when any of these change
    tick().then(() => {
      if (!viewport) return;
      let w = 0;
      for (const el of viewport.querySelectorAll<HTMLElement>('.in')) w = Math.max(w, el.offsetWidth);
      longestPx = w;
      const tx = viewport.querySelector<HTMLElement>('.tx');
      paneW = tx ? tx.clientWidth - 12 : 0; // minus the padding on the left
    });
  });
  const maxX = $derived(wrap ? 0 : Math.max(0, longestPx - paneW + 24));
  // Shorter lines on screen, a wider window or a smaller zoom: don't stay scrolled past the end.
  $effect(() => {
    if (longestPx && scrollX > maxX) scrollX = maxX; // (not before the lines have been measured)
  });

  // After a live update the view was at the very end: stay there, so new lines at the end show.
  // (Until the user scrolls; the height isn't known yet when this view starts.)
  // svelte-ignore state_referenced_locally
  let followEnd = $state(!!place?.atEnd);
  $effect(() => {
    if (followEnd) top = maxTop;
  });

  function setTop(t: number) {
    followEnd = false;
    top = Math.max(0, Math.min(maxTop, Math.round(t)));
  }
  function setX(x: number) {
    scrollX = Math.max(0, Math.min(maxX, x));
  }

  // Load the rows that are on screen.
  let req = 0;
  $effect(() => {
    const t = top;
    const n = visible + 1;
    const id = ++req;
    fetchRows(t, n)
      .then((r) => {
        if (id === req) {
          rows = r;
          rowsTop = t;
        }
      })
      .catch((e) => {
        if (id === req && String(e) === CHANGED) onFilesChanged?.();
      });
  });

  // Keep the view inside the file when the window is resized or zoomed.
  $effect(() => {
    if (top > maxTop) top = maxTop;
  });

  // Measure the width of one letter, for sideways scrolling. Again once the built-in code font
  // has loaded, since until then the browser measures a stand-in font.
  let fontsReady = $state(false);
  onMount(() => {
    document.fonts?.ready.then(() => (fontsReady = true));
  });
  $effect(() => {
    void fontsReady, prefs.codeFont;
    const ctx = document.createElement('canvas').getContext('2d');
    if (!ctx || !viewport) return;
    ctx.font = `${fontSize}px ${getComputedStyle(viewport).getPropertyValue('--font-code')}`;
    charW = ctx.measureText('0'.repeat(100)).width / 100 || fontSize * 0.6;
  });

  // The difference map beside the scrollbar.
  $effect(() => {
    const h = Math.round(height);
    if (h > 0) overview(h).then((m) => (map = m));
  });
  $effect(() => {
    if (!canvas || !height) return;
    void lookChanges; // draw again in the new colours after a theme change
    const colors = mapColors();
    const dpr = window.devicePixelRatio || 1;
    canvas.width = MAP_W * dpr;
    canvas.height = height * dpr;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    const step = height / Math.max(map.length, 1);
    map.forEach((k, i) => {
      if (!k) return;
      ctx.fillStyle = colors[k];
      ctx.fillRect(3, i * step, MAP_W - 6, Math.max(2, step));
    });
  });

  /** Scroll so `row` is on screen, and briefly mark it. */
  export function reveal(row: number, mark = true) {
    if (row < top || row >= top + fitting) setTop(row - Math.floor(fitting / 3));
    if (mark) flashRow = row;
  }
  export function getTop() {
    return top;
  }
  /** Where the view is, so a live update can keep it there. */
  // ---- Bookmarks ----
  const marked = $derived(new Set(bookmarks));
  /** The bookmark last jumped to (F2 / Shift+F2 carry on from there while it is on screen). */
  let lastMark: number | null = null;
  function toggleMark(row: number) {
    bookmarks = marked.has(row) ? bookmarks.filter((b) => b !== row) : [...bookmarks, row].sort((a, b) => a - b);
  }
  // ---- Inline view: one line holding both sides ----
  type Kind = 'eq' | 'del' | 'ins';
  /** A changed row as one line: the parts both sides share once, then what was removed and added. */
  function mergeCells(l: Cell, r: Cell): { cell: Cell; kinds: Kind[] } {
    const segs: { t: string; h: boolean }[] = [];
    const kinds: Kind[] = [];
    const put = (t: string, k: Kind) => {
      if (!t) return;
      if (kinds.length && kinds[kinds.length - 1] === k) segs[segs.length - 1].t += t;
      else {
        segs.push({ t, h: k !== 'eq' });
        kinds.push(k);
      }
    };
    let [i, lo, j, ro] = [0, 0, 0, 0];
    while (i < l.s.length || j < r.s.length) {
      const a = l.s[i];
      const b = r.s[j];
      if (a && (a.h || !b)) {
        put(a.t.slice(lo), a.h ? 'del' : 'eq');
        i++;
        lo = 0;
      } else if (b && (b.h || !a)) {
        put(b.t.slice(ro), b.h ? 'ins' : 'eq');
        j++;
        ro = 0;
      } else {
        // Both sides are in a shared part: it is the same text, so take it once.
        const n = Math.min(a.t.length - lo, b.t.length - ro);
        put(a.t.slice(lo, lo + n), 'eq');
        lo += n;
        ro += n;
        if (lo >= a.t.length) [i, lo] = [i + 1, 0];
        if (ro >= b.t.length) [j, ro] = [j + 1, 0];
      }
    }
    // Mostly rewritten: mixing old and new words would be hard to read, so show all of the old
    // line (struck through) and then all of the new one.
    const same = segs.reduce((n, s, k) => n + (kinds[k] === 'eq' ? s.t.length : 0), 0);
    const lt = l.s.reduce((n, s) => n + s.t.length, 0);
    const rt = r.s.reduce((n, s) => n + s.t.length, 0);
    if (same < Math.max(lt, rt) * 0.5) {
      const old = l.s.map((s) => s.t).join('');
      const now = r.s.map((s) => s.t).join('').trimStart(); // (its indent is already shown by the old part)
      return { cell: { n: r.n, s: [{ t: old, h: true }, { t: ' ', h: false }, { t: now, h: true }] }, kinds: ['del', 'eq', 'ins'] };
    }
    return { cell: { n: r.n, s: segs }, kinds };
  }
  function inlineOf(row: Row): { cell: Cell; kinds: Kind[] } | null {
    if (row.l && row.r) return row.k === 1 ? mergeCells(row.l, row.r) : { cell: row.l, kinds: row.l.s.map(() => 'eq') };
    const c = row.l ?? row.r;
    if (!c) return null;
    return { cell: c, kinds: c.s.map(() => (row.l ? 'del' : 'ins')) };
  }

  // ---- Editing a line: double-click it, Enter keeps the change, Esc (or clicking away) drops it ----
  let editing = $state<{ row: number; left: boolean; line: number; text: string } | null>(null);
  function startEdit(row: number, left: boolean, c: Cell) {
    if (!(left ? canEdit.left : canEdit.right) || !onEditLine) return;
    const text = c.s.map((s) => s.t).join('');
    if (text.length > 9000) return; // very long lines are shown cut short; editing them here would lose the rest
    editing = { row, left, line: c.n, text };
  }
  function finishEdit(keep: boolean) {
    const e = editing;
    editing = null;
    if (keep && e) onEditLine?.(e.left, e.line, e.text);
    viewport?.focus();
  }
  function focusOnMount(el: HTMLInputElement) {
    el.focus();
    el.select();
  }

  /** Ctrl+B: the line in the middle of the screen. */
  export function toggleMarkHere() {
    toggleMark(Math.min(total - 1, top + Math.floor(visible / 2)));
  }
  function jumpMark(dir: 1 | -1) {
    if (!bookmarks.length) return;
    const onScreen = lastMark !== null && lastMark >= top && lastMark < top + visible;
    const from = onScreen ? lastMark! : dir === 1 ? top - 1 : top + visible;
    const list = bookmarks;
    // The next one in that direction, or round to the first / last.
    const to = dir === 1 ? (list.find((b) => b > from) ?? list[0]) : ([...list].reverse().find((b) => b < from) ?? list[list.length - 1]);
    lastMark = to;
    reveal(to);
  }
  export function getPlace() {
    return { top, x: scrollX, atEnd: top >= maxTop };
  }

  // The frame around the current difference. It slides to the next one when that is a click
  // and the view doesn't have to scroll; keyboard jumps (F7/F8) and scrolling move it at once.
  let lensSlide = $state(false);
  let lensPop = $state(0);
  const lens = $derived.by(() => {
    if (!current) return null;
    // Measured from the rows on screen (rowsTop), so the frame always sits on the text it marks.
    const a = Math.max(current.row - rowsTop, -1);
    const b = Math.min(current.row + current.rows - rowsTop, visible + 1);
    if (b <= a || b < 0 || a > visible) return null;
    if (wrap) {
      // Rows have their own heights: use the measured ones.
      const first = boxes[Math.max(a, 0)];
      const last = boxes[Math.min(b, boxes.length) - 1];
      if (!first || !last) return null;
      const y = a < 0 ? -rowH : first.top;
      return { y, h: last.top + last.h - y };
    }
    return { y: a * rowH, h: (b - a) * rowH };
  });

  async function go(hit: Hit | null, animate = false) {
    if (!hit) return;
    const onScreen = hit.row >= top && hit.row + Math.min(hit.rows, fitting) <= top + fitting;
    lensSlide = animate && onScreen && current !== null;
    // Once it has slid, scrolling by hand moves the frame with the text again.
    if (lensSlide) setTimeout(() => (lensSlide = false), 320);
    if (animate && !onScreen) lensPop++; // after a jump, the frame glows in instead
    current = hit;
    flashRow = null;
    onCurrent(hit);
    if (!onScreen) setTop(hit.row - Math.min(3, Math.floor(fitting / 4)));
  }
  // One step at a time: pressing F8 twice quickly moves two differences, not one.
  let steps: Promise<void> = Promise.resolve();
  function step(dir: 1 | -1, animate: boolean) {
    steps = steps
      .then(async () => {
        const from = current ? current.row : top;
        await go(await nav(from, dir === 1 && !current ? 0 : dir), animate);
      })
      .catch(() => {});
    return steps;
  }
  /** animate: true for mouse clicks; keyboard jumps stay instant. */
  export function next(animate = false) {
    return step(1, animate);
  }
  export function prev(animate = false) {
    return step(-1, animate);
  }
  /** A pin in the middle was clicked: go to the difference that starts at `row`. */
  function goToRow(row: number) {
    steps = steps.then(async () => go(await nav(row, 0), true)).catch(() => {});
  }

  // Code colours, for each side's file type (none for .txt, .log, pasted text...).
  // (Pasted or copied text has no file type of its own: it gets the other side's.)
  const pasted = (name: string) => name.startsWith('Pasted text (');
  const langOf = (me: string, other: string) => langFor(me) ?? (pasted(me) ? langFor(other) : null);
  const langL = $derived(prefs.codeColors ? langOf(summary.left.name, summary.right.name) : null);
  const langR = $derived(prefs.codeColors ? langOf(summary.right.name, summary.left.name) : null);

  // A line is drawn in three layers: changed words (the engine's pieces), code colours and search
  // matches. All three are worked out on the whole line, then cut where any of them starts or ends,
  // so a search for "const x" is still found when the two words have different colours.
  interface Bit {
    t: string;
    c: Tok;
    hit: boolean;
  }
  interface Run {
    h: boolean;
    bits: Bit[];
  }
  function runs(cell: Cell, lang: string | null): Run[] {
    const text = cell.s.length === 1 ? cell.s[0].t : cell.s.map((s) => s.t).join('');
    const toks = colorLine(text, lang);
    const found: [number, number][] = [];
    if (query) {
      const hay = matchCase ? text : text.toLowerCase();
      const needle = matchCase ? query : query.toLowerCase();
      if (hay.length === text.length) {
        for (let i = hay.indexOf(needle); i !== -1; i = hay.indexOf(needle, i + needle.length)) {
          found.push([i, i + needle.length]);
        }
      }
    }
    if (toks.length === 1 && !found.length) return cell.s.map((s) => ({ h: s.h, bits: [{ t: s.t, c: toks[0].c, hit: false }] }));

    const out: Run[] = [];
    let ti = 0;
    let tEnd = toks[0]?.t.length ?? 0;
    let fi = 0;
    let pos = 0;
    for (const s of cell.s) {
      const run: Run = { h: s.h, bits: [] };
      const sEnd = pos + s.t.length;
      while (pos < sEnd) {
        while (tEnd <= pos && ti < toks.length - 1) tEnd += toks[++ti].t.length;
        while (fi < found.length && found[fi][1] <= pos) fi++;
        const f = found[fi];
        const hit = f !== undefined && f[0] <= pos;
        const end = Math.min(sEnd, tEnd, f ? (hit ? f[1] : f[0]) : sEnd);
        const stop = end > pos ? end : sEnd;
        const bit = { t: text.slice(pos, stop), c: toks[ti]?.c ?? '', hit };
        const last = run.bits[run.bits.length - 1];
        if (last && last.c === bit.c && last.hit === bit.hit) last.t += bit.t;
        else run.bits.push(bit);
        pos = stop;
      }
      out.push(run);
    }
    return out;
  }

  // Mouse wheel and touchpad. Ctrl + wheel zooms.
  let acc = 0;
  function onWheel(e: WheelEvent) {
    e.preventDefault();
    if (e.ctrlKey) {
      onZoom?.(e.deltaY < 0 ? 1 : -1);
      return;
    }
    if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      setX(scrollX + (e.shiftKey ? e.deltaY : e.deltaX));
      return;
    }
    const px = e.deltaMode === 1 ? e.deltaY * rowH : e.deltaMode === 2 ? e.deltaY * height : e.deltaY;
    acc += px;
    const n = Math.trunc(acc / rowH);
    if (n) {
      acc -= n * rowH;
      setTop(top + n);
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    const moves: Record<string, () => void> = {
      ArrowDown: () => setTop(top + 1),
      ArrowUp: () => setTop(top - 1),
      PageDown: () => setTop(top + Math.max(1, fitting - 1)),
      PageUp: () => setTop(top - Math.max(1, fitting - 1)),
      Home: () => setTop(0),
      End: () => setTop(maxTop),
      ArrowRight: () => setX(scrollX + charW * 8),
      ArrowLeft: () => setX(scrollX - charW * 8),
    };
    const m = moves[e.key];
    if (m) {
      e.preventDefault();
      m();
    }
  }

  function onGlobalKey(e: KeyboardEvent) {
    if (e.key === 'F2') {
      e.preventDefault();
      jumpMark(e.shiftKey ? -1 : 1);
      return;
    }
    if (e.ctrlKey && !e.altKey && e.key.toLowerCase() === 'b') {
      e.preventDefault();
      toggleMarkHere();
      return;
    }
    if (e.key === 'F8' || (e.altKey && e.key === 'ArrowDown')) {
      e.preventDefault();
      next();
    } else if (e.key === 'F7' || (e.altKey && e.key === 'ArrowUp')) {
      e.preventDefault();
      prev();
    }
  }

  // Dragging on the difference map scrolls.
  let dragging = false;
  let grab = 0;
  function yToTop(clientY: number) {
    const y = clientY - mapEl.getBoundingClientRect().top - grab;
    setTop((y / Math.max(1, height - thumbH)) * maxTop);
  }
  function onMapDown(e: PointerEvent) {
    const y = e.clientY - mapEl.getBoundingClientRect().top;
    grab = y >= thumbY && y <= thumbY + thumbH ? y - thumbY : thumbH / 2;
    dragging = true;
    mapEl.setPointerCapture(e.pointerId);
    yToTop(e.clientY);
  }
  function onMapMove(e: PointerEvent) {
    if (dragging) yToTop(e.clientY);
  }
  function onMapUp(e: PointerEvent) {
    dragging = false;
    mapEl.releasePointerCapture(e.pointerId);
  }

  onMount(() => {
    viewport.addEventListener('wheel', onWheel, { passive: false });
    viewport.addEventListener('keydown', onKey);
    if (place) {
      // A live update: keep the place (and the keyboard focus where it was, e.g. in Find).
      if (!place.atEnd) top = place.top;
      scrollX = place.x;
      // The old view had the focus (it is gone now): give it to this one, so the arrow keys keep working.
      if (!document.activeElement || document.activeElement === document.body) viewport.focus();
    } else {
      viewport.focus();
      nav(0, 0).then(go);
    }
    return () => {
      viewport.removeEventListener('wheel', onWheel);
      viewport.removeEventListener('keydown', onKey);
    };
  });
</script>

<svelte:window onkeydown={onGlobalKey} />

{#snippet bits(list: Bit[])}{#each list as b}{#if b.hit}<span class="hit">{b.t}</span>{:else if b.c}<span class="c-{b.c}">{b.t}</span>{:else}{b.t}{/if}{/each}{/snippet}
{#snippet inlineText(m: { cell: Cell; kinds: Kind[] }, lang: string | null)}{#each runs(m.cell, lang) as s, n}{#if m.kinds[n] === 'del'}<del>{@render bits(s.bits)}</del>{:else if m.kinds[n] === 'ins'}<ins>{@render bits(s.bits)}</ins>{:else}{@render bits(s.bits)}{/if}{/each}{/snippet}
{#snippet cellText(c: Cell, lang: string | null)}{#each runs(c, lang) as s}{#if s.h}<mark>{@render bits(s.bits)}</mark>{:else}{@render bits(s.bits)}{/if}{/each}{/snippet}

<div class="view" class:inline class:wrap bind:clientHeight={height} bind:clientWidth={width} style:--fs="{fontSize}px" style:--rh="{rowH}px" style:--lnw="{lnW}px">
  <!-- The rows area is focusable so the arrow keys and Page Up/Down can scroll it. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="rows" id="rows" bind:this={viewport} tabindex="0" role="region" aria-label="Differences">
    {#if total === 0}
      <p class="empty">Both files are empty.</p>
    {/if}
    {#each rows as row, i (rowsTop + i)}
      {@const r = rowsTop + i}
      {@const starts = row.k !== 0 && (i === 0 ? r === 0 : rows[i - 1].k === 0) /* changed + added lines together are one difference */}
      {@const isCur = current !== null && r === current.row}
      {@const moveStart = (row.k === 5 || row.k === 6) && (i === 0 || rows[i - 1].k !== row.k)}
      {#if row.k === 4}
        <div class="row k4">
          <button type="button" class="fold" onclick={() => onOpenFold?.(r)} title="Show these lines">
            <span class="dots" aria-hidden="true">⋯</span>{fmtNum(row.f ?? 0)} unchanged {row.f === 1 ? 'line' : 'lines'}<span class="show">Show</span>
          </button>
        </div>
      {:else if inline}
        {@const m = inlineOf(row)}
        <div class="row il k{row.k}" class:bm={marked.has(r)} class:cur={current !== null && r >= current.row && r < current.row + current.rows} class:found={flashRow === r}>
          <button type="button" class="ln" tabindex="-1" title="{marked.has(r) ? 'Remove the bookmark' : 'Bookmark this line (Ctrl+B)'}" onclick={() => toggleMark(r)}>{row.l ? row.l.n : ''}</button>
          <button type="button" class="ln r" tabindex="-1" title="{marked.has(r) ? 'Remove the bookmark' : 'Bookmark this line (Ctrl+B)'}" onclick={() => toggleMark(r)}>{row.r ? row.r.n : ''}</button>
          <span class="sign" aria-hidden="true">{row.k === 2 ? '+' : row.k === 3 ? '−' : row.k === 1 ? '±' : row.k === 5 ? '↗' : row.k === 6 ? '↘' : ''}</span>
          <span class="tx">{#if m}<span class="in" style:transform="translateX({-scrollX}px)">{@render inlineText(m, row.r ? langR : langL)}</span>{/if}</span>
        </div>
      {:else}
      <div class="row k{row.k}" class:bm={marked.has(r)} class:cur={current !== null && r >= current.row && r < current.row + current.rows} class:found={flashRow === r}>
        <button type="button" class="ln" tabindex="-1" title="{marked.has(r) ? 'Remove the bookmark' : 'Bookmark this line (Ctrl+B)'}" onclick={() => toggleMark(r)}>{row.l ? row.l.n : ''}</button>
        <span class="tx" class:fill={!row.l} ondblclick={() => row.l && startEdit(r, true, row.l)} role="presentation">{#if editing && editing.row === r && editing.left}<input class="lineedit" bind:value={editing.text} use:focusOnMount onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') finishEdit(true); else if (e.key === 'Escape') finishEdit(false); }} onblur={() => finishEdit(false)} aria-label="Edit line {row.l?.n}" />{:else if row.l}<span class="in" style:transform="translateX({-scrollX}px)">{@render cellText(row.l, langL)}</span>{:else if row.k === 6 && moveStart && row.m}<button type="button" class="moved-to" onclick={() => onGotoLine?.(true, row.m ?? 1)}>↖ moved here from line {fmtNum(row.m)}</button>{/if}</span>
        <span class="seam">
          {#if starts}
            <button
              type="button"
              class="pin p{row.k}"
              class:on={isCur}
              tabindex="-1"
              title={isCur ? `Difference ${(current?.index ?? 0) + 1}` : 'Go to this difference'}
              onclick={() => goToRow(r)}
            >{isCur ? (current?.index ?? 0) + 1 : ''}</button>
          {/if}
        </span>
        <button type="button" class="ln r" tabindex="-1" title="{marked.has(r) ? 'Remove the bookmark' : 'Bookmark this line (Ctrl+B)'}" onclick={() => toggleMark(r)}>{row.r ? row.r.n : ''}</button>
        <span class="tx" class:fill={!row.r} ondblclick={() => row.r && startEdit(r, false, row.r)} role="presentation">{#if editing && editing.row === r && !editing.left}<input class="lineedit" bind:value={editing.text} use:focusOnMount onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') finishEdit(true); else if (e.key === 'Escape') finishEdit(false); }} onblur={() => finishEdit(false)} aria-label="Edit line {row.r?.n}" />{:else if row.r}<span class="in" style:transform="translateX({-scrollX}px)">{@render cellText(row.r, langR)}</span>{:else if row.k === 5 && moveStart && row.m}<button type="button" class="moved-to" onclick={() => onGotoLine?.(false, row.m ?? 1)}>moved to line {fmtNum(row.m)} ↘</button>{/if}</span>
      </div>
      {/if}
    {/each}
    {#if lens && current && onCopy && (canEdit.left || canEdit.right)}
      <!-- Merge: copy the current difference to the other side. -->
      <div class="mergebar" style:top="{lens.y >= 34 ? lens.y - 32 : lens.y + lens.h + 4}px">
        <button type="button" disabled={!canEdit.left} onclick={() => current && onCopy?.(current.row, true)} title="Make the original match the new version here">← Copy to the left</button>
        <button type="button" disabled={!canEdit.right} onclick={() => current && onCopy?.(current.row, false)} title="Make the new version match the original here">Copy to the right →</button>
      </div>
    {/if}
    {#if lens}
      {#key lensPop}
        <div class="lens" class:slide={lensSlide} class:pop={lensPop > 0} style:transform="translateY({lens.y}px)" style:height="{lens.h}px" aria-hidden="true"></div>
      {/key}
    {/if}
  </div>

  <div
    class="map"
    bind:this={mapEl}
    role="scrollbar"
    aria-controls="rows"
    aria-orientation="vertical"
    aria-valuemin={0}
    aria-valuemax={maxTop}
    aria-valuenow={top}
    tabindex="-1"
    onpointerdown={onMapDown}
    onpointermove={onMapMove}
    onpointerup={onMapUp}
  >
    <canvas bind:this={canvas} style:width="{MAP_W}px" style:height="{height}px"></canvas>
    {#each bookmarks as b (b)}
      <i class="mapmark" style:top="{(b / Math.max(total, 1)) * height}px" aria-hidden="true"></i>
    {/each}
    <div class="thumb" style:top="{thumbY}px" style:height="{thumbH}px"></div>
  </div>
</div>

<style>
  .view {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 8px;
    position: relative;
  }
  .rows {
    position: relative;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    padding-bottom: 12px;
    font-family: var(--font-code);
    font-size: var(--fs);
    line-height: var(--rh);
  }
  /* The two cards behind the text (their tops are the file headers above). */
  .rows::before,
  .rows::after {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    width: calc(50% - var(--gw) / 2);
    background: var(--t-card);
    border: 1px solid var(--t-line);
    border-top: 0;
    border-radius: 0 0 14px 14px;
  }
  .rows::before {
    left: 0;
  }
  .rows::after {
    right: 0;
  }
  .rows:focus {
    outline: none;
  }
  .row {
    position: relative;
    z-index: 1;
    display: grid;
    grid-template-columns: var(--lnw) 1fr var(--gw) var(--lnw) 1fr;
    height: var(--rh);
  }
  .row > :first-child {
    margin-left: 1px;
  }
  .row > :last-child {
    margin-right: 1px;
  }
  button.ln {
    border: 0;
    margin: 0;
    padding: 0 10px 0 0;
    font: inherit;
    font-size: 0.92em;
    line-height: inherit;
    background: none;
    cursor: pointer;
  }
  button.ln:hover {
    color: var(--t-strong);
  }
  /* A bookmarked line: a small ribbon at the left of both line numbers. */
  .row.bm .ln {
    color: var(--t-acc);
    font-weight: 600;
    background-image: linear-gradient(var(--t-acc), var(--t-acc));
    background-size: 3px 70%;
    background-position: 4px 50%;
    background-repeat: no-repeat;
  }
  .ln {
    color: var(--t-faint);
    text-align: right;
    padding-right: 10px;
    font-size: 0.92em;
    font-variant-numeric: tabular-nums;
  }
  .tx {
    overflow: hidden;
    padding-left: 12px;
    color: var(--t-fg);
    user-select: text;
  }
  .in {
    display: inline-block;
    white-space: pre;
    tab-size: 4;
  }
  .k1 .tx,
  .k1 .ln {
    background: var(--t-chg);
  }
  .k2 .tx:not(.fill),
  .k2 .ln.r {
    background: var(--t-add);
  }
  .k3 .tx:not(.fill),
  .k3 .ln:first-child {
    background: var(--t-del);
  }
  .k1 .ln,
  .k2 .ln,
  .k3 .ln {
    color: var(--t-muted);
  }
  /* A thin coloured edge on the line numbers of changed lines. */
  .k1 .ln {
    box-shadow: inset 2px 0 0 var(--t-chg-s);
  }
  .k2 .ln.r {
    box-shadow: inset 2px 0 0 var(--t-add-s);
  }
  .k3 .ln:first-child {
    box-shadow: inset 2px 0 0 var(--t-del-s);
  }
  /* Moved blocks: their own colour, with a link to where they went. */
  .k5 .tx:not(.fill),
  .k5 .ln:first-child,
  .k6 .tx:not(.fill),
  .k6 .ln.r,
  .row.il.k5 .tx,
  .row.il.k6 .tx,
  .row.il.k5 .ln,
  .row.il.k6 .ln {
    background: color-mix(in srgb, var(--t-sec) 10%, transparent);
  }
  .k5 .ln:first-child,
  .k6 .ln.r,
  .row.il.k5 .ln:first-child,
  .row.il.k6 .ln:first-child {
    box-shadow: inset 2px 0 0 var(--t-sec);
    color: var(--t-muted);
  }
  .k5 .sign,
  .k6 .sign {
    color: var(--t-sec);
  }
  .moved-to {
    margin: 0;
    padding: 0 8px;
    border: 1px solid color-mix(in srgb, var(--t-sec) 45%, transparent);
    border-radius: 999px;
    background: var(--t-card);
    color: var(--t-sec);
    font: 500 11.5px/calc(var(--rh) - 4px) var(--font-ui);
    cursor: pointer;
    white-space: nowrap;
  }
  .moved-to:hover {
    background: color-mix(in srgb, var(--t-sec) 12%, var(--t-card));
  }
  .tx.fill {
    background: repeating-linear-gradient(135deg, transparent 0 6px, var(--t-stripe) 6px 7px);
  }
  mark {
    background: var(--t-mark);
    color: inherit;
    border-radius: 3px;
  }
  .hit {
    background: var(--t-acc);
    color: var(--t-on-acc);
    border-radius: 2px;
  }
  /* Code colours (see syntax.ts); every theme has its own. */
  .c-k {
    color: var(--t-k);
  }
  .c-s {
    color: var(--t-s);
  }
  .c-num {
    color: var(--t-num);
  }
  .c-sec {
    color: var(--t-sec);
  }
  .c-cmc {
    color: var(--t-cmc);
  }
  .row.found {
    outline: 1px solid var(--t-acc);
    outline-offset: -1px;
  }

  /* The seam between the cards, with a pin at the start of each difference. */
  .seam {
    display: grid;
    place-items: center;
    overflow: visible;
  }
  .pin {
    min-width: 12px;
    height: 12px;
    padding: 0;
    border-radius: 999px;
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-on-acc);
    font: 600 10px/1 var(--font-ui);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    transition:
      transform 0.2s cubic-bezier(0.23, 1, 0.32, 1),
      background-color 0.2s ease,
      box-shadow 0.25s ease;
  }
  .pin.p1 {
    border-color: color-mix(in srgb, var(--t-chg-s) 70%, transparent);
  }
  .pin.p2 {
    border-color: color-mix(in srgb, var(--t-add-s) 70%, transparent);
  }
  .pin.p3 {
    border-color: color-mix(in srgb, var(--t-del-s) 70%, transparent);
  }
  .pin.p5,
  .pin.p6 {
    border-color: color-mix(in srgb, var(--t-sec) 70%, transparent);
  }
  .pin:hover {
    transform: scale(1.25);
  }
  .pin:active {
    transform: scale(0.95);
  }
  .pin.on {
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    border-color: transparent;
    background: var(--t-acc);
    box-shadow:
      0 0 0 4px var(--t-acc-bg),
      0 0 18px var(--t-glow1);
    animation: pin-in 0.3s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .pin.on:hover {
    transform: none;
  }
  @keyframes pin-in {
    from {
      transform: scale(0.6);
      opacity: 0;
    }
  }

  /* ---- Word wrap: long lines continue on the next line ---- */
  .wrap .row {
    height: auto;
    min-height: var(--rh);
  }
  .wrap .in {
    display: block;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    transform: none !important;
  }
  .wrap .tx {
    padding-right: 10px;
  }
  /* (a button centres its text; in a tall row the number belongs at the top) */
  .wrap button.ln {
    display: flex;
    align-items: flex-start;
    justify-content: flex-end;
  }
  .wrap .seam {
    place-items: start center;
    padding-top: calc((var(--rh) - 12px) / 2);
  }

  /* ---- Inline view: one card, two line-number columns ---- */
  .inline .rows::before {
    width: 100%;
  }
  .inline .rows::after {
    display: none;
  }
  .row.il {
    grid-template-columns: var(--lnw) var(--lnw) 18px 1fr;
  }
  .row.il .tx {
    padding-left: 2px;
  }
  .sign {
    text-align: center;
    color: var(--t-muted);
  }
  .k2 .sign {
    color: var(--t-add-s);
  }
  .k3 .sign {
    color: var(--t-del-s);
  }
  .k1 .sign {
    color: var(--t-chg-s);
  }
  .row.il.k2 .tx,
  .row.il.k2 .ln,
  .row.il.k2 .sign {
    background: var(--t-add);
  }
  .row.il.k3 .tx,
  .row.il.k3 .ln,
  .row.il.k3 .sign {
    background: var(--t-del);
  }
  .row.il.k1 .sign {
    background: var(--t-chg);
  }
  .row.il.k2 .ln.r,
  .row.il.k3 .ln:first-child {
    box-shadow: none;
  }
  .row.il.k2 .ln:first-child {
    box-shadow: inset 2px 0 0 var(--t-add-s);
  }
  del {
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--t-del-s) 70%, transparent);
    color: var(--t-del-s);
    background: var(--t-del);
    border-radius: 3px;
  }
  ins {
    text-decoration: none;
    color: var(--t-add-s);
    background: var(--t-add);
    border-radius: 3px;
    box-shadow: inset 0 -1px 0 color-mix(in srgb, var(--t-add-s) 60%, transparent);
  }
  .row.il.k3 del,
  .row.il.k2 ins {
    background: none;
    box-shadow: none;
  }
  .row.il.k2 ins {
    color: inherit;
  }

  /* ---- Editing ---- */
  .lineedit {
    width: calc(100% - 6px);
    height: calc(var(--rh) - 2px);
    margin: 1px 0;
    padding: 0 4px;
    border: 1px solid var(--t-acc);
    border-radius: 4px;
    background: var(--t-card);
    color: var(--t-strong);
    font: inherit;
    line-height: inherit;
    outline: none;
    box-shadow: 0 0 0 3px var(--t-acc-bg);
    user-select: text;
  }
  .mergebar {
    position: absolute;
    left: 50%;
    z-index: 3;
    display: flex;
    gap: 4px;
    padding: 3px;
    border-radius: 10px;
    background: var(--t-card2);
    border: 1px solid var(--t-line2);
    box-shadow: 0 8px 24px -10px rgba(0, 0, 0, 0.45);
    transform: translateX(-50%);
    font-family: var(--font-ui);
    animation: merge-in 0.18s cubic-bezier(0.23, 1, 0.32, 1);
  }
  @keyframes merge-in {
    from {
      opacity: 0;
      transform: translate(-50%, 3px);
    }
  }
  .mergebar button {
    height: 24px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--t-fg);
    font: 500 12px var(--font-ui);
    white-space: nowrap;
    cursor: pointer;
  }
  .mergebar button:hover:not(:disabled) {
    background: var(--t-acc-bg);
    color: var(--t-acc);
  }
  .mergebar button:active:not(:disabled) {
    transform: scale(0.97);
  }
  .mergebar button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  @media (prefers-reduced-motion: reduce) {
    .mergebar {
      animation: none;
    }
  }

  /* A folded run of unchanged lines: one quiet row across both cards. */
  .row.k4 {
    grid-template-columns: 1fr;
  }
  .fold {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 0 1px;
    border: 0;
    border-block: 1px dashed var(--t-line2);
    background: var(--t-stripe);
    color: var(--t-muted);
    font: 500 12px var(--font-ui);
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      color 0.15s ease;
  }
  .fold:hover {
    background: var(--t-acc-bg);
    color: var(--t-strong);
  }
  .fold .dots {
    font-size: 15px;
    letter-spacing: 1px;
  }
  .fold .show {
    color: var(--t-acc);
    font-weight: 600;
  }

  /* The frame around the current difference. */
  .lens {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    z-index: 2;
    pointer-events: none;
    border-radius: 8px;
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--t-acc) 60%, transparent),
      0 0 24px -6px var(--t-glow1);
  }
  .lens.slide {
    transition:
      transform 0.26s cubic-bezier(0.77, 0, 0.175, 1),
      height 0.26s cubic-bezier(0.77, 0, 0.175, 1);
  }
  .lens.pop {
    animation: lens-in 0.32s cubic-bezier(0.23, 1, 0.32, 1);
  }
  @keyframes lens-in {
    from {
      opacity: 0;
      box-shadow:
        0 0 0 6px color-mix(in srgb, var(--t-acc) 25%, transparent),
        0 0 32px 0 var(--t-glow1);
    }
  }

  .empty {
    position: relative;
    z-index: 1;
    color: var(--t-muted);
    text-align: center;
    margin-top: 60px;
    font-family: var(--font-ui);
  }
  /* The difference map, as a slim rounded strip beside the cards. */
  .map {
    position: relative;
    width: 14px;
    flex: none;
    margin-bottom: 12px;
    overflow: hidden;
    background: var(--t-card);
    border: 1px solid var(--t-line);
    border-radius: 10px;
    touch-action: none;
  }
  .map canvas {
    display: block;
    margin-left: -1px;
  }
  .mapmark {
    position: absolute;
    left: 1px;
    right: 1px;
    height: 3px;
    margin-top: -1px;
    border-radius: 2px;
    background: var(--t-acc);
    pointer-events: none;
    z-index: 1;
  }
  .thumb {
    position: absolute;
    left: 0;
    right: 0;
    background: var(--t-line2);
    border-radius: 3px;
    pointer-events: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .lens.slide {
      transition: none;
    }
    .lens.pop,
    .pin.on {
      animation: none;
    }
  }
</style>
