<script lang="ts">
  // The bar for "Find" (Ctrl+F) and "Go to line" (Ctrl+G).
  let {
    mode,
    query = $bindable(''),
    matchCase = $bindable(false),
    maxLeft,
    maxRight,
    onFind,
    onGoto,
    onClose,
  }: {
    mode: 'find' | 'goto';
    query?: string;
    matchCase?: boolean;
    maxLeft: number;
    maxRight: number;
    onFind: (forward: boolean) => Promise<boolean>;
    onGoto: (left: boolean, line: number) => Promise<boolean>;
    onClose: () => void;
  } = $props();

  let input = $state<HTMLInputElement>();
  let status = $state('');
  let searching = $state(false);
  let line = $state('');
  let side = $state<'left' | 'right'>('left');

  /** Put the cursor in the box (also called when Ctrl+F / Ctrl+G is pressed while the bar is open). */
  export function focus() {
    queueMicrotask(() => {
      input?.focus();
      input?.select();
    });
  }

  // Put the cursor in the box whenever the bar opens or switches mode.
  $effect(() => {
    void mode;
    focus();
  });

  async function doFind(forward: boolean) {
    if (!query || searching) return;
    searching = true;
    status = '';
    const ok = await onFind(forward);
    searching = false;
    status = ok ? '' : 'No matches';
  }

  async function doGoto() {
    // Allow "500,000", "500.000" and "500 000" (the app shows big numbers with commas).
    const digits = line.trim().replace(/[\s,.'’]/g, '');
    const n = /^\d+$/.test(digits) ? Number(digits) : NaN;
    const max = side === 'left' ? maxLeft : maxRight;
    if (!(n >= 1 && n <= max)) {
      status = `Type a number from 1 to ${max.toLocaleString('en-US')}`;
      return;
    }
    status = (await onGoto(side === 'left', n)) ? '' : 'Line not found';
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (mode === 'find') doFind(!e.shiftKey);
      else doGoto();
    }
  }
</script>

<div class="bar" role="search">
  {#if mode === 'find'}
    <svg class="ico" viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="M15.5 15.5 21 21" /></svg>
    <input bind:this={input} bind:value={query} onkeydown={onKey} oninput={() => (status = '')} placeholder="Find in both files" aria-label="Find" spellcheck="false" />
    <button type="button" class="tg" class:on={matchCase} aria-pressed={matchCase} onclick={() => (matchCase = !matchCase)} title="Match UPPER and lower case exactly">Aa</button>
    <button type="button" onclick={() => doFind(false)} disabled={!query || searching} title="Previous match (Shift+Enter)" aria-label="Previous match">↑</button>
    <button type="button" onclick={() => doFind(true)} disabled={!query || searching} title="Next match (Enter)" aria-label="Next match">↓</button>
  {:else}
    <span class="lbl">Go to line</span>
    <input bind:this={input} bind:value={line} onkeydown={onKey} oninput={() => (status = '')} class="num" inputmode="numeric" placeholder="Number" aria-label="Line number" />
    <span class="lbl">in the</span>
    <select bind:value={side} aria-label="Which file">
      <option value="left">Original file</option>
      <option value="right">New file</option>
    </select>
    <button type="button" onclick={doGoto}>Go</button>
  {/if}
  <span class="status" aria-live="polite">{searching ? 'Searching…' : status}</span>
  <span class="grow"></span>
  <button type="button" class="x" onclick={onClose} aria-label="Close" title="Close (Esc)">×</button>
</div>

<style>
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-raised);
    font-size: 12px;
  }
  .ico {
    fill: none;
    stroke: var(--muted);
    stroke-width: 2;
    stroke-linecap: round;
  }
  input,
  select {
    font: inherit;
    color: var(--fg-strong);
    background: var(--bg);
    border: 1px solid var(--border-2);
    border-radius: 5px;
    padding: 4px 8px;
    user-select: text;
  }
  input {
    width: 260px;
  }
  input.num {
    width: 90px;
  }
  input:focus,
  select:focus {
    outline: none;
    border-color: var(--accent);
  }
  button {
    font: inherit;
    background: none;
    border: 1px solid var(--border-2);
    color: var(--fg);
    border-radius: 5px;
    padding: 3px 9px;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--hover);
  }
  button:disabled {
    color: var(--faint);
    cursor: default;
  }
  .tg.on {
    background: var(--accent-bg);
    border-color: var(--accent-border);
    color: var(--accent-fg);
  }
  .x {
    border-color: transparent;
    font-size: 15px;
    line-height: 1;
    padding: 2px 7px;
  }
  .lbl {
    color: var(--muted);
  }
  .status {
    color: var(--muted);
    margin-left: 4px;
  }
  .grow {
    flex: 1;
  }
</style>
