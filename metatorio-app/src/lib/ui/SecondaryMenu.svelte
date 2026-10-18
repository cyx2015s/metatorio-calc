<script lang="ts">
  import { tick, type Snippet } from "svelte";

  let { title, trigger, children, onOpenChange }: {
    title: string;
    trigger: Snippet<[() => void, boolean]>;
    children: Snippet;
    onOpenChange?: (open: boolean) => void;
  } = $props();
  let open = $state(false);
  let anchor = $state<HTMLSpanElement>();
  let panel = $state<HTMLDivElement>();
  let left = $state(0);
  let top = $state(0);

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
  function position() {
    if (!anchor || !panel) return;
    const rect = anchor.getBoundingClientRect();
    const width = panel.offsetWidth;
    const height = panel.offsetHeight;
    left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
    top = Math.max(8, Math.min(rect.bottom + 6, window.innerHeight - height - 8));
  }
  function close(restoreFocus = false) {
    open = false;
    if (restoreFocus) anchor?.querySelector<HTMLButtonElement>("button")?.focus();
  }
  $effect(() => {
    onOpenChange?.(open);
  });
  $effect(() => {
    if (!open) return;
    let alive = true;
    tick().then(() => {
      if (!alive) return;
      position();
      panel?.focus({ preventScroll: true });
    });
    function outside(event: PointerEvent) {
      const target = event.target as Element;
      if (panel?.contains(target) || anchor?.contains(target)) return;
      // A prototype selector is a child interaction, even though it lives elsewhere in the DOM.
      if (target.closest('[aria-modal="true"], .backdrop')) return;
      close();
    }
    function keydown(event: KeyboardEvent) {
      if (event.key !== "Escape" || (event.target as Element)?.closest('[aria-modal="true"]')) return;
      event.preventDefault();
      close(true);
    }
    const observer = new ResizeObserver(position);
    tick().then(() => { if (alive && panel) observer.observe(panel); });
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", keydown);
    window.addEventListener("resize", position);
    window.addEventListener("scroll", position, true);
    return () => {
      alive = false;
      observer.disconnect();
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", keydown);
      window.removeEventListener("resize", position);
      window.removeEventListener("scroll", position, true);
    };
  });
</script>

<span class="menu-anchor" bind:this={anchor}>
  {@render trigger(() => { open = !open; }, open)}
</span>
{#if open}
  <div use:portal bind:this={panel} class="secondary-menu" role="dialog" aria-label={title}
    tabindex="-1" style:left={`${left}px`} style:top={`${top}px`}>
    <header><strong>{title}</strong><button class="icon-btn" aria-label="关闭" onclick={() => close(true)}>×</button></header>
    <div class="menu-content">{@render children()}</div>
  </div>
{/if}

<style>
  .menu-anchor { display: inline-flex; min-width: 0; max-width: 100%; }
  .secondary-menu { position: fixed; z-index: 30; width: min(440px, calc(100vw - 16px));
    max-height: calc(100dvh - 16px); overflow: auto; color: var(--text); background: var(--panel);
    border: 1px solid var(--line-strong); border-radius: var(--radius); box-shadow: var(--shadow-strong); }
  header { display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 8px 10px; border-bottom: 1px solid var(--line); }
  header strong { font-size: 12px; }
  .menu-content { padding: 10px; }
</style>
