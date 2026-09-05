<script lang="ts">
  // 游戏内物品图标按钮：真实图标来自后端 `icon` 命令（blob URL 缓存）；
  // 虚拟流（电/热/燃料/火箭运力）用 /static/icons 下的合成图标；
  // 都没有时回退到首字母占位块。
  import { runtime } from "$lib/runtime/store.svelte.ts";

  const synthetic: Record<string, string> = {
    Electricity: "/icons/electricity.png",
    Heat: "/icons/heat.png",
    FluidHeat: "/icons/fluid-heat.png",
    ItemFuel: "/icons/item-fuel.png",
    RocketSlotCapacity: "/icons/rocket-capacity.png",
    RocketWeightCapacity: "/icons/rocket-capacity.png",
  };

  let {
    type = "item",
    name = "",
    size = 28,
    title,
    bare = false,
  }: {
    type?: string;
    name?: string;
    size?: number;
    title?: string;
    /** 无底色（品质图标等叠加场景）：背景透明、去掉圆角底。 */
    bare?: boolean;
  } = $props();

  let url = $state<string | null>(null);

  $effect(() => {
    url = null;
    // 显式依赖有效上下文 id：上下文就绪/切换后需重新拉取图标。否则首屏渲染
    // 时若上下文尚未就绪，getIcon 会拿到 null 并停在首字母占位（而选择器/悬停
    // 卡片是后来才渲染、用到了已就绪的上下文，所以有图标）。
    runtime.effectiveContextId;
    const syntheticUrl = synthetic[name];
    if (syntheticUrl) {
      url = syntheticUrl;
      return;
    }
    if (!type || !name) return;
    let alive = true;
    runtime.getIcon(type, name).then((iconUrl) => {
      if (alive) url = iconUrl;
    });
    return () => {
      alive = false;
    };
  });
</script>

{#if url}
  <img
    class="icon"
    class:bare={bare || type === "quality"}
    src={url}
    alt={name}
    title={title || undefined}
    style={`width:${size}px;height:${size}px`}
    draggable="false"
  />
{:else}
  <span
    class="icon placeholder"
    class:bare={bare || type === "quality"}
    title={title || undefined}
    style={`width:${size}px;height:${size}px;font-size:${Math.max(9, Math.round(size * 0.32))}px`}
  >{name ? name.slice(0, 2) : "?"}</span>
{/if}

<style>
  .icon {
    display: inline-block;
    flex: 0 0 auto;
    border-radius: 6px;
    object-fit: contain;
    background: var(--icon-bg);
  }

  /* 品质图标（角标/挑选）是透明菱形 PNG：必须透明底，否则会盖住
     底下的物品图标主题。 */
  .icon.bare {
    background: transparent;
    border-radius: 0;
  }

  .placeholder {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--muted);
    background: var(--icon-bg);
    border: 1px solid var(--line);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }
</style>
