<script lang="ts">
  // Asks before installing, then installs and reopens SideBySide from its new home.
  import { installApp, openInstalled, type InstallInfo } from '$lib/api';

  let { info, onClose }: { info: InstallInfo; onClose: () => void } = $props();

  let contextMenu = $state(true);
  let desktop = $state(false);
  let step = $state<'ask' | 'working' | 'done'>('ask');
  let error = $state<string | null>(null);
  let dialog = $state<HTMLDivElement>();

  $effect(() => {
    dialog?.focus();
  });

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

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && step !== 'working') onClose();
  }
</script>

<div class="backdrop" role="presentation" onkeydown={onKey}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="install-title" tabindex="-1" bind:this={dialog}>
    {#if step === 'done'}
      <h2 id="install-title">✓ SideBySide is installed</h2>
      <p>You'll find it in your <b>Start Menu</b>. This window will now close and SideBySide will open from its new home.</p>
      <p class="note">You can delete the file you downloaded — it's no longer needed.</p>
      <div class="actions">
        <span class="grow"></span>
        <button type="button" class="primary" onclick={() => openInstalled()}>Open installed SideBySide</button>
      </div>
    {:else}
      <h2 id="install-title">{info.installed ? 'Update the installed SideBySide?' : 'Install SideBySide?'}</h2>
      <p>
        SideBySide works fine without installing. Installing just makes it easier to find. It's installed for you only, so no
        administrator password is needed.
      </p>
      <ul class="facts">
        <li>Adds SideBySide to your <b>Start Menu</b></li>
        <li>Adds it to <b>Settings → Apps</b>, so you can uninstall it any time</li>
      </ul>
      <label class="opt">
        <input type="checkbox" bind:checked={contextMenu} />
        <span>Add <b>"Compare with SideBySide"</b> when you right-click a file</span>
      </label>
      <label class="opt">
        <input type="checkbox" bind:checked={desktop} />
        <span>Put a shortcut on the desktop</span>
      </label>
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
      <div class="actions">
        <span class="path" title={info.installPath}>{info.installPath}</span>
        <span class="grow"></span>
        <button type="button" class="ghost" onclick={onClose} disabled={step === 'working'}>Not now</button>
        <button type="button" class="primary" onclick={install} disabled={step === 'working'}>
          {step === 'working' ? 'Installing…' : info.installed ? 'Update' : 'Install'}
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
    background: rgba(0, 0, 0, 0.45);
    z-index: 10;
  }
  .dialog {
    width: min(520px, calc(100vw - 32px));
    background: var(--bg-raised);
    border: 1px solid var(--border-2);
    border-radius: 12px;
    padding: 22px 24px 18px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.35);
    color: var(--fg);
  }
  .dialog:focus {
    outline: none;
  }
  h2 {
    margin: 0 0 10px;
    font-size: 17px;
    color: var(--fg-strong);
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
    border-radius: 6px;
    padding: 6px 16px;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .ghost {
    background: none;
    border: 1px solid var(--border-2);
    color: var(--fg);
  }
  .ghost:hover:not(:disabled) {
    background: var(--hover);
  }
  .primary {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--on-accent);
    font-weight: 600;
  }
  .primary:hover:not(:disabled) {
    filter: brightness(1.08);
  }
</style>
