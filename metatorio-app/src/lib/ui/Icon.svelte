<script lang="ts">
  // 游戏内物品图标按钮：真实图标来自后端 `icon` 命令（blob URL 缓存）；
  // 虚拟流（电/热/燃料/火箭运力）用 /static/icons 下的合成图标；
  // 都没有时回退到首字母占位块。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { richTextPlain } from "$lib/rich-text";

  import { syntheticIconUrls } from "./icon-types";

  let {
    type = "item",
    name = "",
    size = 28,
    title,
    bare = false,
    fallbackLabel,
  }: {
    type?: string;
    name?: string;
    size?: number;
    title?: string;
    /** 无底色（品质图标等叠加场景）：背景透明、去掉圆角底。 */
    bare?: boolean;
    /** Inline rich-text references retain readable content if no icon exists. */
    fallbackLabel?: string;
  } = $props();

  let url = $state<string | null>(null);
  let lastKey = $state("");
  // 名称防御：目录条目/分组键偶尔会以非字符串形态传入（历史上选择器把
  // 分组对象传给 name，导致渲染期 name.slice 抛错、整块 UI 不渲染）。
  // 统一转成字符串，占位符与 alt/title 都使用这个安全值。
  let label = $derived(typeof name === "string" ? name : name == null ? "" : String(name));
  // 没有显式 tooltip 时回退到名称，保证每个图标至少有可辨识的悬停信息。
  let displayLabel = $derived(runtime.localizedName(type, label));
  let tip = $derived(richTextPlain(title || displayLabel) || undefined);

  $effect(() => {
    const ctxId = runtime.effectiveContextId;
    runtime.iconRevision;
    // 仅当图标"身份"（上下文/类型/名称）真正变化时才清空（恢复占位）重拉；
    // 若只是缓存失效/上下文重载触发重跑，保留当前图标直到新图标到位，
    // 避免"每次操作都闪一下首字母占位"。
    const key = `${ctxId}/${type}/${name}`;
    if (key !== lastKey) {
      lastKey = key;
      url = null;
    }
    const syntheticUrl = syntheticIconUrls[name];
    if (syntheticUrl) {
      url = syntheticUrl;
      return;
    }
    if (!type || !name) return;
    let alive = true;
    runtime.getIcon(type, name)
      .then((iconUrl) => { if (alive) url = iconUrl; })
      .catch(() => { if (alive) url = null; });
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
    onerror={() => { url = null; }}
    alt={displayLabel}
    title={tip}
    style={`width:${size}px;height:${size}px`}
    draggable="false"
  />
{:else if fallbackLabel}
  <span class="text-fallback" title={tip}>{fallbackLabel}</span>
{:else}
  <span
    class="icon placeholder"
    class:bare={bare || type === "quality"}
    title={tip}
    style={`width:${size}px;height:${size}px;font-size:${Math.max(9, Math.round(size * 0.32))}px`}
  >{label ? label.slice(0, 2) : "?"}</span>
{/if}

<style>
  .text-fallback { font: inherit; white-space: normal; overflow-wrap: anywhere; min-width: 0; }
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
