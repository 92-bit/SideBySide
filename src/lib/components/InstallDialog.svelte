<script lang="ts">
  import { t, tHtml } from '#lib/i18n.svelte.ts';
  // Asks before installing, then installs and reopens SideBySide from its new home.
  import { installApp, openInstalled, type InstallInfo } from '#lib/api.ts';
  import { trapFocus } from '#lib/trapFocus.ts';

  let { info, onClose }: { info: InstallInfo; onClose: () => void } = $props();

  let contextMenu = $state(true);
  let desktop = $state(false);
  let step = $state<'ask' | 'working' | 'done'>('ask');
  let error = $state<string | null>(null);

  async function install() {
    step = 'working';
    error = null;
    try {
      await installApp(contextMenu, desktop);
      step = 'done';
    } catch (e) {
      error = String(e);
      step = 'ask';
    }
  }

  function onEscape() {
    if (step !== 'working') onClose();
  }
</script>

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="install-title" tabindex="-1" use:trapFocus={{ onEscape }}>
    {#if step === 'done'}
      <h2 id="install-title">{t("✓ SideBySide is installed")}</h2>
      <p>{@html tHtml("You'll find it in your {menu}. This window will now close and SideBySide will open from its new home.", { menu: `<b>${t('Start Menu')}</b>` })}</p>
      <p class="note">{t("You can delete the file you downloaded — it's no longer needed.")}</p>
      <div class="actions">
        <span class="grow"></span>
        <button type="button" class="primary" onclick={() => openInstalled()}>{t("Open installed SideBySide")}</button>
      </div>
    {:else}
      <h2 id="install-title">{info.installed ? t('Update the installed SideBySide?') : t('Install SideBySide?')}</h2>
      <p>
        {t("SideBySide works fine without installing. Installing just makes it easier to find. It's installed for you only, so no administrator password is needed.")}
      </p>
      <ul class="facts">
        <li>{@html tHtml('Adds SideBySide to your {menu}', { menu: `<b>${t('Start Menu')}</b>` })}</li>
        <li>{@html tHtml('Adds it to {apps}, so you can uninstall it any time', { apps: `<b>${t('Settings → Apps')}</b>` })}</li>
      </ul>
      <label class="opt">
        <input type="checkbox" bind:checked={contextMenu} />
        <span>{@html tHtml('Add {entry} when you right-click a file', { entry: `<b>${t('"Compare with SideBySide"')}</b>` })}</span>
      </label>
      <label class="opt">
        <input type="checkbox" bind:checked={desktop} />
        <span>{t("Put a shortcut on the desktop")}</span>
      </label>
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
      <div class="actions">
        <span class="path" title={info.installPath}>{info.installPath}</span>
        <span class="grow"></span>
        <button type="button" class="ghost" onclick={onClose} disabled={step === 'working'}>{t("Not now")}</button>
        <button type="button" class="primary" onclick={install} disabled={step === 'working'}>
          {step === 'working' ? t('Installing…') : info.installed ? t('Update') : t('Install')}
        </button>
      </div>
    {/if}
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
    width: min(520px, calc(100vw - 32px));
    background: var(--t-card2);
    border: 1px solid var(--t-line2);
    border-radius: 18px;
    padding: 24px 24px 20px;
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
    margin: 0 0 10px;
    font-size: 18px;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--t-strong);
  }
  p {
    margin: 0 0 12px;
    line-height: 1.5;
  }
  .note {
    color: var(--muted);
    font-size: 12px;
  }
  .facts {
    margin: 0 0 14px;
    padding-left: 18px;
    line-height: 1.7;
  }
  .opt {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 0 0 8px;
    cursor: pointer;
    line-height: 1.4;
  }
  .opt input {
    margin-top: 2px;
    accent-color: var(--accent);
  }
  .error {
    color: var(--error);
    font-size: 12px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 18px;
  }
  .path {
    font-size: 11px;
    color: var(--faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    max-width: 230px;
  }
  .grow {
    flex: 1;
  }
  button {
    font: inherit;
    height: 36px;
    border-radius: 10px;
    padding: 0 18px;
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  button:active:not(:disabled) {
    transform: scale(0.97);
  }
  button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .ghost {
    background: none;
    border: 1px solid var(--t-line2);
    color: var(--t-fg);
  }
  .ghost:hover:not(:disabled) {
    background: var(--t-line);
  }
  .primary {
    border: 0;
    background: linear-gradient(135deg, var(--t-acc), color-mix(in srgb, var(--t-acc) 60%, var(--t-acc2)));
    color: var(--t-on-acc);
    font-weight: 600;
    box-shadow: 0 10px 30px -12px var(--t-glow1);
  }
</style>
