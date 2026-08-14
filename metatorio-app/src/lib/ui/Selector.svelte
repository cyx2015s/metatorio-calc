<script lang="ts">
  // 目录选择器：搜索 + 分类页签 + 大组分栏，条目为图标 + 名称。
  // 选择结果通过 onSelect(name) 回调给上层。
  import Icon from "./Icon.svelte";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import type { CatalogEntry, CatalogKind } from "$lib/runtime/types";

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

  let query = $state("");
  let activeKind = $state<CatalogKind>("item");
  let entries = $state<CatalogEntry[]>([]);
  let loading = $state(false);
  let groupFilter = $state<string | null>(null);
  let searchBox = $state<HTMLInputElement | null>(null);

  let groups = $derived([...new Set(entries.map((entry) => entry.group))].filter(Boolean));
  let filtered = $derived(
    groupFilter ? entries.filter((entry) => entry.group === groupFilter) : entries,
  );

  $effect(() => {
    activeKind = kind;
    groupFilter = null;
  });

  $effect(() => {
    searchBox?.focus();
  });

  // 搜索防抖；切换分类/关键字时重新拉取。
  $effect(() => {
    const timer = setTimeout(async () => {
      loading = true;
      let alive = true;
      try {
        const result = await runtime.searchCatalog(activeKind, query);
        if (alive) {
          entries = result;
          groupFilter = null;
        }
      } catch {
        if (alive) entries = [];
      } finally {
        if (alive) loading = false;
      }
      return;
    }, 120);
    return () => {
      clearTimeout(timer);
    };
  });

  function pick(name: string) {
    onSelect(name);
    onClose();
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    } else if (event.key === "Enter" && filtered.length > 0) {
      pick(filtered[0].name);
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
      <button class="btn ghost" onclick={onClose} title="关闭 (Esc)">关闭</button>
    </div>

    {#if kindOptions.length > 0}
      <div class="kind-tabs">
        {#each kindOptions as option (option.kind)}
          <button
            class:active={activeKind === option.kind}
            class="kind-tab"
            onclick={() => (activeKind = option.kind)}
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

    {#if groups.length > 1}
      <div class="group-chips">
        <button class:active={groupFilter === null} class="group-chip" onclick={() => (groupFilter = null)}>全部</button>
        {#each groups as group (group)}
          <button
            class:active={groupFilter === group}
            class="group-chip"
            onclick={() => (groupFilter = group)}
          >{group}</button>
        {/each}
      </div>
    {/if}

    <div class="list">
      {#if filtered.length === 0}
        <div class="empty">
          {loading ? "加载中…" : "没有匹配的原型"}
        </div>
      {:else}
        {#each filtered as entry (entry.name)}
          <button class="row" onclick={() => pick(entry.name)}>
            <Icon type={entry.icon_type} name={entry.name} size={30} />
            <span class="row-name">{entry.name}</span>
            {#if entry.subgroup}<span class="row-sub">{entry.subgroup}</span>{/if}
            {#if entry.module_slots != null}
              <span class="row-meta">{entry.module_slots} 模块槽</span>
            {/if}
          </button>
        {/each}
      {/if}
    </div>
  </div>
</div>

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
    width: min(620px, 100%);
    max-height: min(680px, calc(100vh - 48px));
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

  .kind-tab {
    padding: 5px 9px;
    color: var(--muted);
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 11px;
    cursor: pointer;
  }

  .kind-tab:hover {
    border-color: var(--accent-line);
  }

  .kind-tab.active {
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

  .group-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 8px 0 2px;
  }

  .group-chip {
    padding: 3px 8px;
    color: var(--muted);
    background: transparent;
    border: 1px solid var(--line);
    border-radius: 999px;
    font-size: 10px;
    cursor: pointer;
  }

  .group-chip:hover {
    border-color: var(--accent-line);
  }

  .group-chip.active {
    color: var(--accent);
    border-color: var(--accent-line);
  }

  .list {
    flex: 1;
    min-height: 120px;
    margin-top: 8px;
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: 2px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 7px;
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

  .row-meta {
    color: var(--faint);
    font-size: 10px;
  }

  .empty {
    padding: 28px;
    text-align: center;
    color: var(--muted);
    font-size: 11px;
  }
</style>
