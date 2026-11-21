<script lang="ts">
  import { tick, untrack } from "svelte";
  import {locationKey, locationLabel} from "$lib/mechanics/located";
  import LocationIcon from "./LocationIcon.svelte";
  import { dndzone } from 'svelte-dnd-action';
  import { runtime } from '$lib/runtime/store.svelte';
  import { mechanicPrototype } from '$lib/display-labels';
  import { compactNumber } from "$lib/format";
  import type { Location, MechanicEntry } from '$lib/runtime/types';
  import type { FlowTraceHandler, MechanicViewProps } from '$lib/mechanics/workspace';
  import MechanicCard from './MechanicCard.svelte';
  import MechanicRow from './MechanicRow.svelte';
  import { rememberMechanicScroll } from '$lib/mechanics/view-scroll';

  let {
    project,
    factory,
    entries,
    visibleIds,
    mergeRecipes,
    groupLocations = false,
    locationFilter = null,
    highlightMechanicId = null,
    highlightSeq = 0,
    onTraceFlow,
    onRevealInNetwork,
    actions,
    solutions,
    scrollState,
  }: {
    mergeRecipes: boolean;
    groupLocations?: boolean;
    locationFilter?: Location | null;
    highlightMechanicId?: number | null;
    highlightSeq?: number;
    onTraceFlow?: FlowTraceHandler;
    onRevealInNetwork?: (mechanicId: number) => void;
  } & MechanicViewProps = $props();
  let filterActive = $derived(visibleIds !== null || !!locationFilter);
  let dragMechanics = $state<MechanicEntry[]>(untrack(() => entries));
  $effect(() => { dragMechanics = entries; });
  let visibleMechanics = $derived.by(() => {
    const ids = visibleIds !== null ? new Set(visibleIds) : null;
    const locKey = locationFilter ? locationKey(locationFilter) : null;
    if (ids === null && locKey === null) return dragMechanics;
    return dragMechanics.filter(entry =>
      (ids === null || ids.has(entry.id)) &&
      (locKey === null || locationKey(entry.location) === locKey)
    );
  });
  function groupTitle(entries: MechanicEntry[]): string {
    const first = entries[0]?.mechanic;
    if (!first) return '';
    const ref = mechanicPrototype(first);
    return runtime.localizedName(ref.kind, ref.name);
  }
  function groupMachine(entries: MechanicEntry[]): string {
    const machine = entries[0]?.mechanic.machine?.id;
    return machine ? runtime.localizedName('entity', machine) || machine : '';
  }
  /** 聚合键：合并开启时仅 recipe 按 (配方, 机器) 聚合；其余类型各自独立。 */
  function mechGroupKey(entry: import("$lib/runtime/types").MechanicEntry): string {
    if (!mergeRecipes) return `single:${entry.id}`;
    const m = entry.mechanic;
    if (m.type === "recipe") return `recipe:${JSON.stringify(entry.location)}:${m.recipe?.id ?? ""}:${m.machine?.id ?? ""}`;
    // 非 recipe 机制：永不合并（每机制独立一组）。
    return `single:${entry.id}`;
  }

  /** 网格分组：保持原始顺序，同键条目聚成一组（不同品质放一行）。 */
  let mechGroups = $derived(
    (() => {
      const groups: {
        key: string;
        entries: import("$lib/runtime/types").MechanicEntry[];
      }[] = [];
      const index = new Map<string, number>();
      for (const entry of visibleMechanics) {
        const key = mechGroupKey(entry);
        const existing = index.get(key);
        if (existing !== undefined) {
          groups[existing].entries.push(entry);
        } else {
          index.set(key, groups.length);
          groups.push({ key, entries: [entry] });
        }
      }
      return groupLocations ? [...groups].sort((a,b)=>locationKey(a.entries[0].location).localeCompare(locationKey(b.entries[0].location))) : groups;
    })(),
  );
  type MechanicGroup = { id: string; key: string; entries: import("$lib/runtime/types").MechanicEntry[] };
  let dragGroups = $state<MechanicGroup[]>(untrack(() => mechGroups.map((group) => ({ ...group, id: group.key }))));
  $effect(() => { dragGroups = mechGroups.map((group) => ({ ...group, id: group.key })); });

  let collapsedGroups = $state<Record<string, boolean>>({});
  function isCollapsed(key: string): boolean {
    return !!collapsedGroups[key];
  }
  function toggleGroup(key: string) {
    collapsedGroups = { ...collapsedGroups, [key]: !collapsedGroups[key] };
  }
  function groupSummary(groupEntries: MechanicEntry[]) {
    let totalAmount = 0;
    let totalCost = 0;
    let hasSolution = false;
    let enabledCount = 0;
    for (const entry of groupEntries) {
      if (entry.enabled) enabledCount += 1;
      const sol = solutions.get(entry.id);
      if (sol && Number.isFinite(sol.amount)) {
        hasSolution = true;
        const amt = Math.max(0, sol.amount);
        totalAmount += amt;
        if (Number.isFinite(sol.cost)) totalCost += amt * sol.cost;
      }
    }
    return { hasSolution, totalAmount, totalCost, enabledCount };
  }

  let listNode = $state<HTMLDivElement>();
  let activeHighlightId = $state<number | null>(null);
  let highlightTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const targetId = highlightMechanicId;
    void highlightSeq;
    if (targetId == null) return;
    let alive = true;
    let frame1 = 0;
    let frame2 = 0;
    untrack(() => {
      const ownerGroup = dragGroups.find((group) => group.entries.some((entry) => entry.id === targetId));
      if (ownerGroup && collapsedGroups[ownerGroup.key]) {
        collapsedGroups = { ...collapsedGroups, [ownerGroup.key]: false };
      }
      activeHighlightId = targetId;
      clearTimeout(highlightTimer);
      highlightTimer = setTimeout(() => {
        if (alive) activeHighlightId = null;
      }, 2400);
    });
    void tick().then(() => {
      if (!alive) return;
      // 双帧延迟确保在 rememberMechanicScroll 的初始 rAF 恢复之后再滚动定位。
      frame1 = requestAnimationFrame(() => {
        if (!alive) return;
        frame2 = requestAnimationFrame(() => {
          if (!alive || !listNode) return;
          const targetRow = listNode.querySelector<HTMLElement>(`[data-mechanic-row="${targetId}"]`);
          targetRow?.scrollIntoView({ behavior: "smooth", block: "center" });
        });
      });
    });
    return () => {
      alive = false;
      cancelAnimationFrame(frame1);
      cancelAnimationFrame(frame2);
      clearTimeout(highlightTimer);
    };
  });

  function handleMechanicsConsider(event: CustomEvent<{ items: MechanicGroup[] }>) {
    if (filterActive || groupLocations) return;
    dragGroups = event.detail.items;
  }
  function handleMechanicsFinalize(event: CustomEvent<{ items: MechanicGroup[] }>) {
    if (filterActive || groupLocations) return;
    dragGroups = event.detail.items;
    dragMechanics = event.detail.items.flatMap((group) => group.entries);
    actions.commands.reorderMechanics(dragMechanics.map((entry) => entry.id)).catch(() => {});
  }
</script>

{#snippet row(entry: MechanicEntry, draggable: boolean)}
  <MechanicRow
    mechanicId={entry.id}
    virtualize={entries.length > 80}
    {draggable}
    highlighted={activeHighlightId === entry.id}
  >
    {#snippet children(pin)}
      <MechanicCard
        {entry}
        {project}
        {factory}
        {actions}
        {onTraceFlow}
        {onRevealInNetwork}
        solution={solutions.get(entry.id) ?? null}
        onInteractionChange={pin}
      />
    {/snippet}
  </MechanicRow>
{/snippet}

{#snippet renderGroup(group: MechanicGroup)}
          {#if group.entries.length > 1}
            {@const collapsed = isCollapsed(group.key)}
            {@const summary = collapsed ? groupSummary(group.entries) : null}
            <!-- 配方合并组：标题（配方名 + 机器名），支持一键折叠/展开 -->
            <div class="mech-group" class:collapsed>
              <div class="mech-group-head">
                {#if !filterActive && !groupLocations}
                  <span class="drag-handle-inline" aria-hidden="true" title="拖拽整组排序">⋮⋮</span>
                {/if}
                <button
                  type="button"
                  class="mech-group-toggle"
                  aria-expanded={!collapsed}
                  onclick={() => toggleGroup(group.key)}
                >
                  <span class="collapse-chevron" aria-hidden="true">{collapsed ? "▸" : "▾"}</span>
                  <span class="mech-group-title" title={groupTitle(group.entries)}>
                    {groupTitle(group.entries)}
                  </span>
                  {#if groupMachine(group.entries)}
                    <span class="chip">{groupMachine(group.entries)}</span>
                  {/if}
                  <span class="chip muted">{group.entries.length} 变体</span>
                  {#if summary}
                    {#if summary.hasSolution}
                      <span class="chip mono">{compactNumber(summary.totalAmount)} 台</span>
                      <span class="chip mono muted">总成本 {compactNumber(summary.totalCost)}</span>
                    {:else}
                      <span class="chip muted">{summary.enabledCount}/{group.entries.length} 启用 · 未求解</span>
                    {/if}
                  {/if}
                </button>
              </div>
              {#if !collapsed}
                {#each group.entries as entry (entry.id)}
                  <div class="mech-group-row">
                    {@render row(entry, false)}
                  </div>
                {/each}
              {/if}
            </div>
          {:else}
            {#each group.entries as entry (entry.id)}
              {@render row(entry, !filterActive && !groupLocations)}
            {/each}
          {/if}
{/snippet}
      <div
        class="mech-list"
        bind:this={listNode}
        use:rememberMechanicScroll={scrollState}
        use:dndzone={{ items: dragGroups, type: "mechanic-groups", flipDurationMs: 120, dragDisabled: filterActive || groupLocations }}
        onconsider={handleMechanicsConsider}
        onfinalize={handleMechanicsFinalize}
      >
        {#each dragGroups as group, i (group.id)}
          {@const location = group.entries[0].location}
          {#if groupLocations}
          <div class="mechanic-list-item">
          {#if groupLocations && location && (i === 0 || locationKey(dragGroups[i-1].entries[0].location) !== locationKey(location))}
            <div class="location-heading"><LocationIcon {location} size={24}/><strong>{locationLabel(location,(k,n)=>runtime.localizedName(k,n))}</strong></div>
          {/if}
          {@render renderGroup(group)}
          </div>
          {:else}
            {@render renderGroup(group)}
          {/if}
        {/each}
      </div>
<style>
  .location-heading {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 8px 6px;
    margin-bottom: 4px;
    color: var(--text);
    border-bottom: 1px solid var(--line);
  }

  .location-heading strong {
    font-size: 12px;
    font-weight: 650;
    letter-spacing: 0.02em;
  }

  .mech-list {
    /* 关键：机制列表必须按内容高度撑开，由 .center 负责滚动。
       若它作为可收缩 flex 项被压到剩余高度，内部的 .mech-group 会因
       overflow:hidden（自动最小尺寸为 0）被压成一条 2px 的横线。 */
    flex: 0 0 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 6px;
  }

  .mech-group {
    flex: 0 0 auto;
    overflow: hidden;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: var(--shadow-sm, var(--shadow));
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .mech-group:hover {
    border-color: var(--line-strong);
  }

  .mech-group-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    min-height: 34px;
    background: var(--panel-elev);
    border-bottom: 1px solid var(--line);
  }

  .mech-group.collapsed .mech-group-head {
    border-bottom: 0;
  }

  .drag-handle-inline {
    flex: 0 0 auto;
    padding: 4px 2px;
    color: var(--faint);
    font-size: 11px;
    letter-spacing: -1px;
    user-select: none;
    cursor: grab;
  }

  .mech-group-head:hover .drag-handle-inline {
    color: var(--text);
  }

  .mech-group-toggle {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 4px;
    background: transparent;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .collapse-chevron {
    flex: 0 0 auto;
    color: var(--muted);
    font-size: 11px;
    width: 12px;
    text-align: center;
  }

  .mech-group-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-overflow: ellipsis;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .mech-group-row + .mech-group-row {
    border-top: 1px solid var(--line);
  }

  /* 分组内的机制卡去掉自己的外框，由分组容器统一承载边界。 */
  .mech-group-row :global(.mech-card) {
    border: 0;
    border-radius: 0;
    background: transparent;
  }

  .mech-group-row :global(.mech-card:hover) {
    background: var(--card-hover);
  }

  /* 顶层机制卡由 MechanicCard 自己提供边框；这里只保证悬停/间距一致。 */
  .mech-list > :global(.mechanic-row) {
    flex: 0 0 auto;
    margin-bottom: 0;
  }

</style>
