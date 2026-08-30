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
    flow,
  }: {
    type?: string;
    name?: string;
    size?: number;
    title?: string;
    detailKind?: string;
    /** 带品质时在图标左下角叠加品质角标（边长为主图标一半）。 */
    quality?: string;
    /** 图标可点击（如触发生产/消耗建议）时传入；悬停详情仍可用。 */
    onClick?: (event: MouseEvent) => void;
    /** 当前流（抽象能量流/定温流体等）。传入时悬停卡片会显示流的具体参数
     * （流体实际温度、ItemFuel 类别列表等），无原型详情也能弹出。 */
    flow?: import("$lib/runtime/types").DualVar;
  } = $props();

  let hoverActive = $state(false);
  let pos = $state({ x: 0, y: 0 });
  let detail = $state<PrototypeDetail | null>(null);

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

<span
  class="hover-icon"
  class:clickable={!!onClick}
  onmouseenter={enter}
  onmousemove={move}
  onmouseleave={leave}
  onclick={onClick}
>
  <Icon {type} {name} {size} {title} />
  {#if quality && quality !== "normal"}
    <span class="quality-corner" style={`--corner:${Math.max(10, Math.round(size / 2))}px`}>
      <Icon type="quality" name={quality} size={Math.max(10, Math.round(size / 2))} title={`${name} · ${quality}`} />
    </span>
  {/if}
</span>

{#if hoverActive && (detail || flowActive)}
  <HoverCard kind={detailKind ?? ""} {detail} x={pos.x} y={pos.y} {flow} />
{/if}

<style>
  .hover-icon {
    position: relative;
    display: inline-flex;
    flex: 0 0 auto;
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
