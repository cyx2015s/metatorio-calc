<script lang="ts">
  import { type Snippet } from "svelte";
  import { observeRow } from "./row-visibility";
  let { mechanicId, virtualize = false, children }: {
    mechanicId: number;
    virtualize?: boolean;
    children: Snippet<[(active: boolean) => void]>;
  } = $props();
  let node = $state<HTMLDivElement>();
  let visible = $state(false);
  let editing = $state(false);
  let focused = $state(false);
  $effect(() => {
    if (!virtualize || !node) return;
    return observeRow(node, (value) => visible = value);
  });
  $effect(() => {
    if (!node) return;
    const element = node;
    const focus = () => focused = true;
    const blur = () => queueMicrotask(() => focused = element.contains(document.activeElement));
    element.addEventListener("focusin", focus);
    element.addEventListener("focusout", blur);
    return () => { element.removeEventListener("focusin", focus); element.removeEventListener("focusout", blur); };
  });
</script>

<div data-mechanic-row={mechanicId} class="mechanic-row" bind:this={node}>
  {#if !virtualize || visible || editing || focused}
    {@render children((active) => editing = active)}
  {:else}
    <div class="row-placeholder" aria-hidden="true"></div>
  {/if}
</div>

<style>
  .mechanic-row { height: var(--mechanic-row-height); min-height: var(--mechanic-row-height); flex: 0 0 auto; min-width: 0; }
  .row-placeholder { height: 100%; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel); }
</style>
