<script lang="ts">
  // Recent comparisons: a list (side panel) or small cards (start screen).
  import { fileName, folderOf, timeAgo, type Recent } from '#lib/recent.ts';

  let {
    items,
    limit = 10,
    cards = false,
    onOpen,
    onRemove,
    onClear,
  }: {
    items: Recent[];
    limit?: number;
    cards?: boolean;
    onOpen: (r: Recent) => void;
    onRemove: (r: Recent) => void;
    onClear?: () => void;
  } = $props();
</script>

{#if items.length}
  <ul class="list" class:cards>
    {#each items.slice(0, limit) as r (r.left + '|' + r.right)}
      <li>
        <button type="button" class="item" onclick={() => onOpen(r)} title={`${r.left}\n${r.right}`}>
          <span class="names">{fileName(r.left)} <span class="arrow">↔</span> {fileName(r.right)}</span>
          <!-- On the small cards the time comes first, so a long folder can't push it out of view. -->
          <span class="sub">{cards ? `${timeAgo(r.at)} · ${folderOf(r.left)}` : `${folderOf(r.left)} · ${timeAgo(r.at)}`}</span>
        </button>
        <button type="button" class="rm" aria-label="Remove from list" title="Remove from list" onclick={() => onRemove(r)}>×</button>
      </li>
    {/each}
  </ul>
  {#if onClear}
    <button type="button" class="clear" onclick={onClear}>Clear list</button>
  {/if}
{:else}
  <p class="none">No recent comparisons yet.</p>
{/if}

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  li {
    position: relative;
    display: flex;
    align-items: stretch;
    border-radius: 9px;
    transition:
      background-color 0.15s ease,
      border-color 0.15s ease,
      transform 0.16s cubic-bezier(0.23, 1, 0.32, 1);
  }
  li:hover {
    background: var(--t-line);
  }
  .item {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    background: none;
    border: 0;
    padding: 7px 8px;
    text-align: left;
    font: inherit;
    cursor: pointer;
    color: var(--t-strong);
  }
  .names,
  .sub {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .names {
    font-size: 12.5px;
    font-weight: 500;
  }
  .arrow {
    color: var(--t-muted);
  }
  .sub {
    font-size: 11.5px;
    color: var(--t-muted);
  }
  .rm {
    background: none;
    border: 0;
    color: var(--t-faint);
    width: 26px;
    font-size: 15px;
    cursor: pointer;
    border-radius: 6px;
    opacity: 0;
    transition: opacity 0.15s ease;
  }
  li:hover .rm,
  .rm:focus-visible {
    opacity: 1;
  }
  .rm:hover {
    color: var(--t-strong);
  }

  /* Start screen: small cards, three in a row */
  .list.cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 8px;
  }
  .cards li {
    background: color-mix(in srgb, var(--t-card) 75%, transparent);
    border: 1px solid var(--t-line);
    border-radius: 12px;
  }
  .cards li:hover {
    background: var(--t-card);
    border-color: var(--t-line2);
  }
  .cards li:active {
    transform: scale(0.98);
  }
  .cards .item {
    padding: 10px 4px 10px 12px;
  }
  .cards .rm {
    width: 24px;
    align-self: flex-start;
    height: 24px;
    margin: 6px 6px 0 0;
  }

  .clear {
    margin-top: 8px;
    background: none;
    border: 0;
    color: var(--t-muted);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
    padding: 2px 8px;
  }
  .clear:hover {
    color: var(--t-strong);
    text-decoration: underline;
  }
  .none {
    margin: 0;
    color: var(--t-faint);
    font-size: 12px;
    padding: 4px 8px;
  }
</style>
