<script lang="ts">
  import { fileName, folderOf, timeAgo, type Recent } from '$lib/recent';

  let {
    items,
    limit = 10,
    onOpen,
    onRemove,
    onClear,
  }: {
    items: Recent[];
    limit?: number;
    onOpen: (r: Recent) => void;
    onRemove: (r: Recent) => void;
    onClear?: () => void;
  } = $props();
</script>

{#if items.length}
  <ul class="list">
    {#each items.slice(0, limit) as r (r.left + '|' + r.right)}
      <li>
        <button type="button" class="item" onclick={() => onOpen(r)} title={`${r.left}\n${r.right}`}>
          <span class="names">{fileName(r.left)} <span class="arrow">↔</span> {fileName(r.right)}</span>
          <span class="sub">{folderOf(r.left)} · {timeAgo(r.at)}</span>
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
    display: flex;
    align-items: stretch;
    border-radius: 6px;
  }
  li:hover {
    background: var(--hover);
  }
  .item {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    background: none;
    border: 0;
    padding: 6px 8px;
    text-align: left;
    cursor: pointer;
    color: var(--fg-strong);
  }
  .names,
  .sub {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .arrow {
    color: var(--muted);
  }
  .sub {
    font-size: 11px;
    color: var(--muted);
  }
  .rm {
    background: none;
    border: 0;
    color: var(--faint);
    width: 26px;
    font-size: 15px;
    cursor: pointer;
    border-radius: 6px;
    opacity: 0;
  }
  li:hover .rm,
  .rm:focus-visible {
    opacity: 1;
  }
  .rm:hover {
    color: var(--fg-strong);
  }
  .clear {
    margin-top: 8px;
    background: none;
    border: 0;
    color: var(--muted);
    font-size: 11px;
    cursor: pointer;
    padding: 2px 8px;
  }
  .clear:hover {
    color: var(--fg-strong);
    text-decoration: underline;
  }
  .none {
    margin: 0;
    color: var(--faint);
    font-size: 12px;
    padding: 4px 8px;
  }
</style>
