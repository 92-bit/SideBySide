<script lang="ts">
  // Shows two files side by side. Only the rows on screen are ever loaded,
  // so this works the same for a 10-line file and a 10-million-line file.
  import { onMount, tick } from 'svelte';
  import { CHANGED, fetchRows, nav, overview, type Cell, type Hit, type Row, type Summary } from '$lib/api';
  import { themeColor } from '$lib/theme';

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
  // Changed / added / removed, in the current theme's colours. Read again when the theme changes.
  const mapColors = () => ['', themeColor('chg-s'), themeColor('add-s'), themeColor('del-s')];
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

  const rowH = $derived(Math.round(fontSize * 1.54));
  // Line-number column: wide enough for the biggest line number in either file.
  const digits = $derived(Math.max(3, String(Math.max(summary.left.lines, summary.right.lines)).length));
  const lnW = $derived(Math.round(charW * 0.92 * digits + 22));
  const total = $derived(summary.totalRows);
  const visible = $derived(Math.max(1, Math.floor(height / rowH)));
  const maxTop = $derived(Math.max(0, total - visible));
  const thumbH = $derived(height ? Math.max(28, Math.min(height, (height * visible) / Math.max(total, 1))) : 0);
  const thumbY = $derived(maxTop ? (top / maxTop) * (height - thumbH) : 0);
  // How far the text can scroll sideways. The widths are measured on screen, so tabs and
  // wide characters (Chinese, Japanese, emoji...) are counted at their real size.
  let longestPx = $state(0);
  let paneW = $state(0);
  $effect(() => {
    void rows, fontSize, width, lnW; // measure again when any of these change
    tick().then(() => {
      if (!viewport) return;
      let w = 0;
      for (const el of viewport.querySelectorAll<HTMLElement>('.in')) w = Math.max(w, el.offsetWidth);
      longestPx = w;
      const tx = viewport.querySelector<HTMLElement>('.tx');
      paneW = tx ? tx.clientWidth - 12 : 0; // minus the padding on the left
    });
  });
  const maxX = $derived(Math.max(0, longestPx - paneW + 24));
  // Shorter lines on screen, a wider window or a smaller zoom: don't stay scrolled past the end.
  $effect(() => {
    if (scrollX > maxX) scrollX = maxX;
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
    if (row < top || row >= top + visible) setTop(row - Math.floor(visible / 3));
    if (mark) flashRow = row;
  }
  export function getTop() {
    return top;
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
    return { y: a * rowH, h: (b - a) * rowH };
  });

  async function go(hit: Hit | null, animate = false) {
    if (!hit) return;
    const onScreen = hit.row >= top && hit.row + Math.min(hit.rows, visible) <= top + visible;
    lensSlide = animate && onScreen && current !== null;
    // Once it has slid, scrolling by hand moves the frame with the text again.
    if (lensSlide) setTimeout(() => (lensSlide = false), 320);
    if (animate && !onScreen) lensPop++; // after a jump, the frame glows in instead
    current = hit;
    flashRow = null;
    onCurrent(hit);
    if (!onScreen) setTop(hit.row - Math.min(3, Math.floor(visible / 4)));
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
      {@const starts = row.k !== 0 && (i === 0 ? r === 0 : rows[i - 1].k === 0) /* changed + added lines together are one difference */}
      {@const isCur = current !== null && r === current.row}
      <div class="row k{row.k}" class:cur={current !== null && r >= current.row && r < current.row + current.rows} class:found={flashRow === r}>
        <span class="ln">{row.l ? row.l.n : ''}</span>
        <span class="tx" class:fill={!row.l}>{#if row.l}<span class="in" style:transform="translateX({-scrollX}px)">{@render cellText(row.l)}</span>{/if}</span>
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
        <span class="ln r">{row.r ? row.r.n : ''}</span>
        <span class="tx" class:fill={!row.r}>{#if row.r}<span class="in" style:transform="translateX({-scrollX}px)">{@render cellText(row.r)}</span>{/if}</span>
      </div>
    {/each}
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
    font-family: 'Cascadia Mono', Consolas, ui-monospace, monospace;
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
    font: 600 10px/1 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif;
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
    font-family: 'Segoe UI', system-ui, sans-serif;
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
