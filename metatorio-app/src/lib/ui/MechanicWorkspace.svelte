<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte";
  import { MechanicWorkspaceState } from "$lib/mechanics/workspace.svelte";
  import type { MechanicEntry } from '$lib/runtime/types';
  import type { MechanicActions } from '$lib/mechanics/actions';
  import { DEFAULT_MECHANIC_VIEW, type MechanicSolutionMap, type MechanicViewMode } from '$lib/mechanics/workspace';
  import MechanicFilter from './MechanicFilter.svelte';
  import MechanicListView from './MechanicListView.svelte';
  import MechanicNetworkView from './MechanicNetworkView.svelte';
  import { createNetworkViewState, type NetworkViewState } from '$lib/mechanics/network';
  import type { MechanicScrollState } from '$lib/mechanics/view-scroll';

  let {project, factory, entries, solutions, solved, actions, filterOpen, mergeRecipes,
    view = DEFAULT_MECHANIC_VIEW, onFilterActiveChange}: {
    project: number | null; factory: number | null; entries: MechanicEntry[];
    solutions: MechanicSolutionMap; solved: boolean; actions: MechanicActions | null;
    filterOpen: boolean; mergeRecipes: boolean; view?: MechanicViewMode;
    onFilterActiveChange: (active: boolean) => void;
  } = $props();

  // Filters live outside the renderer: changing views must not reset the query.
  let scope = $derived(`${project ?? 0}:${factory ?? 0}`);
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
  const workspace = new MechanicWorkspaceState(() => {
    const document = runtime.document?.projects.find(entry => entry.id === project);
    return {project, factory, entries, expanded: filterOpen, needsAllFlows: view === 'network',
      environmentKey: JSON.stringify([document?.context_id, runtime.effectiveContextId, document?.settings,
        document?.factories.find(entry => entry.id === factory)?.settings])};
  }, (project, factory, id, mechanic) => runtime.loadMechanicFlow(project, factory, id, mechanic));
  $effect(() => { workspace.syncScope(); });
  $effect(() => workspace.loadIndex());
  let visibleIds = $derived(workspace.visibleIds);
  let filterActive = $derived(workspace.active);
  $effect(() => { onFilterActiveChange(filterActive); });
</script>

{#if project !== null && factory !== null && actions}
  {#key scope}
    <MechanicFilter {workspace} {entries} expanded={filterOpen} />
  {/key}
{/if}
{#if filterActive && view === 'list'}<div class="filter-order-note">筛选中暂停手动拖拽排序，清除筛选后恢复。</div>{/if}

{#if project !== null && factory !== null && actions && entries.length > 0 && (visibleIds === null || visibleIds.length > 0)}
  {#if view === 'list'}
    {#key scope}
      <MechanicListView {project} {factory} {entries} {visibleIds} {mergeRecipes} {actions} {solutions} {solved} {workspace} {scrollState} />
    {/key}
  {:else if view === 'network'}
    {#key scope}
      <MechanicNetworkView {project} {factory} {entries} {visibleIds} {mergeRecipes} {actions} {solutions} {solved} {workspace} {scrollState} viewState={networkState} />
    {/key}
  {/if}
{:else}
  <div class="empty-state">
    {#if filterActive}
      <strong>没有匹配的机制</strong>
      <span>可更换消耗或产出条件，或清除筛选查看全部机制。读取进度见筛选栏。</span>
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
