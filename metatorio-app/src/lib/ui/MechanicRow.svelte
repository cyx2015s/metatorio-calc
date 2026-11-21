<script lang="ts">
  import { type Snippet } from "svelte";
  import { observeRow } from "./row-visibility";
  let { mechanicId, virtualize = false, draggable = false, highlighted = false, children }: {
    mechanicId: number;
    virtualize?: boolean;
    draggable?: boolean;
    highlighted?: boolean;
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

<div
  data-mechanic-row={mechanicId}
  class="mechanic-row"
  class:has-handle={draggable}
  class:highlighted
  bind:this={node}
>
  {#if draggable}
    <span class="drag-handle" aria-hidden="true" title="拖拽排序">⋮⋮</span>
  {/if}
  <div class="row-content">
    {#if !virtualize || visible || editing || focused || highlighted}
      {@render children((active) => editing = active)}
    {:else}
      <div class="row-placeholder" aria-hidden="true"></div>
    {/if}
  </div>
</div>

<style>
  .mechanic-row {
    display: flex;
    align-items: stretch;
    height: var(--mechanic-row-height, 84px);
    min-height: var(--mechanic-row-height, 84px);
    flex: 0 0 auto;
    min-width: 0;
  }
  .row-content {
    flex: 1 1 0;
    min-width: 0;
    height: 100%;
  }
  .drag-handle {
    flex: 0 0 16px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--faint);
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-right: 0;
    border-radius: var(--radius) 0 0 var(--radius);
    font-size: 11px;
    letter-spacing: -1px;
    user-select: none;
    cursor: grab;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .mechanic-row:hover .drag-handle {
    color: var(--text);
    border-color: var(--line-strong);
    background: var(--card-hover);
  }
  .mechanic-row.has-handle .row-content :global(.mech-card),
  .mechanic-row.has-handle .row-placeholder {
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
  }
  .mechanic-row.highlighted .drag-handle {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-dim);
  }
  .mechanic-row.highlighted .row-content :global(.mech-card) {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 32%, transparent);
    background: var(--accent-dim);
  }
  .row-placeholder { height: 100%; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel); }
</style>
