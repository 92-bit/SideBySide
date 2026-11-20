<script lang="ts">
  import { t } from '#lib/i18n.svelte.ts';
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
      <span class="head"><span class="ic old" aria-hidden="true">{t("OLD")}</span>{t("Original")}</span>
      <textarea bind:this={first} bind:value={original} placeholder={t("Paste the original text here")} spellcheck="false"></textarea>
    </label>
    <label class="col">
      <span class="head"><span class="ic new" aria-hidden="true">{t("NEW")}</span>{t("New")}</span>
      <textarea bind:value={changed} placeholder={t("Paste the new text here")} spellcheck="false"></textarea>
    </label>
  </div>
  <div class="actions">
    <button type="button" class="ghost" onclick={onBack}>{t("← Use files instead")}</button>
    <span class="grow">
      {#if error}<span class="error" role="alert">{error}</span>{/if}
    </span>
    <button type="button" class="primary" disabled={!original && !changed} onclick={() => onCompare(original, changed)}>{t("Compare")}</button>
  </div>
</section>

<style>
  .paste {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 14px 14px;
    animation: rise 0.35s cubic-bezier(0.23, 1, 0.32, 1) both;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  /* Each side is a card: a header with OLD/NEW, and the text box below. */
  .col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-radius: 14px;
    border: 1px solid var(--t-line);
    background: var(--t-card);
    overflow: hidden;
    transition: border-color 0.15s ease;
  }
  .col:focus-within {
    border-color: color-mix(in srgb, var(--t-acc) 55%, transparent);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: var(--t-card2);
    border-bottom: 1px solid var(--t-line);
    color: var(--t-strong);
    font-weight: 600;
    font-size: 13px;
  }
  .ic {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    font-size: 10px;
    font-weight: 700;
  }
  .ic.old {
    background: var(--t-del);
    color: var(--t-del-s);
  }
  .ic.new {
    background: var(--t-add);
    color: var(--t-add-s);
  }
  textarea {
    flex: 1;
    min-height: 0;
    resize: none;
    font: 13px/1.5 var(--font-code);
    color: var(--t-strong);
    background: transparent;
    border: 0;
    padding: 12px 14px;
    user-select: text;
    tab-size: 4;
  }
  textarea::placeholder {
    color: var(--t-faint);
  }
  textarea:focus {
    outline: none;
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
    color: var(--t-del-s);
  }
  button {
    font: inherit;
    height: 36px;
    border-radius: 10px;
    padding: 0 18px;
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      color 0.15s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  button:active:not(:disabled) {
    transform: scale(0.97);
  }
  .ghost {
    background: none;
    border: 0;
    color: var(--t-muted);
  }
  .ghost:hover {
    color: var(--t-strong);
    background: var(--t-line);
  }
  .primary {
    border: 0;
    background: linear-gradient(135deg, var(--t-acc), color-mix(in srgb, var(--t-acc) 60%, var(--t-acc2)));
    color: var(--t-on-acc);
    font-weight: 600;
    box-shadow: 0 10px 30px -12px var(--t-glow1);
  }
  .primary:disabled {
    opacity: 0.45;
    cursor: default;
  }
  @media (prefers-reduced-motion: reduce) {
    .paste {
      animation: none;
    }
  }
</style>
