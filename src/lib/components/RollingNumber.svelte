<script lang="ts">
  // A number that rolls up or down when it changes (like a counter on a meter).
  // dir: 1 = the new number comes from below, -1 = from above, 0 = no motion (keyboard, first show).
  import { quintOut } from 'svelte/easing';

  let { value, dir = 0 }: { value: string; dir?: number } = $props();

  const reduce = typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  function roll(_node: Element, { d }: { d: number }) {
    return {
      duration: d && !reduce ? 280 : 0,
      easing: quintOut,
      css: (t: number) => `transform: translateY(${(1 - t) * d * 100}%); opacity: ${t};`,
    };
  }
</script>

<span class="roll">
  {#key value}
    <span in:roll={{ d: dir }} out:roll={{ d: -dir }}>{value}</span>
  {/key}
</span>

<style>
  .roll {
    display: inline-grid;
    overflow: hidden;
    vertical-align: bottom;
  }
  .roll > span {
    grid-area: 1 / 1;
    text-align: center;
  }
</style>
