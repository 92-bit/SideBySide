<script lang="ts">
  // "Lines…" on a file card: compare only some lines of that file (from … to …).
  import { trapFocus } from '#lib/trapFocus.ts';
  import { fmtNum } from '#lib/api.ts';

  let {
    side,
    lines,
    range,
    onApply,
    onClose,
  }: {
    side: 'left' | 'right';
    /** How many lines the whole file has (as far as it is known). */
    lines: number;
    range: [number, number] | null;
    /** null = the whole file */
    onApply: (range: [number, number] | null) => void;
    onClose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let from = $state(range ? range[0] : 1);
  // svelte-ignore state_referenced_locally
  let to = $state(range ? range[1] : lines);
  const valid = $derived(Number.isInteger(from) && Number.isInteger(to) && from >= 1 && to >= from);

  function apply(e: SubmitEvent) {
    e.preventDefault();
    if (valid) onApply([from, to]);
  }
</script>

<form class="panel" class:right={side === 'right'} aria-label="Compare only some lines" tabindex="-1" use:trapFocus={{ onEscape: onClose }} onsubmit={apply}>
  <h3>Compare only some lines</h3>
  <p class="hint">Of the {side === 'left' ? 'original' : 'new'} file{lines ? ` (${fmtNum(lines)} lines)` : ''}. The line numbers stay the same as in the file.</p>
  <div class="fields">
    <label>From line <input type="number" min="1" step="1" bind:value={from} /></label>
    <label>to <input type="number" min="1" step="1" bind:value={to} /></label>
  </div>
  {#if !valid}
    <p class="bad">"From" must be 1 or more, and "to" can't be before it.</p>
  {/if}
  <div class="actions">
    {#if range}
      <button type="button" class="ghost" onclick={() => onApply(null)}>Whole file</button>
    {/if}
    <span class="grow"></span>
    <button type="submit" class="primary" disabled={!valid}>Compare these lines</button>
  </div>
</form>

<style>
  .panel {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    z-index: 30;
    width: 320px;
    padding: 14px;
    border-radius: 14px;
    background: var(--t-card2);
    border: 1px solid var(--t-line2);
    box-shadow: 0 24px 60px -18px rgba(0, 0, 0, 0.55);
    color: var(--t-fg);
    transform-origin: calc(100% - 40px) 0;
    animation: open 0.18s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .panel:focus {
    outline: none;
  }
  @keyframes open {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(-4px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .panel {
      animation: none;
    }
  }
  h3 {
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--t-strong);
  }
  .hint {
    margin: 0 0 12px;
    font-size: 12px;
    line-height: 1.45;
    color: var(--t-muted);
  }
  .fields {
    display: flex;
    gap: 10px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
  }
  input {
    width: 92px;
    height: 30px;
    padding: 0 8px;
    border-radius: 8px;
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-strong);
    font: 12.5px var(--font-code);
    user-select: text;
  }
  input:focus {
    outline: 2px solid var(--t-acc);
    outline-offset: -1px;
  }
  .bad {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--t-del-s);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
  }
  .grow {
    flex: 1;
  }
  button {
    font: inherit;
    font-size: 12.5px;
    height: 30px;
    padding: 0 12px;
    border-radius: 8px;
    cursor: pointer;
  }
  button:active:not(:disabled) {
    transform: scale(0.97);
  }
  .ghost {
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-fg);
  }
  .ghost:hover {
    background: var(--t-line);
  }
  .primary {
    border: 0;
    background: linear-gradient(135deg, var(--t-acc), color-mix(in srgb, var(--t-acc) 70%, var(--t-acc2)));
    color: var(--t-on-acc);
    font-weight: 600;
  }
  .primary:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
