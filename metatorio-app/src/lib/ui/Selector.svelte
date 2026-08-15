<script lang="ts">
  // 原型选择器：复刻原始实现——大组分 tab、小组换行；筛选/分组/排序
  // 全部在前端对全量目录索引进行（索引由后端一次性下发并按上下文缓存）。
  // 悬停显示信息卡片（详情按需拉取 + 缓存）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import Icon from "./Icon.svelte";
  import HoverCard from "./HoverCard.svelte";
  import type { CatalogKind, IndexEntry } from "$lib/runtime/types";

  let {
    kind,
    title,
    kindOptions = [],
    onSelect,
    onClose,
  }: {
    kind: CatalogKind;
    title: string;
    kindOptions?: { kind: CatalogKind; label: string }[];
    onSelect: (name: string) => void;
    onClose: () => void;
  } = $props();

  let activeKind = $state<CatalogKind>(kind);
  let query = $state("");
  let activeGroup = $state<string | null>(null);
  let loading = $state(false);
  let searchBox = $state<HTMLInputElement | null>(null);

  let hover = $state<{ x: number; y: number; kind: string; name: string } | null>(null);
  let hoverDetail = $state<import("$lib/runtime/types").PrototypeDetail | null>(null);

  let entries = $derived(
    (runtime.catalogIndex?.entries ?? []).filter((entry) => entry.kind === activeKind),
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

  function pick(name: string) {
    onSelect(name);
    onClose();
  }

  function showHover(event: MouseEvent, entry: IndexEntry) {
    hover = { x: event.clientX, y: event.clientY, kind: entry.kind, name: entry.name };
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    } else if (event.key === "Enter" && visible.length > 0) {
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

    {#if kindOptions.length > 0}
      <div class="kind-tabs">
        {#each kindOptions as option (option.kind)}
          <button
            class:active={activeKind === option.kind}
            class="tab"
            onclick={() => {
              activeKind = option.kind;
              activeGroup = null;
            }}
          >{option.label}</button>
        {/each}
      </div>
    {/if}

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

    <div class="body">
      {#if visible.length === 0}
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
</style>
