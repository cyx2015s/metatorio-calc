<script lang="ts">
  import { onMount, untrack, tick } from 'svelte';
  import type { Core, ElementDefinition, StylesheetJson } from 'cytoscape';
  import type { ElkNode } from 'elkjs/lib/elk-api';
  import { runtime } from '$lib/runtime/store.svelte';
  import { saveNetworkPng } from '$lib/runtime/client';
  import { buildNetwork, networkRate, networkIconQualities, type NetworkNode, type NetworkEdge, type NetworkViewState } from '$lib/mechanics/network';
  import type { MechanicViewProps } from '$lib/mechanics/workspace';
  import { mechanicPrototype } from '$lib/display-labels';
  import { presentFlow } from '$lib/dual-var';
  import { normalizeSearch } from '$lib/rich-text';
  import { iconTypeForKind, syntheticIconUrls } from './icon-types';
  import FlowRef from './FlowRef.svelte';
  import MechanicEditor from './MechanicEditor.svelte';
  import MechanicFilter from './MechanicFilter.svelte';
  import SecondaryMenu from './SecondaryMenu.svelte';
  import { rememberMechanicScroll } from '$lib/mechanics/view-scroll';

  let {project, factory, entries, visibleIds, solutions, solved, actions, workspace, mergeRecipes, viewState, scrollState, filterOpen, onFilterClose}: MechanicViewProps & {
    mergeRecipes: boolean; viewState: NetworkViewState; filterOpen: boolean; onFilterClose: () => void;
  } = $props();
  let container = $state<HTMLDivElement>();
  let detailsPanel = $state<HTMLElement>(), searchBox = $state<HTMLDivElement>();
  let searchOpen = $state(false), searchIndex = $state(0);
  let popupLeft = $state(8), popupTop = $state(8);
  let positionFrame = 0;
  const searchResultsId = $derived(`network-search-${project}-${factory}`);
  let ready = $state(false), busy = $state(false), error = $state('');
  let exporting = $state(false), exportStatus = $state(''), exportError = $state('');
  let collapse = $state(untrack(() => viewState.collapse));
  let showUtilities = $state(untrack(() => viewState.showUtilities));
  let onlyEnabled = $state(untrack(() => viewState.onlyEnabled));
  let query = $state(untrack(() => viewState.query));
  let iconScale = $state(untrack(() => viewState.iconScale ?? 1));
  let selected = $state<string | undefined>(untrack(() => viewState.selected));
  let selectedMechanic = $state<number | null>(untrack(() => viewState.selectedMechanic ?? null));
  let renderRevision = $state(0);
  let cy: Core | undefined;
  let elk: {layout: (graph: ElkNode) => Promise<ElkNode>; terminateWorker: () => void} | undefined;
  let generation = 0, mounted = true;
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
    targets: [...(factoryDocument?.targets.map(t => t.flow) ?? []),
      ...(factoryDocument?.target_expressions.flatMap(expression => expression.terms.map(term => term.flow)) ?? [])],
    externalInputs: factoryDocument?.external_inputs ?? [], implicitInputs: universeDocument ? [] : runtime.implicitSourcesCache,
    localize: (kind, name) => runtime.localizedName(kind, name), timeScale,
    mergeRecipes, collapse, showUtilities, onlyEnabled}));
  const needle = $derived(normalizeSearch(query));
  const matches = $derived(model.nodes.filter(node => !needle || normalizeSearch(node.search).includes(needle)));
  // Recipe/machine edits change a merged node's ID. Keep the editor attached to
  // the stable mechanism identity, including when it moves to another group.
  const selectedNode = $derived(selectedMechanic === null
    ? model.nodes.find(node => node.id === selected)
    : model.nodes.find(node => node.entries.some(entry => entry.id === selectedMechanic)));
  const selectedId = $derived(selectedNode?.id ?? (selectedMechanic === null ? selected : undefined));
  const selectedEdge = $derived(selectedMechanic === null ? model.edges.find(edge => edge.id === selected) : undefined);
  const editing = $derived(selectedNode?.entries.find(entry => entry.id === selectedMechanic) ?? selectedNode?.entries[0]);
  const neighbours = $derived.by(() => {
    const ids = new Set<string>();
    for (const edge of model.edges) {
      if (edge.source === selectedId) ids.add(edge.target);
      if (edge.target === selectedId) ids.add(edge.source);
    }
    return model.nodes.filter(node => ids.has(node.id));
  });

  function palette(): Record<string, string> {
    const style = getComputedStyle(container!);
    const color = (key: string, fallback: string) => style.getPropertyValue(key).trim() || fallback;
    return {text: color('--text', '#ccd3df'), muted: color('--muted', '#7e8b9d'),
      panel: color('--panel', '#202632'), line: color('--line-strong', '#627084')};
  }
  function styles(): StylesheetJson {
    const p = palette();
    const computed = getComputedStyle(container!);
    const uiFont = computed.fontFamily;
    const monoFont = computed.getPropertyValue('--mono').trim() || uiFont;
    return [
      {selector: 'node', style: {label: 'data(label)', color: p.text, 'font-size': 11, 'font-family': uiFont,
        'text-wrap': 'wrap', 'text-max-width': '150px', 'text-valign': 'bottom', 'text-margin-y': 5,
        width: 38 * iconScale, height: 38 * iconScale, 'background-color': '#477761', 'border-width': 2, 'border-color': p.line}},
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
        'font-size': 10, 'font-family': monoFont, 'text-wrap': 'wrap', 'text-max-width': '240px', 'text-background-color': p.panel,
        'text-background-opacity': 0.9, 'text-background-padding': '2px'}},
      {selector: 'edge[utility]', style: {opacity: 0.3, label: '', 'curve-style': 'straight'}},
      {selector: 'edge[kind="collapsed"]', style: {'font-family': uiFont}},
      {selector: 'edge[kind="compatibility"]', style: {'line-style': 'dashed', 'line-color': '#9673af', 'target-arrow-color': '#9673af', 'font-size': 9, 'font-family': uiFont}},
      {selector: ':selected, .search-match', style: {'border-color': '#d49b14', 'border-width': 4, 'line-color': '#d49b14', 'target-arrow-color': '#d49b14'}},
    ];
  }
  function saveViewport(savePositions = false) {
    if (!cy || cy.destroyed() || renderRevision === 0) return;
    viewState.zoom = cy.zoom(); viewState.pan = {...cy.pan()};
    if (savePositions) for (const node of cy.nodes()) viewState.positions.set(node.id(), {...node.position()});
  }
  function choose(id: string) {
    searchOpen = false;
    selectObject(id);
    const element = cy?.getElementById(id);
    cy?.elements().unselect(); element?.select();
    if (element?.isNode() && cy) {
      if (cy.zoom() < 0.65) cy.zoom(0.85);
      cy.animate({center: {eles: element}, duration: 150});
    }
  }
  function selectObject(id?: string) {
    const changed = selectedId !== id;
    selected = id;
    if (!id || changed || selectedMechanic === null) selectedMechanic = model.nodes.find(node => node.id === id)?.entries[0]?.id ?? null;
    if (!id) cy?.elements().unselect();
    schedulePosition();
    if (id && changed) void tick().then(() => detailsPanel?.focus({preventScroll: true}));
  }
  function positionPopup() {
    if (!cy || !container || !detailsPanel || !selectedId) return;
    const element = cy.getElementById(selectedId);
    if (element.empty()) return;
    const bounds = element.isNode() ? element.renderedBoundingBox({includeLabels: false}) : undefined;
    const point = bounds ? {x: bounds.x2, y: bounds.y1} : element.renderedMidpoint();
    const width = detailsPanel.offsetWidth, height = detailsPanel.offsetHeight;
    const x = point.x + 12 + width <= container.clientWidth ? point.x + 12 : (bounds?.x1 ?? point.x) - width - 12;
    popupLeft = Math.max(8, Math.min(x, container.clientWidth - width - 8));
    popupTop = Math.max(8, Math.min(point.y, container.clientHeight - height - 8));
  }
  function schedulePosition() {
    cancelAnimationFrame(positionFrame);
    positionFrame = requestAnimationFrame(positionPopup);
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
      cy.on('tap', 'node, edge', event => selectObject(event.target.id()));
      cy.on('tap', event => { if (event.target === cy) selectObject(); });
      cy.on('pan zoom', () => { saveViewport(); schedulePosition(); });
      cy.on('position', 'node', schedulePosition);
      cy.on('dragfree', 'node', event => {
        const node = event.target;
        viewState.positions.set(node.id(), {...node.position()});
      });
      ready = true;
    }).catch(reason => { if (mounted) error = `无法加载网络绘图库：${String(reason)}`; });
    const observer = new ResizeObserver(() => { cy?.resize(); schedulePosition(); });
    observer.observe(container!);
    const themeObserver = new MutationObserver(() => { if (cy && !cy.destroyed()) { cy.style(styles()); schedulePosition(); } });
    themeObserver.observe(globalThis.document.documentElement, {attributes: true, attributeFilter: ['data-theme', 'class', 'style']});
    const outsideSearch = (event: PointerEvent) => {
      if (!searchBox?.contains(event.target as Node)) searchOpen = false;
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented || (event.target as Element)?.closest('[aria-modal="true"], .secondary-menu')) return;
      if (filterOpen) { onFilterClose(); return; }
      if (selectedId) { selectObject(); container?.focus({preventScroll: true}); }
    };
    globalThis.document.addEventListener('pointerdown', outsideSearch);
    window.addEventListener('keydown', escape);
    return () => {
      mounted = false; generation++; saveViewport(true);
      cancelAnimationFrame(positionFrame);
      globalThis.document.removeEventListener('pointerdown', outsideSearch);
      window.removeEventListener('keydown', escape);
      Object.assign(viewState, {collapse, showUtilities, onlyEnabled, query, iconScale, selected: selectedId, selectedMechanic});
      observer.disconnect(); themeObserver.disconnect(); cy?.destroy(); elk?.terminateWorker(); cy = undefined;
    };
  });

  $effect(() => {
    if (!ready) return;
    const snapshot = model;
    runtime.iconRevision;
    const revision = ++generation;
    const timer = setTimeout(() => {
      void draw(snapshot, revision).catch(reason => {
        if (revision === generation && mounted) { error = `网络布局失败：${String(reason)}`; busy = false; }
      });
    }, 100);
    return () => { clearTimeout(timer); if (generation === revision) generation++; };
  });
  $effect(() => { needle; matches; renderRevision; highlight(); });
  $effect(() => { if (searchIndex >= Math.min(100, matches.length)) searchIndex = 0; });
  $effect(() => {
    selectedId; renderRevision;
    schedulePosition();
  });
  $effect(() => {
    if (!detailsPanel) return;
    const observer = new ResizeObserver(schedulePosition);
    observer.observe(detailsPanel);
    return () => observer.disconnect();
  });
  // Size changes only restyle the existing canvas; no layout, pan or zoom reset.
  $effect(() => { if (ready && cy) { iconScale; cy.style(styles()); schedulePosition(); } });

  async function draw(snapshot: typeof model, revision: number, reset = false) {
    if (!cy || !elk || !mounted || revision !== generation) return;
    busy = true; error = '';
    if (!reset) {
      saveViewport(true);
      // A renamed group should stay where the user was editing it. Existing
      // destination groups retain their own position.
      const oldPosition = cy.$(':selected').nodes().first().position();
      if (selectedId && oldPosition && !viewState.positions.has(selectedId)) {
        viewState.positions.set(selectedId, {...oldPosition});
      }
    }
    const material = snapshot.nodes.filter(node => !node.utility);
    const laidOut = await elk.layout({id: 'root', layoutOptions: {
      'elk.algorithm': 'layered', 'elk.direction': 'RIGHT', 'elk.edgeRouting': 'ORTHOGONAL',
      'elk.spacing.nodeNode': '45', 'elk.layered.spacing.nodeNodeBetweenLayers': '100',
      'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX',
    // Reserve the largest supported icon plus its multiline caption up front,
    // so changing icon size remains layout-free without covering other labels.
    }, children: material.map(node => ({id: node.id, width: node.kind === 'mechanic' ? 145 : 115,
      height: Math.max(100, (node.kind === 'mechanic' ? 48 : 38) * 2 + 16 + node.label.split('\n').length * 13)})),
      edges: snapshot.edges.filter(edge => !edge.utility).map(edge => ({id: edge.id, sources: [edge.source], targets: [edge.target]}))});
    if (!cy || !mounted || revision !== generation) return;
    const positions = new Map(laidOut.children?.map(node => [node.id, {x: (node.x ?? 0) + 75, y: (node.y ?? 0) + 45}]));
    const bottom = Math.max(0, ...[...positions.values()].map(p => p.y)) + 140;
    snapshot.nodes.filter(node => node.utility).forEach((node, i) => positions.set(node.id, {x: 80 + i * 150, y: bottom}));
    const hadViewport = viewState.zoom !== undefined;
    const elements: ElementDefinition[] = snapshot.nodes.map(node => ({data: {id: node.id, label: node.label, kind: node.kind,
      ...(node.flow && typeof node.flow !== 'string' && 'Fluid' in node.flow ? {fluid: true} : {}),
      ...(node.utility ? {utility: true} : {}), ...(node.target ? {target: true} : {}),
      ...(node.implicit ? {implicit: true} : {}), ...(node.disabled ? {disabled: true} : {}), ...(node.unused ? {unused: true} : {})},
      position: viewState.positions.get(node.id) ?? positions.get(node.id)}));
    elements.push(...snapshot.edges.map(edge => ({data: {id: edge.id, source: edge.source, target: edge.target,
      label: edge.label, kind: edge.kind, ...(edge.utility ? {utility: true} : {})}})));
    cy.batch(() => { cy!.elements().remove(); cy!.add(elements); });
    if (hadViewport) { cy.zoom(viewState.zoom!); cy.pan(viewState.pan ?? {x: 0, y: 0}); }
    else {
      cy.fit(undefined, 35);
      if (cy.zoom() < 0.4) {
        const focus = snapshot.nodes.find(node => node.target) ?? snapshot.nodes.find(node => node.kind === 'mechanic');
        cy.zoom(0.65);
        if (focus) cy.center(cy.getElementById(focus.id));
      }
    }
    if (selectedId) cy.getElementById(selectedId).select();
    schedulePosition();
    busy = false; renderRevision++;
    // Icon requests are deduplicated by the existing runtime cache. Old generations
    // cannot add icons after a document, context, factory or view change.
    for (let offset = 0; offset < snapshot.nodes.length && revision === generation && mounted; offset += 8) {
      await Promise.all(snapshot.nodes.slice(offset, offset + 8).map(async node => {
        const info = node.flow ? presentFlow(node.flow, (k, n) => runtime.localizedName(k, n)) : undefined;
        const ref = node.entries[0] ? mechanicPrototype(node.entries[0].mechanic) : undefined;
        const name = info?.name ?? ref?.name, type = info?.type ?? (ref ? iconTypeForKind(ref.kind) : undefined);
        if (!name || !type) return;
        const url = syntheticIconUrls[name] ?? await runtime.getIcon(type, name);
        const qualities = networkIconQualities(node);
        const badges = qualities.length > 1 ? qualities : qualities.filter(quality => quality !== 'normal');
        const qualityUrls = await Promise.all(badges.map(quality => runtime.getIcon('quality', quality)));
        if (url && cy && mounted && revision === generation) {
          const images = qualityUrls.filter((url): url is string => !!url);
          const badgeSize = Math.min(45, 100 / Math.max(1, images.length));
          cy.getElementById(node.id).data('icon', url).style({
            'background-image': [url, ...images],
            'background-fit': ['contain', ...images.map(() => 'none' as const)],
            'background-width': ['auto', ...images.map(() => `${badgeSize}%`)],
            'background-height': ['auto', ...images.map(() => `${badgeSize}%`)],
            'background-position-x': ['50%', ...images.map((_, i) => `${images.length === 1 ? 0 : i * 100 / (images.length - 1)}%`)],
            'background-position-y': ['50%', ...images.map(() => '100%')],
          });
        }
      }));
    }
  }
  function relayout() {
    viewState.positions.clear(); viewState.zoom = undefined;
    void draw(model, ++generation, true).catch(reason => { error = `网络布局失败：${String(reason)}`; busy = false; });
  }
  async function exportPng() {
    if (!cy || busy || exporting) return;
    exporting = true; exportError = ''; exportStatus = '正在生成 PNG…';
    try {
      const blob = await cy.png({output: 'blob-promise', full: true, scale: 1, bg: palette().panel});
      const bytes = new Uint8Array(await blob.arrayBuffer());
      exportStatus = '请选择保存位置并确认保存…';
      const path = await saveNetworkPng(`metatorio-network-${project}-${factory}.png`, bytes);
      exportStatus = path === null ? '已取消导出' : `PNG 已保存到：${path}`;
    } catch (reason) {
      exportStatus = ''; exportError = `PNG 导出失败：${String(reason)}`;
    } finally { exporting = false; }
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
      <input aria-label="搜索网络节点" placeholder="搜索名称、ID、品质或温度…"
        role="combobox" aria-autocomplete="list" aria-expanded={searchOpen} aria-controls={searchResultsId}
        aria-activedescendant={searchOpen && matches[searchIndex] ? `${searchResultsId}-${searchIndex}` : undefined}
        bind:value={query} onfocus={() => searchOpen = true} oninput={() => { searchOpen = true; searchIndex = 0; }} onkeydown={searchKey} />
      {#if searchOpen}
        <div class="search-dropdown">
          <div class="search-count">{matches.length} 个{needle ? '匹配' : '节点'}</div>
          <div id={searchResultsId} class="search-results" role="listbox" aria-label="网络节点搜索结果">
            {#each matches.slice(0, 100) as node, i (node.id)}
              <button id={`${searchResultsId}-${i}`} class="search-result" role="option" aria-selected={searchIndex === i}
                onpointermove={() => searchIndex = i} onpointerdown={event => event.preventDefault()} onclick={() => choose(node.id)}>{node.label.replaceAll('\n', ' · ')}</button>
            {:else}<span class="no-matches">没有匹配的节点</span>{/each}
          </div>
          {#if matches.length > 100}<div class="search-count">仅列出前 100 个，请搜索缩小范围。</div>{/if}
        </div>
      {/if}
    </div>
    <button class="btn" disabled={!ready || busy} onclick={relayout}>重排</button>
    <button class="btn" disabled={!ready} onclick={() => cy?.fit(undefined, 35)}>适应窗口</button>
    {#if needle}<button class="btn" disabled={!ready || !matches.length} onclick={() => choose(matches[0].id)}>定位匹配</button>{/if}
    <button class="btn" disabled={!ready || busy || exporting} onclick={exportPng}>{exporting ? '导出中…' : '导出 PNG'}</button>
    <label title="只调整图标大小，保持节点位置和画布缩放">图标大小
      <select aria-label="网络图标大小" bind:value={iconScale}>
        {#each [0.75, 1, 1.25, 1.5, 2] as scale}<option value={scale}>{scale * 100}%</option>{/each}
      </select>
    </label>
    <label><input type="checkbox" bind:checked={collapse} />折叠中间流</label>
    <label><input type="checkbox" bind:checked={showUtilities} />能量与污染</label>
    <label><input type="checkbox" bind:checked={onlyEnabled} />仅启用</label>
  </div>
  <div class="network-status" role="status">
    <span>{model.nodes.filter(node => node.kind === 'mechanic').length} 个机制节点 · {model.edges.length} 条关系</span>
    <span>{solved ? '实际求解流量' : '未求解：单位机制流量；分组变体分别列量'}</span>
    {#if !ready}<span>加载绘图库…</span>{:else if busy}<span>布局中…</span>{/if}
    {#if workspace.pending}<span>展开 {workspace.completed} / {entries.length}</span>{/if}
    {#if workspace.failures.length}<span class="warn">{workspace.failures.length} 个机制展开失败</span><button class="btn" onclick={() => workspace.retry++}>重试展开</button>{/if}
  </div>
  {#if error}<div class="network-error" role="alert">{error}<button class="btn" onclick={relayout}>重试布局</button></div>{/if}
  <div class="export-status" role="status" aria-live="polite" aria-atomic="true">{exportStatus}</div>
  {#if exportError}<div class="network-error" role="alert">{exportError}</div>{/if}
  <div class="network-stage">
    <!-- Keyboard focus returns here when the node editor is closed. -->
    <div class="network-canvas" bind:this={container} role="group" tabindex="-1" aria-label="生产网络画布"></div>
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
      <div class="network-empty">{workspace.active ? '没有匹配的机制，可更换条件或清除筛选。' : '还没有机制，可添加机制或使用自动规划。'}</div>
    {/if}
    {#if filterOpen || workspace.active}
      <div class="network-filter">
        {#if filterOpen}<header><strong>机制流筛选</strong><button class="icon-btn" aria-label="收起机制筛选" onclick={onFilterClose}>×</button></header>{/if}
        <MechanicFilter {workspace} {entries} expanded={filterOpen} />
      </div>
    {/if}
  {#if selectedNode || selectedEdge}
    <div class="network-details" role="dialog" aria-label="网络对象详情" tabindex="-1" bind:this={detailsPanel} style:left={`${popupLeft}px`} style:top={`${popupTop}px`}>
      <button class="icon-btn close" aria-label="关闭网络详情" onclick={() => { selectObject(); container?.focus({preventScroll: true}); }}>×</button>
      {#if selectedNode}
        {#if selectedNode.flow}<FlowRef flow={selectedNode.flow} />{:else}<strong>{selectedNode.label.replaceAll('\n', ' · ')}</strong>{/if}
        {#if selectedNode.kind === 'external'}<p>{selectedNode.implicit ? '隐式外部输入 · 成本 0' : `手动外部输入 · 成本 ${selectedNode.external?.penalty}`}</p>{/if}
        {#if selectedNode.entries.length > 1}
          <div class="variant-options">{#each selectedNode.entries as entry (entry.id)}<button class="btn" class:active={editing?.id === entry.id} onclick={() => selectedMechanic = entry.id}>机制 #{entry.id}</button>{/each}</div>
        {/if}
        {#if editing}<div class="edit-card"><MechanicEditor entry={editing} {project} {factory} {actions} solution={solutions.get(editing.id) ?? (solved ? {amount: 0, cost: 0} : null)} /></div>{/if}
        {#if neighbours.length}<details class="neighbours"><summary>相邻对象</summary><div class="node-options">{#each neighbours as node (node.id)}<button class="btn" onclick={() => choose(node.id)}>{node.label.replaceAll('\n', ' · ')}</button>{/each}</div></details>{/if}
      {:else if selectedEdge}
        {#if selectedEdge.flow}<FlowRef flow={selectedEdge.flow} />{/if}
        <p>{edgeDescription(selectedEdge)}</p>
        {#each selectedEdge.contributions as item, i (`${item.mechanic}:${i}`)}
          <p>机制 #{item.mechanic} · {item.unitAmount < 0 ? '消耗' : '产出'} {networkRate(selectedEdge.flow!, solved ? item.amount : item.unitAmount, timeScale)}</p>
        {/each}
      {/if}
    </div>
  {/if}
  </div>

</section>

<style>
  .network-view { display: flex; flex-direction: column; flex: 1 0 auto; min-width: 0; margin-top: 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel); overflow: hidden; }
  .network-toolbar, .network-status, .node-options, .variant-options { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .network-toolbar { padding: 8px; border-bottom: 1px solid var(--line); }
  .network-search { position: relative; min-width: 130px; width: 210px; max-width: 100%; }
  .network-search input { width: 100%; box-sizing: border-box; }
  .search-dropdown { position: absolute; z-index: 22; top: calc(100% + 6px); left: 0; width: min(430px, calc(100vw - 32px));
    max-width: calc(100vw - 32px); background: var(--panel); border: 1px solid var(--line-strong); border-radius: var(--radius); box-shadow: var(--shadow-strong); }
  .search-count, .no-matches { padding: 6px 8px; color: var(--muted); font-size: 11px; }
  .search-results { max-height: min(320px, 45vh); overflow: auto; display: grid; }
  .search-result { text-align: left; padding: 7px 8px; color: var(--text); background: none; border: 0; font: inherit; font-size: 11px; cursor: pointer; overflow-wrap: anywhere; }
  .search-result:hover, .search-result[aria-selected="true"] { background: var(--card); }
  .network-toolbar label { display: inline-flex; align-items: center; gap: 4px; font-size: 11px; }
  .network-status { padding: 5px 8px; color: var(--muted); font-size: 11px; }
  .network-stage { position: relative; min-width: 0; min-height: 360px; flex: 1 0 clamp(360px, 62vh, 820px); }
  .network-canvas { position: absolute; inset: 0; height: 100%; width: 100%; }
  .network-help { position: absolute; right: 8px; bottom: 8px; z-index: 10; }
  .help-button { min-height: 24px; padding: 2px 7px; font: inherit; font-size: 11px; cursor: pointer;
    color: var(--muted); background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius-sm); }
  .help-button:hover { color: var(--text); border-color: var(--line-strong); }
  .network-legend { display: grid; grid-template-columns: auto 1fr; gap: 5px 12px; font-size: 12px; margin: 0; }
  .network-legend dt { color: var(--muted); }
  .network-legend dd { margin: 0; }
  .network-note, .network-credit { color: var(--muted); font-size: 11px; line-height: 1.6; margin: 10px 0 0; }
  .neighbours summary { cursor: pointer; color: var(--muted); font-size: 11px; margin-top: 8px; }
  .node-options { padding-top: 6px; max-height: 200px; overflow: auto; }
  .node-options .btn { white-space: normal; text-align: left; font-size: 11px; max-width: 100%; overflow-wrap: anywhere; }
  .network-details { position: absolute; z-index: 15; width: min(420px, calc(100% - 16px)); max-height: calc(100% - 16px); overflow: auto;
    box-sizing: border-box; padding: 10px; color: var(--text); background: var(--panel); border: 1px solid var(--line-strong); border-radius: var(--radius); box-shadow: var(--shadow-strong); }
  .network-details > strong { display: block; padding-right: 24px; font-size: 12px; overflow-wrap: anywhere; }
  .network-filter { position: absolute; z-index: 20; top: 8px; right: 8px; width: min(640px, calc(100% - 16px)); max-height: calc(100% - 16px); overflow: auto; background: var(--panel); border: 1px solid var(--line-strong); border-radius: var(--radius); box-shadow: var(--shadow-strong); }
  .network-filter header { display: flex; justify-content: space-between; align-items: center; padding: 4px 8px; font-size: 12px; }
  .network-filter :global(.mechanic-filter) { margin: 0; border: 0; }
  .network-empty { position: absolute; inset: 45% 12px auto; text-align: center; color: var(--muted); pointer-events: none; }
  .network-details p { font-size: 12px; color: var(--muted); margin: 8px 0; }
  .close { float: right; }
  .variant-options { margin: 6px 0; }
  .edit-card { margin-top: 8px; }
  .edit-card :global(.identity) { padding-inline: 0; }
  .network-error { padding: 8px; color: var(--warn); }
  .export-status { font-size: 12px; padding: 0 8px; overflow-wrap: anywhere; user-select: text; }
  .export-status:not(:empty) { padding-block: 8px; }
  .warn { color: var(--warn); }
</style>
