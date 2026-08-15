<script lang="ts">
  // 原型/流选择器：
  // - 目录模式（flowMode=false）：复刻原始实现——大组分 tab、小组换行，
  //   筛选/分组/排序全部前端本地（索引由后端一次性下发并缓存），悬停显示卡片。
  // - 流模式（flowMode=true）：选择 DualVar 流（物品/流体/实体走目录，
  //   电/热/火箭运力直接添加，自定义/污染输入名称）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import Icon from "./Icon.svelte";
  import HoverCard from "./HoverCard.svelte";
  import type { CatalogKind, DualVar, IndexEntry } from "$lib/runtime/types";

  let {
    kind,
    title,
    kindOptions = [],
    flowMode = false,
    onSelect,
    onSelectFlow,
    onClose,
  }: {
    kind: CatalogKind;
    title: string;
    kindOptions?: { kind: CatalogKind; label: string }[];
    flowMode?: boolean;
    onSelect?: (name: string) => void;
    onSelectFlow?: (flow: DualVar) => void;
    onClose: () => void;
  } = $props();

  const flowTabs = [
    { id: "item", label: "物品" },
    { id: "fluid", label: "流体" },
    { id: "entity", label: "实体" },
    { id: "electricity", label: "电" },
    { id: "heat", label: "热" },
    { id: "rocket-slot", label: "火箭·堆叠" },
    { id: "rocket-weight", label: "火箭·重量" },
    { id: "custom", label: "自定义" },
  ] as const;
  type FlowTab = (typeof flowTabs)[number]["id"];

  const directFlows: Partial<Record<FlowTab, string>> = {
    electricity: "Electricity",
    heat: "Heat",
    "rocket-slot": "RocketSlotCapacity",
    "rocket-weight": "RocketWeightCapacity",
  };

  let activeKind = $state<CatalogKind>(kind);
  let flowTab = $state<FlowTab>("item");
  let customVariant = $state<"Pollution" | "Custom">("Custom");
  let customName = $state("");
  let query = $state("");
  let activeGroup = $state<string | null>(null);
  let loading = $state(false);
  let searchBox = $state<HTMLInputElement | null>(null);

  let hover = $state<{ x: number; y: number; kind: string; name: string } | null>(null);
  let hoverDetail = $state<import("$lib/runtime/types").PrototypeDetail | null>(null);

  // 目录模式下用 activeKind；流模式下仅 item/fluid/entity 走目录
  let catalogKind = $derived(
    flowMode
      ? flowTab === "item" || flowTab === "fluid" || flowTab === "entity"
        ? (flowTab as CatalogKind)
        : null
      : activeKind,
  );
  let entries = $derived(
    catalogKind
      ? (runtime.catalogIndex?.entries ?? []).filter((entry) => entry.kind === catalogKind)
      : [],
  );
  let groups = $derived([...new Set(entries.map((entry) => entry.group))]);
  let searching = $derived(query.trim().length > 0);
  let visible = $derived(
    searching
      ? entries.filter((entry) => entry.name.toLowerCase().includes(query.trim().toLowerCase()))
      : activeGroup
        ? entries.filter((entry) => entry.group === activeGroup)
        : entries,
  );
  let bySubgroup = $derived(groupBySubgroup(visible));
  let isDirect = $derived(flowMode && flowTab in directFlows);
  let isCustom = $derived(flowMode && flowTab === "custom");

  function groupBySubgroup(list: IndexEntry[]): { subgroup: string; items: IndexEntry[] }[] {
    const map = new Map<string, IndexEntry[]>();
    for (const entry of list) {
      const key = entry.subgroup || "其他";
      const arr = map.get(key) ?? [];
      arr.push(entry);
      map.set(key, arr);
    }
    return [...map.entries()].map(([subgroup, items]) => ({ subgroup, items }));
  }

  $effect(() => {
    activeKind = kind;
    activeGroup = null;
    query = "";
  });

  $effect(() => {
    searchBox?.focus();
  });

  // 上下文变化时重新拉取索引。
  $effect(() => {
    const ctxId = runtime.activeContext?.id;
    loading = true;
    let alive = true;
    runtime.loadCatalogIndex().then(() => {
      if (alive) loading = false;
    });
    return () => {
      alive = false;
    };
  });

  // 悬停详情（防抖由缓存兜底，首次 hover 拉取一次）。
  $effect(() => {
    if (!hover) {
      hoverDetail = null;
      return;
    }
    let alive = true;
    runtime.getDetail(hover.kind, hover.name).then((detail) => {
      if (alive) hoverDetail = detail;
    });
    return () => {
      alive = false;
    };
  });

  function resetView() {
    activeGroup = null;
    query = "";
  }

  async function pick(name: string) {
    if (flowMode && onSelectFlow) {
      const flow = await buildCatalogFlow(name);
      if (flow) {
        onSelectFlow(flow);
        onClose();
      }
      return;
    }
    onSelect?.(name);
    onClose();
  }

  async function buildCatalogFlow(name: string): Promise<DualVar | null> {
    switch (flowTab) {
      case "item":
        return { Item: { id: name, quality: "normal" } };
      case "fluid": {
        const detail = await runtime.getDetail("fluid", name);
        const temperature =
          detail?.default_temperature != null ? Math.round(detail.default_temperature) : 0;
        return { Fluid: { name, temperature: [temperature, temperature] } };
      }
      case "entity":
        return { Entity: { id: name, quality: "normal" } };
      default:
        return null;
    }
  }

  function commitDirect(flow: string) {
    if (!onSelectFlow) return;
    onSelectFlow(flow as DualVar);
    onClose();
  }

  function commitCustom() {
    const name = customName.trim();
    if (!name || !onSelectFlow) return;
    onSelectFlow(
      customVariant === "Pollution" ? { Pollution: { name } } : { Custom: { name } },
    );
    onClose();
  }

  function showHover(event: MouseEvent, entry: IndexEntry) {
    hover = { x: event.clientX, y: event.clientY, kind: entry.kind, name: entry.name };
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    } else if (event.key === "Enter" && !isDirect && !isCustom && visible.length > 0) {
      pick(visible[0].name);
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="backdrop" role="presentation" onclick={onClose}>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    onclick={(event) => event.stopPropagation()}
  >
    <div class="modal-head">
      <div class="modal-title">
        {title}
        {#if loading}<span class="spinner"></span>{/if}
      </div>
      <button class="btn ghost" onclick={onClose}>关闭</button>
    </div>

    {#if flowMode}
      <div class="kind-tabs">
        {#each flowTabs as option (option.id)}
          <button
            class:active={flowTab === option.id}
            class="tab"
            onclick={() => {
              flowTab = option.id;
              resetView();
            }}
          >{option.label}</button>
        {/each}
      </div>
    {:else if kindOptions.length > 0}
      <div class="kind-tabs">
        {#each kindOptions as option (option.kind)}
          <button
            class:active={activeKind === option.kind}
            class="tab"
            onclick={() => {
              activeKind = option.kind;
              resetView();
            }}
          >{option.label}</button>
        {/each}
      </div>
    {/if}

    {#if !isDirect && !isCustom}
      <input
        bind:this={searchBox}
        bind:value={query}
        class="search"
        placeholder="搜索原型名称…"
        spellcheck="false"
      />

      {#if !searching && groups.length > 1}
        <div class="group-tabs">
          {#each groups as group (group)}
            <button
              class:active={activeGroup === group}
              class="gtab"
              onclick={() => (activeGroup = activeGroup === group ? null : group)}
            >{group}</button>
          {/each}
        </div>
      {/if}
    {/if}

    <div class="body">
      {#if isDirect}
        {@const flow = directFlows[flowTab]!}
        <div class="direct-panel">
          <Icon type="flow" name={flow} size={40} />
          <div class="direct-copy">
            <strong>{flow}</strong>
            <small>点击添加该流</small>
          </div>
          <button class="btn primary" onclick={() => commitDirect(flow)}>添加</button>
        </div>
      {:else if isCustom}
        <div class="custom-panel">
          <select bind:value={customVariant}>
            <option value="Custom">自定义</option>
            <option value="Pollution">污染</option>
          </select>
          <input bind:value={customName} placeholder="流名称" spellcheck="false" />
          <button class="btn primary" onclick={commitCustom} disabled={!customName.trim()}>添加</button>
        </div>
      {:else if visible.length === 0}
        <div class="empty">{loading ? "加载中…" : "没有匹配的原型"}</div>
      {:else if searching}
        <div class="search-list">
          {#each visible as entry (entry.kind + entry.name)}
            <button
              class="row"
              onclick={() => pick(entry.name)}
              onmouseenter={(event) => showHover(event, entry)}
              onmousemove={(event) => showHover(event, entry)}
              onmouseleave={() => (hover = null)}
            >
              <Icon type={entry.icon_type} name={entry.name} size={28} />
              <span class="row-name">{entry.name}</span>
              {#if entry.subgroup}<span class="row-sub">{entry.subgroup}</span>{/if}
            </button>
          {/each}
        </div>
      {:else}
        {#each bySubgroup as section (section.subgroup)}
          <div class="subgroup">
            <div class="sg-label">{section.subgroup}</div>
            <div class="sg-items">
              {#each section.items as entry (entry.name)}
                <button
                  class="icon-btn"
                  title={entry.name}
                  onclick={() => pick(entry.name)}
                  onmouseenter={(event) => showHover(event, entry)}
                  onmousemove={(event) => showHover(event, entry)}
                  onmouseleave={() => (hover = null)}
                >
                  <Icon type={entry.icon_type} name={entry.name} size={34} />
                </button>
              {/each}
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

{#if hover}
  <HoverCard kind={hover.kind} detail={hoverDetail} x={hover.x} y={hover.y} />
{/if}

<style>
  .backdrop {
    position: fixed;
    z-index: 40;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(4, 7, 8, 0.72);
    backdrop-filter: blur(3px);
  }

  .modal {
    width: min(640px, 100%);
    max-height: min(700px, calc(100vh - 48px));
    display: flex;
    flex-direction: column;
    padding: 14px;
    background: var(--panel);
    border: 1px solid var(--accent-line);
    border-radius: var(--radius);
    box-shadow: 0 22px 60px rgba(0, 0, 0, 0.5);
  }

  .modal-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }

  .modal-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 700;
  }

  .spinner {
    width: 11px;
    height: 11px;
    border: 2px solid var(--accent-line);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .kind-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 8px;
  }

  .tab {
    padding: 5px 9px;
    color: var(--muted);
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 11px;
    cursor: pointer;
  }

  .tab:hover {
    border-color: var(--accent-line);
  }

  .tab.active {
    color: var(--accent);
    background: var(--accent-dim);
    border-color: var(--accent-line);
  }

  .search {
    min-height: 32px;
    max-width: 100%;
    padding: 0 10px;
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  .search:focus {
    outline: none;
    border-color: var(--accent-line);
  }

  /* 大组分 tab */
  .group-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 8px 0 2px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--line);
  }

  .gtab {
    padding: 4px 9px;
    color: var(--muted);
    background: transparent;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 10px;
    cursor: pointer;
  }

  .gtab:hover {
    border-color: var(--accent-line);
  }

  .gtab.active {
    color: var(--accent);
    background: var(--accent-dim);
    border-color: var(--accent-line);
  }

  .body {
    flex: 1;
    min-height: 120px;
    margin-top: 8px;
    overflow-y: auto;
  }

  .subgroup {
    display: grid;
    gap: 6px;
    margin-bottom: 10px;
  }

  .sg-label {
    color: var(--muted);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    border-bottom: 1px solid var(--line);
    padding-bottom: 3px;
  }

  /* 小组内图标换行 */
  .sg-items {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
  }

  .sg-items .icon-btn {
    padding: 3px;
  }

  .search-list {
    display: grid;
    align-content: start;
    gap: 2px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 4px 7px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .row:hover {
    background: var(--card-hover);
    border-color: var(--line);
  }

  .row-name {
    overflow: hidden;
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-sub {
    overflow: hidden;
    margin-left: auto;
    color: var(--faint);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    padding: 28px;
    text-align: center;
    color: var(--muted);
    font-size: 11px;
  }

  .direct-panel,
  .custom-panel {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 22px;
  }

  .direct-copy {
    display: grid;
    gap: 3px;
    flex: 1;
  }

  .direct-copy strong {
    font-size: 13px;
  }

  .direct-copy small {
    color: var(--muted);
    font-size: 11px;
  }

  .custom-panel {
    flex-wrap: wrap;
  }

  .custom-panel select,
  .custom-panel input {
    min-height: 30px;
    max-width: 100%;
    padding: 0 9px;
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  .custom-panel input {
    flex: 1;
    min-width: 140px;
  }
</style>
