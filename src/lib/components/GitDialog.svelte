<script lang="ts">
  import { t, tHtml, safe } from '#lib/i18n.svelte.ts';
  // Help → Use with Git…: make "git difftool" open changes in SideBySide.
  import { onMount } from 'svelte';
  import { gitSetup, type GitSetup } from '#lib/api.ts';
  import { trapFocus } from '#lib/trapFocus.ts';

  let { onClose }: { onClose: () => void } = $props();

  let info = $state<GitSetup | null>(null);
  let busy = $state(false);
  let done = $state(false);
  let error = $state('');
  let copied = $state(false);

  onMount(() => {
    gitSetup(false)
      .then((i) => (info = i))
      .catch((e) => (error = String(e)));
  });

  async function apply() {
    busy = true;
    error = '';
    try {
      info = await gitSetup(true);
      done = true;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  async function copy() {
    if (!info) return;
    await navigator.clipboard.writeText(info.commands.join('\n'));
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="git-title" tabindex="-1" use:trapFocus={{ onEscape: onClose }}>
    <h2 id="git-title">{t("Use SideBySide with Git")}</h2>
    <p>
      {@html tHtml('Git is a tool programmers use to keep track of changes to their files. After this setup, the command {cmd} shows each changed file in SideBySide, one after another.', { cmd: '<code>git difftool</code>' })}
    </p>
    {#if info && !info.gitFound}
      <p class="warn">{t("Git doesn't seem to be installed on this computer, so there's nothing to set up.")}</p>
      {#if info.problem}<p class="small">{t('(Windows said: {why})', { why: info.problem })}</p>{/if}
    {:else if info}
      <p class="small">{t("It changes three of Git's own settings for your user account (nothing else). These are the commands:")}</p>
      <pre>{info.commands.join('\n')}</pre>
      {#if done}
        <p class="ok">{@html tHtml('✓ Done. Try it: run {cmd} in a folder that Git keeps track of.', { cmd: '<code>git difftool</code>' })}</p>
      {:else if info.current === 'sidebyside'}
        <p class="ok">{t("✓ Git already uses SideBySide.")}</p>
      {:else if info.current}
        <p class="small">{@html tHtml("Git's compare tool is now {tool}; this would change it to SideBySide.", { tool: `<b>${safe(info.current)}</b>` })}</p>
      {/if}
      <p class="small">{t("Tip: install SideBySide first (the Install button), so these settings keep working if you move this copy.")}</p>
    {:else if !error}
      <p class="small">{t("Looking for Git…")}</p>
    {/if}
    {#if error}
      <p class="warn">{error}</p>
    {/if}
    <div class="actions">
      {#if info?.gitFound}
        <button type="button" class="ghost" onclick={copy}>{copied ? t('Copied') : t('Copy the commands')}</button>
      {/if}
      <span class="grow"></span>
      <button type="button" class="ghost" onclick={onClose}>{t("Close")}</button>
      {#if info?.gitFound && !done}
        <button type="button" class="primary" disabled={busy} onclick={apply}>{busy ? t('Setting up…') : t('Set it up for me')}</button>
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
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 80px);
    overflow-y: auto;
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
    margin: 0 0 10px;
    font-size: 18px;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--t-strong);
  }
  p {
    margin: 0 0 10px;
    line-height: 1.5;
  }
  .small {
    font-size: 12.5px;
    color: var(--t-muted);
  }
  .ok {
    color: var(--t-add-s);
  }
  .warn {
    color: var(--t-del-s);
  }
  .dialog :global(code) {
    font: 12px var(--font-code);
    padding: 1px 5px;
    border-radius: 5px;
    background: var(--t-line);
    color: var(--t-strong);
  }
  pre {
    margin: 0 0 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--t-card);
    border: 1px solid var(--t-line);
    font: 11.5px/1.6 var(--font-code);
    color: var(--t-strong);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
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
    height: 34px;
    padding: 0 14px;
    border-radius: 10px;
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
    opacity: 0.6;
    cursor: default;
  }
</style>
