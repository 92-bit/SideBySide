<script lang="ts">
  // Every keyboard shortcut in one small window (F1, or Help → Keyboard shortcuts).
  import { trapFocus } from '#lib/trapFocus.ts';

  let { onClose }: { onClose: () => void } = $props();

  // Each shortcut: one or more ways to press it ("or"), each a list of keys pressed together.
  type Keys = string[][];
  const groups: { title: string; items: [Keys, string][] }[] = [
    {
      title: 'Differences',
      items: [
        [[['F8'], ['Alt', '↓']], 'Next difference'],
        [[['F7'], ['Alt', '↑']], 'Previous difference'],
      ],
    },
    {
      title: 'Bookmarks',
      items: [
        [[['Ctrl', 'B']], 'Bookmark the line in the middle (or click a line number)'],
        [[['F2']], 'Next bookmark'],
        [[['Shift', 'F2']], 'Previous bookmark'],
      ],
    },
    {
      title: 'Editing',
      items: [
        [[['Double-click']], 'Edit a line (Enter keeps it, Esc cancels)'],
        [[['Ctrl', 'Z']], 'Undo'],
        [[['Ctrl', 'Y']], 'Redo'],
        [[['Ctrl', 'S']], 'Save the changes'],
      ],
    },
    {
      title: 'Find',
      items: [
        [[['Ctrl', 'F']], 'Find text'],
        [[['Enter']], 'Find the next match'],
        [[['Shift', 'Enter']], 'Find the previous match'],
        [[['Ctrl', 'G']], 'Go to a line number'],
        [[['Esc']], 'Close the find bar'],
      ],
    },
    {
      title: 'Moving around',
      items: [
        [[['↑'], ['↓']], 'Scroll one line'],
        [[['Page Up'], ['Page Down']], 'Scroll one screen'],
        [[['Home'], ['End']], 'Go to the start or the end'],
        [[['←'], ['→']], 'Scroll sideways'],
        [[['Shift', 'Wheel']], 'Scroll sideways with the mouse'],
      ],
    },
    {
      title: 'View and files',
      items: [
        [[['Ctrl', '+'], ['Ctrl', '−']], 'Bigger or smaller text'],
        [[['Ctrl', 'Wheel']], 'Bigger or smaller text with the mouse'],
        [[['Ctrl', '0']], 'Normal text size'],
        [[['Ctrl', 'O']], 'Choose a file'],
        [[['Ctrl', 'T']], 'New tab'],
        [[['Ctrl', 'W']], 'Close the tab'],
        [[['Ctrl', 'Tab']], 'Next tab'],
        [[['Ctrl', 'L']], 'Open a file from a web link'],
        [[['Ctrl', 'Shift', 'V']], 'Compare the original with what you copied'],
        [[['F1']], 'Show this list'],
      ],
    },
  ];
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="keys-title" tabindex="-1" use:trapFocus={{ onEscape: onClose }}>
    <h2 id="keys-title">Keyboard shortcuts</h2>
    <div class="groups">
      {#each groups as g (g.title)}
        <section>
          <h3>{g.title}</h3>
          <dl>
            {#each g.items as [ways, what] (what)}
              <dt>
                {#each ways as keys, w (w)}
                  {#if w > 0}<span class="or">or</span>{/if}
                  <span class="combo">
                    {#each keys as k, i (i)}{#if i > 0}<span class="plus" aria-hidden="true">+</span>{/if}<kbd>{k}</kbd>{/each}
                  </span>
                {/each}
              </dt>
              <dd>{what}</dd>
            {/each}
          </dl>
        </section>
      {/each}
    </div>
    <div class="actions">
      <button type="button" class="ghost" onclick={onClose}>Close</button>
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
    width: min(720px, calc(100vw - 32px));
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
    margin: 0 0 16px;
    font-size: 18px;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--t-strong);
  }
  /* Two balanced columns; the sections never split. */
  .groups {
    columns: 2;
    column-gap: 32px;
  }
  section {
    break-inside: avoid;
    margin-bottom: 16px;
  }
  @media (max-width: 700px) {
    .groups {
      columns: 1;
    }
  }
  h3 {
    margin: 0 0 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--t-muted);
  }
  dl {
    margin: 0;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 14px;
    align-items: center;
  }
  dt {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
  }
  dd {
    margin: 0;
    color: var(--t-fg);
  }
  .combo {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .plus,
  .or {
    color: var(--t-faint);
    font-size: 11px;
  }
  .or {
    margin: 0 2px;
    color: var(--t-muted);
  }
  /* Keys drawn as small key caps. */
  kbd {
    display: inline-block;
    min-width: 22px;
    padding: 1px 6px;
    border-radius: 6px;
    border: 1px solid var(--t-line2);
    border-bottom-width: 2px;
    background: var(--t-card);
    color: var(--t-strong);
    font: 500 11.5px/18px var(--font-ui);
    text-align: center;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 18px;
  }
  button {
    font: inherit;
    height: 36px;
    border-radius: 10px;
    padding: 0 18px;
    cursor: pointer;
    border: 1px solid var(--t-line2);
    background: var(--t-card);
    color: var(--t-fg);
    transition: background-color 0.15s ease;
  }
  button:hover {
    background: var(--t-line);
  }
  button:active {
    transform: scale(0.97);
  }
</style>
