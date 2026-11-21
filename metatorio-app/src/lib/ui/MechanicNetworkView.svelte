<script lang="ts">
  import {layoutLocations} from "$lib/mechanics/location-layout";
  import {networkPngTiles} from "$lib/mechanics/network-export";
  import {createNetworkPngSnapshot} from "$lib/mechanics/network-render";
  import {locationKey, locationLabel, transportLabel} from "$lib/mechanics/located";
  import { onMount, untrack, tick } from 'svelte';
  import type { Core, ElementDefinition, StylesheetJson } from 'cytoscape';
  import type { ElkNode } from 'elkjs/lib/elk-api';
  import { runtime } from '$lib/runtime/store.svelte';
  import {beginNetworkPng,writeNetworkPngTile,finishNetworkPng,abortNetworkPng} from '$lib/runtime/client';
  import { buildNetwork, networkFlowKey, networkRate, networkIconQualities, type NetworkContribution, type NetworkEdge, type NetworkViewState } from '$lib/mechanics/network';
  import type { MechanicViewProps } from '$lib/mechanics/workspace';
  import type { DualVar, Location, MechanicEntry } from '$lib/runtime/types';
  import { mechanicPrototype } from '$lib/display-labels';
  import { mechanicKindLabels, mechanicPrimary } from '$lib/mechanics/presentation';
  import { presentFlow, isEnergyFlow } from '$lib/dual-var';
  import { compactNumber } from '$lib/format';
  import { normalizeSearch } from '$lib/rich-text';
  import { iconTypeForKind, syntheticIconUrls } from './icon-types';
  import FlowRef from './FlowRef.svelte';
  import Icon from './Icon.svelte';
  import LocationIcon from './LocationIcon.svelte';
  import MechanicEditor from './MechanicEditor.svelte';
  import MechanicFilter from './MechanicFilter.svelte';
  import SecondaryMenu from './SecondaryMenu.svelte';
  import { rememberMechanicScroll } from '$lib/mechanics/view-scroll';

  let {
    project,
    factory,
    entries,
    visibleIds,
    solutions,
    solved,
    actions,
    workspace,
    mergeRecipes,
    groupLocations = false,
    viewState,
    scrollState,
    filterOpen,
    onFilterClose,
    locationFilter = null,
    focusRequest = null,
    onRevealInList,
    searchFocusSeq = 0,
    onFocusHandled,
  }: MechanicViewProps & {
    mergeRecipes: boolean;
    groupLocations?: boolean;
    viewState: NetworkViewState;
    filterOpen: boolean;
    onFilterClose: () => void;
    locationFilter?: Location | null;
    focusRequest?: { mechanicId?: number; flow?: DualVar; location?: Location; seq: number } | null;
    onRevealInList?: (mechanicId: number) => void;
    searchFocusSeq?: number;
    onFocusHandled?: (seq: number) => void;
  } = $props();
  let container = $state<HTMLDivElement>();
  let detailsPanel = $state<HTMLElement>(), searchBox = $state<HTMLDivElement>(), searchInput = $state<HTMLInputElement>();
  let searchOpen = $state(false), searchIndex = $state(0);
  const searchResultsId = $derived(`network-search-${project}-${factory}`);
  let ready = $state(false), busy = $state(false), error = $state('');
  let exporting = $state(false), exportCancelled = $state(false), exportStatus = $state(''), exportError = $state('');
  // 本渲染器按作用域通过 {#key} 挂载；在组件销毁阶段保持自身状态对象引用稳定。
  const ownerState = untrack(() => viewState);
  let collapse = $state(untrack(() => ownerState.collapse));
  let showUtilities = $state(untrack(() => ownerState.showUtilities));
  let onlyEnabled = $state(untrack(() => ownerState.onlyEnabled));
  let query = $state(untrack(() => ownerState.query));
  let iconScale = $state(untrack(() => ownerState.iconScale ?? 1));
  let chainScope = $state<'direct' | 'chain'>(untrack(() => ownerState.chainScope ?? 'chain'));
  let currentZoom = $state(untrack(() => ownerState.zoom ?? 1));
  let selected = $state<string | undefined>(untrack(() => ownerState.selected));
  let selectedMechanic = $state<number | null>(untrack(() => ownerState.selectedMechanic ?? null));
  let renderRevision = $state(0);
  let cy: Core | undefined;
  let elk: {layout: (graph: ElkNode) => Promise<ElkNode>; terminateWorker: () => void} | undefined;
  let generation = 0, mounted = true, renderedGrouping = untrack(() => groupLocations);
  let renderedLocationKey = untrack(() => locationFilter ? locationKey(locationFilter) : '');
  let renderedOwners = new Map<string,string>();
  let handledFocusSeq = 0;
  let unfilterAttemptSeq = 0;
  const document = $derived(runtime.document?.projects.find(p => p.id === project));
  const factoryDocument = $derived(actions.commands.scope.kind === "universe" ? null : document?.factories.find(f => f.id === factory));
  const universeDocument = $derived(actions.commands.scope.kind === 'universe' ? document?.universes?.find(u => u.id === factory) : null);
  let locatedSources = $state<import('$lib/runtime/types').LocatedDualVar[]>([]);
  $effect(() => {
    void universeDocument; void document?.settings;
    if (!universeDocument) { locatedSources = []; return; }
    let alive = true;
    runtime.loadUniversePreview(project, factory).then(p => {if (alive) locatedSources = p.sources;}).catch(() => {if (alive) locatedSources = [];});
    return () => {alive = false;};
  });
  const timeScale = $derived(document?.settings.time_scale ?? 'seconds');
  const model = $derived(buildNetwork({entries, visibleIds, index: workspace.index, solutions, solved,
    locatedTargets: [...(universeDocument?.targets.map(t => t.flow) ?? []), ...(universeDocument?.target_expressions.flatMap(e => e.terms.map(t => t.flow)) ?? [])],
    locatedImplicitInputs: locatedSources,
    locatedExternalInputs: universeDocument?.external_inputs??[],
    targets: [...(factoryDocument?.targets.map(t => t.flow) ?? []),
      ...(factoryDocument?.target_expressions.flatMap(expression => expression.terms.map(term => term.flow)) ?? [])],
    externalInputs: factoryDocument?.external_inputs ?? [], implicitInputs: universeDocument ? [] : runtime.implicitSourcesCache,
    localize: (kind, name) => runtime.localizedName(kind, name), timeScale,
    mergeRecipes, collapse, showUtilities, onlyEnabled, isolateLocations: groupLocations, locationFilter}));
  const entryById = $derived(new Map(entries.map(entry => [entry.id, entry])));
  const nodeById = $derived(new Map(model.nodes.map(node => [node.id, node])));
  const searchableNodes = $derived(model.nodes.map(node => ({ node, searchKey: normalizeSearch(node.search) })));
  const needle = $derived(normalizeSearch(query));
  const matches = $derived(
    needle
      ? searchableNodes.filter(({ searchKey }) => searchKey.includes(needle)).map(({ node }) => node)
      : model.nodes,
  );
  // 配方或机器编辑会改变合并后节点的 ID；即使节点移到另一分组，编辑器仍跟随稳定机制 ID。
  const selectedNode = $derived(selectedMechanic === null
    ? model.nodes.find(node => node.id === selected)
    : model.nodes.find(node => node.entries.some(entry => entry.id === selectedMechanic)));
  const selectedId = $derived(selectedNode?.id ?? (selectedMechanic === null ? selected : undefined));
  const selectedEdge = $derived(selectedMechanic === null ? model.edges.find(edge => edge.id === selected) : undefined);
  const editing = $derived(selectedNode?.entries.find(entry => entry.id === selectedMechanic) ?? selectedNode?.entries[0]);
  // 仅在组件存活期间持久保存视图状态；销毁钩子不得在作用域已切换后读取父级派生属性或模型。
  $effect(() => {
    Object.assign(ownerState, {collapse, showUtilities, onlyEnabled, query, iconScale, chainScope, selected: selectedId, selectedMechanic});
  });
  const neighbours = $derived.by(() => {
    const ids = new Set<string>();
    for (const edge of model.edges) {
      if (edge.source === selectedId) ids.add(edge.target);
      if (edge.target === selectedId) ids.add(edge.source);
    }
    return model.nodes.filter(node => ids.has(node.id));
  });
  const selectedEdgeEndpoints = $derived.by(() => {
    if (!selectedEdge) return [];
    const list: { role: string; node: (typeof model.nodes)[number] }[] = [];
    const sourceNode = nodeById.get(selectedEdge.source);
    const targetNode = nodeById.get(selectedEdge.target);
    if (sourceNode) list.push({ role: '来源', node: sourceNode });
    if (targetNode && targetNode.id !== sourceNode?.id) list.push({ role: '去向', node: targetNode });
    return list;
  });

  function palette(): Record<string, string> {
    const style = getComputedStyle(container!);
    const color = (key: string, fallback: string) => style.getPropertyValue(key).trim() || fallback;
    const isLight = globalThis.document.documentElement.getAttribute('data-theme') === 'light';
    return {text: color('--text', '#ccd3df'), muted: color('--muted', '#7e8b9d'),
      panel: color('--panel', '#202632'), line: color('--line-strong', '#627084'),
      highlight: isLight ? '#0969da' : '#4ab8ff'};
  }
  function styles(): StylesheetJson {
    const p = palette();
    const computed = getComputedStyle(container!);
    const uiFont = computed.fontFamily;
    const monoFont = computed.getPropertyValue('--mono').trim() || uiFont;
    return [
      {selector: 'node', style: {label: 'data(label)', color: p.text, 'font-size': 11 * iconScale, 'font-family': uiFont,
        'text-wrap': 'wrap', 'text-max-width': '150px', 'text-valign': 'bottom', 'text-margin-y': 5 * iconScale,
        width: 38 * iconScale, height: 38 * iconScale, 'background-color': '#477761', 'border-width': 2, 'border-color': p.line}},
      {selector: 'node[kind="location"]', style: {shape:'round-rectangle', 'background-opacity':0.06, 'background-color':p.text, 'border-style':'dashed', 'border-width':1, padding:'35px', 'text-valign':'top', 'font-size':16 * iconScale, 'font-weight':600, 'text-margin-y':-10 * iconScale}},
      {selector: 'node[kind="mechanic"]', style: {shape: 'round-rectangle', width: 52 * iconScale, height: 48 * iconScale, 'background-color': '#99703b'}},
      {selector: 'node[fluid]', style: {'background-color': '#3b78a4'}},
      {selector: 'node[utility]', style: {'background-color': '#79599d'}},
      {selector: 'node[kind="external"]', style: {shape: 'round-rectangle', 'background-color': '#69717b'}},
      {selector: 'node[implicit]', style: {'border-style': 'dashed'}},
      {selector: 'node[target]', style: {'border-color': '#d49b14', 'border-width': 4}},
      {selector: 'node[disabled], node[unused]', style: {opacity: 0.45, 'border-style': 'dotted'}},
      {selector: 'node[icon]', style: {'background-image': 'data(icon)', 'background-fit': 'contain', 'background-clip': 'none'}},
      {selector: 'edge', style: {width: 1.5, 'line-color': p.muted, 'target-arrow-color': p.muted,
        'target-arrow-shape': 'triangle', 'curve-style': 'bezier', label: 'data(label)', color: p.text,
        'font-size': 10 * iconScale, 'font-family': monoFont, 'text-wrap': 'wrap', 'text-max-width': '240px', 'text-background-color': p.panel,
        'text-background-opacity': 0.9, 'text-background-padding': '2px'}},
      {selector: 'edge[utility]', style: {opacity: 0.3, label: '', 'curve-style': 'straight'}},
      {selector: 'edge[kind="collapsed"]', style: {'font-family': uiFont}},
      {selector: 'edge[kind="compatibility"]', style: {'line-style': 'dashed', 'line-color': '#9673af', 'target-arrow-color': '#9673af', 'font-size': 9 * iconScale, 'font-family': uiFont}},
      {selector: '.dimmed', style: {opacity: 0.16}},
      {selector: 'node.chain-highlight', style: {'border-color': p.highlight, 'border-width': 3}},
      {selector: 'edge.chain-highlight', style: {width: 2.5, 'line-color': p.highlight, 'target-arrow-color': p.highlight, opacity: 1}},
      {selector: ':selected, .search-match', style: {'border-color': '#d49b14', 'border-width': 4, 'line-color': '#d49b14', 'target-arrow-color': '#d49b14'}},
    ];
  }
  function saveViewport(savePositions = false) {
    if (!cy || cy.destroyed() || renderRevision === 0) return;
    const z = cy.zoom();
    if (Number.isFinite(z)) currentZoom = z;
    ownerState.zoom = z; ownerState.pan = {...cy.pan()};
    if (savePositions) {
      for (const node of cy.nodes().filter(n=>!n.isParent())) {
        ownerState.positions.set(node.id(), {...node.position()});
      }
    }
  }
  function choose(id: string, preferredMechanicId?: number | null) {
    searchOpen = false;
    selectObject(id, preferredMechanicId);
    const element = cy?.getElementById(id);
    cy?.elements().unselect(); element?.select();
    if (element && !element.empty() && cy) {
      if (cy.zoom() < 0.65) cy.zoom(0.85);
      const target = element.isNode() ? element : cy.getElementById(id).connectedNodes();
      cy.animate({center: {eles: target}, duration: 150});
    }
  }
  function selectObject(id?: string, preferredMechanicId?: number | null) {
    const changed = selectedId !== id;
    selected = id;
    if (!id) {
      selectedMechanic = null;
      if (cy && !cy.destroyed()) {
        cy.batch(() => {
          cy!.elements().unselect().removeClass('dimmed chain-highlight');
        });
      }
    } else if (preferredMechanicId !== undefined) {
      selectedMechanic = preferredMechanicId;
    } else if (changed || selectedMechanic === null) {
      selectedMechanic = model.nodes.find(node => node.id === id)?.entries[0]?.id ?? null;
    }
    if (id && changed) void tick().then(() => detailsPanel?.focus({preventScroll: true}));
  }
  function computeChainIds(): { nodes: Set<string>; edges: Set<string> } | null {
    if (!selectedNode && !selectedEdge) return null;
    const highlightNodes = new Set<string>();
    const highlightEdges = new Set<string>();
    if (chainScope === 'direct') {
      if (selectedNode) {
        highlightNodes.add(selectedNode.id);
        for (const edge of model.edges) {
          if (edge.source === selectedNode.id) {
            highlightNodes.add(edge.target);
            highlightEdges.add(edge.id);
          } else if (edge.target === selectedNode.id) {
            highlightNodes.add(edge.source);
            highlightEdges.add(edge.id);
          }
        }
      } else if (selectedEdge) {
        highlightNodes.add(selectedEdge.source);
        highlightNodes.add(selectedEdge.target);
        highlightEdges.add(selectedEdge.id);
      }
      return { nodes: highlightNodes, edges: highlightEdges };
    }
    const incoming = new Map<string, NetworkEdge[]>();
    const outgoing = new Map<string, NetworkEdge[]>();
    for (const edge of model.edges) {
      let list = incoming.get(edge.target);
      if (!list) { list = []; incoming.set(edge.target, list); }
      list.push(edge);
      list = outgoing.get(edge.source);
      if (!list) { list = []; outgoing.set(edge.source, list); }
      list.push(edge);
    }
    const upstream = new Set<string>();
    const downstream = new Set<string>();
    const upQueue: string[] = [];
    const downQueue: string[] = [];
    if (selectedNode) {
      upstream.add(selectedNode.id);
      downstream.add(selectedNode.id);
      upQueue.push(selectedNode.id);
      downQueue.push(selectedNode.id);
    } else if (selectedEdge) {
      upstream.add(selectedEdge.source);
      downstream.add(selectedEdge.target);
      upQueue.push(selectedEdge.source);
      downQueue.push(selectedEdge.target);
      highlightEdges.add(selectedEdge.id);
    }
    while (upQueue.length > 0) {
      const current = upQueue.shift()!;
      for (const edge of incoming.get(current) ?? []) {
        highlightEdges.add(edge.id);
        if (!upstream.has(edge.source)) {
          upstream.add(edge.source);
          upQueue.push(edge.source);
        }
      }
    }
    while (downQueue.length > 0) {
      const current = downQueue.shift()!;
      for (const edge of outgoing.get(current) ?? []) {
        highlightEdges.add(edge.id);
        if (!downstream.has(edge.target)) {
          downstream.add(edge.target);
          downQueue.push(edge.target);
        }
      }
    }
    for (const id of upstream) highlightNodes.add(id);
    for (const id of downstream) highlightNodes.add(id);
    return { nodes: highlightNodes, edges: highlightEdges };
  }
  function applyChainHighlight() {
    if (!cy || cy.destroyed()) return;
    const chain = computeChainIds();
    cy.batch(() => {
      const targets = cy!.elements().not('[kind="location"]');
      targets.removeClass('dimmed chain-highlight');
      if (!chain) return;
      for (const ele of targets) {
        const id = ele.id();
        const hit = ele.isNode() ? chain.nodes.has(id) : chain.edges.has(id);
        ele.addClass(hit ? 'chain-highlight' : 'dimmed');
      }
    });
  }
  function focusMechanicInGraph(mechanicId: number) {
    const node = model.nodes.find(n => n.entries.some(e => e.id === mechanicId));
    if (!node) return false;
    choose(node.id, mechanicId);
    return true;
  }
  function resolveFocusTarget(req: NonNullable<typeof focusRequest>): { elementId: string; mechanicId?: number | null } | null {
    if (req.mechanicId !== undefined) {
      const node = model.nodes.find(n => n.entries.some(e => e.id === req.mechanicId));
      if (node) return { elementId: node.id, mechanicId: req.mechanicId };
      return null;
    }
    if (req.flow !== undefined) {
      const flowKey = networkFlowKey(req.flow);
      const locKey = req.location ? locationKey(req.location) : null;
      const flowNode = model.nodes.find(n =>
        n.kind === 'flow' &&
        n.flow &&
        networkFlowKey(n.flow) === flowKey &&
        (locKey === null || locationKey(n.location) === locKey)
      );
      if (flowNode) return { elementId: flowNode.id, mechanicId: null };
      const matchingEdge = model.edges.find(e => {
        if (!e.flow || networkFlowKey(e.flow) !== flowKey) return false;
        if (locKey === null) return true;
        return locationKey(nodeById.get(e.source)?.location) === locKey || locationKey(nodeById.get(e.target)?.location) === locKey;
      });
      if (matchingEdge) return { elementId: matchingEdge.id, mechanicId: null };
      return null;
    }
    if (req.location) {
      const locKey = locationKey(req.location);
      if (groupLocations) {
        const groupId = `location:${locKey}`;
        if (cy && !cy.getElementById(groupId).empty()) return { elementId: groupId, mechanicId: null };
      }
      const node = model.nodes.find(n => locationKey(n.location) === locKey);
      if (node) return { elementId: node.id, mechanicId: null };
    }
    return null;
  }
  function zoomStep(factor: number) {
    if (!cy || cy.destroyed()) return;
    const next = Math.max(0.08, Math.min(3, cy.zoom() * factor));
    cy.zoom({ level: next, renderedPosition: { x: cy.width() / 2, y: cy.height() / 2 } });
    saveViewport();
  }
  function resetZoom() {
    if (!cy || cy.destroyed()) return;
    cy.zoom({ level: 1, renderedPosition: { x: cy.width() / 2, y: cy.height() / 2 } });
    saveViewport();
  }
  function fitGraph() {
    if (!cy || cy.destroyed() || cy.elements().length === 0) return;
    cy.fit(undefined, 35);
    saveViewport();
  }
  function searchKey(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault(); event.stopPropagation(); searchOpen = false;
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault(); searchOpen = true;
      const count = Math.min(100, matches.length);
      searchIndex = count ? (searchIndex + (event.key === 'ArrowDown' ? 1 : count - 1)) % count : 0;
      void tick().then(() => globalThis.document.getElementById(`${searchResultsId}-${searchIndex}`)?.scrollIntoView({block: 'nearest'}));
    } else if (event.key === 'Enter' && searchOpen && matches[searchIndex]) {
      event.preventDefault(); choose(matches[searchIndex].id);
    }
  }
  function highlight() {
    if (!cy || cy.destroyed()) return;
    cy.batch(() => {
      cy!.nodes().removeClass('search-match');
      if (needle) for (const node of matches) cy!.getElementById(node.id).addClass('search-match');
    });
  }

  onMount(() => {
    mounted = true;
    void Promise.all([import('cytoscape'), import('elkjs/lib/elk-api.js'), import('elkjs/lib/elk-worker.min.js?worker')]).then(([graph, layout, worker]) => {
      if (!mounted || !container) return;
      cy = graph.default({container, elements: [], style: styles(), layout: {name: 'preset'},
        minZoom: 0.08, maxZoom: 3, pixelRatio: 'auto', textureOnViewport: false});
      elk = new layout.default({workerFactory: () => new worker.default()});
      cy.on('tap', 'node, edge', event => {if(event.target.isNode() && event.target.data('kind')==='location') return; selectObject(event.target.id());});
      cy.on('tap', event => { if (event.target === cy) selectObject(); });
      cy.on('pan zoom', () => { saveViewport(); });
      cy.on('dragfree', 'node', event => {
        const node = event.target;
        if(node.isParent()) for(const child of node.descendants()) ownerState.positions.set(child.id(), {...child.position()});
        else ownerState.positions.set(node.id(), {...node.position()});
      });
      ready = true;
    }).catch(reason => { if (mounted) error = `无法加载网络绘图库：${String(reason)}`; });
    const observer = new ResizeObserver(() => { cy?.resize(); });
    observer.observe(container!);
    const themeObserver = new MutationObserver(() => { if (cy && !cy.destroyed()) { cy.style(styles()); } });
    themeObserver.observe(globalThis.document.documentElement, {attributes: true, attributeFilter: ['data-theme', 'class', 'style']});
    const outsideSearch = (event: PointerEvent) => {
      if (!searchBox?.contains(event.target as Node)) searchOpen = false;
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented || (event.target as Element)?.closest('[aria-modal="true"], .secondary-menu')) return;
      if (filterOpen) { onFilterClose(); return; }
      if (selectedNode || selectedEdge || selectedId) { selectObject(); container?.focus({preventScroll: true}); }
    };
    globalThis.document.addEventListener('pointerdown', outsideSearch);
    window.addEventListener('keydown', escape);
    return () => {
      mounted = false; generation++; saveViewport(true);
      globalThis.document.removeEventListener('pointerdown', outsideSearch);
      window.removeEventListener('keydown', escape);
      observer.disconnect(); themeObserver.disconnect(); cy?.destroy(); elk?.terminateWorker(); cy = undefined;
    };
  });

  $effect(() => {
    if (!ready) return;
    const snapshot = model;
    const grouping = groupLocations;
    const locKey = locationFilter ? locationKey(locationFilter) : '';
    runtime.iconRevision;
    const revision = ++generation;
    const timer = setTimeout(() => {
      void draw(snapshot, revision, false, grouping, locKey).catch(reason => {
        if (revision === generation && mounted) { error = `网络布局失败：${String(reason)}`; busy = false; }
      });
    }, 100);
    return () => { clearTimeout(timer); if (generation === revision) generation++; };
  });
  $effect(() => { needle; matches; renderRevision; highlight(); });
  $effect(() => { selectedNode; selectedEdge; chainScope; renderRevision; model; applyChainHighlight(); });
  $effect(() => { if (searchIndex >= Math.min(100, matches.length)) searchIndex = 0; });
  // 图标大小调整只更新画布样式，不重新布局，也不重置平移或缩放。
  $effect(() => { if (ready && cy) { iconScale; cy.style(styles()); } });

  let prevSearchFocusSeq = untrack(() => searchFocusSeq ?? 0);
  $effect(() => {
    const seq = searchFocusSeq ?? 0;
    if (seq > prevSearchFocusSeq) {
      prevSearchFocusSeq = seq;
      void tick().then(() => {
        searchInput?.focus();
        searchInput?.select();
      });
    } else {
      prevSearchFocusSeq = seq;
    }
  });

  $effect(() => {
    const req = focusRequest;
    if (!req || req.seq === handledFocusSeq) return;
    if (!ready || busy || renderRevision === 0 || !cy || cy.destroyed()) return;
    const target = resolveFocusTarget(req);
    if (target) {
      const ele = cy.getElementById(target.elementId);
      if (!ele.empty()) {
        handledFocusSeq = req.seq;
        onFocusHandled?.(req.seq);
        if (ele.isNode() && ele.data('kind') === 'location') {
          cy.animate({ center: { eles: ele }, duration: 150 });
        } else {
          choose(target.elementId, target.mechanicId);
        }
        return;
      }
    }
    if (req.flow && !showUtilities && unfilterAttemptSeq !== req.seq &&
        (isEnergyFlow(req.flow) || (typeof req.flow !== 'string' && 'Pollution' in req.flow))) {
      unfilterAttemptSeq = req.seq;
      showUtilities = true;
      return;
    }
    if (req.mechanicId !== undefined && onlyEnabled && unfilterAttemptSeq !== req.seq &&
        entries.some(e => e.id === req.mechanicId && !e.enabled)) {
      unfilterAttemptSeq = req.seq;
      onlyEnabled = false;
      return;
    }
    if (!workspace.pending) {
      handledFocusSeq = req.seq;
      onFocusHandled?.(req.seq);
    }
  });

  async function draw(snapshot: typeof model, revision: number, reset = false, grouping = groupLocations, locKey = renderedLocationKey) {
    if (!cy || !elk || !mounted || revision !== generation) return;
    busy = true; error = '';
    if (renderedGrouping !== grouping || renderedLocationKey !== locKey) {
      reset = true;
      ownerState.positions.clear();
      ownerState.zoom = undefined;
    }
    if (!reset) {
      saveViewport(true);
      // 被重命名的分组尽量停留在用户当前编辑位置；若目标分组已存在则保留其原有位置。
      const oldPosition = cy.$(':selected').nodes().first().position();
      if (selectedId && oldPosition && !ownerState.positions.has(selectedId)) {
        ownerState.positions.set(selectedId, {...oldPosition});
      }
    }
    const material = snapshot.nodes.filter(node => !node.utility);
    const laidOut = grouping ? {children:[]} : await elk.layout({id: 'root', layoutOptions: {
      'elk.algorithm': 'layered', 'elk.direction': 'RIGHT', 'elk.edgeRouting': 'ORTHOGONAL',
      'elk.spacing.nodeNode': '45', 'elk.layered.spacing.nodeNodeBetweenLayers': '100',
      'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX',
    // 预先按最大图标尺寸与多行标签预留节点占位，确保调节图标大小时无需重排且不遮挡文字。
    }, children: material.map(node => ({id: node.id, width: node.kind === 'mechanic' ? 145 : 115,
      height: Math.max(100, (node.kind === 'mechanic' ? 48 : 38) * 2 + 16 + node.label.split('\n').length * 26)})),
      edges: snapshot.edges.filter(edge => !edge.utility).map(edge => ({id: edge.id, sources: [edge.source], targets: [edge.target]}))});
    if (!cy || !mounted || revision !== generation) return;
    let positions = new Map(laidOut.children?.map(node => [node.id, {x: (node.x ?? 0) + 75, y: (node.y ?? 0) + 45}]));
    const bottom = Math.max(0, ...[...positions.values()].map(p => p.y)) + 140;
    snapshot.nodes.filter(node => node.utility).forEach((node, i) => positions.set(node.id, {x: 80 + i * 150, y: bottom}));
    const clustered = grouping ? await layoutLocations(snapshot, graph=>elk!.layout(graph)) : null;
    if (!cy || !mounted || revision !== generation) return;
    if(clustered) {
      positions = clustered.positions;
      // 新增或移入其他地点的节点应加入新分区，而不是把新地点外框拉扯回旧聚类里保存过的位置。
      if(clustered.owner.size!==renderedOwners.size || [...clustered.owner].some(([id,owner])=>renderedOwners.get(id)!==owner)) ownerState.positions.clear();
      renderedOwners = clustered.owner;
    } else renderedOwners.clear();
    renderedGrouping = grouping;
    renderedLocationKey = locKey;
    const hadViewport = ownerState.zoom !== undefined;
    const elements: ElementDefinition[] = snapshot.nodes.map(node => ({data: {id: node.id, label: node.label, kind: node.kind,
      ...(clustered?.owner.has(node.id)?{parent:clustered.owner.get(node.id)}:{}),
      ...(node.flow && typeof node.flow !== 'string' && 'Fluid' in node.flow ? {fluid: true} : {}),
      ...(node.utility ? {utility: true} : {}), ...(node.target ? {target: true} : {}),
      ...(node.implicit ? {implicit: true} : {}), ...(node.disabled ? {disabled: true} : {}), ...(node.unused ? {unused: true} : {})},
      position: ownerState.positions.get(node.id) ?? positions.get(node.id)}));
    if(clustered) elements.unshift(...clustered.groups.map(group=>({data:{id:group.id,kind:'location',label:locationLabel(group.location,(k,n)=>runtime.localizedName(k,n))},selectable:false})));
    elements.push(...snapshot.edges.map(edge => ({data: {id: edge.id, source: edge.source, target: edge.target,
      label: edge.label, kind: edge.kind, ...(edge.utility ? {utility: true} : {})}})));
    cy.batch(() => { cy!.elements().remove(); cy!.add(elements); });
    if (hadViewport) { cy.zoom(ownerState.zoom!); cy.pan(ownerState.pan ?? {x: 0, y: 0}); }
    else {
      if (elements.length > 0) cy.fit(undefined, 35);
      if (cy.zoom() < 0.4) {
        const focus = snapshot.nodes.find(node => node.target) ?? snapshot.nodes.find(node => node.kind === 'mechanic');
        cy.zoom(0.65);
        if (focus) cy.center(cy.getElementById(focus.id));
      }
    }
    if (selectedId) cy.getElementById(selectedId).select();
    renderRevision++;
    const z = cy.zoom();
    if (Number.isFinite(z)) currentZoom = z;
    // 图标请求由 runtime 缓存统一去重；文档、上下文、工厂或视图切换后，旧代数请求不得再写入画布。
    for (let offset = 0; offset < snapshot.nodes.length && revision === generation && mounted; offset += 8) {
      const updates = await Promise.all(snapshot.nodes.slice(offset, offset + 8).map(async node => {
        const info = node.flow ? presentFlow(node.flow, (k, n) => runtime.localizedName(k, n)) : undefined;
        const ref = node.entries[0] ? mechanicPrototype(node.entries[0].mechanic) : undefined;
        const name = info?.name ?? ref?.name, type = info?.type ?? (ref ? iconTypeForKind(ref.kind) : undefined);
        if (!name || !type) return null;
        const url = syntheticIconUrls[name] ?? await runtime.getIcon(type, name);
        if (!url || !mounted || revision !== generation) return null;
        const qualities = networkIconQualities(node);
        const badges = qualities.length > 1 ? qualities : qualities.filter(quality => quality !== 'normal');
        const qualityUrls = await Promise.all(badges.map(quality => runtime.getIcon('quality', quality)));
        const locationUrl = node.location ? await runtime.getIcon(node.location.kind==='planet'?'planet':'item',node.location.kind==='planet'?node.location.name:'space-platform-starter-pack') : null;
        const images = qualityUrls.filter((badgeUrl): badgeUrl is string => !!badgeUrl);
        const badgeSize = Math.min(45, 100 / Math.max(1, images.length));
        return {
          id: node.id,
          url,
          style: {
            'background-image': [url, ...images, ...(locationUrl ? [locationUrl] : [])],
            'background-fit': ['contain' as const, ...images.map(() => 'none' as const), ...(locationUrl ? ['none' as const] : [])],
            'background-width': ['auto', ...images.map(() => `${badgeSize}%`), ...(locationUrl ? ['40%'] : [])],
            'background-height': ['auto', ...images.map(() => `${badgeSize}%`), ...(locationUrl ? ['40%'] : [])],
            'background-position-x': ['50%', ...images.map((_, i) => `${images.length === 1 ? 0 : i * 100 / (images.length - 1)}%`), ...(locationUrl ? ['100%'] : [])],
            'background-position-y': ['50%', ...images.map(() => '100%'), ...(locationUrl ? ['0%'] : [])],
          },
        };
      }));
      if (cy && !cy.destroyed() && mounted && revision === generation) {
        const readyUpdates = updates.filter((u): u is NonNullable<typeof u> => u !== null);
        if (readyUpdates.length > 0) {
          cy.batch(() => {
            for (const update of readyUpdates) {
              cy!.getElementById(update.id).data('icon', update.url).style(update.style);
            }
          });
        }
      }
    }
    if (revision === generation && mounted) busy = false;
  }
  function relayout() {
    ownerState.positions.clear(); ownerState.zoom = undefined;
    void draw(model, ++generation, true).catch(reason => { error = `网络布局失败：${String(reason)}`; busy = false; });
  }
  async function exportPng() {
    if (!cy || busy || exporting) return;
    exporting = true; exportCancelled = false; exportError = ''; exportStatus = '正在准备分区渲染…';
    let exportId:number|null=null;
    let snapshot:Awaited<ReturnType<typeof createNetworkPngSnapshot>>|undefined;
    try {
      snapshot=await createNetworkPngSnapshot(cy,palette().panel,()=>exportCancelled||!mounted);
      exportStatus='请选择保存位置并确认保存…';
      const session=await beginNetworkPng(`metatorio-network-${project}-${factory}.png`,snapshot.width,snapshot.height);
      if(!session){exportStatus='已取消导出';return;}
      exportId=session.id;
      const total=Math.ceil(snapshot.width/session.tile_width)*Math.ceil(snapshot.height/session.tile_height);
      let completed=0;
      for(const tile of networkPngTiles(snapshot.width,snapshot.height,session.tile_width,session.tile_height)){
        if(exportCancelled||!mounted)throw new Error('已取消导出');
        exportStatus=`正在渲染与合成分区 ${completed+1}/${total}…`;
        const png=await snapshot.render(tile);
        await writeNetworkPngTile(exportId,tile.x,tile.y,png);
        completed++;
      }
      if(exportCancelled||!mounted)throw new Error('已取消导出');
      exportStatus='正在完成 PNG 文件…';
      const path=await finishNetworkPng(exportId);exportId=null;
      exportStatus=`PNG 已保存到：${path}`;
    } catch(reason){
      if(exportCancelled||!mounted){exportStatus='已取消导出';}
      else {exportStatus='';exportError=`PNG 导出失败：${String(reason)}`;}
    } finally {
      if(exportId!==null)await abortNetworkPng(exportId).catch(reason=>{exportError ||= `清理临时导出失败：${String(reason)}`;});
      snapshot?.destroy();exporting=false;
    }
  }

  function mechanicUnit(entry: MechanicEntry): string {
    if (entry.transport) return '件/s';
    const kind = entry.mechanic.type;
    if (kind === 'recipe' || kind === 'mining' || kind === 'tile-extract' ||
        ['generator', 'fusion-generator', 'boiler', 'reactor', 'fusion-reactor', 'solar'].includes(kind)) {
      return '台';
    }
    return '系数';
  }
  function machineDisplay(entry: MechanicEntry): string {
    const machineId = entry.mechanic.machine?.id;
    if (!machineId) return '';
    const ns = entry.mechanic.type === 'mining' ? 'mining-machine' : entry.mechanic.type === 'tile-extract' ? 'entity' : 'machine';
    const name = runtime.localizedName(ns, machineId);
    const quality = entry.mechanic.machine?.quality;
    return quality && quality !== 'normal' ? `${name}（${runtime.localizedName('quality', quality)}）` : name;
  }
  function variantMeta(entry: MechanicEntry) {
    const primary = mechanicPrimary(entry.mechanic);
    const quality = primary.quality;
    const qualityLabel = runtime.localizedName('quality', quality);
    const machineLabel = machineDisplay(entry);
    const sol = solutions.get(entry.id);
    const unit = mechanicUnit(entry);
    const amountText = solved ? `${compactNumber(entry.enabled ? (sol?.amount ?? 0) : 0)} ${unit}` : '';
    return { quality, qualityLabel, machineLabel, amountText, enabled: entry.enabled };
  }
  function contributionInfo(item: NetworkContribution) {
    const entry = entryById.get(item.mechanic);
    const role = item.unitAmount < 0 ? '消耗' : '产出';
    const rate = selectedEdge?.flow ? networkRate(selectedEdge.flow, solved ? item.amount : item.unitAmount, timeScale) : '';
    if (!entry) {
      return { name: '未知机制', kind: '机制', machine: '', quality: 'normal', location: undefined as Location | undefined, role, rate, enabled: true };
    }
    const ref = mechanicPrototype(entry.mechanic);
    const primary = mechanicPrimary(entry.mechanic);
    const kind = entry.transport ? transportLabel(entry.transport) : mechanicKindLabels[entry.mechanic.type] ?? entry.mechanic.type;
    const name = ref.name ? runtime.localizedName(ref.kind, ref.name) : kind;
    return {
      name,
      kind,
      machine: machineDisplay(entry),
      quality: primary.quality,
      location: entry.location,
      role,
      rate,
      enabled: entry.enabled,
    };
  }
  function edgeDescription(edge: NetworkEdge) {
    return edge.kind === 'compatibility' ? '可能的兼容转换，未表示实际分配量。'
      : edge.kind === 'collapsed' ? '中间流的依赖折叠；产出与消耗分别计量，不代表点对点运输分配。'
      : edge.kind === 'external' ? '可用的外部来源；此边不表示实际导入数量。' : edge.label;
  }
</script>

<section class="network-view" use:rememberMechanicScroll={scrollState} aria-label="机制网络视图" data-network-ready={ready && !busy && renderRevision > 0}>
  <div class="network-toolbar">
    <div class="network-search" bind:this={searchBox}>
      <input bind:this={searchInput} aria-label="搜索网络节点" placeholder="搜索名称、ID、品质或温度…"
        role="combobox" aria-autocomplete="list" aria-expanded={searchOpen} aria-controls={searchResultsId}
        aria-activedescendant={searchOpen && matches[searchIndex] ? `${searchResultsId}-${searchIndex}` : undefined}
        bind:value={query} onfocus={() => searchOpen = true} oninput={() => { searchOpen = true; searchIndex = 0; }} onkeydown={searchKey} />
      {#if searchOpen}
        <div class="search-dropdown">
          <div class="search-count">{matches.length} 个{needle ? '匹配' : '节点'}</div>
          <div id={searchResultsId} class="search-results" role="listbox" aria-label="网络节点搜索结果">
            {#each matches.slice(0, 100) as node, i (node.id)}
              <button id={`${searchResultsId}-${i}`} class="search-result" role="option" aria-selected={searchIndex === i}
                onpointermove={() => searchIndex = i} onpointerdown={event => event.preventDefault()} onclick={() => choose(node.id)}>{#if node.location}<LocationIcon location={node.location} size={18}/>{/if}{node.label.replaceAll('\n', ' · ')}</button>
            {:else}<span class="no-matches">没有匹配的节点</span>{/each}
          </div>
          {#if matches.length > 100}<div class="search-count">仅列出前 100 个，请搜索缩小范围。</div>{/if}
        </div>
      {/if}
    </div>
    <button class="btn" disabled={!ready || busy} onclick={relayout}>重排</button>
    <button class="btn" disabled={!ready} onclick={fitGraph}>适应窗口</button>
    {#if needle}<button class="btn" disabled={!ready || !matches.length} onclick={() => choose(matches[0].id)}>定位匹配</button>{/if}
    <button class="btn" disabled={!ready || busy || exporting} onclick={exportPng}>{exporting ? '导出中…' : '导出 PNG'}</button>
    {#if exporting}<button class="btn" disabled={exportCancelled} onclick={()=>exportCancelled=true}>取消导出</button>{/if}
    <label title="图标与网络文字同比缩放，保持节点位置和画布缩放">显示大小
      <select aria-label="网络显示大小" bind:value={iconScale}>
        {#each [0.75, 1, 1.25, 1.5, 2] as scale}<option value={scale}>{scale * 100}%</option>{/each}
      </select>
    </label>
    <label><input type="checkbox" bind:checked={collapse} />折叠中间流</label>
    <label><input type="checkbox" bind:checked={showUtilities} />能量与污染</label>
    <label><input type="checkbox" bind:checked={onlyEnabled} />仅启用</label>
  </div>
  <div class="network-status" role="status">
    <span class="status-pill">{model.nodes.filter(node => node.kind === 'mechanic').length} 个机制节点 · {model.edges.length} 条关系</span>
    <span>{solved ? '实际求解流量' : '未求解：单位机制流量；分组变体分别列量'}</span>
    {#if !ready}<span>加载绘图库…</span>{:else if busy}<span>布局中…</span>{/if}
    {#if workspace.pending}<span>展开 {workspace.completed} / {entries.length}</span>{/if}
    {#if workspace.failures.length}<span class="warn">{workspace.failures.length} 个机制展开失败</span><button class="btn status-btn" onclick={() => workspace.retry++}>重试展开</button>{/if}
  </div>
  {#if error}<div class="network-error" role="alert">{error}<button class="btn" onclick={relayout}>重试布局</button></div>{/if}
  <div class="export-status" role="status" aria-live="polite" aria-atomic="true">{exportStatus}</div>
  {#if exportError}<div class="network-error" role="alert">{exportError}</div>{/if}
  <div class="network-stage" class:has-details={!!(selectedNode || selectedEdge)}>
    <!-- Keyboard focus returns here when the node editor is closed. -->
    <div class="network-canvas" bind:this={container} role="group" tabindex="-1" aria-label="生产网络画布"></div>
    <div class="zoom-hud" role="group" aria-label="画布缩放控制">
      <button type="button" class="hud-btn" disabled={!ready} aria-label="缩小画布" title="缩小画布" onclick={() => zoomStep(1 / 1.25)}>−</button>
      <button type="button" class="hud-btn hud-zoom mono" disabled={!ready} title="点击复位为 100%" onclick={resetZoom}>{Math.round(currentZoom * 100)}%</button>
      <button type="button" class="hud-btn" disabled={!ready} aria-label="放大画布" title="放大画布" onclick={() => zoomStep(1.25)}>+</button>
      <button type="button" class="hud-btn hud-fit" disabled={!ready} title="适应窗口" onclick={fitGraph}>适应</button>
    </div>
    <div class="network-help">
      <SecondaryMenu title="图例与来源">
        {#snippet trigger(toggle, open)}
          <button class="help-button" aria-label="图例与来源" aria-expanded={open} aria-haspopup="dialog" onclick={toggle}>ⓘ 图例</button>
        {/snippet}
        <dl class="network-legend">
          <dt>方框</dt><dd>机制／外部来源</dd>
          <dt>圆形</dt><dd>流</dd>
          <dt>金边</dt><dd>目标</dd>
          <dt>虚线来源</dt><dd>隐式输入</dd>
          <dt>紫色虚线</dt><dd>兼容关系，不表示实际分配量</dd>
        </dl>
        <p class="network-note">未显示求解器的降温与提热辅助变量。</p>
        <p class="network-credit">参考 <a href="https://github.com/YoisakiKanade222" target="_blank" rel="noopener noreferrer">stduuzurl / yoisaki kanade（YoisakiKanade222）</a> 的网络可视化器</p>
      </SecondaryMenu>
    </div>
    {#if !model.nodes.length && ready}
      <div class="network-empty">
        {#if workspace.active}
          没有匹配的机制，可更换条件或清除筛选。
        {:else if locationFilter}
          当前地点（{locationLabel(locationFilter, (k, n) => runtime.localizedName(k, n))}）暂无网络节点。
        {:else}
          还没有机制，可添加机制或使用自动规划。
        {/if}
      </div>
    {/if}
    {#if filterOpen || workspace.active}
      <div class="network-filter" class:expanded={filterOpen}>
        {#if filterOpen}<header><strong>机制流筛选</strong><button class="icon-btn" aria-label="收起机制筛选" onclick={onFilterClose}>×</button></header>{/if}
        <MechanicFilter {workspace} {entries} expanded={filterOpen} />
      </div>
    {/if}
  {#if selectedNode || selectedEdge}
    <div class="network-details" role="dialog" aria-label="网络对象详情" tabindex="-1" bind:this={detailsPanel}>
      <div class="details-head">
        <div class="details-title">
          {#if selectedNode}
            {#if selectedNode.location}<LocationIcon location={selectedNode.location} size={18} />{/if}
            {#if selectedNode.flow}<FlowRef flow={selectedNode.flow} />{:else}<strong>{selectedNode.label.replaceAll('\n', ' · ')}</strong>{/if}
          {:else if selectedEdge}
            {#if selectedEdge.flow}<FlowRef flow={selectedEdge.flow} />{/if}
          {/if}
        </div>
        <div class="details-head-actions">
          {#if onRevealInList && selectedNode?.kind === 'mechanic' && editing}
            <button type="button" class="btn ghost details-reveal" onclick={() => onRevealInList(editing.id)}>在列表中定位</button>
          {/if}
          <button type="button" class="icon-btn close" aria-label="关闭网络详情" onclick={() => { selectObject(); container?.focus({preventScroll: true}); }}>×</button>
        </div>
      </div>
      <div class="chain-scope-bar" role="group" aria-label="上下游高亮范围">
        <span class="chain-scope-label">链路高亮</span>
        <div class="chain-scope-switch">
          <button type="button" class="scope-btn" class:active={chainScope === 'direct'} aria-pressed={chainScope === 'direct'} onclick={() => chainScope = 'direct'}>直接相邻</button>
          <button type="button" class="scope-btn" class:active={chainScope === 'chain'} aria-pressed={chainScope === 'chain'} onclick={() => chainScope = 'chain'}>完整上下游</button>
        </div>
      </div>
      {#if selectedNode}
        {#if selectedNode.kind === 'external'}<p class="details-meta">{selectedNode.implicit ? '隐式外部输入 · 成本 0' : `手动外部输入 · 成本 ${selectedNode.external?.penalty}`}</p>{/if}
        {#if selectedNode.entries.length > 1}
          <div class="variant-options" role="group" aria-label="合并机制变体">
            {#each selectedNode.entries as entry (entry.id)}
              {@const meta = variantMeta(entry)}
              <button
                type="button"
                class="btn variant-btn"
                class:active={editing?.id === entry.id}
                class:disabled-variant={!meta.enabled}
                data-mechanic-id={entry.id}
                aria-pressed={editing?.id === entry.id}
                title={[meta.qualityLabel, meta.machineLabel, meta.amountText || '未求解', !meta.enabled ? '已停用' : ''].filter(Boolean).join(' · ')}
                onclick={() => selectedMechanic = entry.id}
              >
                <Icon type="quality" name={meta.quality} size={13} bare />
                <span>{meta.qualityLabel}</span>
                {#if solved}
                  <span class="variant-amount mono">{meta.amountText}</span>
                {:else if meta.machineLabel}
                  <span class="variant-machine">{meta.machineLabel}</span>
                {/if}
                {#if !meta.enabled}<span class="variant-off">停用</span>{/if}
              </button>
            {/each}
          </div>
        {/if}
        {#if editing}<div class="edit-card"><MechanicEditor entry={editing} {project} {factory} {actions} solution={solutions.get(editing.id) ?? (solved ? {amount: 0, cost: 0} : null)} /></div>{/if}
        {#if neighbours.length}<details class="neighbours"><summary>相邻对象（{neighbours.length}）</summary><div class="node-options">{#each neighbours as node (node.id)}<button type="button" class="btn" onclick={() => choose(node.id)}>{#if node.location}<LocationIcon location={node.location} size={18}/>{/if}{node.label.replaceAll('\n', ' · ')}</button>{/each}</div></details>{/if}
      {:else if selectedEdge}
        <p class="details-meta">{edgeDescription(selectedEdge)}</p>
        {#if selectedEdge.contributions.length}
          <div class="contrib-list" role="group" aria-label="关系贡献机制">
            {#each selectedEdge.contributions as item, i (`${item.mechanic}:${i}`)}
              {@const info = contributionInfo(item)}
              <button
                type="button"
                class="btn contrib-btn"
                class:disabled-variant={!info.enabled}
                data-mechanic-id={item.mechanic}
                onclick={() => focusMechanicInGraph(item.mechanic)}
              >
                <span class="contrib-main">
                  {#if info.location}<LocationIcon location={info.location} size={15} />{/if}
                  {#if info.quality && info.quality !== 'normal'}<Icon type="quality" name={info.quality} size={13} bare />{/if}
                  <strong class="contrib-name">{info.name}</strong>
                  <span class="contrib-meta">{info.kind}{#if info.machine} · {info.machine}{/if}</span>
                </span>
                <span class="contrib-rate mono">{info.role} {info.rate}</span>
              </button>
            {/each}
          </div>
        {/if}
        {#if selectedEdgeEndpoints.length}
          <details class="neighbours" open>
            <summary>连接节点</summary>
            <div class="node-options">
              {#each selectedEdgeEndpoints as endpoint (endpoint.role + ':' + endpoint.node.id)}
                <button type="button" class="btn" onclick={() => choose(endpoint.node.id)}>
                  <span class="endpoint-role">{endpoint.role}</span>
                  {#if endpoint.node.location}<LocationIcon location={endpoint.node.location} size={16} />{/if}
                  <span>{endpoint.node.label.replaceAll('\n', ' · ')}</span>
                </button>
              {/each}
            </div>
          </details>
        {/if}
      {/if}
    </div>
  {/if}
  </div>

</section>

<style>
  .network-view {
    display: flex;
    flex-direction: column;
    flex: 1 0 auto;
    min-width: 0;
    margin-top: 6px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--panel);
    box-shadow: var(--shadow-sm, var(--shadow));
    overflow: hidden;
  }
  .network-toolbar, .network-status, .node-options, .variant-options { display: flex; flex-wrap: wrap; gap: 5px; align-items: center; }
  .network-toolbar {
    gap: 4px;
    padding: 5px 8px;
    background: var(--panel-elev);
    border-bottom: 1px solid var(--line);
  }
  .network-toolbar .btn { min-height: 24px; padding: 0 7px; font-size: 11px; }
  .network-search { position: relative; min-width: 120px; width: 158px; max-width: 100%; }
  .network-search input {
    width: 100%;
    min-height: 24px;
    padding: 0 7px;
    box-sizing: border-box;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 11px;
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      box-shadow var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .network-search input::placeholder { color: var(--faint); }
  .network-search input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }
  .search-dropdown {
    position: absolute;
    z-index: 22;
    top: calc(100% + 6px);
    left: 0;
    width: min(430px, calc(100vw - 32px));
    max-width: calc(100vw - 32px);
    background: var(--surface-glass, var(--panel));
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
    overflow: hidden;
  }
  .search-count, .no-matches { padding: 6px 10px; color: var(--faint); font-size: 10.5px; }
  .search-count { background: var(--panel-elev); border-bottom: 1px solid var(--line); font-variant-numeric: tabular-nums; }
  .search-results { max-height: min(320px, 45vh); overflow: auto; display: grid; }
  .search-result {
    display: flex;
    align-items: center;
    gap: 6px;
    text-align: left;
    padding: 7px 10px;
    color: var(--text);
    background: none;
    border: 0;
    font: inherit;
    font-size: 11px;
    cursor: pointer;
    overflow-wrap: anywhere;
    transition: background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .search-result:hover, .search-result[aria-selected="true"] { background: var(--card-hover); }
  .network-toolbar label {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-height: 24px;
    color: var(--muted);
    font-size: 11px;
    white-space: nowrap;
    user-select: none;
  }
  .network-toolbar select {
    min-height: 24px;
    padding: 0 5px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 11px;
    cursor: pointer;
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .network-toolbar select:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }
  .network-toolbar label:has(input[type="checkbox"]) {
    gap: 4px;
    padding: 0 6px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .network-toolbar label:has(input[type="checkbox"]):hover {
    color: var(--text);
    background: var(--card-hover);
    border-color: var(--line-strong);
  }
  .network-toolbar label:has(input[type="checkbox"]:checked) {
    color: var(--text);
    background: var(--accent-dim);
    border-color: var(--accent-line);
  }
  .network-toolbar input[type="checkbox"] {
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .network-status {
    padding: 3px 8px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    color: var(--muted);
    font-size: 10.5px;
    line-height: 1.3;
    font-variant-numeric: tabular-nums;
    gap: 4px 10px;
  }
  .status-pill { color: var(--text); font-weight: 500; }
  .status-btn { min-height: 20px; padding: 0 7px; font-size: 10.5px; }
  .network-stage { position: relative; min-width: 0; min-height: 360px; flex: 1 0 clamp(360px, 62vh, 820px); }
  .network-canvas { position: absolute; inset: 0; height: 100%; width: 100%; }
  .zoom-hud {
    position: absolute;
    left: 8px;
    bottom: 8px;
    z-index: 10;
    display: inline-flex;
    align-items: center;
    gap: 1px;
    padding: 2px;
    background: var(--surface-glass, var(--panel));
    backdrop-filter: blur(8px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-sm, var(--shadow));
  }
  .hud-btn {
    min-width: 24px;
    min-height: 24px;
    padding: 0 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    font: inherit;
    font-size: 11px;
    font-weight: 550;
    line-height: 1;
    cursor: pointer;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .hud-btn:hover:not(:disabled) {
    color: var(--text);
    background: var(--card-hover);
    border-color: var(--line);
  }
  .hud-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .hud-zoom { min-width: 44px; font-variant-numeric: tabular-nums; }
  .network-help { position: absolute; right: 8px; bottom: 8px; z-index: 10; transition: right var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)); }
  @media (min-width: 700px) {
    .network-stage.has-details .network-help { right: calc(min(390px, calc(100% - 16px)) + 16px); }
  }
  .help-button {
    min-height: 26px;
    padding: 2px 9px;
    font: inherit;
    font-size: 11px;
    font-weight: 550;
    cursor: pointer;
    color: var(--muted);
    background: var(--surface-glass, var(--panel));
    backdrop-filter: blur(8px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-sm, var(--shadow));
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .help-button:hover { color: var(--text); background: var(--card-hover); border-color: var(--muted); }
  .network-legend { display: grid; grid-template-columns: auto 1fr; gap: 5px 12px; font-size: 12px; margin: 0; }
  .network-legend dt { color: var(--muted); }
  .network-legend dd { margin: 0; }
  .network-note, .network-credit { color: var(--muted); font-size: 11px; line-height: 1.6; margin: 10px 0 0; }
  .neighbours summary { cursor: pointer; color: var(--muted); font-size: 11px; margin-top: 8px; }
  .neighbours summary:hover { color: var(--text); }
  .node-options { padding-top: 6px; max-height: 200px; overflow: auto; }
  .node-options .btn { white-space: normal; text-align: left; font-size: 11px; max-width: 100%; overflow-wrap: anywhere; }
  .endpoint-role {
    color: var(--faint);
    font-size: 10px;
    font-weight: 600;
    padding: 1px 4px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .network-details {
    position: absolute;
    z-index: 15;
    top: 8px;
    right: 8px;
    bottom: 8px;
    width: min(390px, calc(100% - 16px));
    overflow: auto;
    box-sizing: border-box;
    padding: 12px;
    color: var(--text);
    background: var(--surface-glass, var(--panel));
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }
  .details-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 8px;
  }
  .details-title {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1 1 auto;
  }
  .details-title > strong {
    display: block;
    font-size: 12.5px;
    font-weight: 650;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .details-head-actions {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: 0 0 auto;
  }
  .details-reveal { min-height: 26px; padding: 0 8px; font-size: 11px; }
  .chain-scope-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 5px 8px;
    margin-bottom: 8px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .chain-scope-label { color: var(--muted); font-size: 10.5px; font-weight: 600; }
  .chain-scope-switch {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .scope-btn {
    min-height: 22px;
    padding: 0 7px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 10.5px;
    font-weight: 550;
    cursor: pointer;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .scope-btn:hover { color: var(--text); background: var(--card-hover); }
  .scope-btn.active {
    color: var(--accent-ink);
    background: var(--accent);
    border-color: var(--accent);
    font-weight: 700;
  }
  .network-filter {
    position: absolute;
    z-index: 12;
    top: 8px;
    right: 8px;
    width: min(640px, calc(100% - 16px));
    max-height: calc(100% - 16px);
    overflow: auto;
    background: var(--surface-glass, var(--panel));
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
    transition: right var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      width var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .network-filter.expanded {
    z-index: 20;
  }
  @media (min-width: 700px) {
    .network-stage.has-details .network-filter {
      right: calc(min(390px, calc(100% - 16px)) + 16px);
      width: min(640px, calc(100% - min(390px, calc(100% - 16px)) - 24px));
    }
  }
  .network-filter header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    background: var(--panel-elev);
    border-bottom: 1px solid var(--line);
    font-size: 12px;
  }
  .network-filter :global(.mechanic-filter) { margin: 0; border: 0; }
  .network-empty { position: absolute; inset: 45% 12px auto; text-align: center; color: var(--faint); font-size: 12px; pointer-events: none; }
  .network-details p.details-meta { font-size: 11.5px; color: var(--muted); margin: 6px 0; line-height: 1.45; }
  .variant-options { margin: 8px 0; }
  .variant-btn {
    min-height: 26px;
    padding: 2px 8px;
    font-size: 11px;
    gap: 5px;
  }
  .variant-btn.active {
    color: var(--accent-ink);
    background: var(--accent);
    border-color: var(--accent);
    font-weight: 700;
  }
  .variant-machine { color: var(--muted); font-weight: 500; }
  .variant-btn.active .variant-machine { color: inherit; opacity: 0.85; }
  .variant-off {
    font-size: 9.5px;
    padding: 0 4px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    color: var(--faint);
  }
  .variant-btn.active .variant-off { color: inherit; border-color: currentColor; }
  .disabled-variant { opacity: 0.68; }
  .contrib-list { display: grid; gap: 5px; margin: 8px 0; }
  .contrib-btn {
    width: 100%;
    min-height: 30px;
    padding: 5px 8px;
    justify-content: space-between;
    text-align: left;
    white-space: normal;
    gap: 8px;
  }
  .contrib-main {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
    min-width: 0;
  }
  .contrib-name { font-size: 11.5px; font-weight: 600; overflow-wrap: anywhere; }
  .contrib-meta { font-size: 10.5px; color: var(--muted); }
  .contrib-rate { font-size: 11px; font-weight: 600; white-space: nowrap; flex: 0 0 auto; }
  .edit-card { margin-top: 10px; padding-top: 8px; border-top: 1px solid var(--line); }
  .edit-card :global(.identity) { padding-inline: 0; }
  .network-error {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    color: var(--danger);
    background: var(--danger-dim);
    border-bottom: 1px solid var(--danger-line);
    font-size: 11.5px;
  }
  .export-status { font-size: 11.5px; padding: 0 10px; color: var(--muted); overflow-wrap: anywhere; user-select: text; }
  .export-status:not(:empty) { padding-block: 7px; border-bottom: 1px solid var(--line); background: var(--panel-elev); }
  .warn { color: var(--warn); }
</style>
