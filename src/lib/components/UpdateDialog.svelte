<script lang="ts">
  // "About SideBySide" and "Check for updates" in one small window.
  import { onMount } from 'svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { checkUpdate, installUpdate, PROJECT_URL, RELEASES_URL, type UpdateInfo } from '$lib/api';
  import { loadAutoUpdate, markUpdateChecked, saveAutoUpdate } from '$lib/settings';
  import { trapFocus } from '$lib/trapFocus';

  let {
    version,
    portable,
    known = null,
    onClose,
  }: { version: string; portable: boolean; known?: UpdateInfo | null; onClose: () => void } = $props();

  let step = $state<'checking' | 'result' | 'installing' | 'error'>('checking');
  let info = $state<UpdateInfo | null>(null);
  let error = $state('');
  let auto = $state(loadAutoUpdate());

  async function check() {
    step = 'checking';
    try {
      info = await checkUpdate();
      markUpdateChecked();
      step = 'result';
    } catch (e) {
      error = String(e);
      step = 'error';
    }
  }

  async function update() {
    step = 'installing';
    try {
      await installUpdate(); // SideBySide closes and reopens as the new version
    } catch (e) {
      error = String(e);
      step = 'error';
    }
  }

  function onEscape() {
    if (step !== 'installing') onClose();
  }

  onMount(() => {
    if (known) {
      info = known;
      step = 'result';
    } else {
      check();
    }
  });
</script>

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="about-title" tabindex="-1" use:trapFocus={{ onEscape }}>
    <div class="head">
      <img src="/icon.png" alt="" width="40" height="40" />
      <div>
        <h2 id="about-title">SideBySide <span class="by">by 92-bit</span></h2>
        <p class="ver">Version {version}{portable ? ' · portable' : ''}</p>
      </div>
    </div>
    <p class="small">
      Free and open source under the GPL-3.0 license.
      <button type="button" class="link" onclick={() => openUrl(PROJECT_URL)}>Project page on GitHub</button>
    </p>

    <section class="update" aria-live="polite">
      {#if step === 'checking'}
        <p class="muted">Checking for updates…</p>
      {:else if step === 'installing'}
        <p>Downloading the update and checking its signature… SideBySide will restart by itself.</p>
        <span class="bar"><span></span></span>
      {:else if step === 'error'}
        <p class="error">{error}</p>
      {:else if info && info.available}
        <p><b>Version {info.latest} is available.</b> You have {info.current}.</p>
        {#if info.notes}
          <div class="notes">{info.notes}</div>
        {/if}
      {:else}
        <p class="ok">✓ You have the newest version.</p>
      {/if}
    </section>

    <label class="opt">
      <input type="checkbox" bind:checked={auto} onchange={() => saveAutoUpdate(auto)} />
      <span>Check for updates automatically (once a day)</span>
    </label>

    <div class="actions">
      {#if step === 'error'}
        <button type="button" class="ghost" onclick={check}>Try again</button>
      {:else if step === 'result' && !info?.available}
        <button type="button" class="ghost" onclick={check}>Check again</button>
      {/if}
      <span class="grow"></span>
      <button type="button" class="ghost" onclick={onClose} disabled={step === 'installing'}>
        {info?.available && step === 'result' ? 'Later' : 'Close'}
      </button>
      {#if step === 'result' && info?.available}
        {#if info.canInstall}
          <button type="button" class="primary" onclick={update}>Update now</button>
        {:else}
          <button type="button" class="primary" onclick={() => openUrl(RELEASES_URL)}>Open download page</button>
        {/if}
      {/if}
    </div>
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
    width: min(480px, calc(100vw - 32px));
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
  .head {
    display: flex;
    gap: 14px;
    align-items: center;
    margin-bottom: 10px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--t-strong);
  }
  .by {
    font-weight: 400;
    color: var(--muted);
    font-size: 13px;
  }
  p {
    margin: 0 0 8px;
    line-height: 1.5;
  }
  .ver,
  .small,
  .muted {
    color: var(--muted);
  }
  .ver {
    margin: 2px 0 0;
    font-size: 12px;
  }
  .small {
    font-size: 12px;
  }
  .update {
    margin: 14px 0;
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--t-card);
    border: 1px solid var(--t-line);
    min-height: 48px;
  }
  .update p:last-child {
    margin-bottom: 0;
  }
  .ok {
    color: var(--ok);
  }
  .error {
    color: var(--error);
  }
  .notes {
    max-height: 150px;
    overflow-y: auto;
    white-space: pre-wrap;
    font-size: 12px;
    color: var(--fg);
    user-select: text;
    margin-top: 6px;
  }
  .bar {
    display: block;
    height: 4px;
    border-radius: 2px;
    background: var(--t-line);
    overflow: hidden;
    margin-top: 10px;
  }
  .bar span {
    display: block;
    width: 40%;
    height: 100%;
    border-radius: 2px;
    background: linear-gradient(90deg, var(--t-acc), var(--t-acc2));
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(250%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bar span {
      animation: none;
      width: 100%;
      opacity: 0.6;
    }
  }
  .opt {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 12px;
    cursor: pointer;
  }
  .opt input {
    accent-color: var(--accent);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
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
  .link {
    font: inherit;
    background: none;
    border: 0;
    padding: 0;
    color: var(--accent);
    text-decoration: underline;
    cursor: pointer;
  }
</style>
