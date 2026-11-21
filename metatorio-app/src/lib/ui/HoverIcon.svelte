<script lang="ts">
  // 带悬停详情的游戏内图标：鼠标悬停显示信息卡片（详情按需拉取 + 缓存）。
  // `detailKind` 是目录 kind（item/recipe/fluid/machine/module/...）；
  // 缺省或为合成流（flow/quality）时不启用悬停。
  import { onDestroy, onMount, untrack } from "svelte";
  import {
    dismissHover,
    hoverOpenDelay,
    markHoverActive,
    markHoverInactive,
    registerHoverDismiss,
  } from "./hover-controller";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import Icon from "./Icon.svelte";
  import HoverCard from "./HoverCard.svelte";
  import type { PrototypeDetail } from "$lib/runtime/types";
  import type { FlowMatch } from "$lib/dual-var";

  let {
    type = "item",
    name = "",
    size = 28,
    title,
    hint,
    detailKind,
    quality,
    onClick,
    onContextMenu,
    flow,
    flowRate,
    rateBasis,
    match,
    focusable = false,
  }: {
    type?: string;
    name?: string;
    size?: number;
    title?: string;
    /** 悬停卡片底部操作提示（如左键/右键动作）。 */
    hint?: string;
    detailKind?: string;
    /** 带品质时在图标左下角叠加品质角标（边长为主图标一半）。 */
    quality?: string;
    /** 左键动作（如「更改目标流」）；悬停详情仍可用。 */
    onClick?: (event: MouseEvent) => void;
    /** 右键动作（如「显示建议」）。传入时**阻止浏览器右键菜单**，
     * 并用 `title` 提示这两种操作（右键没有可见的按钮，全靠提示）。 */
    onContextMenu?: (event: MouseEvent) => void;
    /** 当前流（抽象能量流/定温流体等）。传入时悬停卡片会显示流的具体参数
     * （流体实际温度、ItemFuel 类别列表等），无原型详情也能弹出。 */
    flow?: import("$lib/runtime/types").DualVar;
    flowRate?: number;
    rateBasis?: "solution" | "unit";
    match?: FlowMatch;
    focusable?: boolean;
  } = $props();

  let hoverActive = $state(false);
  let pos = $state({ x: 0, y: 0 });
  let detail = $state<PrototypeDetail | null>(null);
  /** 该图标是否可交互（左键/右键任一有动作）：可交互时按按钮语义实现
   * （role/tabindex/键盘）。 */
  let interactive = $derived(!!onClick || !!onContextMenu);
  /** 悬停信息兜底：任何图标至少有「本地化名称」可辨认；调用方可覆盖为更完整的动作说明。 */
  let tip = $derived(
    title ||
      (name
        ? detailKind && detailKind !== "flow" && detailKind !== "quality"
          ? runtime.localizedName(detailKind, name)
          : runtime.localizedName(type, name) || name
        : undefined),
  );

  // 抽象流（无原型详情）也会触发悬停，只要它能提供具体参数（定温流体/燃料类别等）。
  let flowActive = $derived(!!flow);
  let enabled = $derived((!!detailKind && detailKind !== "flow") || flowActive);
  const hoverToken = Symbol("hover-icon");
  let openTimer: ReturnType<typeof setTimeout> | undefined;
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;
  let rafId: number | undefined;
  let pendingPos: { x: number; y: number } | null = null;
  let inLeaveBuffer = false;
  let insideCard = false;

  function cancelPendingPosition() {
    if (rafId != null) {
      cancelAnimationFrame(rafId);
      rafId = undefined;
    }
    pendingPos = null;
  }

  function clearTimers() {
    clearTimeout(openTimer);
    openTimer = undefined;
    clearTimeout(leaveTimer);
    leaveTimer = undefined;
  }

  function closeHover() {
    clearTimers();
    cancelPendingPosition();
    inLeaveBuffer = false;
    insideCard = false;
    if (hoverActive) {
      hoverActive = false;
      markHoverInactive(hoverToken);
    }
  }

  function activateHover() {
    dismissHover();
    hoverActive = true;
    markHoverActive(hoverToken);
  }

  function keepOpen() {
    clearTimeout(leaveTimer);
    leaveTimer = undefined;
    inLeaveBuffer = false;
    insideCard = true;
  }

  function leaveCard() {
    insideCard = false;
    leave();
  }

  onDestroy(closeHover);
  onMount(() => registerHoverDismiss(closeHover));

  function enter(event: MouseEvent | FocusEvent) {
    if (!enabled || !name) return;
    clearTimeout(leaveTimer);
    leaveTimer = undefined;
    inLeaveBuffer = false;
    insideCard = false;
    cancelPendingPosition();
    if (event instanceof MouseEvent) {
      pos = { x: event.clientX, y: event.clientY };
      if (hoverActive) return;
      clearTimeout(openTimer);
      openTimer = undefined;
      const delay = hoverOpenDelay();
      if (delay <= 0) {
        activateHover();
      } else {
        openTimer = setTimeout(() => {
          openTimer = undefined;
          activateHover();
        }, delay);
      }
      return;
    }
    clearTimeout(openTimer);
    openTimer = undefined;
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    pos = { x: rect.left, y: rect.bottom };
    activateHover();
  }

  function move(event: MouseEvent) {
    if (!enabled || !name || inLeaveBuffer || insideCard) return;
    if (!hoverActive && openTimer == null) return;
    pendingPos = { x: event.clientX, y: event.clientY };
    if (rafId == null) {
      rafId = requestAnimationFrame(() => {
        rafId = undefined;
        if (pendingPos && !inLeaveBuffer && !insideCard) {
          pos = pendingPos;
        }
        pendingPos = null;
      });
    }
  }

  function leave() {
    clearTimeout(openTimer);
    openTimer = undefined;
    cancelPendingPosition();
    if (!hoverActive) return;
    inLeaveBuffer = true;
    clearTimeout(leaveTimer);
    leaveTimer = setTimeout(() => {
      leaveTimer = undefined;
      closeHover();
    }, 180);
  }

  $effect(() => {
    if (!enabled || !name) {
      untrack(closeHover);
    }
  });

  $effect(() => {
    if (!hoverActive || !detailKind || !name) {
      detail = null;
      return;
    }
    let alive = true;
    detail = null;
    runtime.getDetail(detailKind, name).then((value) => {
      if (alive) detail = value;
    });
    return () => {
      alive = false;
    };
  });
</script>

<!-- 悬停工具提示容器：非可点击态用 span + 鼠标悬停展示详情，非"可交互"语义，
     此规则对该场景是误报，抑制。 -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex (Object references reveal details on keyboard focus.) -->
<svelte:element
  this={interactive ? "button" : "span"}
  class="hover-icon"
  class:clickable={interactive}
  type={interactive ? "button" : undefined}
  title={enabled ? undefined : tip}
  aria-label={tip || name}
  role={!interactive && focusable ? "img" : undefined}
  tabindex={!interactive && focusable && enabled ? 0 : undefined}
  onfocus={enter}
  onblur={leave}
  onmouseenter={enter}
  onmousemove={move}
  onmouseleave={leave}
  onclick={onClick}
  oncontextmenu={onContextMenu
    ? (event: MouseEvent) => {
        // 右键动作：压掉浏览器菜单，否则弹出的菜单会盖住我们自己的交互。
        event.preventDefault();
        onContextMenu(event);
      }
    : undefined}
  onkeydown={onClick
    ? (event: KeyboardEvent) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onClick(event as unknown as MouseEvent);
        }
      }
    : undefined}
>
  <Icon {type} {name} {size} title={enabled ? undefined : tip} suppressTitle={enabled} />
  {#if quality && quality !== "normal"}
    <span class="quality-corner" style={`--corner:${Math.max(10, Math.round(size / 2))}px`}>
      <Icon
        type="quality"
        name={quality}
        size={Math.max(10, Math.round(size / 2))}
        title={enabled ? undefined : runtime.localizedName("quality", quality)}
        suppressTitle={enabled}
      />
    </span>
  {/if}
</svelte:element>

{#if hoverActive && (detail || flowActive)}
  <HoverCard kind={detailKind ?? ""} {detail} x={pos.x} y={pos.y} {flow} {quality} {flowRate} {rateBasis} {match} {hint} onEnter={keepOpen} onLeave={leaveCard} />
{/if}

<style>
  .hover-icon {
    position: relative;
    display: inline-flex;
    flex: 0 0 auto;
    padding: 0;
    color: inherit;
    background: none;
    border: none;
    font: inherit;
  }

  .hover-icon.clickable {
    cursor: pointer;
  }

  .quality-corner {
    position: absolute;
    left: -1px;
    bottom: -1px;
    width: var(--corner);
    height: var(--corner);
    display: inline-flex;
    border-radius: 4px;
    overflow: hidden;
  }

  .quality-corner :global(.icon) {
    border-radius: 0;
  }
</style>
