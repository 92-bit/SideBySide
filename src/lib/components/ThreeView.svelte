<script lang="ts">
  import { t } from '#lib/i18n.svelte.ts';
  // Three files: "A | original | B". Each row is lined up on the original's lines. Changes made only
  // in A or only in B are tinted in that column; a change made in both the same way is green in
  // both; a conflict (both changed the same lines differently) is red across all three.
  import { onMount, tick } from 'svelte';
  import { fmtNum, threeNav, threeRows, type Hit, type Row3, type Summary3 } from '#lib/api.ts';
  import { colorLine, langFor, type Tok } from '#lib/syntax.ts';
  import { prefs, spacingFactor } from '#lib/prefs.svelte.ts';

  let {
    summary,
    fontSize = 13,
    onCurrent,
  }: { summary: Summary3; fontSize?: number; onCurrent: (hit: Hit | null) => void } = $props();

  let viewport: HTMLDivElement;
  let height = $state(0);
  let top = $state(0);
  let rows = $state<Row3[]>([]);
  let rowsTop = $state(0);
  let current = $state<Hit | null>(null);
  const rowH = $derived(Math.round(fontSize * spacingFactor()));
  const total = $derived(summary.totalRows);
  const visible = $derived(Math.max(1, Math.floor(height / rowH)));
  const maxTop = $derived(Math.max(0, total - visible));
  const lang = $derived(prefs.codeColors ? langFor(summary.original.name) : null);

  function setTop(t: number) {
    top = Math.max(0, Math.min(maxTop, Math.round(t)));
  }
  let req = 0;
  $effect(() => {
    const t = top;
    const id = ++req;
    threeRows(t, visible + 1).then((r) => {
      if (id === req) {
        rows = r;
        rowsTop = t;
      }
    });
  });
  $effect(() => {
    if (top > maxTop) top = maxTop;
  });

  async function go(hit: Hit | null) {
    if (!hit) return;
    current = hit;
    onCurrent(hit);
    if (hit.row < top || hit.row + Math.min(hit.rows, visible) > top + visible) setTop(hit.row - Math.min(3, Math.floor(visible / 4)));
  }
  export async function next() {
    await go(await threeNav(current ? current.row : top, current ? 1 : 0));
  }
  export async function prev() {
    await go(await threeNav(current ? current.row : top, -1));
  }

  let acc = 0;
  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const px = e.deltaMode === 1 ? e.deltaY * rowH : e.deltaY;
    acc += px;
    const n = Math.trunc(acc / rowH);
    if (n) {
      acc -= n * rowH;
      setTop(top + n);
    }
  }
  function onKey(e: KeyboardEvent) {
    const moves: Record<string, () => void> = {
      ArrowDown: () => setTop(top + 1),
      ArrowUp: () => setTop(top - 1),
      PageDown: () => setTop(top + visible - 1),
      PageUp: () => setTop(top - visible + 1),
      Home: () => setTop(0),
      End: () => setTop(maxTop),
    };
    if (!e.ctrlKey && moves[e.key]) {
      e.preventDefault();
      moves[e.key]();
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

  const colored = (t: string): { t: string; c: Tok }[] => colorLine(t, lang);
  const cur = (r: number) => current !== null && r >= current.row && r < current.row + current.rows;
  const KIND = $derived(['', t('Changed in A'), t('Changed in B'), t('Changed the same way in both'), t('Conflict: A and B changed this differently')]);

  onMount(() => {
    viewport.addEventListener('wheel', onWheel, { passive: false });
    viewport.addEventListener('keydown', onKey);
    viewport.focus();
    tick().then(() => threeNav(0, 0).then(go));
    return () => {
      viewport.removeEventListener('wheel', onWheel);
      viewport.removeEventListener('keydown', onKey);
    };
  });
</script>

<svelte:window onkeydown={onGlobalKey} />

{#snippet text(t: string)}{#each colored(t) as p}{#if p.c}<span class="c-{p.c}">{p.t}</span>{:else}{p.t}{/if}{/each}{/snippet}

<div class="three" style:--fs="{fontSize}px" style:--rh="{rowH}px">
  <div class="heads">
    {#each [['A', summary.a], ['Original', summary.original], ['B', summary.b]] as const as [label, info] (label)}
      <div class="fh" class:mid={label === 'Original'}>
        <span class="tag" class:o={label === 'Original'}>{label === 'Original' ? t('Original') : label}</span>
        <b title={info.path}>{info.name}</b>
        <small>{info.lines === 1 ? t('1 line') : t('{n} lines', { n: fmtNum(info.lines) })}</small>
      </div>
    {/each}
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="rows" bind:this={viewport} bind:clientHeight={height} tabindex="0" role="region" aria-label={t("Three-way comparison")}>
    {#each rows as row, i (rowsTop + i)}
      {@const r = rowsTop + i}
      <div class="row k{row.k}" class:cur={cur(r)} title={KIND[row.k] || undefined}>
        <span class="ln">{row.a?.n ?? ''}</span>
        <span class="tx a" class:fill={!row.a}>{#if row.a}{@render text(row.a.t)}{/if}</span>
        <span class="ln">{row.o?.n ?? ''}</span>
        <span class="tx o" class:fill={!row.o}>{#if row.o}{@render text(row.o.t)}{/if}</span>
        <span class="ln">{row.b?.n ?? ''}</span>
        <span class="tx b" class:fill={!row.b}>{#if row.b}{@render text(row.b.t)}{/if}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .three {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    margin: 4px 14px 12px;
    border-radius: 14px;
    background: var(--t-card);
    border: 1px solid var(--t-line);
    overflow: hidden;
  }
  .heads {
    flex: none;
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    border-bottom: 1px solid var(--t-line);
  }
  .fh {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 10px 14px;
    color: var(--t-strong);
    font-size: 13px;
  }
  .fh.mid {
    background: var(--t-card2);
    border-inline: 1px solid var(--t-line);
  }
  .fh b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fh small {
    margin-left: auto;
    color: var(--t-muted);
    white-space: nowrap;
  }
  .tag {
    flex: none;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    background: var(--t-acc-bg);
    color: var(--t-acc);
  }
  .tag.o {
    background: var(--t-line);
    color: var(--t-muted);
  }
  .rows {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    font-family: var(--font-code);
    font-size: var(--fs);
    line-height: var(--rh);
  }
  .rows:focus {
    outline: none;
  }
  .row {
    display: grid;
    grid-template-columns: 52px 1fr 52px 1fr 52px 1fr;
    height: var(--rh);
  }
  .row.cur {
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--t-acc) 60%, transparent), inset 0 -1px 0 color-mix(in srgb, var(--t-acc) 60%, transparent);
  }
  .ln {
    text-align: right;
    padding-right: 10px;
    color: var(--t-faint);
    font-size: 0.92em;
    font-variant-numeric: tabular-nums;
  }
  .tx {
    overflow: hidden;
    white-space: pre;
    tab-size: 4;
    padding-left: 6px;
    color: var(--t-fg);
    user-select: text;
  }
  .tx.o,
  .row > .ln:nth-child(3) {
    background: color-mix(in srgb, var(--t-card2) 70%, transparent);
  }
  .tx.fill {
    background: repeating-linear-gradient(135deg, transparent 0 6px, var(--t-stripe) 6px 7px);
  }
  /* Who changed what */
  .k1 .tx.a,
  .k1 .ln:nth-child(1) {
    background: var(--t-chg);
  }
  .k2 .tx.b,
  .k2 .ln:nth-child(5) {
    background: var(--t-chg);
  }
  .k3 .tx.a,
  .k3 .tx.b,
  .k3 .ln:nth-child(1),
  .k3 .ln:nth-child(5) {
    background: var(--t-add);
  }
  .k4 .tx,
  .k4 .ln {
    background: var(--t-del);
  }
  .k4 .ln:nth-child(1) {
    box-shadow: inset 2px 0 0 var(--t-del-s);
  }
  .k1 .ln:nth-child(1),
  .k2 .ln:nth-child(5) {
    box-shadow: inset 2px 0 0 var(--t-chg-s);
  }
  .k3 .ln:nth-child(1),
  .k3 .ln:nth-child(5) {
    box-shadow: inset 2px 0 0 var(--t-add-s);
  }
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
</style>
