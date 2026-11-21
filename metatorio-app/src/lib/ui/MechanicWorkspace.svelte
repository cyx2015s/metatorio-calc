<script lang="ts">
  import { untrack } from "svelte";
  import { runtime } from "$lib/runtime/store.svelte";
  import { MechanicWorkspaceState } from "$lib/mechanics/workspace.svelte";
  import type { DualVar, Location, MechanicEntry } from '$lib/runtime/types';
  import type { FlowFilter } from '$lib/mechanic-filter';
  import type { MechanicActions } from '$lib/mechanics/actions';
  import {
    DEFAULT_MECHANIC_VIEW,
    type FlowTraceHandler,
    type FlowTraceRequest,
    type MechanicFocusRequest,
    type MechanicSolutionMap,
    type MechanicViewMode,
  } from '$lib/mechanics/workspace';
  import MechanicFilter from './MechanicFilter.svelte';
  import MechanicListView from './MechanicListView.svelte';
  import MechanicNetworkView from './MechanicNetworkView.svelte';
  import { createNetworkViewState, type NetworkViewState } from '$lib/mechanics/network';
  import type { MechanicScrollState } from '$lib/mechanics/view-scroll';

  let {
    project,
    factory,
    entries,
    solutions,
    solved,
    actions,
    filterOpen,
    mergeRecipes,
    groupLocations = false,
    view = DEFAULT_MECHANIC_VIEW,
    onFilterActiveChange,
    onFilterClose,
    locationFilter,
    focusRequest = null,
    traceRequest = null,
    onTraceFlow,
    onRevealInList,
    onRevealInNetwork,
    searchFocusSeq = 0,
  }: {
    project: number | null;
    factory: number | null;
    entries: MechanicEntry[];
    solutions: MechanicSolutionMap;
    solved: boolean;
    actions: MechanicActions | null;
    filterOpen: boolean;
    mergeRecipes: boolean;
    groupLocations?: boolean;
    view?: MechanicViewMode;
    onFilterActiveChange: (active: boolean) => void;
    onFilterClose: () => void;
    locationFilter?: Location | null;
    focusRequest?: MechanicFocusRequest | null;
    traceRequest?: FlowTraceRequest | null;
    onTraceFlow?: FlowTraceHandler;
    onRevealInList?: (mechanicId: number) => void;
    onRevealInNetwork?: (mechanicId: number) => void;
    searchFocusSeq?: number;
  } = $props();

  // Stage 2 wire: pass {onRevealInList} to MechanicNetworkView when network-ux merges.
  const _stage2RevealInList = $derived(onRevealInList);

  function flowFilterFromDualVar(flow: DualVar): FlowFilter {
    if (typeof flow !== "string") {
      if ("Item" in flow || "Entity" in flow) {
        const inner = Object.values(flow)[0] as { id: string; quality?: string };
        return inner.quality ? { flow, quality: inner.quality } : { flow };
      }
      if ("Fluid" in flow) {
        const temp = (flow.Fluid as { temperature?: [number, number] }).temperature;
        if (Array.isArray(temp) && temp.length === 2 && Number.isFinite(temp[0]) && Number.isFinite(temp[1])) {
          return { flow, temperature: [temp[0], temp[1]] };
        }
        return { flow };
      }
    }
    return { flow };
  }

  let localFilterOpen = $state(false);
  let filterFocusTick = $state(0);
  let prevFilterOpen = untrack(() => filterOpen);
  $effect(() => {
    if (filterOpen !== prevFilterOpen) {
      prevFilterOpen = filterOpen;
      if (!filterOpen) localFilterOpen = false;
    }
  });
  const filterExpanded = $derived(filterOpen || localFilterOpen);

  // 筛选条件保存在渲染器之外：切换列表/网络视图时保留当前筛选条件。
  let scope = $derived(`${actions?.commands.scope.kind ?? "factory"}:${project ?? 0}:${factory ?? 0}`);
  const networkStates = new Map<string, NetworkViewState>();
  const scrollStates = new Map<string, MechanicScrollState>();
  let scrollState = $derived.by(() => {
    const key = `${scope}:${view}`;
    if (!scrollStates.has(key)) scrollStates.set(key, {top: 0});
    return scrollStates.get(key)!;
  });
  let networkState = $derived.by(() => {
    if (!networkStates.has(scope)) networkStates.set(scope, createNetworkViewState());
    return networkStates.get(scope)!;
  });
  const environmentKey = $derived(actions ? runtime.mechanicEnvironmentKey(actions.commands.scope) : '');
  const workspace = new MechanicWorkspaceState(() => {
    return {
      project,
      factory,
      entries,
      scopeKey: scope,
      expanded: filterExpanded,
      needsAllFlows: view === 'network',
      environmentKey,
      localize: (kind, name) => runtime.localizedName(kind, name),
    };
  }, (project, factory, id, mechanic) => actions!.loadFlow(id, mechanic));
  $effect(() => { workspace.syncScope(); });
  $effect(() => { void runtime.flowRevision; return workspace.loadIndex(); });

  $effect(() => {
    if (locationFilter !== undefined) {
      workspace.locationFilter = locationFilter;
    }
  });

  let lastTraceSeq = 0;
  $effect(() => {
    if (!traceRequest || traceRequest.seq === lastTraceSeq) return;
    lastTraceSeq = traceRequest.seq;
    untrack(() => {
      const filter = flowFilterFromDualVar(traceRequest.flow);
      workspace.textQuery = "";
      workspace.setFilter(traceRequest.side, filter);
      workspace.setFilter(traceRequest.side === "input" ? "output" : "input", null);
    });
  });

  let lastFocusSeq = 0;
  let highlightMechanicId = $state<number | null>(null);
  let highlightSeq = $state(0);
  $effect(() => {
    if (!focusRequest || focusRequest.seq === lastFocusSeq) return;
    if (view !== "list" || focusRequest.mechanicId == null) return;
    lastFocusSeq = focusRequest.seq;
    const targetId = focusRequest.mechanicId;
    untrack(() => {
      if (workspace.visibleIds && !workspace.visibleIds.includes(targetId)) {
        workspace.input = null;
        workspace.output = null;
        workspace.textQuery = "";
        if (locationFilter === undefined) workspace.locationFilter = null;
      }
      highlightMechanicId = targetId;
      highlightSeq += 1;
    });
  });

  let lastSearchFocusSeq = 0;
  $effect(() => {
    if (!searchFocusSeq || searchFocusSeq === lastSearchFocusSeq) return;
    lastSearchFocusSeq = searchFocusSeq;
    if (view !== "network") {
      untrack(() => {
        localFilterOpen = true;
        filterFocusTick += 1;
      });
    }
  });

  let visibleIds = $derived(workspace.visibleIds);
  let filterActive = $derived(workspace.active);
  $effect(() => { onFilterActiveChange(filterActive); });
</script>

{#if project !== null && factory !== null && actions && view !== 'network'}
  {#key scope}
    <MechanicFilter
      {workspace}
      {entries}
      expanded={filterExpanded}
      focusSearchSeq={filterFocusTick}
      externalLocationActive={locationFilter != null}
      onRequestClose={() => { localFilterOpen = false; onFilterClose(); }}
    />
  {/key}
{/if}
{#if (filterActive || groupLocations) && view === 'list'}<div class="filter-order-note">筛选或地点聚类时暂停手动排序，关闭后恢复原始顺序。</div>{/if}

{#if project !== null && factory !== null && actions && view === 'network'}
  {#key scope}
    <!-- Stage 2 wire: pass {locationFilter} {focusRequest} {onRevealInList} {searchFocusSeq} to MechanicNetworkView -->
    <MechanicNetworkView {project} {factory} {entries} {visibleIds} {mergeRecipes} {groupLocations} {actions} {solutions} {solved} {workspace} {scrollState} {filterOpen} {onFilterClose} viewState={networkState} />
  {/key}
{:else if project !== null && factory !== null && actions && entries.length > 0 && (visibleIds === null || visibleIds.length > 0)}
  {#if view === 'list'}
    {#key scope}
      <MechanicListView
        {project}
        {factory}
        {entries}
        {visibleIds}
        {mergeRecipes}
        {groupLocations}
        locationFilter={locationFilter ?? workspace.locationFilter}
        {highlightMechanicId}
        {highlightSeq}
        {onTraceFlow}
        {onRevealInNetwork}
        {actions}
        {solutions}
        {solved}
        {workspace}
        {scrollState}
      />
    {/key}
  {/if}
{:else}
  <div class="empty-state">
    {#if filterActive}
      <strong>没有匹配的机制</strong>
      <span>可更换搜索关键词、消耗或产出条件，或清除筛选查看全部机制。读取进度见筛选栏。</span>
    {:else if project !== null && factory !== null}
      <strong>还没有机制</strong>
      <span>从「+ 添加机制」开始，或让「自动规划」生成候选。</span>
    {:else if project !== null}
      <strong>选择一个工厂</strong>
      <span>在顶部工厂栏切换，或点「+」新建工厂。</span>
    {:else}
      <strong>新建或打开一个项目</strong>
      <span>项目栏在左上角；首次使用请先在「数据」里加载游戏数据。</span>
    {/if}
  </div>
{/if}

<style>
  .filter-order-note { flex: 0 0 auto; color: var(--muted); font-size: 12px; padding-top: 6px; }
  .empty-state { display: grid; justify-items: center; gap: 6px; padding: 56px 24px; color: var(--muted);
    border: 1px dashed var(--line-strong); border-radius: var(--radius); text-align: center; }
  .empty-state strong { color: var(--text); font-size: 12.5px; }
  .empty-state span { max-width: 360px; font-size: 11.5px; line-height: 1.6; }
</style>
