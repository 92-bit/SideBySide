<script lang="ts">
  import { t } from '#lib/i18n.svelte.ts';
  // Ctrl+L (or "Open a web link" on the start screen): download a file from the web and compare it.
  import { fetchUrl } from '#lib/api.ts';
  import { trapFocus } from '#lib/trapFocus.ts';

  let {
    onOpen,
    onClose,
  }: {
    /** The file was downloaded to `path`; side = which side it goes on. */
    onOpen: (side: 'left' | 'right', path: string) => void;
    onClose: () => void;
  } = $props();

  let url = $state('');
  let busy = $state<'left' | 'right' | null>(null);
  let error = $state('');
  const ok = $derived(/^https?:\/\/\S+$/i.test(url.trim()));

  async function get(side: 'left' | 'right') {
    if (!ok || busy) return;
    busy = side;
    error = '';
    try {
      onOpen(side, await fetchUrl(url.trim()));
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && !busy && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="url-title" tabindex="-1" use:trapFocus={{ onEscape: () => !busy && onClose() }}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      get('right');
    }}
  >
    <h2 id="url-title">{t("Open a file from a web link")}</h2>
    <p class="small">{t("For example a file on GitHub (\"Raw\" link) or on any web page. It's downloaded and compared like a file on your computer.")}</p>
    <!-- svelte-ignore a11y_autofocus -->
    <input type="url" bind:value={url} placeholder={t("https://…")} spellcheck="false" autofocus disabled={!!busy} aria-label={t("Web link")} />
    {#if error}
      <p class="warn">{error}</p>
    {/if}
    <div class="actions">
      <button type="button" class="ghost" onclick={onClose} disabled={!!busy}>{t("Cancel")}</button>
      <span class="grow"></span>
      <button type="button" class="ghost" disabled={!ok || !!busy} onclick={() => get('left')}>{busy === 'left' ? t('Downloading…') : t('Use as the original')}</button>
      <button type="submit" class="primary" disabled={!ok || !!busy}>{busy === 'right' ? t('Downloading…') : t('Use as the new version')}</button>
    </div>
  </form>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--t-bg) 60%, transparent);
    backdrop-filter: blur(4px);
    z-index: 40;
    animation: fade 0.2s ease both;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .dialog {
    width: min(560px, calc(100vw - 32px));
    background: var(--t-card2);
    border: 1px solid var(--t-line2);
    border-radius: 18px;
    padding: 22px 24px 18px;
    box-shadow: 0 24px 60px -18px rgba(0, 0, 0, 0.55);
    color: var(--t-fg);
    animation: pop 0.22s cubic-bezier(0.23, 1, 0.32, 1) both;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.96);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .backdrop,
    .dialog {
      animation: none;
    }
  }
  .dialog:focus {
    outline: none;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 18px;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--t-strong);
  }
  .small {
    margin: 0 0 12px;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--t-muted);
  }
  .warn {
    margin: 10px 0 0;
    color: var(--t-del-s);
    font-size: 12.5px;
  }
  input {
    width: 100%;
    height: 38px;
    padding: 0 12px;
    border-radius: 10px;
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-strong);
    font: 13px var(--font-code);
    user-select: text;
  }
  input:focus {
    outline: 2px solid var(--t-acc);
    outline-offset: -1px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
  }
  button {
    font: inherit;
    height: 34px;
    padding: 0 14px;
    border-radius: 10px;
    cursor: pointer;
  }
  button:active:not(:disabled) {
    transform: scale(0.97);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ghost {
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-fg);
  }
  .ghost:hover:not(:disabled) {
    background: var(--t-line);
  }
  .primary {
    border: 0;
    background: linear-gradient(135deg, var(--t-acc), color-mix(in srgb, var(--t-acc) 70%, var(--t-acc2)));
    color: var(--t-on-acc);
    font-weight: 600;
  }
</style>
