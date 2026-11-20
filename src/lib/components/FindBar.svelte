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
  /* The find / go-to-line bar: a slim rounded card above the file cards. It opens from the
     keyboard (Ctrl+F, Ctrl+G) a lot, so it appears at once, without animation. */
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 4px 14px 6px;
    padding: 6px 8px 6px 12px;
    border-radius: 12px;
    border: 1px solid var(--t-line2);
    background: var(--t-card2);
    font-size: 12.5px;
  }
  .ico {
    fill: none;
    stroke: var(--t-muted);
    stroke-width: 2;
    stroke-linecap: round;
  }
  input,
  select {
    font: inherit;
    height: 28px;
    color: var(--t-strong);
    background: var(--t-card);
    border: 1px solid var(--t-line2);
    border-radius: 8px;
    padding: 0 10px;
    user-select: text;
    transition: border-color 0.15s ease;
  }
  input {
    width: 260px;
  }
  input.num {
    width: 90px;
  }
  input::placeholder {
    color: var(--t-faint);
  }
  input:focus,
  select:focus {
    outline: none;
    border-color: var(--t-acc);
  }
  button {
    font: inherit;
    height: 28px;
    background: none;
    border: 1px solid var(--t-line2);
    color: var(--t-fg);
    border-radius: 8px;
    padding: 0 10px;
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  button:hover:not(:disabled) {
    background: var(--t-line);
  }
  button:active:not(:disabled) {
    transform: scale(0.96);
  }
  button:disabled {
    color: var(--t-faint);
    cursor: default;
  }
  .tg.on {
    background: var(--t-acc-bg);
    border-color: color-mix(in srgb, var(--t-acc) 45%, transparent);
    color: var(--t-acc);
  }
  .x {
    border-color: transparent;
    font-size: 15px;
    line-height: 1;
    width: 28px;
    padding: 0;
  }
  .lbl {
    color: var(--t-muted);
  }
  .status {
    color: var(--t-muted);
    margin-left: 4px;
  }
  .grow {
    flex: 1;
  }
</style>
