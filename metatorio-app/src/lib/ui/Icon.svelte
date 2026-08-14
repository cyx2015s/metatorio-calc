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
  }: { type?: string; name?: string; size?: number; title?: string } = $props();

  let url = $state<string | null>(null);

  $effect(() => {
    url = null;
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
    src={url}
    alt={name}
    title={title ?? name}
    style={`width:${size}px;height:${size}px`}
    draggable="false"
  />
{:else}
  <span
    class="icon placeholder"
    title={title ?? name}
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
