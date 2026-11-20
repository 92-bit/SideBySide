<script lang="ts">
  import { t } from '#lib/i18n.svelte.ts';
  // The top bar: name on the left, the main buttons in the middle (the "island", passed in by the
  // page), and Install, Theme, Help and the window buttons on the right. Drag it to move the window.
  import type { Snippet } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { Look } from '#lib/theme.ts';
  import ThemePanel from './ThemePanel.svelte';

  type HelpAction = 'check' | 'about' | 'site' | 'keys' | 'git';

  let {
    look,
    onLook,
    onHelp,
    lead,
    island,
    actions,
  }: {
    look: Look;
    onLook: (look: Look, from: { x: number; y: number }) => void;
    onHelp: (action: HelpAction) => void;
    lead?: Snippet;
    island?: Snippet;
    actions?: Snippet;
  } = $props();

  const win = getCurrentWindow();
  let helpOpen = $state(false);
  let helpEl = $state<HTMLDivElement>();
  let themeOpen = $state(false);
  let themeEl = $state<HTMLDivElement>();

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

<header class="bar" data-tauri-drag-region>
  <div class="aura" aria-hidden="true"></div>

  <div class="brand" data-tauri-drag-region>
    <span class="logo" aria-hidden="true"></span>
    <span class="name" data-tauri-drag-region>{t("SideBySide")}</span>
    {#if lead}
      <span class="lead">{@render lead()}</span>
    {/if}
  </div>

  {#if island}
    <div class="island">{@render island()}</div>
  {/if}

  <div class="right" data-tauri-drag-region>
    {@render actions?.()}

    <div class="pop" bind:this={themeEl}>
      <button
        type="button"
        class="icon"
        class:open={themeOpen}
        aria-haspopup="dialog"
        aria-expanded={themeOpen}
        aria-label={t("Theme")}
        title={t("Theme and light/dark")}
        onclick={() => (themeOpen = !themeOpen)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M12 3a9 9 0 100 18c1.1 0 1.6-.8 1.6-1.6 0-.9-.7-1.3-.7-2.2 0-1 .8-1.7 1.8-1.7H17a4 4 0 004-4c0-4.7-4-8.5-9-8.5z"
          />
          <circle cx="7.5" cy="11" r="1" /><circle cx="10" cy="7" r="1" /><circle cx="14.5" cy="7" r="1" />
        </svg>
      </button>
      {#if themeOpen}
        <ThemePanel {look} onPick={onLook} onClose={() => (themeOpen = false)} />
      {/if}
    </div>

    <div class="pop" bind:this={helpEl}>
      <button
        type="button"
        class="icon"
        class:open={helpOpen}
        aria-haspopup="menu"
        aria-expanded={helpOpen}
        aria-label={t("Help")}
        title={t("Help")}
        onclick={() => (helpOpen = !helpOpen)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="12" r="9" /><path d="M9.5 9.5a2.5 2.5 0 114 2c-1 .6-1.5 1.2-1.5 2.2M12 17h.01" />
        </svg>
      </button>
      {#if helpOpen}
        <div class="dropdown" role="menu">
          <button type="button" role="menuitem" onclick={() => pick('check')}>{t("Check for updates…")}</button>
          <button type="button" role="menuitem" onclick={() => pick('keys')}>{t("Keyboard shortcuts")} <span class="hint">F1</span></button>
          <button type="button" role="menuitem" onclick={() => pick('git')}>{t("Use with Git…")}</button>
          <button type="button" role="menuitem" onclick={() => pick('site')}>{t("Project page on GitHub")}</button>
          <hr />
          <button type="button" role="menuitem" onclick={() => pick('about')}>{t("About SideBySide")}</button>
        </div>
      {/if}
    </div>

    <div class="wc">
      <button type="button" aria-label={t("Minimize")} onclick={() => win.minimize()}>
        <svg viewBox="0 0 10 10" width="10" height="10"><path d="M0 5h10" stroke="currentColor" /></svg>
      </button>
      <button type="button" aria-label={t("Maximize")} onclick={() => win.toggleMaximize()}>
        <svg viewBox="0 0 10 10" width="10" height="10"><rect x=".5" y=".5" width="9" height="9" fill="none" stroke="currentColor" /></svg>
      </button>
      <button type="button" class="close" aria-label={t("Close")} onclick={() => win.close()}>
        <svg viewBox="0 0 10 10" width="10" height="10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" /></svg>
      </button>
    </div>
  </div>
</header>

<style>
  .bar {
    position: relative;
    z-index: 5;
    height: 52px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    padding-left: 16px;
    background: var(--t-bg);
    color: var(--t-muted);
    font-size: 12.5px;
    container-type: inline-size;
    container-name: bar;
  }
  /* A slow, soft glow behind the bar, in the theme's colours. */
  .aura {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }
  .aura::before {
    content: '';
    position: absolute;
    inset: -260% -10% -40%;
    background:
      radial-gradient(40% 60% at 25% 45%, var(--t-glow1), transparent 70%),
      radial-gradient(35% 55% at 75% 40%, var(--t-glow2), transparent 70%);
    filter: blur(10px);
    animation: drift 18s ease-in-out infinite alternate;
  }
  @keyframes drift {
    from {
      transform: translate3d(-4%, 0, 0) scale(1);
    }
    to {
      transform: translate3d(5%, 4%, 0) scale(1.08);
    }
  }
  /* Both sides keep their content's width (never squeezed under the island). */
  .brand,
  .right {
    position: relative;
    flex: 1 0 auto;
    display: flex;
    align-items: center;
  }
  .brand {
    gap: 9px;
    color: var(--t-strong);
    font-weight: 600;
    font-size: 14px;
    letter-spacing: -0.01em;
    white-space: nowrap;
  }
  .logo {
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 6px;
    position: relative;
    background: linear-gradient(135deg, var(--t-acc), var(--t-acc2));
    box-shadow: 0 0 18px -2px var(--t-glow1);
  }
  .logo::after {
    content: '';
    position: absolute;
    inset: 4px 9px 4px 4px;
    border-radius: 2px;
    background: var(--t-bg);
    opacity: 0.85;
  }
  .lead {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: 6px;
    font-weight: 400;
    letter-spacing: 0;
  }
  .island {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    border-radius: 14px;
    background: var(--t-island);
    border: 1px solid var(--t-line2);
    backdrop-filter: blur(16px) saturate(140%);
    box-shadow: 0 10px 30px -12px rgba(0, 0, 0, 0.5);
  }
  .right {
    /* The right side never shrinks; when space is short the name and the island give way. */
    flex: 1 0 auto;
    justify-content: flex-end;
    gap: 4px;
    align-self: stretch;
  }
  .pop {
    position: relative;
  }
  .icon {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--t-muted);
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      color 0.15s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .icon:hover,
  .icon.open {
    background: var(--t-line);
    color: var(--t-strong);
  }
  .icon:active {
    transform: scale(0.95);
  }
  .icon svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .dropdown {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 30;
    min-width: 210px;
    padding: 4px;
    background: var(--t-card2);
    border: 1px solid var(--t-line2);
    border-radius: 10px;
    box-shadow: 0 16px 40px -12px rgba(0, 0, 0, 0.5);
    transform-origin: calc(100% - 15px) 0;
    animation: open 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  @keyframes open {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(-4px);
    }
  }
  .dropdown button {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 6px;
    padding: 7px 10px;
    color: var(--t-fg);
    font-size: 12.5px;
    cursor: pointer;
  }
  .dropdown button:hover {
    background: var(--t-line);
    color: var(--t-strong);
  }
  .dropdown .hint {
    float: right;
    margin-left: 16px;
    color: var(--t-muted);
    font-size: 11.5px;
  }
  .dropdown hr {
    border: 0;
    border-top: 1px solid var(--t-line);
    margin: 4px 2px;
  }
  .wc {
    display: flex;
    align-self: stretch;
    margin-left: 6px;
  }
  .wc button {
    width: 46px;
    display: grid;
    place-items: center;
    background: none;
    border: 0;
    color: var(--t-fg);
    cursor: default;
  }
  .wc button:hover {
    background: var(--t-line);
  }
  .wc .close:hover {
    background: var(--close-hover);
    color: #fff;
  }
  /* Narrow window: keep just the logo, so the island has room. */
  @container bar (max-width: 1280px) {
    .name {
      display: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .aura::before,
    .dropdown {
      animation: none;
    }
  }
</style>
