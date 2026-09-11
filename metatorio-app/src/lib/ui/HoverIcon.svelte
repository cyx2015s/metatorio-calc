<script lang="ts">
  // 带悬停详情的游戏内图标：鼠标悬停显示信息卡片（详情按需拉取 + 缓存）。
  // `detailKind` 是目录 kind（item/recipe/fluid/machine/module/...）；
  // 缺省或为合成流（flow/quality）时不启用悬停。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import Icon from "./Icon.svelte";
  import HoverCard from "./HoverCard.svelte";
  import type { PrototypeDetail } from "$lib/runtime/types";

  let {
    type = "item",
    name = "",
    size = 28,
    title,
    detailKind,
    quality,
    onClick,
    onContextMenu,
    flow,
  }: {
    type?: string;
    name?: string;
    size?: number;
    title?: string;
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
  } = $props();

  let hoverActive = $state(false);
  let pos = $state({ x: 0, y: 0 });
  let detail = $state<PrototypeDetail | null>(null);
  /** 该图标是否可交互（左键/右键任一有动作）：可交互时按按钮语义实现
   * （role/tabindex/键盘）。 */
  let interactive = $derived(!!onClick || !!onContextMenu);

  // 抽象流（无原型详情）也会触发悬停，只要它能提供具体参数（定温流体/燃料类别等）。
  let flowActive = $derived(
    !!flow &&
      typeof flow === "object" &&
      ("ItemFuel" in flow || "FluidFuel" in flow || "FluidHeat" in flow || "Fluid" in flow),
  );
  let enabled = $derived(
    (!!detailKind && detailKind !== "flow" && detailKind !== "quality") || flowActive,
  );

  function enter(event: MouseEvent) {
    if (!enabled || !name) return;
    hoverActive = true;
    pos = { x: event.clientX, y: event.clientY };
  }

  function move(event: MouseEvent) {
    if (hoverActive) pos = { x: event.clientX, y: event.clientY };
  }

  function leave() {
    hoverActive = false;
  }

  $effect(() => {
    if (!hoverActive || !detailKind || !name) {
      detail = null;
      return;
    }
    let alive = true;
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
<svelte:element
  this={interactive ? "button" : "span"}
  class="hover-icon"
  class:clickable={interactive}
  type={interactive ? "button" : undefined}
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
  <Icon {type} {name} {size} {title} />
  {#if quality && quality !== "normal"}
    <span class="quality-corner" style={`--corner:${Math.max(10, Math.round(size / 2))}px`}>
      <Icon type="quality" name={quality} size={Math.max(10, Math.round(size / 2))} title={`${name} · ${quality}`} />
    </span>
  {/if}
</svelte:element>

{#if hoverActive && (detail || flowActive)}
  <HoverCard kind={detailKind ?? ""} {detail} x={pos.x} y={pos.y} {flow} />
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
