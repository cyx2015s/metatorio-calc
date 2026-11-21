<script lang="ts">
  import { tick } from "svelte";
  import FlowRef from "./FlowRef.svelte";
  import { isEnergyFlow, presentFlow } from "$lib/dual-var";
  // 统一流展示 chip：机制卡/求解结果共用。
  // 图标用正常大小的 HoverIcon（悬停详情 + 品质角标），数量带 +- 号，
  // 能量流用功率文本（W/kW/MW）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { compactNumber } from "$lib/format";
  import type { DualVar, Location } from "$lib/runtime/types";
  import type { FlowTraceHandler, FlowTraceMode } from "$lib/mechanics/workspace";
  import { dismissHover } from "./hover-controller";

  let {
    flow,
    amount,
    scale = 1,
    size = 22,
    showName = false,
    solved = false,
    location,
    onTraceFlow,
  }: {
    flow: DualVar;
    /** 每秒流量（正 = 产出，负 = 消耗）。 */
    amount: number;
    /** 求解用量缩放（无求解结果时为 1）。 */
    scale?: number;
    /** 图标边长。 */
    size?: number;
    showName?: boolean;
    solved?: boolean;
    location?: Location;
    onTraceFlow?: FlowTraceHandler;
  } = $props();

  const ZERO_EPS = 1e-12;

  /** 项目时间刻度：秒/分/时。能量（W/kW/MW）始终按秒（功率），其它流按刻度缩放。 */
  const timeScale = $derived(runtime.selectedProject?.settings.time_scale ?? "seconds");
  const timeFactor = $derived(
    timeScale === "minutes" ? 60 : timeScale === "hours" ? 3600 : 1,
  );
  const timeUnit = $derived(
    timeScale === "minutes" ? "/min" : timeScale === "hours" ? "/h" : "/s",
  );

  /** 瓦特 → 功率文本（数值为 J/s，统一采用 compactNumber 保留小数量级）。 */
  function powerValue(watts: number): string {
    const abs = Math.abs(watts);
    if (abs >= 1e12) return `${compactNumber(watts / 1e12)} TW`;
    if (abs >= 1e9) return `${compactNumber(watts / 1e9)} GW`;
    if (abs >= 1e6) return `${compactNumber(watts / 1e6)} MW`;
    if (abs >= 1e3) return `${compactNumber(watts / 1e3)} kW`;
    return `${compactNumber(watts)} W`;
  }

  /** 数量文本：能量流用功率（带符号，按秒），否则按时间刻度缩放的紧凑数（带 +- 和单位）；零或极微量显示 0。 */
  function qtyText(flow: DualVar, amount: number, scale: number): string {
    if (isEnergyFlow(flow)) {
      const scaled = amount * scale;
      if (!Number.isFinite(scaled)) return "—";
      if (Math.abs(scaled) < ZERO_EPS) return "0";
      const prefix = scaled < 0 ? "-" : "+";
      return `${prefix}${powerValue(Math.abs(scaled))}`;
    }
    const scaled = amount * scale * timeFactor;
    if (!Number.isFinite(scaled)) return "—";
    if (Math.abs(scaled) < ZERO_EPS) return "0";
    const prefix = scaled < 0 ? "-" : "+";
    return `${prefix}${compactNumber(Math.abs(scaled))}${timeUnit}`;
  }

  const info = $derived(presentFlow(flow, (kind, name) => runtime.localizedName(kind, name)));
  const isZero = $derived(Math.abs(amount * scale) < ZERO_EPS);
  const formattedQty = $derived(qtyText(flow, amount, scale));
  const chipLabel = $derived(
    `${info.description} ${isZero ? "净零" : amount > 0 ? "产出" : "消耗"} ${formattedQty}`,
  );

  let menuOpen = $state(false);
  let chipEl = $state<HTMLButtonElement>();
  let menuEl = $state<HTMLDivElement>();
  let menuPos = $state({ left: 8, top: 8 });

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  function updateMenuPosition() {
    if (!chipEl || !menuEl) return;
    const rect = chipEl.getBoundingClientRect();
    const width = menuEl.offsetWidth || 168;
    const height = menuEl.offsetHeight || 124;
    const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
    const below = rect.bottom + 4 + height <= window.innerHeight - 8;
    const top = below
      ? rect.bottom + 4
      : Math.max(8, rect.top - height - 4);
    menuPos = { left, top };
  }

  function closeMenu(restoreFocus = false) {
    menuOpen = false;
    if (restoreFocus) chipEl?.focus();
  }

  function toggleMenu() {
    if (!onTraceFlow) return;
    dismissHover();
    menuOpen = !menuOpen;
  }

  function handleContextMenu(event: MouseEvent) {
    if (!onTraceFlow) return;
    event.preventDefault();
    dismissHover();
    menuOpen = false;
    onTraceFlow(flow, "suggest", location);
  }

  function trace(mode: FlowTraceMode) {
    menuOpen = false;
    onTraceFlow?.(flow, mode, location);
  }

  $effect(() => {
    if (!menuOpen) return;
    let alive = true;
    void tick().then(() => {
      if (!alive) return;
      updateMenuPosition();
      menuEl?.querySelector<HTMLButtonElement>("button")?.focus();
    });
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (menuEl?.contains(target) || chipEl?.contains(target)) return;
      closeMenu();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        closeMenu(true);
      }
    };
    const onScrollOrResize = () => updateMenuPosition();
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("resize", onScrollOrResize);
    window.addEventListener("scroll", onScrollOrResize, true);
    return () => {
      alive = false;
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("resize", onScrollOrResize);
      window.removeEventListener("scroll", onScrollOrResize, true);
    };
  });
</script>

{#if onTraceFlow}
  <button
    type="button"
    bind:this={chipEl}
    class="flow-chip interactive"
    class:out={amount > 0 && !isZero}
    class:zero={isZero}
    class:menu-open={menuOpen}
    aria-label={chipLabel}
    aria-haspopup="menu"
    aria-expanded={menuOpen}
    onclick={toggleMenu}
    oncontextmenu={handleContextMenu}
  >
    <FlowRef {flow} {size} {showName} showQualifiers={showName} nested flowRate={amount * scale} rateBasis={solved ? "solution" : "unit"} hint="左键打开快捷操作 · 右键查看建议" />
    <span class="mono">{formattedQty}</span>
  </button>
{:else}
  <span
    class="flow-chip"
    class:out={amount > 0 && !isZero}
    class:zero={isZero}
    aria-label={chipLabel}
  >
    <FlowRef {flow} {size} {showName} showQualifiers={showName} flowRate={amount * scale} rateBasis={solved ? "solution" : "unit"} />
    <span class="mono">{formattedQty}</span>
  </span>
{/if}

{#if menuOpen && onTraceFlow}
  <div
    use:portal
    bind:this={menuEl}
    class="flow-chip-menu"
    role="menu"
    tabindex="-1"
    aria-label={`${info.label} 快捷操作`}
    style:left={`${menuPos.left}px`}
    style:top={`${menuPos.top}px`}
    onpointerdown={(event) => event.stopPropagation()}
  >
    <button type="button" role="menuitem" onclick={() => trace("producers")}>筛产出</button>
    <button type="button" role="menuitem" onclick={() => trace("consumers")}>筛消耗</button>
    <button type="button" role="menuitem" onclick={() => trace("network")}>网络定位</button>
    <button type="button" role="menuitem" onclick={() => trace("suggest")}>查看建议</button>
  </div>
{/if}

<style>
  .flow-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 26px;
    padding: 1px 8px 1px 3px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font: inherit;
    font-size: 10.5px;
    white-space: nowrap;
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .flow-chip.interactive {
    cursor: pointer;
  }

  .flow-chip:hover,
  .flow-chip.menu-open {
    border-color: var(--muted);
    background: var(--card-hover);
  }

  .flow-chip.out {
    border-color: var(--ok-line);
    background: color-mix(in srgb, var(--ok-dim) 70%, var(--input-bg));
  }

  .flow-chip.out:hover,
  .flow-chip.out.menu-open {
    border-color: var(--ok);
  }

  .flow-chip.zero {
    opacity: 0.72;
  }

  .flow-chip .mono {
    color: var(--text);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .flow-chip.out .mono {
    color: var(--ok);
  }

  .flow-chip.zero .mono {
    color: var(--muted);
  }

  .flow-chip-menu {
    position: fixed;
    z-index: 60;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 3px;
    padding: 5px;
    min-width: 156px;
    color: var(--text);
    background: var(--surface-glass, var(--panel));
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-strong);
  }

  .flow-chip-menu button {
    min-height: 26px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: 3px;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 11px;
    text-align: center;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .flow-chip-menu button:hover,
  .flow-chip-menu button:focus-visible {
    background: var(--card-hover);
    border-color: var(--line);
    outline: none;
  }
</style>
