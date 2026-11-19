<script lang="ts">
  // Metatorio 主界面：紧凑布局，圆角卡片分区；游戏内物品一律图标按钮，
  // 一般操作一律文字按钮。
  import { onMount } from "svelte";
  import { DEFAULT_ORBITAL_TRANSPORT_COSTS } from "$lib/runtime/project-settings";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { suggest } from "$lib/runtime/client";
  import type { ContextInfo, DualVar, Location, MechanicKind } from "$lib/runtime/types";
  import HoverIcon from "$lib/ui/HoverIcon.svelte";
  import Selector from "$lib/ui/Selector.svelte";
  import MechanicWorkspace from "$lib/ui/MechanicWorkspace.svelte";
  import FactorySidebar from "$lib/ui/FactorySidebar.svelte";
  import FactoryTargetsPanel from "$lib/ui/FactoryTargetsPanel.svelte";
  import SolvePanel from "$lib/ui/SolvePanel.svelte";
  import LocationPicker from "$lib/ui/LocationPicker.svelte";
  import UniverseInputsPanel from "$lib/ui/UniverseInputsPanel.svelte";
  import AutoPlanProgress from "$lib/ui/AutoPlanProgress.svelte";
  import AppTopBar from "$lib/ui/AppTopBar.svelte";
  import ErrorBanner from "$lib/ui/ErrorBanner.svelte";
  import FactoryExternalInputsPanel from "$lib/ui/FactoryExternalInputsPanel.svelte";
  import ProjectSettingsPanel from "$lib/ui/ProjectSettingsPanel.svelte";
  import GameDataModal from "$lib/ui/GameDataModal.svelte";
  import PlanningPreferencesModal, {
    type OpenSelectorFn,
    type SelectorRequest,
  } from "$lib/ui/PlanningPreferencesModal.svelte";
  import SuggestionsModal, { type SuggestionsState } from "$lib/ui/SuggestionsModal.svelte";
  import WorkspaceDialogs, {
    type CloseProjectState,
    type ConfirmState,
    type RenameContextState,
    type RenameTargetState,
  } from "$lib/ui/WorkspaceDialogs.svelte";
  import { universeView } from "$lib/mechanics/located";
  import { createMechanicActions } from "$lib/mechanics/actions";
  import { DEFAULT_MECHANIC_VIEW, type MechanicViewMode } from "$lib/mechanics/workspace";

  const mechKinds: { kind: MechanicKind; label: string }[] = [
    { kind: "recipe", label: "配方" },
    { kind: "mining", label: "采矿" },
    { kind: "spoil", label: "变质" },
    { kind: "plant", label: "种植" },
    { kind: "item-fuel", label: "物品燃料" },
    { kind: "virtual-science", label: "虚拟科研" },
    { kind: "item-launch", label: "火箭发射" },
    { kind: "generator", label: "发电机" },
    { kind: "boiler", label: "锅炉" },
    { kind: "reactor", label: "反应堆" },
    { kind: "fusion-reactor", label: "聚变反应堆" },
    { kind: "fusion-generator", label: "聚变发电机" },
    { kind: "solar", label: "太阳能" },
    { kind: "fluid-fuel", label: "流体燃料" },
    { kind: "fluid-heat", label: "流体热" },
    { kind: "tile-extract", label: "地格抽取" },
    { kind: "tile-dispose", label: "地格销毁" },
  ];

  onMount(() => {
    runtime.initTheme();
    runtime.initFonts();
    runtime.init().catch(() => {});
    runtime.clearCatalogCache();
  });

  // ── 弹窗与菜单状态 ──────────────────────────────────────────────
  // 添加机制菜单用 fixed 定位（视口坐标），脱离滚动容器，避免开合时
  // 改变机制列表滚动区域的高度。
  let addMechMenuPos = $state<{ top: number; right: number } | null>(null);
  let mechMenuOpen = $state(false);
  let dataOpen = $state(false);
  let loadGameOpen = $state(false);
  /** 字体设置弹窗（纯前端偏好，存 localStorage）。 */
  let fontSettingsOpen = $state(false);
  /** 左栏编辑范围：工厂（目标/输入/环境）或项目（全局设置）。 */
  let leftTab = $state<"factory" | "project">("factory");
  let newProjectOpen = $state(false);
  let newFactoryOpen = $state(false);
  let newUniverseOpen = $state(false);
  // 规划偏好弹窗（机器偏好/替代数量/枚举插件与插件塔）
  let prefsOpen = $state(false);
  // 建议系统弹窗
  let suggestions = $state<SuggestionsState | null>(null);

  let suggestionGeneration = 0;
  async function openSuggestions(flow: DualVar, location?: Location) {
    if (!project || !actions) return;
    const scope = { ...actions.commands.scope },
      generation = ++suggestionGeneration;
    suggestions = { flow, location, scope, items: [], loading: true };
    try {
      const items = await suggest(flow, runtime.effectiveContextId);
      if (generation === suggestionGeneration && suggestions)
        suggestions = { flow, location, scope, items, loading: false };
    } catch {
      if (generation === suggestionGeneration && suggestions)
        suggestions = { flow, location, scope, items: [], loading: false };
    }
  }

  // 应用内确认/重命名弹窗（Tauri WebView2 不支持 window.confirm/prompt，会阻塞）
  let confirmState = $state<ConfirmState | null>(null);
  let renameCtx = $state<RenameContextState | null>(null);
  let renameTarget = $state<RenameTargetState | null>(null);
  let closeProjectState = $state<CloseProjectState | null>(null);

  // 轻量提示（自动消失）
  let notice = $state<string | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function showNotice(message: string) {
    notice = message;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), 4000);
  }

  // ── 选择器状态 ──────────────────────────────────────────────────
  let selector = $state<SelectorRequest | null>(null);

  // 流选择器（目标/外部输入支持任意 DualVar 流；编辑时预选当前流）
  let flowSelector = $state<{
    title: string;
    initialTab?: string;
    initialName?: string;
    initialQuality?: string;
    /** 精确名称过滤（如燃料选择：只允许匹配机器燃料类别且有热值的物品）。 */
    allowedNames?: string[];
    /** 是否按项目可达性过滤。外部输入要允许"运行时才产生的特殊物品/流体"
     *  （可达性图里没有这些节点），所以那些入口显式置 false。 */
    respectAccessibility?: boolean;
    onSelectFlow: (flow: DualVar) => void;
  } | null>(null);

  const openSelector: OpenSelectorFn = (request) => {
    selector = { kinds: [], ...request };
  };

  function renameContextPrompt(context: ContextInfo) {
    renameCtx = { id: context.id, name: context.name };
  }

  function promptRenameProject() {
    const item = runtime.selectedProject;
    if (!item) return;
    renameTarget = { kind: "project", name: item.name };
  }

  function promptRenameFactory(name: string) {
    renameTarget = { kind: "factory", name };
  }

  // ── 派生数据 ────────────────────────────────────────────────────
  let project = $derived(runtime.selectedProject);
  let planning = $derived(project?.planning ?? null);
  const universe = $derived(runtime.selectedUniverse);
  let factory = $derived(universe ? universeView(universe) : runtime.selectedFactory);
  let universeBusy = $state(false);
  async function autoPlanUniverse() {
    if (universeBusy) return;
    universeBusy = true;
    try {
      await runtime.autoPlanSelectedUniverse();
    } finally {
      universeBusy = false;
    }
  }
  /** 当前项目绑定的游戏上下文（未绑定时回退到全局激活上下文）。 */
  let projectContext = $derived(
    project?.context_id
      ? runtime.contexts.find((context) => context.id === project?.context_id) ?? null
      : runtime.activeContext,
  );
  const missingProjectContext = $derived(!!project?.context_id && !projectContext);

  function closeMenus() {
    mechMenuOpen = false;
    addMechMenuPos = null;
  }

  let mechanics = $derived(factory?.mechanics ?? []);
  let targets = $derived(factory?.targets ?? []);
  let targetExpressions = $derived(factory?.target_expressions ?? []);
  let mechanicProjectId = $derived(project?.id ?? null);
  let mechanicFactoryId = $derived(factory?.id ?? null);
  let actions = $derived(
    mechanicProjectId !== null && mechanicFactoryId !== null
      ? createMechanicActions(
          runtime,
          {
            project: mechanicProjectId,
            factory: mechanicFactoryId,
            ...(universe ? { kind: "universe" as const } : {}),
          },
          {
            select: (request) => (selector = request),
            selectFlow: (request) => (flowSelector = request),
            notice: showNotice,
          },
        )
      : null,
  );

  // 机制列表：配方合并开关（同配方×机器合并成一张卡，品质变体并列）；
  // 其他机制类型始终单条展示，不额外嵌套。
  let mergeRecipes = $state(true);
  let groupLocations = $state(false);
  let mechanicView = $state<MechanicViewMode>(DEFAULT_MECHANIC_VIEW);
  let filterOpen = $state(false);
  let filterActive = $state(false);

  let solve = $derived(universe ? null : runtime.solve);
  let solveMap = $derived(
    new Map(
      universe && runtime.universeSolve?.status.status === "solved"
        ? runtime.universeSolve.status.mechanics.map(
            (m) => [m.mechanic, { amount: m.amount ?? NaN, cost: m.cost ?? NaN }] as const,
          )
        : solve && "solved" in solve.status
          ? solve.status.solved.mechanics.map(
              (m) => [m.mechanic, { amount: m.amount ?? NaN, cost: m.cost ?? NaN }] as const,
            )
          : [],
    ),
  );
  let solved = $derived(
    universe
      ? runtime.universeSolve?.status.status === "solved"
      : solve != null && "solved" in solve.status,
  );

  /** 全局快捷键：Ctrl/Cmd+Z 撤销；Ctrl/Cmd+Shift+Z 或 Ctrl+Y 重做。
   *  焦点在输入框/文本域/可编辑元素时交给浏览器处理文本撤销，不拦截。 */
  function handleGlobalKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && (mechMenuOpen || addMechMenuPos)) {
      closeMenus();
      return;
    }
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
    const target = event.target as HTMLElement | null;
    const tag = target?.tagName;
    if (
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      target?.isContentEditable === true
    ) {
      return;
    }
    const key = event.key.toLowerCase();
    if (key === "z" && !event.shiftKey) {
      event.preventDefault();
      runtime.undo().catch(() => {});
    } else if ((key === "z" && event.shiftKey) || key === "y") {
      event.preventDefault();
      runtime.redo().catch(() => {});
    }
  }
</script>

<svelte:head>
  <title>切向量化</title>
</svelte:head>

<svelte:window onkeydown={handleGlobalKeydown} />

<div class="app">
  <!-- ══ 应用栏 ══ -->
  <AppTopBar
    {project}
    {factory}
    {projectContext}
    onOpenData={() => (dataOpen = true)}
    onOpenLoadGame={() => (loadGameOpen = true)}
    onOpenFontSettings={() => (fontSettingsOpen = true)}
    onNewProject={() => (newProjectOpen = true)}
    onNewFactory={() => (newFactoryOpen = true)}
    onNewUniverse={() => (newUniverseOpen = true)}
    onRenameProject={promptRenameProject}
    onRenameFactory={promptRenameFactory}
    onCloseProject={(state) => (closeProjectState = state)}
    onRequestConfirm={(state) => (confirmState = state)}
    onShowNotice={showNotice}
    onMenuOpen={closeMenus}
  />

  <ErrorBanner
    {notice}
    {missingProjectContext}
    onOpenData={() => (dataOpen = true)}
    onSuggest={openSuggestions}
    onNotice={showNotice}
  />

  <!-- ══ 工作区 ══ -->
  <main class="workspace">
    <!-- 左栏：生产目标 / 外部输入 / 目标表达式（低频设置收进右上角抽屉） -->
    <!-- 左栏：工厂 / 项目 双 tab -->
    <aside class="col rail rail-left">
      <FactorySidebar
        bind:tab={leftTab}
        factoryName={factory?.name ?? "未选择工厂"}
        projectName={project?.name ?? "未选择项目"}
      >
        {#snippet factoryContent()}
          {#if factory}
            {#if universe}
              <section class="panel universe-settings">
                <div class="section-head"><span>环境与约束</span><span class="count">跨星球 · 太空</span></div>
                <div class="env-row">
                  <span>新增条目地点</span>
                  <LocationPicker
                    value={runtime.universeAddLocation}
                    onChange={(location) => (runtime.universeAddLocation = location)}
                  />
                </div>
                <label class="check">
                  <input
                    type="checkbox"
                    checked={universe.strict_source}
                    onchange={(e) =>
                      runtime.dispatchUniverse(project!.id, universe.id, {
                        "set-strict-source": { strict: e.currentTarget.checked },
                      })}
                  />严格供给（只允许环境资源作为外部来源）
                </label>
                <label class="check">
                  <input
                    type="checkbox"
                    checked={universe.strict_sink}
                    onchange={(e) =>
                      runtime.dispatchUniverse(project!.id, universe.id, {
                        "set-strict-sink": { strict: e.currentTarget.checked },
                      })}
                  />严格消耗（未出现在目标中的物品必须配平）
                </label>
              </section>
            {:else}
              <section class="panel">
                <div class="section-head">
                  <span>环境与约束</span>
                  <span class="count">星球 · 地表 · 品质</span>
                </div>
                <div class="env-row">
                  <button
                    class="icon-btn"
                    class:empty={!factory.settings.planet}
                    title={`星球：${factory.settings.planet ? runtime.localizedName("planet", factory.settings.planet) : "未选择"}`}
                    onclick={() =>
                      openSelector({
                        kind: "planet",
                        title: "选择星球",
                        onSelect: (name) => runtime.setFactoryPlanet(name),
                      })}
                  >
                    <HoverIcon
                      type="planet"
                      name={factory.settings.planet || "planet"}
                      size={24}
                      detailKind={factory.settings.planet ? "planet" : undefined}
                    />
                  </button>
                  <span class="sub">
                    {factory.settings.planet
                      ? `星球：${runtime.localizedName("planet", factory.settings.planet)}`
                      : "星球：未选择"}
                  </span>
                  {#if factory.settings.planet}
                    <button
                      class="btn ghost"
                      title="清除星球"
                      onclick={() => runtime.setFactoryPlanet(null).catch(() => {})}
                    >×</button>
                  {/if}
                </div>
                <div class="env-row">
                  <button
                    class="icon-btn"
                    class:empty={!factory.settings.surface}
                    title={`地表：${factory.settings.surface ? runtime.localizedName("surface", factory.settings.surface) : "未选择"}`}
                    onclick={() =>
                      openSelector({
                        kind: "surface",
                        title: "选择地表",
                        onSelect: (name) => runtime.setFactorySurface(name),
                      })}
                  >
                    <HoverIcon
                      type="surface"
                      name={factory.settings.surface || "surface"}
                      size={24}
                      detailKind={factory.settings.surface ? "surface" : undefined}
                    />
                  </button>
                  <span class="sub">
                    {factory.settings.surface
                      ? `地表：${runtime.localizedName("surface", factory.settings.surface)}`
                      : "地表：未选择"}
                  </span>
                  {#if factory.settings.surface}
                    <button
                      class="btn ghost"
                      title="清除地表"
                      onclick={() => runtime.setFactorySurface(null).catch(() => {})}
                    >×</button>
                  {/if}
                </div>
                <label class="check">
                  <input
                    type="checkbox"
                    checked={factory.strict_source}
                    onchange={(event) =>
                      runtime
                        .setStrictSource((event.currentTarget as HTMLInputElement).checked)
                        .catch(() => {})}
                  />
                  严格供给（只允许从外部输入获得未配平物品）
                </label>
                <label class="check">
                  <input
                    type="checkbox"
                    checked={factory.strict_sink}
                    onchange={(event) =>
                      runtime
                        .setStrictSink((event.currentTarget as HTMLInputElement).checked)
                        .catch(() => {})}
                  />
                  严格消耗（未出现在目标中的物品必须配平）
                </label>
              </section>
            {/if}
            {#if actions}
              <FactoryTargetsPanel
                scope={actions.commands.scope}
                {targets}
                expressions={targetExpressions}
                selectFlow={(request) => (flowSelector = request)}
                onSuggest={openSuggestions}
              />
            {/if}

            {#if universe && project}
              <UniverseInputsPanel
                project={project.id}
                {universe}
                selectFlow={(request) => (flowSelector = { ...request, respectAccessibility: false })}
              />
            {/if}
            {#if !universe && project}
              <FactoryExternalInputsPanel
                {project}
                {factory}
                selectFlow={(request) => (flowSelector = request)}
              />
            {/if}
          {:else}
            <div class="rail-empty">
              <strong>还没有选中的工厂</strong>
              <span>在顶部选择一个工厂，或点「+」新建工厂开始规划。</span>
            </div>
          {/if}
        {/snippet}
        {#snippet projectContent()}
          <ProjectSettingsPanel
            {project}
            onOpenSelector={openSelector}
            onOpenPrefs={() => (prefsOpen = true)}
          />
        {/snippet}
      </FactorySidebar>
    </aside>

    <!-- 中栏：机制列表 -->
    <section class="col center">
      <div class="toolbar">
        <button
          class="btn primary"
          title="自动规划：按规划偏好的枚举列表生成候选机制 → 选优 → 回写机制列表。总是按严格供给求解，并在完成后自动开启严格供给"
          onclick={() => (universe ? autoPlanUniverse() : runtime.autoPlan().catch(() => {}))}
          disabled={universeBusy || runtime.autoPlanning || !factory}
        >
          {universeBusy || runtime.autoPlanning ? "规划中…" : "自动规划"}
        </button>
        {#if runtime.autoPlanning}
          <AutoPlanProgress progress={runtime.autoPlanProgress} />
        {/if}
        {#if factory}
          <button
            class="btn"
            title="选择机制类型并添加到当前工厂"
            onclick={(event) => {
              const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
              if (addMechMenuPos) {
                addMechMenuPos = null;
                return;
              }
              // 下拉菜单：优先向下，底部空间不足时向上翻转。
              const estimate = mechKinds.length * 30 + 18;
              const below = window.innerHeight - rect.bottom - 8;
              const above = rect.top - 8;
              addMechMenuPos = {
                top:
                  below >= estimate || below >= above
                    ? rect.bottom + 4
                    : Math.max(8, rect.top - estimate),
                right: Math.max(8, window.innerWidth - rect.right),
              };
            }}
          >
            + 添加机制{addMechMenuPos ? " ▴" : " ▾"}
          </button>
        {/if}
        <button
          class="btn"
          class:active={filterOpen || filterActive}
          aria-expanded={filterOpen}
          title="按消耗或产出对象筛选机制；仅影响显示，不改变工厂或求解"
          onclick={() => (filterOpen = !filterOpen)}
          disabled={!factory}
        >筛选{filterActive ? " · 已启用" : ""}</button>
        {#if universe}
          {#each ["rocket-export", "orbital-drop"] as direction}
            <button
              class="btn"
              disabled={runtime.universeAddLocation.kind !== "planet"}
              title={direction === "rocket-export"
                ? `从左栏所选星球发往太空；消耗物品与火箭载荷；每件成本：重量（kg）× ${project?.settings.orbital_transport_costs?.rocket_export_per_kg ?? DEFAULT_ORBITAL_TRANSPORT_COSTS.rocket_export_per_kg}`
                : `从太空投送到左栏所选星球；每件成本：${project?.settings.orbital_transport_costs?.orbital_drop_per_stack ?? DEFAULT_ORBITAL_TRANSPORT_COSTS.orbital_drop_per_stack} ÷ 堆叠数量；变质物品额外成本：${project?.settings.orbital_transport_costs?.orbital_drop_spoil_penalty ?? DEFAULT_ORBITAL_TRANSPORT_COSTS.orbital_drop_spoil_penalty} ÷ 有效变质时间（tick）`}
              onclick={() => {
                const location = runtime.universeAddLocation;
                if (location.kind !== "planet") return;
                selector = {
                  kind: "item",
                  kinds: [],
                  title: direction === "rocket-export" ? "选择火箭发运物品" : "选择轨道投送物品",
                  onSelect: (name, quality) =>
                    direction === "rocket-export"
                      ? runtime.addUniverseRocketExport(location, { id: name, quality })
                      : runtime.addUniverseOrbitalDrop(location, { id: name, quality }),
                };
              }}
            >
              + {direction === "rocket-export" ? "火箭发运" : "轨道投送"}
            </button>
          {/each}
        {/if}
        <div class="view-switch" role="group" aria-label="机制展示方式">
          <button
            class="btn"
            class:active={mechanicView === "list"}
            aria-pressed={mechanicView === "list"}
            onclick={() => (mechanicView = "list")}
          >列表</button>
          <button
            class="btn"
            class:active={mechanicView === "network"}
            aria-pressed={mechanicView === "network"}
            onclick={() => (mechanicView = "network")}
          >网络</button>
        </div>
        {#if universe}<label class="check"><input type="checkbox" bind:checked={groupLocations} />按地点聚类</label>{/if}
        {#if !runtime.activeContext}
          <span class="chip warn">未载入游戏数据</span>
        {/if}
        <span class="spacer"></span>
        <div class="toolbar-stats">
          <span class="chip">{mechanics.length} 机制</span>
          <span class="chip">{targets.length} 目标</span>
          {#if runtime.solveError}<span class="chip warn">求解错误</span>{/if}
        </div>
        <div class="menu-wrap">
          <button
            class="icon-btn"
            aria-label="更多机制操作"
            aria-expanded={mechMenuOpen}
            title="更多机制操作：保留已有机制、清理未用、按流量排序、配方合并"
            onclick={() => {
              mechMenuOpen = !mechMenuOpen;
            }}
          >⋯</button>
          {#if mechMenuOpen}
            <div class="menu-catcher" aria-hidden="true" onclick={() => (mechMenuOpen = false)}></div>
            <div class="menu toolbar-menu">
              <div class="menu-label">机制列表</div>
              {#if planning}
                <label
                  class="menu-check"
                  title="开启后，已有的已启用机制也参与优化并保留原列表，只追加新机制；未使用的机制可用「移除未用」清理。"
                >
                  <input
                    type="checkbox"
                    checked={planning.preserve_existing_mechanics ?? false}
                    disabled={universeBusy || runtime.autoPlanning || !factory}
                    onchange={(event) =>
                      runtime.setPreserveExistingMechanics(event.currentTarget.checked).catch(() => {})}
                  />
                  保留已有机制
                </label>
              {/if}
              <button
                disabled={!solved}
                title="移除整个工厂中本次求解用量为 0 的机制，不受显示筛选影响"
                onclick={() => {
                  mechMenuOpen = false;
                  actions?.commands.cleanup("remove-unused").catch(() => {});
                }}
              >移除未用</button>
              <button
                disabled={!solved}
                title="按求解流量从大到小重排整个工厂的机制，不受显示筛选影响"
                onclick={() => {
                  mechMenuOpen = false;
                  actions?.commands.cleanup("sort-by-solution-rate").catch(() => {});
                }}
              >按流量排序</button>
              <button
                class:active={mergeRecipes}
                title="同配方不同品质合并成一张卡"
                onclick={() => (mergeRecipes = !mergeRecipes)}
              >
                配方合并
                <span class="menu-mark">{mergeRecipes ? "✓" : ""}</span>
              </button>
            </div>
          {/if}
        </div>
      </div>

      <MechanicWorkspace
        project={project?.id ?? null}
        factory={factory?.id ?? null}
        entries={mechanics}
        solutions={solveMap}
        {solved}
        {actions}
        {filterOpen}
        {mergeRecipes}
        groupLocations={!!universe && groupLocations}
        view={mechanicView}
        onFilterActiveChange={(active) => (filterActive = active)}
        onFilterClose={() => (filterOpen = false)}
      />
    </section>

    {#if addMechMenuPos}
      <div class="menu-catcher" aria-hidden="true" onclick={() => (addMechMenuPos = null)}></div>
      <div class="menu fixed" style={`top:${addMechMenuPos.top}px;right:${addMechMenuPos.right}px`}>
        {#each mechKinds as option (option.kind)}
          <button
            onclick={() => {
              addMechMenuPos = null;
              actions?.commands.addMechanic(option.kind).catch(() => {});
            }}
          >{option.label}</button>
        {/each}
      </div>
    {/if}

    <!-- 右栏：求解结果（数据上下文移到顶部「数据」入口） -->
    <aside class="col rail rail-right">
      <SolvePanel
        solve={universe ? null : solve}
        universeSolve={universe ? runtime.universeSolve : null}
        error={universe ? runtime.universeSolveError : runtime.solveError}
        diagnostic={universe ? runtime.universeSolveDiagnostic : null}
        busy={universe ? universeBusy || runtime.universeSolving : !!runtime.solving}
        onSuggest={openSuggestions}
      />
    </aside>
  </main>
</div>

<GameDataModal
  bind:dataOpen
  bind:loadGameOpen
  bind:fontSettingsOpen
  {project}
  onRenameContext={renameContextPrompt}
  onRequestConfirm={(state) => (confirmState = state)}
/>

<!-- ══ 选择器 ══ -->
<!-- Each new request object identifies a fresh editing session. Mutating a
     filter within that request must preserve the user's current draft. -->
{#if selector}
  {#key selector}
    <Selector
      kind={selector.kind}
      title={selector.title}
      kindOptions={selector.kinds}
      categoryFilter={selector.categoryFilter}
      tilePurpose={selector.tilePurpose}
      allowedNames={selector.allowedNames}
      initialName={selector.initialName}
      initialQuality={selector.initialQuality}
      onSelect={selector.onSelect}
      onPickKind={selector.onPickKind}
      respectAccessibility={selector.respectAccessibility}
      multiLevelTechOnly={selector.multiLevelTechOnly}
      onClose={() => (selector = null)}
    />
  {/key}
{/if}

{#if flowSelector}
  {#key flowSelector}
    <Selector
      kind="item"
      title={flowSelector.title}
      flowMode
      initialTab={flowSelector.initialTab}
      initialName={flowSelector.initialName}
      initialQuality={flowSelector.initialQuality}
      allowedNames={flowSelector.allowedNames}
      respectAccessibility={flowSelector.respectAccessibility}
      onSelectFlow={flowSelector.onSelectFlow}
      onClose={() => (flowSelector = null)}
    />
  {/key}
{/if}

<WorkspaceDialogs
  bind:newProjectOpen
  bind:newFactoryOpen
  bind:newUniverseOpen
  bind:confirmState
  bind:renameCtx
  bind:renameTarget
  bind:closeProjectState
/>

<PlanningPreferencesModal
  bind:prefsOpen
  {project}
  onOpenSelector={openSelector}
  onNotice={showNotice}
/>

<SuggestionsModal bind:suggestions />

<style>
  /* ── 应用骨架 ───────────────────────────────────────────────── */

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
    background: var(--bg);
  }

  .spacer {
    flex: 1 1 auto;
  }

  .menu-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .menu-catcher {
    position: fixed;
    z-index: 39;
    inset: 0;
  }

  /* ── 菜单 ───────────────────────────────────────────────────── */

  .menu {
    position: absolute;
    z-index: 40;
    top: calc(100% + 6px);
    left: 0;
    min-width: 232px;
    max-height: min(520px, calc(100vh - 76px));
    overflow-y: auto;
    display: grid;
    gap: 1px;
    padding: 6px;
    background: var(--surface-glass);
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .menu.toolbar-menu {
    left: auto;
    right: 0;
    width: 220px;
  }

  .menu-label {
    padding: 6px 9px 4px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .menu button {
    width: 100%;
    min-height: 30px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 9px;
    color: var(--text);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    text-align: left;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .menu button:hover {
    background: var(--card-hover);
  }

  .menu button.active {
    background: var(--accent-dim);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .menu button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .menu-check {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 30px;
    padding: 6px 9px;
    border-radius: var(--radius-sm);
    color: var(--text);
    font-size: 11.5px;
    cursor: pointer;
    transition: background var(--transition-fast);
  }

  .menu-check:hover {
    background: var(--card-hover);
  }

  .menu-check input {
    margin: 0;
    accent-color: var(--accent);
  }

  .menu-mark {
    margin-left: auto;
    color: var(--ok);
    font-family: var(--mono);
    font-size: 11px;
  }

  /* 添加机制菜单：fixed 定位，脱离滚动容器。 */
  .menu.fixed {
    position: fixed;
    z-index: 40;
    left: auto;
    width: max-content;
    min-width: 200px;
    max-width: min(340px, calc(100vw - 16px));
    max-height: min(460px, 64vh);
    overflow-y: auto;
  }

  /* ── 工作区 ─────────────────────────────────────────────────── */

  .workspace {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    display: grid;
    grid-template-columns: 324px minmax(440px, 1fr) 312px;
    overflow: hidden;
    background-color: var(--bg);
    background-image:
      linear-gradient(var(--grid) 1px, transparent 1px),
      linear-gradient(90deg, var(--grid) 1px, transparent 1px);
    background-size: 56px 56px;
  }

  .col {
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .rail {
    display: flex;
    flex-direction: column;
    background: var(--panel);
  }

  .rail-left {
    border-right: 1px solid var(--line);
  }

  .rail-right {
    border-left: 1px solid var(--line);
  }

  .rail .panel {
    padding: 14px 16px;
    background: transparent;
    border: 0;
    border-bottom: 1px solid var(--line);
    border-radius: 0;
    box-shadow: none;
  }

  .rail .panel:last-child {
    border-bottom: 0;
  }

  .rail .panel > .section-head:first-child {
    margin-top: 0;
  }

  .rail-empty {
    display: grid;
    gap: 8px;
    margin: 16px;
    padding: 18px 16px;
    color: var(--muted);
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius);
    font-size: 11.5px;
    line-height: 1.55;
  }

  .rail-empty strong {
    color: var(--text);
    font-size: 12px;
  }

  .center {
    display: flex;
    flex-direction: column;
    padding: 0 16px 28px;
  }

  .view-switch {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .view-switch .btn {
    min-height: 24px;
    padding: 0 9px;
    border-color: transparent;
    color: var(--muted);
    font-size: 11px;
  }

  .view-switch .btn:hover {
    color: var(--text);
    background: var(--card-hover);
    border-color: transparent;
  }

  .view-switch .btn.active,
  .view-switch .btn.active:hover {
    color: var(--accent-ink);
    background: var(--accent);
    border-color: var(--accent);
    font-weight: 700;
  }

  /* ── 中栏工具条 ─────────────────────────────────────────────── */

  .toolbar {
    position: sticky;
    top: 0;
    z-index: 5;
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 12px 0 10px;
    background: var(--surface-glass);
    border-bottom: 1px solid var(--line);
    backdrop-filter: blur(10px);
  }

  .toolbar-stats {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .toolbar .menu-wrap {
    margin-left: 2px;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin: 14px 0 6px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.11em;
    text-transform: uppercase;
  }

  .section-head .count {
    font-family: var(--mono);
    font-weight: 500;
    letter-spacing: 0;
  }

  /* ── 工厂环境行 ─────────────────────────────────────────────── */

  .env-row {
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 40px;
    padding: 6px 0;
    border-bottom: 1px solid var(--line);
  }

  .env-row:last-of-type {
    border-bottom: 0;
  }

  .env-row .sub {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── 响应式：窄窗口先压缩两侧栏 ──────────── */

  @media (max-width: 1360px) {
    .workspace {
      grid-template-columns: 292px minmax(400px, 1fr) 280px;
    }
  }

  @media (max-width: 1120px) {
    .workspace {
      grid-template-columns: 270px minmax(360px, 1fr) 260px;
    }
  }

  @media (max-width: 920px) {
    .workspace {
      grid-template-columns: 1fr;
      overflow-y: auto;
    }
    .rail-left,
    .rail-right {
      border: 0;
      border-top: 1px solid var(--line);
    }
    .center {
      order: -1;
    }
  }
</style>
