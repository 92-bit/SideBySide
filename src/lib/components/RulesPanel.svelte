<script lang="ts">
  // The "Rules" panel under the Ignore chips: which parts of lines don't count (times, IDs...).
  import { PRESETS, type RulesConfig } from '#lib/rules.ts';
  import { trapFocus } from '#lib/trapFocus.ts';

  let {
    config,
    on,
    onApply,
    onClose,
  }: {
    config: RulesConfig;
    /** Rules are in use for the comparison on screen. */
    on: boolean;
    onApply: (config: RulesConfig, on: boolean) => void;
    onClose: () => void;
  } = $props();

  // A copy to edit; nothing changes until "Compare with these rules".
  // svelte-ignore state_referenced_locally
  let picked = $state<string[]>([...config.presets]);
  // svelte-ignore state_referenced_locally
  let custom = $state(config.custom);

  function flip(id: string) {
    picked = picked.includes(id) ? picked.filter((p) => p !== id) : [...picked, id];
  }
  const nothing = $derived(!picked.length && !custom.trim());
</script>

<div class="panel" role="dialog" aria-label="Ignore rules" tabindex="-1" use:trapFocus={{ onEscape: onClose }}>
  <h3>Ignore parts of lines</h3>
  <p class="hint">Lines that only differ in these parts count as the same. Handy for logs, where every line starts with a time.</p>
  <ul>
    {#each PRESETS as p (p.id)}
      <li>
        <label>
          <input type="checkbox" checked={picked.includes(p.id)} onchange={() => flip(p.id)} />
          <span class="name">{p.name}</span>
          <code>{p.example}</code>
        </label>
      </li>
    {/each}
  </ul>
  <label class="own">
    <span>Your own rules <small>(one per line, as a regular expression)</small></span>
    <textarea bind:value={custom} rows="3" spellcheck="false" placeholder={'e.g.  request \\d+\n      session=[a-z0-9]+'}></textarea>
  </label>
  <div class="actions">
    {#if on}
      <button type="button" class="ghost" onclick={() => onApply({ presets: picked, custom }, false)}>Stop ignoring</button>
    {/if}
    <span class="grow"></span>
    <button type="button" class="primary" disabled={nothing} onclick={() => onApply({ presets: picked, custom }, true)}>
      Compare with these rules
    </button>
  </div>
</div>

<style>
  .panel {
    position: absolute;
    top: calc(100% + 10px);
    right: 0;
    z-index: 30;
    width: 380px;
    padding: 14px;
    border-radius: 14px;
    background: var(--t-card2);
    border: 1px solid var(--t-line2);
    box-shadow: 0 24px 60px -18px rgba(0, 0, 0, 0.55);
    color: var(--t-fg);
    text-align: left;
    cursor: default;
    transform-origin: calc(100% - 30px) 0;
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
    margin: 0 0 10px;
    font-size: 12px;
    line-height: 1.45;
    color: var(--t-muted);
    white-space: normal;
  }
  ul {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
    display: grid;
    gap: 2px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    border-radius: 8px;
    font-size: 12.5px;
    cursor: pointer;
  }
  li label:hover {
    background: var(--t-line);
  }
  input[type='checkbox'] {
    margin: 0;
    accent-color: var(--t-acc);
  }
  .name {
    flex: none;
  }
  code {
    margin-left: auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: 11px var(--font-code);
    color: var(--t-muted);
  }
  .own {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    padding: 0;
    cursor: default;
  }
  .own small {
    color: var(--t-muted);
    font-size: 11px;
  }
  textarea {
    resize: vertical;
    min-height: 54px;
    padding: 7px 9px;
    border-radius: 8px;
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-strong);
    font: 12px/1.45 var(--font-code);
    user-select: text;
  }
  textarea:focus {
    outline: 2px solid var(--t-acc);
    outline-offset: -1px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
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
