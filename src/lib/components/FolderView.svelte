<script lang="ts">
  import { t } from '#lib/i18n.svelte.ts';
  // The result of comparing two folders: every file, with filters, and a click to compare a pair.
  // Only the rows on screen are drawn, so folders with many thousands of files stay quick.
  import { fmtNum, fmtSize, type FolderEntry, type FolderResult } from '#lib/api.ts';

  let {
    result,
    onOpen,
  }: {
    result: FolderResult;
    /** Compare this file of the two folders (in a new tab). */
    onOpen: (entry: FolderEntry) => void;
  } = $props();

  type Filter = 'changes' | 'all' | 0 | 1 | 2 | 3;
  let filter = $state<Filter>('changes');
  const shown = $derived(
    result.entries.filter((e) => (filter === 'all' ? true : filter === 'changes' ? e.kind !== 0 : e.kind === filter)),
  );

  const ROW = 32;
  let box = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let height = $state(400);
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 5));
  const last = $derived(Math.min(shown.length, Math.ceil((scrollTop + height) / ROW) + 5));

  const KIND = $derived([t('The same'), t('Different'), t('Only in the first folder'), t('Only in the second folder')]);
  const folderName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  const chips = $derived<{ f: Filter; label: string; n: number }[]>([
    { f: 'changes', label: t('All changes'), n: result.different + result.onlyLeft + result.onlyRight },
    { f: 1, label: t('Different'), n: result.different },
    { f: 2, label: t('Only in {folder}', { folder: folderName(result.left) }), n: result.onlyLeft },
    { f: 3, label: t('Only in {folder}', { folder: folderName(result.right) }), n: result.onlyRight },
    { f: 0, label: t('The same'), n: result.same },
    { f: 'all', label: t('Everything'), n: result.entries.length },
  ]);
</script>

<section class="folders">
  <header>
    <div class="names">
      <span class="tag old">{t("OLD")}</span><b title={result.left}>{folderName(result.left)}</b>
      <span class="vs">{t("vs")}</span>
      <span class="tag new">{t("NEW")}</span><b title={result.right}>{folderName(result.right)}</b>
    </div>
    <div class="chips" role="group" aria-label={t("Show")}>
      {#each chips as c (c.f)}
        <button type="button" class="chip k{c.f}" aria-pressed={filter === c.f} onclick={() => ((filter = c.f), box && (box.scrollTop = 0))}>
          {c.label} <span class="n">{fmtNum(c.n)}</span>
        </button>
      {/each}
    </div>
  </header>

  {#if shown.length === 0}
    <p class="none">{filter === 'changes' ? t('✓ The folders are the same: every file matches.') : t('No files here.')}</p>
  {:else}
    <div class="list" bind:this={box} bind:clientHeight={height} onscroll={() => (scrollTop = box?.scrollTop ?? 0)} role="list">
      <div class="inner" style:height="{shown.length * ROW}px">
        {#each shown.slice(first, last) as e, i (e.path)}
          <div class="row" role="listitem" style:transform="translateY({(first + i) * ROW}px)">
            <button type="button" class="file" disabled={e.kind !== 1} onclick={() => onOpen(e)} title={e.kind === 1 ? t('Compare this file side by side') : KIND[e.kind]}>
              <span class="dot k{e.kind}" aria-label={KIND[e.kind]}></span>
              <span class="path">{e.path}</span>
              <span class="what">{KIND[e.kind]}</span>
              <span class="size">{e.leftSize !== null ? fmtSize(e.leftSize) : '—'}</span>
              <span class="size">{e.rightSize !== null ? fmtSize(e.rightSize) : '—'}</span>
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</section>

<style>
  .folders {
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
  header {
    flex: none;
    padding: 14px 16px 12px;
    border-bottom: 1px solid var(--t-line);
  }
  .names {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
    font-size: 14px;
    color: var(--t-strong);
    min-width: 0;
  }
  .names b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .vs {
    color: var(--t-faint);
    font-size: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    height: 28px;
    padding: 0 10px;
    border-radius: 999px;
    border: 1px solid var(--t-line2);
    background: none;
    color: var(--t-muted);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    white-space: nowrap;
  }
  .chip:hover {
    color: var(--t-strong);
  }
  .chip[aria-pressed='true'] {
    background: var(--t-acc-bg);
    border-color: color-mix(in srgb, var(--t-acc) 45%, transparent);
    color: var(--t-acc);
  }
  .chip .n {
    margin-left: 4px;
    font-variant-numeric: tabular-nums;
    opacity: 0.8;
  }
  .none {
    margin: 40px auto;
    color: var(--t-muted);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    position: relative;
  }
  .inner {
    position: relative;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 32px;
    padding: 0 6px;
  }
  .file {
    width: 100%;
    height: 30px;
    display: grid;
    grid-template-columns: 12px 1fr auto 80px 80px;
    align-items: center;
    gap: 10px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--t-fg);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }
  .file:hover:not(:disabled) {
    background: var(--t-acc-bg);
    color: var(--t-strong);
  }
  .file:disabled {
    cursor: default;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: 12px;
  }
  .what {
    color: var(--t-muted);
    font-size: 11.5px;
    white-space: nowrap;
  }
  .size {
    text-align: right;
    color: var(--t-muted);
    font-variant-numeric: tabular-nums;
    font-size: 11.5px;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  .dot.k0 {
    background: var(--t-line2);
  }
  .dot.k1 {
    background: var(--t-chg-s);
  }
  .dot.k2 {
    background: var(--t-del-s);
  }
  .dot.k3 {
    background: var(--t-add-s);
  }
  @media (max-width: 860px) {
    .file {
      grid-template-columns: 12px 1fr 70px 70px;
    }
    .what {
      display: none;
    }
  }
</style>
