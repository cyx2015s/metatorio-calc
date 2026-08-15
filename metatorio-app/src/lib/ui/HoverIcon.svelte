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
  }: {
    type?: string;
    name?: string;
    size?: number;
    title?: string;
    detailKind?: string;
  } = $props();

  let hoverActive = $state(false);
  let pos = $state({ x: 0, y: 0 });
  let detail = $state<PrototypeDetail | null>(null);

  let enabled = $derived(!!detailKind && detailKind !== "flow" && detailKind !== "quality");

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
  onmouseenter={enter}
  onmousemove={move}
  onmouseleave={leave}
>
  <Icon {type} {name} {size} {title} />
</span>

{#if hoverActive && detail}
  <HoverCard kind={detailKind ?? ""} {detail} x={pos.x} y={pos.y} />
{/if}

<style>
  .hover-icon {
    display: inline-flex;
    flex: 0 0 auto;
  }
</style>
