<script lang="ts">
  // Compare two pieces of text without saving them as files first.
  let {
    onCompare,
    onBack,
    error = null,
  }: { onCompare: (original: string, changed: string) => void; onBack: () => void; error?: string | null } = $props();

  let original = $state('');
  let changed = $state('');
  let first = $state<HTMLTextAreaElement>();

  $effect(() => {
    first?.focus();
  });
</script>

<section class="paste">
  <div class="cols">
    <label class="col">
      <span class="tag old">Original</span>
      <textarea bind:this={first} bind:value={original} placeholder="Paste the original text here" spellcheck="false"></textarea>
    </label>
    <label class="col">
      <span class="tag new">New</span>
      <textarea bind:value={changed} placeholder="Paste the new text here" spellcheck="false"></textarea>
    </label>
  </div>
  <div class="actions">
    <button type="button" class="ghost" onclick={onBack}>← Use files instead</button>
    <span class="grow">
      {#if error}<span class="error" role="alert">{error}</span>{/if}
    </span>
    <button type="button" class="primary" disabled={!original && !changed} onclick={() => onCompare(original, changed)}>Compare</button>
  </div>
</section>

<style>
  .paste {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
  }
  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }
  .col .tag {
    align-self: flex-start;
  }
  textarea {
    flex: 1;
    min-height: 0;
    resize: none;
    font: 13px/1.5 'Cascadia Mono', Consolas, ui-monospace, monospace;
    color: var(--fg-strong);
    background: var(--bg-raised);
    border: 1px solid var(--border-2);
    border-radius: 8px;
    padding: 10px 12px;
    user-select: text;
    tab-size: 4;
  }
  textarea:focus {
    outline: none;
    border-color: var(--accent);
  }
  .actions {
    display: flex;
    align-items: center;
  }
  .grow {
    flex: 1;
    text-align: center;
    padding: 0 12px;
  }
  .error {
    font-size: 13px;
    color: var(--error);
  }
  button {
    font: inherit;
    border-radius: 6px;
    padding: 6px 16px;
    cursor: pointer;
  }
  .ghost {
    background: none;
    border: 1px solid transparent;
    color: var(--muted);
  }
  .ghost:hover {
    color: var(--fg-strong);
    background: var(--hover);
  }
  .primary {
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    color: var(--accent-fg);
    font-weight: 600;
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
