<script lang="ts">
  import { dismissHover } from "./hover-controller";
  import { tick, type Snippet } from "svelte";

  let { title, trigger, children, onOpenChange }: {
    title: string;
    trigger: Snippet<[() => void, boolean]>;
    children: Snippet<[() => void]>;
    onOpenChange?: (open: boolean) => void;
  } = $props();
  let open = $state(false);
  let anchor = $state<HTMLSpanElement>();
  let panel = $state<HTMLDivElement>();
  let left = $state(0);
  let top = $state(0);
  let placement = $state<"below" | "above">("below");
  let rafId = 0;

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
  function position(initial = false) {
    if (!anchor || !panel) return;
    const pad = 8;
    const rect = anchor.getBoundingClientRect();
    const width = panel.offsetWidth;
    const height = panel.offsetHeight;
    const maxLeft = Math.max(pad, window.innerWidth - width - pad);
    const maxTop = Math.max(pad, window.innerHeight - height - pad);

    if (initial) {
      const fitsBelow = rect.bottom + 6 + height <= window.innerHeight - pad;
      const fitsAbove = rect.top - 6 - height >= pad;
      placement = !fitsBelow && fitsAbove ? "above" : "below";
    }

    const rawTop = placement === "above" ? rect.top - height - 6 : rect.bottom + 6;
    const nextLeft = Math.round(Math.max(pad, Math.min(rect.left, maxLeft)));
    const nextTop = Math.round(Math.max(pad, Math.min(rawTop, maxTop)));
    if (Math.abs(nextLeft - left) >= 1) left = nextLeft;
    if (Math.abs(nextTop - top) >= 1) top = nextTop;
  }
  function schedulePosition() {
    if (rafId) return;
    rafId = requestAnimationFrame(() => {
      rafId = 0;
      position(false);
    });
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
      position(true);
      panel?.focus({ preventScroll: true });
    });
    function outside(event: PointerEvent) {
      const target = event.target as Element;
      if (panel?.contains(target) || anchor?.contains(target)) return;
      // 原型选择器虽挂载在 DOM 其他位置，仍属于当前二级菜单的子交互。
      if (target.closest('[aria-modal="true"], .backdrop')) return;
      close();
    }
    function keydown(event: KeyboardEvent) {
      if (event.key !== "Escape" || (event.target as Element)?.closest('[aria-modal="true"]')) return;
      event.preventDefault();
      close(true);
    }
    function onScroll(event: Event) {
      // 二级菜单自身内部滚动不重算定位，避免内容滚动引起外框抖动；
      // 外部容器滚动时只平稳钳制到视口安全边距内，不自动关闭。
      if (panel && event.target instanceof Node && panel.contains(event.target)) return;
      schedulePosition();
    }
    const observer = new ResizeObserver(() => schedulePosition());
    tick().then(() => { if (alive && panel) observer.observe(panel); });
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", keydown);
    window.addEventListener("resize", schedulePosition);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      alive = false;
      if (rafId) {
        cancelAnimationFrame(rafId);
        rafId = 0;
      }
      observer.disconnect();
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", keydown);
      window.removeEventListener("resize", schedulePosition);
      window.removeEventListener("scroll", onScroll, true);
    };
  });
</script>

<span class="menu-anchor" bind:this={anchor}>
  {@render trigger(() => { dismissHover(); open = !open; }, open)}
</span>
{#if open}
  <div use:portal bind:this={panel} class="secondary-menu" role="dialog" aria-label={title}
    tabindex="-1" style:left={`${left}px`} style:top={`${top}px`}>
    <header><strong>{title}</strong><button class="icon-btn" aria-label="关闭" onclick={() => close(true)}>×</button></header>
    <div class="menu-content">{@render children(() => close(true))}</div>
  </div>
{/if}

<style>
  .menu-anchor { display: inline-flex; min-width: 0; max-width: 100%; }
  .secondary-menu {
    position: fixed;
    z-index: 30;
    width: min(440px, calc(100vw - 16px));
    max-height: calc(100dvh - 16px);
    overflow: auto;
    color: var(--text);
    background: var(--surface-glass, var(--panel));
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 12px;
    background: var(--panel-elev);
    border-bottom: 1px solid var(--line);
  }
  header strong {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .menu-content { padding: 12px; }
</style>
