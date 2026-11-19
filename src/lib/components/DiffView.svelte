<script lang="ts">
  // Shows two files side by side. Only the rows on screen are ever loaded,
  // so this works the same for a 10-line file and a 10-million-line file.
  import { onMount } from 'svelte';
  import { CHANGED, fetchRows, nav, overview, type Cell, type Hit, type Row, type Summary } from '$lib/api';

  let {
    summary,
    fontSize = 13,
    query = '',
    matchCase = false,
    onCurrent,
    onZoom,
    onFilesChanged,
  }: {
    summary: Summary;
    fontSize?: number;
    query?: string;
    matchCase?: boolean;
    onCurrent: (hit: Hit | null) => void;
    onZoom?: (step: number) => void;
    /** A file was changed on disk after it was compared, so its text can't be shown correctly. */
    onFilesChanged?: () => void;
  } = $props();

  const MAP_W = 14;
  const MAP_COLORS = ['', '#d29922', '#3fb950', '#f85149'];

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

  const rowH = $derived(Math.round(fontSize * 1.54));
  // Line-number column: wide enough for the biggest line number in either file.
  const digits = $derived(Math.max(3, String(Math.max(summary.left.lines, summary.right.lines)).length));
  const lnW = $derived(Math.round(charW * 0.92 * digits + 22));
  const total = $derived(summary.totalRows);
  const visible = $derived(Math.max(1, Math.floor(height / rowH)));
  const maxTop = $derived(Math.max(0, total - visible));
  const thumbH = $derived(height ? Math.max(28, Math.min(height, (height * visible) / Math.max(total, 1))) : 0);
  const thumbY = $derived(maxTop ? (top / maxTop) * (height - thumbH) : 0);
  const maxX = $derived.by(() => {
    let longest = 0;
    for (const r of rows)
      for (const c of [r.l, r.r])
        if (c) {
          let n = 0;
          for (const s of c.s) n += s.t.length;
          if (n > longest) longest = n;
        }
    const paneW = (width - MAP_W) / 2 - lnW - 16;
    return Math.max(0, longest * charW - paneW + 24);
  });

  function setTop(t: number) {
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

  // Measure the width of one letter, for sideways scrolling.
  $effect(() => {
    const ctx = document.createElement('canvas').getContext('2d');
    if (!ctx || !viewport) return;
    ctx.font = `${fontSize}px 'Cascadia Mono', Consolas, ui-monospace, monospace`;
    charW = ctx.measureText('0'.repeat(100)).width / 100 || fontSize * 0.6;
  });

  // The difference map beside the scrollbar.
  $effect(() => {
    const h = Math.round(height);
    if (h > 0) overview(h).then((m) => (map = m));
  });
  $effect(() => {
    if (!canvas || !height) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = MAP_W * dpr;
    canvas.height = height * dpr;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    const step = height / Math.max(map.length, 1);
    map.forEach((k, i) => {
      if (!k) return;
      ctx.fillStyle = MAP_COLORS[k];
      ctx.fillRect(3, i * step, MAP_W - 6, Math.max(2, step));
    });
  });

  /** Scroll so `row` is on screen, and briefly mark it. */
  export function reveal(row: number, mark = true) {
    if (row < top || row >= top + visible) setTop(row - Math.floor(visible / 3));
    if (mark) flashRow = row;
  }
  export function getTop() {
    return top;
  }

  async function go(hit: Hit | null) {
    if (!hit) return;
    current = hit;
    flashRow = null;
    onCurrent(hit);
    const span = Math.min(hit.rows, visible);
    if (hit.row < top || hit.row + span > top + visible) setTop(hit.row - Math.min(3, Math.floor(visible / 4)));
  }
  // One step at a time: pressing F8 twice quickly moves two differences, not one.
  let steps: Promise<void> = Promise.resolve();
  function step(dir: 1 | -1) {
    steps = steps
      .then(async () => {
        const from = current ? current.row : top;
        await go(await nav(from, dir === 1 && !current ? 0 : dir));
      })
      .catch(() => {});
    return steps;
  }
  export function next() {
    return step(1);
  }
  export function prev() {
    return step(-1);
  }

  // Search highlights: split text into pieces that match the search and pieces that don't.
  function hits(t: string): { t: string; h: boolean }[] {
    if (!query) return [{ t, h: false }];
    const hay = matchCase ? t : t.toLowerCase();
    const needle = matchCase ? query : query.toLowerCase();
    if (hay.length !== t.length) return [{ t, h: false }];
    const out: { t: string; h: boolean }[] = [];
    let from = 0;
    for (let i = hay.indexOf(needle); i !== -1; i = hay.indexOf(needle, i + needle.length)) {
      if (i > from) out.push({ t: t.slice(from, i), h: false });
      out.push({ t: t.slice(i, i + needle.length), h: true });
      from = i + needle.length;
    }
    if (from < t.length) out.push({ t: t.slice(from), h: false });
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
      PageDown: () => setTop(top + visible - 1),
      PageUp: () => setTop(top - visible + 1),
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
    viewport.focus();
    nav(0, 0).then(go);
    return () => {
      viewport.removeEventListener('wheel', onWheel);
      viewport.removeEventListener('keydown', onKey);
    };
  });
</script>

<svelte:window onkeydown={onGlobalKey} />

{#snippet withHits(t: string)}{#each hits(t) as p}{#if p.h}<span class="hit">{p.t}</span>{:else}{p.t}{/if}{/each}{/snippet}
{#snippet cellText(c: Cell)}{#each c.s as s}{#if s.h}<mark>{@render withHits(s.t)}</mark>{:else}{@render withHits(s.t)}{/if}{/each}{/snippet}

<div class="view" bind:clientHeight={height} bind:clientWidth={width} style:--fs="{fontSize}px" style:--rh="{rowH}px" style:--lnw="{lnW}px">
  <!-- The rows area is focusable so the arrow keys and Page Up/Down can scroll it. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="rows" id="rows" bind:this={viewport} tabindex="0" role="region" aria-label="Differences">
    {#if total === 0}
      <p class="empty">Both files are empty.</p>
    {/if}
    {#each rows as row, i (rowsTop + i)}
      {@const r = rowsTop + i}
      <div class="row k{row.k}" class:cur={current !== null && r >= current.row && r < current.row + current.rows} class:found={flashRow === r}>
        <span class="ln">{row.l ? row.l.n : ''}</span>
        <span class="tx" class:fill={!row.l}>{#if row.l}<span class="in" style:transform="translateX({-scrollX}px)">{@render cellText(row.l)}</span>{/if}</span>
        <span class="ln r">{row.r ? row.r.n : ''}</span>
        <span class="tx" class:fill={!row.r}>{#if row.r}<span class="in" style:transform="translateX({-scrollX}px)">{@render cellText(row.r)}</span>{/if}</span>
      </div>
    {/each}
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
    <div class="thumb" style:top="{thumbY}px" style:height="{thumbH}px"></div>
  </div>
</div>

<style>
  .view {
    flex: 1;
    min-height: 0;
    display: flex;
    position: relative;
  }
  .rows {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-family: 'Cascadia Mono', Consolas, ui-monospace, monospace;
    font-size: var(--fs);
    line-height: var(--rh);
  }
  .rows:focus {
    outline: none;
  }
  .row {
    display: grid;
    grid-template-columns: var(--lnw) 1fr var(--lnw) 1fr;
    height: var(--rh);
  }
  .ln {
    color: var(--ln-fg);
    background: var(--ln-bg);
    text-align: right;
    padding-right: 10px;
    font-size: 0.92em;
  }
  .ln.r {
    border-left: 1px solid var(--border);
  }
  .tx {
    overflow: hidden;
    padding-left: 10px;
    user-select: text;
  }
  .in {
    display: inline-block;
    white-space: pre;
    tab-size: 4;
  }
  .k1 .tx,
  .k1 .ln {
    background: var(--chg);
  }
  .k2 .tx:not(.fill),
  .k2 .ln.r {
    background: var(--add);
  }
  .k3 .tx:not(.fill),
  .k3 .ln:first-child {
    background: var(--del);
  }
  .k1 .ln,
  .k2 .ln,
  .k3 .ln {
    color: var(--ln-hot);
  }
  .tx.fill {
    background: repeating-linear-gradient(135deg, var(--fill-a) 0 6px, var(--fill-b) 6px 7px);
  }
  mark {
    background: var(--mark);
    color: inherit;
    border-radius: 2px;
  }
  .hit {
    background: var(--accent);
    color: var(--on-accent);
    border-radius: 2px;
  }
  .row.cur > .ln:first-child,
  .row.cur > .ln.r {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .row.found {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .empty {
    color: var(--muted);
    text-align: center;
    margin-top: 60px;
    font-family: 'Segoe UI', system-ui, sans-serif;
  }
  .map {
    position: relative;
    width: 14px;
    flex: none;
    background: var(--map-bg);
    border-left: 1px solid var(--border);
    touch-action: none;
  }
  .map canvas {
    display: block;
  }
  .thumb {
    position: absolute;
    left: 0;
    right: 0;
    background: var(--thumb);
    border-radius: 2px;
    pointer-events: none;
  }
</style>
