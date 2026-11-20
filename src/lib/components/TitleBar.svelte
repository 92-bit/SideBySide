<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { Look } from '$lib/theme';
  import ThemePanel from './ThemePanel.svelte';

  type HelpAction = 'check' | 'about' | 'site';

  let {
    look,
    onLook,
    onHelp,
  }: {
    look: Look;
    onLook: (look: Look, from: { x: number; y: number }) => void;
    onHelp: (action: HelpAction) => void;
  } = $props();

  let themeOpen = $state(false);
  let themeEl = $state<HTMLDivElement>();

  const win = getCurrentWindow();
  const menus = ['File', 'Edit', 'View', 'Compare'];
  let helpOpen = $state(false);
  let helpEl = $state<HTMLDivElement>();

  function pick(a: HelpAction) {
    helpOpen = false;
    onHelp(a);
  }
  function onWindowClick(e: MouseEvent) {
    if (helpOpen && helpEl && !helpEl.contains(e.target as Node)) helpOpen = false;
    if (themeOpen && themeEl && !themeEl.contains(e.target as Node)) themeOpen = false;
  }
  function onWindowKey(e: KeyboardEvent) {
    if (helpOpen && e.key === 'Escape') helpOpen = false;
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={onWindowKey} />

<header class="titlebar" data-tauri-drag-region>
  <span class="logo" aria-hidden="true">
    <svg viewBox="0 0 16 16" width="16" height="16">
      <rect x="1.5" y="2.5" width="5.5" height="11" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.3" />
      <rect x="9" y="2.5" width="5.5" height="11" rx="1.2" fill="currentColor" opacity=".9" />
    </svg>
  </span>

  <nav class="menu" aria-label="Menu">
    {#each menus as m (m)}
      <button type="button" class="menu-item" title="Coming soon">{m}</button>
    {/each}
    <div class="help" bind:this={helpEl}>
      <button type="button" class="menu-item" class:open={helpOpen} aria-haspopup="menu" aria-expanded={helpOpen} onclick={() => (helpOpen = !helpOpen)}>Help</button>
      {#if helpOpen}
        <div class="dropdown" role="menu">
          <button type="button" role="menuitem" onclick={() => pick('check')}>Check for updates…</button>
          <button type="button" role="menuitem" onclick={() => pick('site')}>Project page on GitHub</button>
          <hr />
          <button type="button" role="menuitem" onclick={() => pick('about')}>About SideBySide</button>
        </div>
      {/if}
    </div>
  </nav>

  <span class="title" data-tauri-drag-region>SideBySide by 92-bit</span>

  <div class="theme" bind:this={themeEl}>
    <button
      type="button"
      class="mode"
      class:open={themeOpen}
      aria-haspopup="dialog"
      aria-expanded={themeOpen}
      title="Theme and light/dark"
      onclick={() => (themeOpen = !themeOpen)}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
        <path
          d="M12 3a9 9 0 100 18c1.1 0 1.6-.8 1.6-1.6 0-.9-.7-1.3-.7-2.2 0-1 .8-1.7 1.8-1.7H17a4 4 0 004-4c0-4.7-4-8.5-9-8.5z"
        />
        <circle cx="7.5" cy="11" r="1" /><circle cx="10" cy="7" r="1" /><circle cx="14.5" cy="7" r="1" />
      </svg>
      Theme
    </button>
    {#if themeOpen}
      <ThemePanel {look} onPick={onLook} onClose={() => (themeOpen = false)} />
    {/if}
  </div>

  <div class="wc">
    <button type="button" aria-label="Minimize" onclick={() => win.minimize()}>
      <svg viewBox="0 0 10 10" width="10" height="10"><path d="M0 5h10" stroke="currentColor" /></svg>
    </button>
    <button type="button" aria-label="Maximize" onclick={() => win.toggleMaximize()}>
      <svg viewBox="0 0 10 10" width="10" height="10"><rect x=".5" y=".5" width="9" height="9" fill="none" stroke="currentColor" /></svg>
    </button>
    <button type="button" class="close" aria-label="Close" onclick={() => win.close()}>
      <svg viewBox="0 0 10 10" width="10" height="10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" /></svg>
    </button>
  </div>
</header>

<style>
  .titlebar {
    position: relative;
    height: 34px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 12px;
    background: var(--bg-chrome);
    color: var(--muted);
    font-size: 12px;
  }
  .logo {
    display: grid;
    color: var(--accent);
    margin-right: 4px;
  }
  .menu {
    display: flex;
    position: relative;
    z-index: 1;
  }
  .menu-item {
    background: none;
    border: 0;
    padding: 4px 8px;
    border-radius: 4px;
    color: var(--muted);
    cursor: default;
  }
  .menu-item:hover,
  .menu-item.open {
    background: var(--hover);
    color: var(--fg-strong);
  }
  .help {
    position: relative;
  }
  .dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 20;
    min-width: 210px;
    padding: 4px;
    background: var(--bg-raised);
    border: 1px solid var(--border-2);
    border-radius: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.3);
  }
  .dropdown button {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 5px;
    padding: 6px 10px;
    color: var(--fg);
    font-size: 12px;
    cursor: pointer;
  }
  .dropdown button:hover {
    background: var(--hover);
    color: var(--fg-strong);
  }
  .dropdown hr {
    border: 0;
    border-top: 1px solid var(--border);
    margin: 4px 2px;
  }
  .title {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--faint);
    pointer-events: none;
  }
  .theme {
    position: relative;
    z-index: 2;
    margin-left: auto;
  }
  .mode {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: 1px solid var(--border-2);
    color: var(--fg-strong);
    border-radius: 5px;
    padding: 2px 10px;
    cursor: pointer;
    transition: transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .mode svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .mode:hover,
  .mode.open {
    background: var(--hover);
  }
  .mode:active {
    transform: scale(0.97);
  }
  .wc {
    position: relative;
    z-index: 1;
    display: flex;
    align-self: stretch;
    margin-left: 8px;
  }
  .wc button {
    width: 46px;
    display: grid;
    place-items: center;
    background: none;
    border: 0;
    color: var(--fg);
    cursor: default;
  }
  .wc button:hover {
    background: var(--hover);
  }
  .wc .close:hover {
    background: var(--close-hover);
    color: #fff;
  }
</style>
