<script lang="ts">
  // The theme picker: Dark / Light / Match Windows, and the colour themes with a small preview each.
  import { THEMES, type Palette } from '#lib/themes.ts';
  import { shadeOf, type Look, type Mode } from '#lib/theme.ts';
  import { trapFocus } from '#lib/trapFocus.ts';
  import {
    CODE_FONTS,
    SPACINGS,
    prefs,
    setCodeColors,
    setCodeFont,
    setFontSize,
    setSpacing,
    setWrap,
  } from '#lib/prefs.svelte.ts';
  import { FONT_DEFAULT, FONT_MAX, FONT_MIN } from '#lib/settings.ts';

  let {
    look,
    onPick,
    onClose,
  }: { look: Look; onPick: (look: Look, from: { x: number; y: number }) => void; onClose: () => void } = $props();

  const modes: { id: Mode; label: string }[] = [
    { id: 'dark', label: 'Dark' },
    { id: 'light', label: 'Light' },
    { id: 'auto', label: 'Match Windows' },
  ];
  const shade = $derived(shadeOf(look.mode));
  let panel = $state<HTMLDivElement>();

  const at = (e: MouseEvent) => ({ x: e.clientX, y: e.clientY });
  const vars = (p: Palette) =>
    `--bg:${p.bg};--card:${p.card};--line:${p.line};--line2:${p.line2};--acc:${p.acc};--glow:${p.glow1};` +
    `--chg:${p['chg-s']};--add:${p['add-s']};--del:${p['del-s']}`;

  // Arrow keys move between the themes (4 per row). Escape and Tab are handled by trapFocus.
  function onKey(e: KeyboardEvent) {
    const step = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: 4, ArrowUp: -4 }[e.key];
    if (!step || !panel) return;
    const items = [...panel.querySelectorAll<HTMLButtonElement>('.pt')];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    if (i < 0) return;
    e.preventDefault();
    items[Math.max(0, Math.min(items.length - 1, i + step))].focus();
  }

  $effect(() => {
    panel?.querySelector<HTMLButtonElement>('.pt[aria-pressed="true"]')?.focus();
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="panel"
  role="dialog"
  aria-label="Theme"
  tabindex="-1"
  bind:this={panel}
  onkeydown={onKey}
  use:trapFocus={{ onEscape: onClose }}
>
  <div class="head">
    <h3>Theme</h3>
    <div class="modes" role="group" aria-label="Light or dark">
      {#each modes as m (m.id)}
        <button type="button" aria-pressed={look.mode === m.id} onclick={(e) => onPick({ ...look, mode: m.id }, at(e))}>
          {m.label}
        </button>
      {/each}
    </div>
  </div>
  <div class="grid">
    {#each THEMES as t (t.id)}
      <button
        type="button"
        class="pt"
        aria-pressed={look.theme === t.id}
        title={t.blurb}
        onclick={(e) => onPick({ ...look, theme: t.id }, at(e))}
      >
        <span class="sw" style={vars(t[shade])} aria-hidden="true">
          <i><u></u><u class="c"></u><u class="d"></u><u></u></i>
          <i><u></u><u class="c"></u><u class="a"></u><u></u></i>
          <s></s>
        </span>
        <span class="name">{t.name}</span>
      </button>
    {/each}
  </div>
  <div class="text">
    <h3>Text in the files</h3>
    <div class="fonts" role="group" aria-label="Font">
      {#each CODE_FONTS as f (f.id)}
        <button type="button" class="fo" aria-pressed={prefs.codeFont === f.id} title="{f.name} ({f.note})" onclick={() => setCodeFont(f.id)}>
          <span class="sample" style:font-family={f.css} aria-hidden="true">{'{a: 0O}'}</span>
          <span class="fn">{f.name}</span>
        </button>
      {/each}
    </div>
    <div class="row">
      <span class="lbl" id="size-lbl">Size</span>
      <div class="size">
        <button type="button" class="step" aria-label="Smaller text" disabled={prefs.fontSize <= FONT_MIN} onclick={() => setFontSize(prefs.fontSize - 1)}>−</button>
        <input
          type="range"
          min={FONT_MIN}
          max={FONT_MAX}
          step="1"
          value={prefs.fontSize}
          aria-labelledby="size-lbl"
          aria-valuetext="{prefs.fontSize} pixels"
          oninput={(e) => setFontSize(Number(e.currentTarget.value))}
        />
        <button type="button" class="step" aria-label="Bigger text" disabled={prefs.fontSize >= FONT_MAX} onclick={() => setFontSize(prefs.fontSize + 1)}>+</button>
        <button type="button" class="val" title="Back to the normal size" onclick={() => setFontSize(FONT_DEFAULT)}>{Math.round((prefs.fontSize / FONT_DEFAULT) * 100)}%</button>
      </div>
    </div>
    <div class="row">
      <span class="lbl">Long lines</span>
      <div class="modes" role="group" aria-label="Long lines">
        <button type="button" aria-pressed={!prefs.wrap} onclick={() => setWrap(false)}>Scroll sideways</button>
        <button type="button" aria-pressed={prefs.wrap} onclick={() => setWrap(true)}>Wrap</button>
      </div>
    </div>
    <div class="row">
      <span class="lbl">Line spacing</span>
      <div class="modes" role="group" aria-label="Line spacing">
        {#each SPACINGS as sp (sp.id)}
          <button type="button" aria-pressed={prefs.spacing === sp.id} onclick={() => setSpacing(sp.id)}>{sp.name}</button>
        {/each}
      </div>
    </div>
  </div>
  <div class="foot">
    <button
      type="button"
      class="sw-row"
      role="switch"
      aria-checked={prefs.codeColors}
      onclick={() => setCodeColors(!prefs.codeColors)}
    >
      <span class="knob" aria-hidden="true"></span>
      Colour code in code files
    </button>
    <span class="saved">Saved for next time</span>
  </div>
</div>

<style>
  .panel {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 30;
    width: 400px;
    padding: 14px;
    border-radius: 14px;
    background: var(--t-card2);
    border: 1px solid var(--t-line2);
    box-shadow:
      0 24px 60px -18px rgba(0, 0, 0, 0.55),
      0 0 0 1px rgba(0, 0, 0, 0.08);
    color: var(--t-fg);
    cursor: default;
    /* Opens from the palette button (top right). */
    transform-origin: calc(100% - 20px) 0;
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
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }
  h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--t-strong);
  }
  .modes {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: var(--t-line);
  }
  .modes button {
    border: 0;
    background: none;
    height: 24px;
    padding: 0 9px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--t-muted);
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      color 0.15s ease;
  }
  .modes button:hover {
    color: var(--t-strong);
  }
  .modes button[aria-pressed='true'] {
    background: var(--t-card);
    color: var(--t-strong);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.18);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .pt {
    border: 0;
    background: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
    text-align: left;
    color: var(--t-fg);
    font-size: 11.5px;
    cursor: pointer;
  }
  .pt:focus-visible {
    outline: none;
  }
  .pt:focus-visible .sw {
    outline: 2px solid var(--t-acc);
    outline-offset: 2px;
  }
  .name {
    padding-left: 2px;
    white-space: nowrap;
  }
  .pt[aria-pressed='true'] .name {
    color: var(--t-strong);
    font-weight: 600;
  }
  /* The little preview: two panes with a few coloured lines, and the theme's accent dot. */
  .sw {
    position: relative;
    height: 58px;
    padding: 7px 6px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    overflow: hidden;
    border-radius: 8px;
    background: var(--bg);
    border: 1px solid var(--line2);
    transition:
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1),
      box-shadow 0.2s ease;
  }
  .sw::before {
    content: '';
    position: absolute;
    inset: -30% -20% auto 30%;
    height: 60%;
    background: radial-gradient(closest-side, var(--glow), transparent);
  }
  .pt:hover .sw {
    transform: translateY(-1px);
  }
  .pt:active .sw {
    transform: scale(0.96);
  }
  .pt[aria-pressed='true'] .sw {
    box-shadow:
      0 0 0 2px var(--t-card2),
      0 0 0 4px var(--t-acc);
  }
  .sw i {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 4px 3px;
    border-radius: 4px;
    background: var(--card);
    border: 1px solid var(--line);
  }
  .sw u {
    display: block;
    height: 3px;
    border-radius: 2px;
    background: var(--line2);
  }
  .sw u.c {
    background: var(--chg);
  }
  .sw u.a {
    background: var(--add);
  }
  .sw u.d {
    background: var(--del);
  }
  .sw s {
    position: absolute;
    right: 5px;
    top: 4px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--acc);
  }
  /* Text in the files: font, size and line spacing. */
  .text {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--t-line);
  }
  .text h3 {
    margin-bottom: 10px;
  }
  .fonts {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }
  .fo {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-width: 0;
    padding: 7px 8px 6px;
    border-radius: 8px;
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-fg);
    cursor: pointer;
    transition:
      border-color 0.15s ease,
      background-color 0.15s ease;
  }
  .fo:hover {
    border-color: color-mix(in srgb, var(--t-acc) 50%, var(--t-line2));
  }
  .fo:active {
    transform: scale(0.97);
  }
  .fo[aria-pressed='true'] {
    border-color: var(--t-acc);
    background: var(--t-acc-bg);
  }
  .sample {
    font-size: 14px;
    line-height: 1.2;
    color: var(--t-strong);
    white-space: nowrap;
  }
  .fn {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    color: var(--t-muted);
  }
  .fo[aria-pressed='true'] .fn {
    color: var(--t-strong);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: 10px;
  }
  .lbl {
    font-size: 12px;
    color: var(--t-fg);
  }
  .size {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .size input {
    width: 130px;
    accent-color: var(--t-acc);
  }
  .step,
  .val {
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-fg);
    border-radius: 6px;
    height: 24px;
    cursor: pointer;
    font-size: 12px;
  }
  .step {
    width: 24px;
    padding: 0;
  }
  .step:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .val {
    min-width: 46px;
    padding: 0 6px;
    font-variant-numeric: tabular-nums;
  }
  .step:not(:disabled):hover,
  .val:hover {
    background: var(--t-line);
    color: var(--t-strong);
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin: 12px 0 0;
    padding-top: 10px;
    border-top: 1px solid var(--t-line);
    color: var(--t-muted);
    font-size: 11.5px;
  }
  .sw-row {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    border: 0;
    background: none;
    padding: 2px 4px 2px 0;
    border-radius: 6px;
    color: var(--t-fg);
    font-size: 12px;
    cursor: pointer;
  }
  .sw-row:hover {
    color: var(--t-strong);
  }
  /* The on/off switch. */
  .knob {
    position: relative;
    width: 28px;
    height: 16px;
    flex: none;
    border-radius: 999px;
    background: var(--t-line2);
    transition: background-color 0.18s ease;
  }
  .knob::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--t-card);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
    transition: transform 0.18s cubic-bezier(0.23, 1, 0.32, 1);
  }
  .sw-row[aria-checked='true'] .knob {
    background: var(--t-acc);
  }
  .sw-row[aria-checked='true'] .knob::after {
    transform: translateX(12px);
  }
  .sw-row:active .knob::after {
    width: 14px;
  }
  .saved {
    white-space: nowrap;
  }
  @media (prefers-reduced-motion: reduce) {
    .panel {
      animation: none;
    }
    .knob,
    .knob::after {
      transition: none;
    }
  }
</style>
