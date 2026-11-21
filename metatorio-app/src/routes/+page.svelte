<script lang="ts">
  // Metatorio 主界面：紧凑布局，圆角卡片分区；游戏内物品一律图标按钮，
  // 一般操作一律文字按钮。
  import { onMount, tick } from "svelte";
  import { DEFAULT_ORBITAL_TRANSPORT_COSTS } from "$lib/runtime/project-settings";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { suggest } from "$lib/runtime/client";
  import type {
    ContextInfo,
    DualVar,
    Location,
    MechanicKind,
    ProjectDocument,
  } from "$lib/runtime/types";
  import HoverIcon from "$lib/ui/HoverIcon.svelte";
  import LocationIcon from "$lib/ui/LocationIcon.svelte";
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
  import CommandPalette, { type PaletteCommand } from "$lib/ui/CommandPalette.svelte";
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
  import { locationKey, locationLabel, transportLabel, universeView } from "$lib/mechanics/located";
  import { createMechanicActions } from "$lib/mechanics/actions";
  import { DEFAULT_MECHANIC_VIEW, type MechanicViewMode } from "$lib/mechanics/workspace";
  import { mechanicKindLabels, mechanicPrimary } from "$lib/mechanics/presentation";
  import { presentFlow } from "$lib/dual-var";

  const LEFT_RAIL_DEFAULT = 324;
  const RIGHT_RAIL_DEFAULT = 312;
  const RAIL_MIN = 240;
  const RAIL_MAX = 480;
  const LEFT_WIDTH_KEY = "metatorio.leftRailWidth";
  const RIGHT_WIDTH_KEY = "metatorio.rightRailWidth";

  const mechKindGroups: {
    label: string;
    items: { kind: MechanicKind; label: string }[];
  }[] = [
    {
      label: "生产加工",
      items: [
        { kind: "recipe", label: "配方" },
        { kind: "mining", label: "采矿" },
        { kind: "plant", label: "种植" },
        { kind: "spoil", label: "变质" },
        { kind: "virtual-science", label: "虚拟科研" },
      ],
    },
    {
      label: "能源与热力",
      items: [
        { kind: "generator", label: "发电机" },
        { kind: "boiler", label: "锅炉" },
        { kind: "reactor", label: "反应堆" },
        { kind: "fusion-reactor", label: "聚变反应堆" },
        { kind: "fusion-generator", label: "聚变发电机" },
        { kind: "solar", label: "太阳能" },
        { kind: "item-fuel", label: "物品燃料" },
        { kind: "fluid-fuel", label: "流体燃料" },
        { kind: "fluid-heat", label: "流体热" },
      ],
    },
    {
      label: "太空物流",
      items: [{ kind: "item-launch", label: "火箭发射" }],
    },
    {
      label: "地格处理",
      items: [
        { kind: "tile-extract", label: "地格抽取" },
        { kind: "tile-dispose", label: "地格销毁" },
      ],
    },
  ];

  function clampRailWidth(value: number): number {
    return Math.round(Math.min(RAIL_MAX, Math.max(RAIL_MIN, value)));
  }

  function loadStoredRailWidth(key: string, fallback: number): number {
    if (typeof localStorage === "undefined") return fallback;
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? clampRailWidth(parsed) : fallback;
  }

  function persistRailWidth(key: string, width: number): void {
    if (typeof localStorage === "undefined") return;
    localStorage.setItem(key, String(width));
  }

  let leftWidth = $state(LEFT_RAIL_DEFAULT);
  let rightWidth = $state(RIGHT_RAIL_DEFAULT);
  let leftCollapsed = $state(false);
  let rightCollapsed = $state(false);
  let savedBeforeFullCanvas = $state<{ left: boolean; right: boolean } | null>(null);
  let resizing = $state<"left" | "right" | null>(null);
  let workspaceEl = $state<HTMLElement | null>(null);
  let dragStartX = 0;
  let dragStartWidth = 0;

  onMount(() => {
    leftWidth = loadStoredRailWidth(LEFT_WIDTH_KEY, LEFT_RAIL_DEFAULT);
    rightWidth = loadStoredRailWidth(RIGHT_WIDTH_KEY, RIGHT_RAIL_DEFAULT);
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
  let commandPaletteOpen = $state(false);
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

  // 轻量提示（自动消失）；noticeTone 供 Stage 2 传给 ErrorBanner Toast
  let notice = $state<string | null>(null);
  let noticeTone = $state<"info" | "warn" | "danger">("info");
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function showNotice(message: string, tone: "info" | "warn" | "danger" = "info") {
    notice = message;
    noticeTone = tone;
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

  // ── 项目未保存 revision 追踪 ────────────────────────────────────
  type SavedProjectTracker = {
    savedRevision: number;
    editRevision: number;
    savedRef: ProjectDocument;
    lastRef: ProjectDocument;
  };
  let projectTrackers = $state<Map<number, SavedProjectTracker>>(new Map());

  $effect(() => {
    const projects = runtime.document?.projects ?? [];
    const rev = runtime.revision;
    const next = new Map<number, SavedProjectTracker>();
    let changed = projects.length !== projectTrackers.size;
    for (const p of projects) {
      const existing = projectTrackers.get(p.id);
      if (!existing) {
        changed = true;
        next.set(p.id, {
          savedRevision: rev,
          editRevision: rev,
          savedRef: p,
          lastRef: p,
        });
      } else if (existing.lastRef !== p) {
        changed = true;
        next.set(p.id, {
          savedRevision: existing.savedRevision,
          editRevision: rev,
          savedRef: existing.savedRef,
          lastRef: p,
        });
      } else {
        next.set(p.id, existing);
      }
    }
    if (changed) projectTrackers = next;
  });

  function markProjectSaved(projectId: number) {
    const current = runtime.document?.projects.find((item) => item.id === projectId);
    if (!current) return;
    const next = new Map(projectTrackers);
    const rev = runtime.revision;
    next.set(projectId, {
      savedRevision: rev,
      editRevision: rev,
      savedRef: current,
      lastRef: current,
    });
    projectTrackers = next;
  }

  let projectUnsaved = $derived.by(() => {
    if (!project) return false;
    const tracker = projectTrackers.get(project.id);
    if (!tracker) return false;
    return (
      tracker.editRevision !== tracker.savedRevision ||
      tracker.lastRef !== tracker.savedRef
    );
  });

  async function saveCurrentProjectAction() {
    if (!project || runtime.busy) return;
    const targetId = project.id;
    try {
      const ok = await runtime.saveCurrentProject();
      if (ok) {
        markProjectSaved(targetId);
        showNotice("项目已保存");
      }
    } catch {
      /* lastError 已由 runtime 展示 */
    }
  }

  async function saveProjectAsAction() {
    if (!project || runtime.busy) return;
    const targetId = project.id;
    try {
      const ok = await runtime.saveProjectAs();
      if (ok) {
        markProjectSaved(targetId);
        showNotice("项目已另存为");
      }
    } catch {
      /* lastError 已由 runtime 展示 */
    }
  }

  // ── 栏宽拖拽与折展 ────────────────────────────────────────────
  function toggleLeftRail() {
    savedBeforeFullCanvas = null;
    leftCollapsed = !leftCollapsed;
  }

  function toggleRightRail() {
    savedBeforeFullCanvas = null;
    rightCollapsed = !rightCollapsed;
  }

  function toggleFullCanvas() {
    if (leftCollapsed && rightCollapsed) {
      const restore = savedBeforeFullCanvas;
      savedBeforeFullCanvas = null;
      if (restore && (!restore.left || !restore.right)) {
        leftCollapsed = restore.left;
        rightCollapsed = restore.right;
      } else {
        leftCollapsed = false;
        rightCollapsed = false;
      }
    } else {
      savedBeforeFullCanvas = { left: leftCollapsed, right: rightCollapsed };
      leftCollapsed = true;
      rightCollapsed = true;
    }
  }

  function startResize(side: "left" | "right", event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    const target = event.currentTarget as HTMLElement;
    target.setPointerCapture(event.pointerId);
    resizing = side;
    dragStartX = event.clientX;
    dragStartWidth = side === "left" ? leftWidth : rightWidth;
  }

  function moveResize(side: "left" | "right", event: PointerEvent) {
    if (resizing !== side) return;
    const delta = side === "left" ? event.clientX - dragStartX : dragStartX - event.clientX;
    const containerWidth = workspaceEl?.clientWidth ?? window.innerWidth;
    const otherWidth =
      side === "left"
        ? (rightCollapsed ? 0 : rightWidth)
        : (leftCollapsed ? 0 : leftWidth);
    const maxAllowed = Math.max(RAIL_MIN, Math.min(RAIL_MAX, containerWidth - otherWidth - 360));
    const clamped = Math.round(Math.min(maxAllowed, Math.max(RAIL_MIN, dragStartWidth + delta)));
    if (side === "left") leftWidth = clamped;
    else rightWidth = clamped;
  }

  function endResize(side: "left" | "right", event: PointerEvent) {
    if (resizing !== side) return;
    const target = event.currentTarget as HTMLElement;
    if (target.hasPointerCapture(event.pointerId)) {
      target.releasePointerCapture(event.pointerId);
    }
    resizing = null;
    persistRailWidth(
      side === "left" ? LEFT_WIDTH_KEY : RIGHT_WIDTH_KEY,
      side === "left" ? leftWidth : rightWidth,
    );
  }

  function resetRailWidth(side: "left" | "right") {
    if (side === "left") {
      leftWidth = LEFT_RAIL_DEFAULT;
      persistRailWidth(LEFT_WIDTH_KEY, LEFT_RAIL_DEFAULT);
    } else {
      rightWidth = RIGHT_RAIL_DEFAULT;
      persistRailWidth(RIGHT_WIDTH_KEY, RIGHT_RAIL_DEFAULT);
    }
  }

  function handleResizerKeydown(side: "left" | "right", event: KeyboardEvent) {
    const step = event.shiftKey ? 32 : 16;
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      const delta = side === "left" ? -step : step;
      if (side === "left") {
        leftWidth = clampRailWidth(leftWidth + delta);
        persistRailWidth(LEFT_WIDTH_KEY, leftWidth);
      } else {
        rightWidth = clampRailWidth(rightWidth + delta);
        persistRailWidth(RIGHT_WIDTH_KEY, rightWidth);
      }
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      const delta = side === "left" ? step : -step;
      if (side === "left") {
        leftWidth = clampRailWidth(leftWidth + delta);
        persistRailWidth(LEFT_WIDTH_KEY, leftWidth);
      } else {
        rightWidth = clampRailWidth(rightWidth + delta);
        persistRailWidth(RIGHT_WIDTH_KEY, rightWidth);
      }
    } else if (event.key === "Home") {
      event.preventDefault();
      resetRailWidth(side);
    }
  }

  // ── 宇宙星球快速筛选条 & Stage 2 跨组件联动接口 ───────────────
  // Stage 2 wire: pass universeLocationFilter, workspaceFocusRequest, workspaceTraceRequest,
  // searchFocusSeq, handleTraceFlow, handleRevealInList, handleRevealInNetwork to MechanicWorkspace.
  let universeLocationFilter = $state<Location | null>(null);
  let searchFocusSeq = $state(0);
  let linkageSeq = 0;
  let workspaceFocusRequest = $state<{
    mechanicId?: number;
    flow?: DualVar;
    location?: Location;
    seq: number;
  } | null>(null);
  let workspaceTraceRequest = $state<{
    flow: DualVar;
    side: "input" | "output";
    seq: number;
  } | null>(null);

  let lastScopeTag = $state("");
  $effect(() => {
    const nextTag = `${runtime.selectedProjectId ?? 0}:${runtime.selectedUniverseId ?? 0}`;
    if (nextTag !== lastScopeTag) {
      lastScopeTag = nextTag;
      universeLocationFilter = null;
    }
  });

  const universeLocations = $derived.by(() => {
    if (!universe) return [] as { location: Location; key: string; label: string; count: number }[];
    const counts = new Map<string, number>();
    const seen = new Map<string, Location>();
    const addLoc = (loc: Location | undefined) => {
      if (!loc) return;
      const key = locationKey(loc);
      if (!seen.has(key)) seen.set(key, loc);
    };
    for (const entry of mechanics) {
      if (!entry.location) continue;
      const key = locationKey(entry.location);
      addLoc(entry.location);
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    for (const target of universe.targets) addLoc(target.flow.location);
    for (const input of universe.external_inputs ?? []) addLoc(input.flow.location);
    for (const expr of universe.target_expressions ?? []) {
      for (const term of expr.terms) addLoc(term.flow.location);
    }
    if (seen.size === 0) return [];
    // 只要宇宙工厂中已有地点，始终在末尾提供「太空」切换切片。
    addLoc({ kind: "space" });
    const list = [...seen.entries()].map(([key, location]) => ({
      key,
      location,
      label: locationLabel(location, (k, n) => runtime.localizedName(k, n)),
      count: counts.get(key) ?? 0,
    }));
    list.sort((a, b) => {
      if (a.location.kind !== b.location.kind) return a.location.kind === "space" ? 1 : -1;
      return a.label.localeCompare(b.label, "zh-Hans");
    });
    return list;
  });

  function selectUniverseScope(loc: Location | null) {
    universeLocationFilter = loc;
    if (loc) runtime.universeAddLocation = loc;
  }

  function triggerSearchFocus() {
    searchFocusSeq += 1;
    // Stage 1 fallback：若当前已有筛选/网络搜索输入框则直接聚焦。
    requestAnimationFrame(() => {
      const input = document.querySelector<HTMLInputElement>(
        '.network-search input, [aria-label="搜索筛选对象"], [aria-label="筛选机制"]',
      );
      input?.focus();
      input?.select();
    });
  }

  // Stage 2 wire: pass to MechanicWorkspace / MechanicNetworkView
  function handleRevealInList(mechanicId: number) {
    mechanicView = "list";
    workspaceFocusRequest = { mechanicId, seq: ++linkageSeq };
    void tick().then(() => {
      document
        .querySelector(`[data-mechanic-row="${mechanicId}"]`)
        ?.scrollIntoView({ behavior: "smooth", block: "center" });
    });
  }

  // Stage 2 wire: pass to MechanicWorkspace / MechanicListView
  function handleRevealInNetwork(mechanicId: number) {
    mechanicView = "network";
    workspaceFocusRequest = { mechanicId, seq: ++linkageSeq };
  }

  // Stage 2 wire: pass to MechanicWorkspace / SolvePanel
  function handleTraceFlow(
    flow: DualVar,
    mode: "producers" | "consumers" | "network" | "suggest",
    location?: Location,
  ) {
    if (mode === "suggest") {
      void openSuggestions(flow, location);
      return;
    }
    if (mode === "network") {
      mechanicView = "network";
      workspaceFocusRequest = { flow, location, seq: ++linkageSeq };
      return;
    }
    mechanicView = "list";
    filterOpen = true;
    workspaceTraceRequest = {
      flow,
      side: mode === "producers" ? "output" : "input",
      seq: ++linkageSeq,
    };
  }

  // Stage 2 wire: pass to SolvePanel / DiagnosticView
  function handleAddExternalInputFromDiagnostic(flow: DualVar, location?: Location) {
    if (!project || !factory) return;
    if (universe) {
      const loc = location ?? runtime.universeAddLocation;
      runtime
        .dispatchUniverse(project.id, universe.id, {
          "add-external-input": { flow: { location: loc, flow }, penalty: 1 },
        })
        .then(() => showNotice("已添加外部输入"))
        .catch(() => {});
      return;
    }
    runtime
      .addExternalInputFlow(flow, 1)
      .then(() => showNotice("已添加外部输入"))
      .catch(() => {});
  }

  // Stage 2 wire: pass to SolvePanel / DiagnosticView
  function handleAddTargetFromDiagnostic(flow: DualVar, location?: Location) {
    if (!project || !factory) return;
    if (universe) {
      const loc = location ?? runtime.universeAddLocation;
      runtime
        .dispatchUniverse(project.id, universe.id, {
          "add-target": { flow: { location: loc, flow }, amount: 1 },
        })
        .then(() => showNotice("已添加生产目标"))
        .catch(() => {});
      return;
    }
    runtime
      .addTargetFlow(flow, 1)
      .then(() => showNotice("已添加生产目标"))
      .catch(() => {});
  }

  // Stage 2 wire: pass to SolvePanel / DiagnosticView
  function handleDisableStrictSink() {
    if (!project || !factory) return;
    if (universe) {
      runtime
        .dispatchUniverse(project.id, universe.id, {
          "set-strict-sink": { strict: false },
        })
        .then(() => showNotice("已关闭严格消耗"))
        .catch(() => {});
      return;
    }
    runtime
      .setStrictSink(false)
      .then(() => showNotice("已关闭严格消耗"))
      .catch(() => {});
  }

  // Stage 2 wire helper anchor (ensures zero unused-symbol warnings in Stage 1 while keeping handlers ready)
  const _stage2WireHooks = {
    get noticeTone() {
      return noticeTone;
    },
    get searchFocusSeq() {
      return searchFocusSeq;
    },
    get workspaceFocusRequest() {
      return workspaceFocusRequest;
    },
    get workspaceTraceRequest() {
      return workspaceTraceRequest;
    },
    handleRevealInList,
    handleRevealInNetwork,
    handleTraceFlow,
    handleAddExternalInputFromDiagnostic,
    handleAddTargetFromDiagnostic,
    handleDisableStrictSink,
  };
  void _stage2WireHooks;

  // ── 添加机制并尝试自动打开主原型选择器 ──────────────────────────
  async function addMechanicAndPick(kind: MechanicKind) {
    const currentActions = actions;
    if (!currentActions) return;
    const scope = currentActions.commands.scope;
    const beforeIds = new Set(
      (runtime.mechanicFactory(scope)?.mechanics ?? []).map((entry) => entry.id),
    );
    await currentActions.commands.addMechanic(kind);
    const stillSameScope =
      runtime.selectedProjectId === scope.project &&
      (scope.kind === "universe" ? runtime.selectedUniverseId : runtime.selectedFactoryId) ===
        scope.factory;
    if (!stillSameScope) return;
    const afterMechanics = runtime.mechanicFactory(scope)?.mechanics ?? [];
    const added = afterMechanics.filter((entry) => !beforeIds.has(entry.id)).at(-1);
    if (!added) return;
    const primary = mechanicPrimary(added.mechanic);
    await currentActions.pick(added.id, primary.kind);
  }

  function jumpToTarget(index: number) {
    leftCollapsed = false;
    leftTab = "factory";
    void tick().then(() => {
      const row = document.querySelectorAll<HTMLElement>(".targets-panel .rows > .row-item")[index];
      row?.scrollIntoView({ behavior: "smooth", block: "center" });
      row?.querySelector<HTMLInputElement>("input.num")?.focus();
    });
  }

  // ── 命令面板指令集 ──────────────────────────────────────────────
  const paletteCommands = $derived.by((): PaletteCommand[] => {
    if (!commandPaletteOpen) return [];
    const items: PaletteCommand[] = [];

    if (project) {
      for (const item of project.factories) {
        const active = item.id === runtime.selectedFactory?.id;
        items.push({
          id: `factory:${item.id}`,
          group: "切换工厂",
          label: item.name,
          detail: "普通工厂",
          badge: active ? "当前" : undefined,
          keywords: `factory 工厂 ${item.name}`,
          action: () => runtime.selectFactory(item.id),
        });
      }
      for (const item of project.universes ?? []) {
        const active = item.id === runtime.selectedUniverseId;
        items.push({
          id: `universe:${item.id}`,
          group: "切换工厂",
          label: item.name,
          detail: "宇宙工厂",
          badge: active ? "当前" : undefined,
          keywords: `universe 宇宙工厂 ${item.name}`,
          action: () => runtime.selectUniverse(item.id),
        });
      }
    }

    if (universe && universeLocations.length > 0) {
      items.push({
        id: "scope:all",
        group: "星球筛选",
        label: "显示全部地点",
        detail: `${mechanics.length} 个机制`,
        badge: universeLocationFilter === null ? "当前" : undefined,
        keywords: "all planet space 全部 星球 地点",
        action: () => selectUniverseScope(null),
      });
      for (const loc of universeLocations) {
        const active = locationKey(universeLocationFilter ?? undefined) === loc.key;
        items.push({
          id: `scope:${loc.key}`,
          group: "星球筛选",
          label: `筛选地点：${loc.label}`,
          detail: `${loc.count} 个机制`,
          badge: active ? "当前" : undefined,
          keywords: `planet space location 星球 地点 ${loc.label} ${loc.key}`,
          action: () => selectUniverseScope(loc.location),
        });
      }
    }

    items.push(
      {
        id: "view:list",
        group: "视图与布局",
        label: "切换到列表视图",
        badge: mechanicView === "list" ? "当前" : undefined,
        keywords: "list view 列表 视图",
        action: () => {
          mechanicView = "list";
        },
      },
      {
        id: "view:network",
        group: "视图与布局",
        label: "切换到网络视图",
        badge: mechanicView === "network" ? "当前" : undefined,
        keywords: "network graph view 网络 视图 画布",
        action: () => {
          mechanicView = "network";
        },
      },
      {
        id: "rail:left",
        group: "视图与布局",
        label: leftCollapsed ? "展开左栏" : "折叠左栏",
        shortcut: "[",
        keywords: "left rail sidebar collapse 折叠 展开 左栏",
        action: toggleLeftRail,
      },
      {
        id: "rail:right",
        group: "视图与布局",
        label: rightCollapsed ? "展开右栏" : "折叠右栏",
        shortcut: "]",
        keywords: "right rail solve collapse 折叠 展开 右栏",
        action: toggleRightRail,
      },
      {
        id: "rail:full-canvas",
        group: "视图与布局",
        label: leftCollapsed && rightCollapsed ? "恢复左右侧栏" : "全宽画布（折起两侧栏）",
        keywords: "full canvas fullscreen 全宽 画布",
        action: toggleFullCanvas,
      },
      {
        id: "search:focus",
        group: "视图与布局",
        label: "聚焦机制搜索 / 筛选",
        shortcut: "Ctrl+F / /",
        keywords: "search filter 搜索 筛选",
        action: triggerSearchFocus,
      },
    );

    items.push(
      {
        id: "plan:auto",
        group: "规划与设置",
        label: "执行自动规划",
        disabled: !factory || universeBusy || runtime.autoPlanning,
        keywords: "auto plan 自动规划",
        action: () => (universe ? autoPlanUniverse() : runtime.autoPlan()),
      },
      {
        id: "plan:prefs",
        group: "规划与设置",
        label: "打开规划偏好…",
        disabled: !project,
        keywords: "preferences modules beacons machines 规划偏好 插件 机器",
        action: () => {
          prefsOpen = true;
        },
      },
      {
        id: "scale:seconds",
        group: "规划与设置",
        label: "时间刻度：每秒 (/s)",
        badge: project?.settings.time_scale === "seconds" ? "当前" : undefined,
        disabled: !project || runtime.busy,
        keywords: "time scale seconds /s 时间刻度 秒",
        action: () => runtime.setTimeScale("seconds"),
      },
      {
        id: "scale:minutes",
        group: "规划与设置",
        label: "时间刻度：每分钟 (/min)",
        badge: project?.settings.time_scale === "minutes" ? "当前" : undefined,
        disabled: !project || runtime.busy,
        keywords: "time scale minutes /m /min 时间刻度 分钟",
        action: () => runtime.setTimeScale("minutes"),
      },
      {
        id: "scale:hours",
        group: "规划与设置",
        label: "时间刻度：每小时 (/h)",
        badge: project?.settings.time_scale === "hours" ? "当前" : undefined,
        disabled: !project || runtime.busy,
        keywords: "time scale hours /h 时间刻度 小时",
        action: () => runtime.setTimeScale("hours"),
      },
      {
        id: "project:save",
        group: "工程文件",
        label: "保存当前项目",
        shortcut: "Ctrl+S",
        disabled: !project || runtime.busy,
        keywords: "save project 保存 项目",
        action: saveCurrentProjectAction,
      },
      {
        id: "project:save-as",
        group: "工程文件",
        label: "项目另存为…",
        shortcut: "Ctrl+Shift+S",
        disabled: !project || runtime.busy,
        keywords: "save as 另存为",
        action: saveProjectAsAction,
      },
      {
        id: "data:manage",
        group: "工程文件",
        label: "管理游戏数据与上下文…",
        keywords: "game data context 游戏数据 上下文",
        action: () => {
          dataOpen = true;
        },
      },
    );

    targets.forEach((target, idx) => {
      const info = presentFlow(target.flow, (k, n) => runtime.localizedName(k, n));
      const locText = target.location
        ? locationLabel(target.location, (k, n) => runtime.localizedName(k, n))
        : "";
      items.push({
        id: `target:${target.id}`,
        group: "跳转到目标",
        label: info.label,
        detail: [locText, info.qualifiers.join(" · ")].filter(Boolean).join(" · "),
        keywords: `target 目标 ${info.description} ${info.identifier} ${locText}`,
        action: () => jumpToTarget(idx),
      });
    });

    for (const entry of mechanics) {
      const primary = mechanicPrimary(entry.mechanic);
      const primaryLabel = primary.name
        ? runtime.localizedName(primary.prototypeKind, primary.name)
        : "未设置";
      const kindText = entry.transport
        ? transportLabel(entry.transport)
        : mechanicKindLabels[entry.mechanic.type] ?? entry.mechanic.type;
      const locText = entry.location
        ? locationLabel(entry.location, (k, n) => runtime.localizedName(k, n))
        : "";
      items.push({
        id: `mech:${entry.id}`,
        group: "跳转到机制",
        label: `${primaryLabel}`,
        detail: [kindText, locText].filter(Boolean).join(" · "),
        badge: entry.enabled ? undefined : "已停用",
        keywords: `mechanic 机制 ${primaryLabel} ${primary.name} ${kindText} ${locText}`,
        action: () => {
          if (mechanicView === "network") {
            handleRevealInNetwork(entry.id);
          } else {
            handleRevealInList(entry.id);
          }
        },
      });
    }

    return items;
  });

  function hasOpenModal(): boolean {
    return Boolean(
      commandPaletteOpen ||
        selector ||
        flowSelector ||
        prefsOpen ||
        dataOpen ||
        loadGameOpen ||
        fontSettingsOpen ||
        newProjectOpen ||
        newFactoryOpen ||
        newUniverseOpen ||
        confirmState ||
        renameCtx ||
        renameTarget ||
        closeProjectState ||
        suggestions,
    );
  }

  /** 全局快捷键：Ctrl/Cmd+K 命令面板；Ctrl/Cmd+S 保存；Ctrl/Cmd+F 或 / 搜索聚焦；
   *  [ / ] 折展左右栏；Ctrl/Cmd+Z 撤销；Ctrl/Cmd+Shift+Z 或 Ctrl+Y 重做。 */
  function handleGlobalKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && (mechMenuOpen || addMechMenuPos)) {
      closeMenus();
      return;
    }

    const target = event.target as HTMLElement | null;
    const tag = target?.tagName;
    const isEditable =
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      target?.isContentEditable === true;
    const mod = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();

    if (mod && !event.altKey) {
      if (key === "k" && !event.shiftKey) {
        event.preventDefault();
        closeMenus();
        commandPaletteOpen = !commandPaletteOpen;
        return;
      }
      if (key === "s") {
        event.preventDefault();
        if (event.shiftKey) void saveProjectAsAction();
        else void saveCurrentProjectAction();
        return;
      }
      if (key === "f" && !event.shiftKey) {
        if (hasOpenModal()) return;
        event.preventDefault();
        triggerSearchFocus();
        return;
      }
      if (isEditable) return;
      if (key === "z" && !event.shiftKey) {
        event.preventDefault();
        runtime.undo().catch(() => {});
      } else if ((key === "z" && event.shiftKey) || key === "y") {
        event.preventDefault();
        runtime.redo().catch(() => {});
      }
      return;
    }

    if (isEditable || event.altKey || event.ctrlKey || event.metaKey || hasOpenModal()) {
      return;
    }

    if (event.key === "/") {
      event.preventDefault();
      triggerSearchFocus();
    } else if (event.key === "[") {
      event.preventDefault();
      toggleLeftRail();
    } else if (event.key === "]") {
      event.preventDefault();
      toggleRightRail();
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
    {projectUnsaved}
    {leftCollapsed}
    {rightCollapsed}
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
    onToggleLeftRail={toggleLeftRail}
    onToggleRightRail={toggleRightRail}
    onOpenCommandPalette={() => {
      closeMenus();
      commandPaletteOpen = true;
    }}
    onSaveProject={() => void saveCurrentProjectAction()}
    onSaveProjectAs={() => void saveProjectAsAction()}
  />

  <!-- Stage 2 wire: pass {noticeTone} to ErrorBanner -->
  <ErrorBanner
    {notice}
    {missingProjectContext}
    onOpenData={() => (dataOpen = true)}
    onSuggest={openSuggestions}
    onNotice={showNotice}
  />

  <!-- ══ 工作区 ══ -->
  <main
    class="workspace"
    class:left-collapsed={leftCollapsed}
    class:right-collapsed={rightCollapsed}
    class:is-resizing={resizing !== null}
    bind:this={workspaceEl}
    style={`--left-rail:${leftWidth}px;--right-rail:${rightWidth}px`}
  >
    <!-- 左栏：生产目标 / 外部输入 / 目标表达式（低频设置收进右上角抽屉） -->
    <!-- 左栏：工厂 / 项目 双 tab -->
    <aside class="col rail rail-left" class:collapsed={leftCollapsed} aria-hidden={leftCollapsed || undefined}>
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

    {#if !leftCollapsed}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        class="rail-resizer"
        class:active={resizing === "left"}
        role="separator"
        aria-orientation="vertical"
        aria-label="调整左栏宽度"
        aria-valuemin={RAIL_MIN}
        aria-valuemax={RAIL_MAX}
        aria-valuenow={leftWidth}
        tabindex="0"
        title="拖拽调整左栏宽度，双击复位默认宽度"
        onpointerdown={(e) => startResize("left", e)}
        onpointermove={(e) => moveResize("left", e)}
        onpointerup={(e) => endResize("left", e)}
        onpointercancel={(e) => endResize("left", e)}
        ondblclick={() => resetRailWidth("left")}
        onkeydown={(e) => handleResizerKeydown("left", e)}
      ></div>
    {/if}

    <!-- 中栏：机制列表 -->
    <section class="col center">
      <div class="center-header">
        <div class="toolbar">
          <button
            class="btn primary"
            title="自动规划：按规划偏好的枚举列表生成候选机制 → 选优 → 回写机制列表。总是按严格供给求解，并在完成后自动开启严格供给"
            onclick={() => (universe ? autoPlanUniverse() : runtime.autoPlan().catch(() => {}))}
            disabled={universeBusy || runtime.autoPlanning || !factory}
          >
            {universeBusy || runtime.autoPlanning ? "规划中…" : "自动规划"}
          </button>
          <button
            class="btn"
            title="打开规划偏好（机器偏好 / 替代数量 / 枚举插件与插件塔）"
            disabled={!project}
            onclick={() => (prefsOpen = true)}
          >
            ⚙ 偏好
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
                const estimate = 356;
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
                title={runtime.universeAddLocation.kind !== "planet"
                  ? direction === "rocket-export"
                    ? "当前新增地点为「太空」，不可发运：火箭发运只能从非太空星球发出，请先选择非太空星球"
                    : "当前新增地点为「太空」，不可投送：轨道投送只能投送到非太空星球，请先选择非太空星球"
                  : direction === "rocket-export"
                    ? `从当前所选星球发往太空；消耗物品与火箭载荷；每件成本：重量（kg）× ${project?.settings.orbital_transport_costs?.rocket_export_per_kg ?? DEFAULT_ORBITAL_TRANSPORT_COSTS.rocket_export_per_kg}`
                    : `从太空投送到当前所选星球；每件成本：${project?.settings.orbital_transport_costs?.orbital_drop_per_stack ?? DEFAULT_ORBITAL_TRANSPORT_COSTS.orbital_drop_per_stack} ÷ 堆叠数量；变质物品额外成本：${project?.settings.orbital_transport_costs?.orbital_drop_spoil_penalty ?? DEFAULT_ORBITAL_TRANSPORT_COSTS.orbital_drop_spoil_penalty} ÷ 有效变质时间（tick）`}
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
            {#if runtime.universeAddLocation.kind !== "planet" && universeLocations.length === 0}
              <span class="scope-space-hint" role="status">当前地点为太空，发运/投送需先选择星球</span>
            {/if}
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
          {#if mechanicView === "network"}
            {@const isFullCanvas = leftCollapsed && rightCollapsed}
            <button
              class="btn"
              class:active={isFullCanvas}
              aria-pressed={isFullCanvas}
              title={isFullCanvas ? "恢复左右侧栏" : "同时折叠左右侧栏，全宽查看网络画布"}
              onclick={toggleFullCanvas}
            >
              {isFullCanvas ? "恢复侧栏" : "全宽画布"}
            </button>
          {/if}
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

        {#if universe && universeLocations.length > 0}
          <div class="scope-strip" role="group" aria-label="宇宙星球快速筛选">
            <button
              class="scope-pill"
              class:active={universeLocationFilter === null}
              aria-pressed={universeLocationFilter === null}
              title="显示全部地点的机制"
              onclick={() => selectUniverseScope(null)}
            >
              <span>全部</span>
              <span class="scope-count">{mechanics.length}</span>
            </button>
            {#each universeLocations as loc (loc.key)}
              <button
                class="scope-pill"
                class:active={locationKey(universeLocationFilter ?? undefined) === loc.key}
                aria-pressed={locationKey(universeLocationFilter ?? undefined) === loc.key}
                title={loc.location.kind === "space"
                  ? "筛选「太空」并设为默认新增地点（太空不可火箭发运或轨道投送）"
                  : `筛选「${loc.label}」并同步设为新增地点（启用火箭发运/轨道投送）`}
                onclick={() => selectUniverseScope(loc.location)}
              >
                <LocationIcon location={loc.location} size={15} />
                <span>{loc.label}</span>
                <span class="scope-count">{loc.count}</span>
              </button>
            {/each}
            {#if runtime.universeAddLocation.kind !== "planet"}
              <span
                class="scope-space-hint"
                role="status"
                title="火箭发运与轨道投送必须在非太空星球上进行"
              >
                当前地点为太空，发运/投送不可用（请选星球）
              </span>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Stage 2 wire: pass locationFilter={universe ? universeLocationFilter : null}
           focusRequest={workspaceFocusRequest} traceRequest={workspaceTraceRequest}
           {searchFocusSeq} onTraceFlow={handleTraceFlow}
           onRevealInList={handleRevealInList} onRevealInNetwork={handleRevealInNetwork}
           to MechanicWorkspace -->
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
      <div
        class="menu fixed mech-add-menu"
        style={`top:${addMechMenuPos.top}px;right:${addMechMenuPos.right}px`}
      >
        {#each mechKindGroups as group (group.label)}
          <div class="mech-add-group">
            <div class="menu-label">{group.label}</div>
            <div class="mech-add-grid">
              {#each group.items as option (option.kind)}
                <button
                  onclick={() => {
                    addMechMenuPos = null;
                    addMechanicAndPick(option.kind).catch(() => {});
                  }}
                >{option.label}</button>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}

    {#if !rightCollapsed}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        class="rail-resizer"
        class:active={resizing === "right"}
        role="separator"
        aria-orientation="vertical"
        aria-label="调整右栏宽度"
        aria-valuemin={RAIL_MIN}
        aria-valuemax={RAIL_MAX}
        aria-valuenow={rightWidth}
        tabindex="0"
        title="拖拽调整右栏宽度，双击复位默认宽度"
        onpointerdown={(e) => startResize("right", e)}
        onpointermove={(e) => moveResize("right", e)}
        onpointerup={(e) => endResize("right", e)}
        onpointercancel={(e) => endResize("right", e)}
        ondblclick={() => resetRailWidth("right")}
        onkeydown={(e) => handleResizerKeydown("right", e)}
      ></div>
    {/if}

    <!-- 右栏：求解结果（数据上下文移到顶部「数据」入口） -->
    <aside class="col rail rail-right" class:collapsed={rightCollapsed} aria-hidden={rightCollapsed || undefined}>
      <!-- Stage 2 wire: pass {mechanics} solutions={solveMap} onTraceFlow={handleTraceFlow}
           onAddExternalInput={handleAddExternalInputFromDiagnostic}
           onAddTarget={handleAddTargetFromDiagnostic}
           onDisableStrictSink={handleDisableStrictSink} to SolvePanel -->
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

<!-- Stage 2 wire: pass existingMechanics={mechanics} to SuggestionsModal -->
<SuggestionsModal bind:suggestions />

<CommandPalette bind:open={commandPaletteOpen} commands={paletteCommands} />

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
    min-width: 248px;
    max-width: min(360px, calc(100vw - 16px));
    max-height: min(520px, 72vh);
    overflow-y: auto;
  }

  .mech-add-menu {
    gap: 6px;
    padding: 8px;
  }

  .mech-add-group + .mech-add-group {
    padding-top: 6px;
    border-top: 1px solid var(--line);
  }

  .mech-add-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(104px, 1fr));
    gap: 2px;
  }

  /* ── 工作区 ─────────────────────────────────────────────────── */

  .workspace {
    --left-col: var(--left-rail, 324px);
    --right-col: var(--right-rail, 312px);
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    display: grid;
    grid-template-columns: var(--left-col) 6px minmax(340px, 1fr) 6px var(--right-col);
    overflow: hidden;
    background-color: var(--bg);
    background-image:
      linear-gradient(var(--grid) 1px, transparent 1px),
      linear-gradient(90deg, var(--grid) 1px, transparent 1px);
    background-size: 56px 56px;
  }

  .workspace.left-collapsed:not(.right-collapsed) {
    grid-template-columns: minmax(340px, 1fr) 6px var(--right-col);
  }

  .workspace.right-collapsed:not(.left-collapsed) {
    grid-template-columns: var(--left-col) 6px minmax(340px, 1fr);
  }

  .workspace.left-collapsed.right-collapsed {
    grid-template-columns: minmax(340px, 1fr);
  }

  .workspace.is-resizing {
    cursor: col-resize;
    user-select: none;
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

  .rail.collapsed {
    display: none;
  }

  .rail-left {
    border-right: 1px solid var(--line);
    overflow-y: hidden;
  }

  .rail-right {
    border-left: 1px solid var(--line);
  }

  .rail-resizer {
    position: relative;
    z-index: 6;
    width: 6px;
    height: 100%;
    background: var(--bg);
    cursor: col-resize;
    touch-action: none;
    transition: background var(--transition-fast);
  }

  .rail-resizer::after {
    content: "";
    position: absolute;
    left: 2px;
    top: 50%;
    transform: translateY(-50%);
    width: 2px;
    height: 30px;
    border-radius: 2px;
    background: var(--line);
    transition: background var(--transition-fast), height var(--transition-fast);
  }

  .rail-resizer:hover,
  .rail-resizer.active,
  .rail-resizer:focus-visible {
    background: var(--accent-dim);
    outline: none;
  }

  .rail-resizer:hover::after,
  .rail-resizer.active::after,
  .rail-resizer:focus-visible::after {
    height: 44px;
    background: var(--accent);
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

  .center-header {
    position: sticky;
    top: 0;
    z-index: 5;
    flex: 0 0 auto;
    background: var(--surface-glass);
    backdrop-filter: blur(10px);
    border-bottom: 1px solid var(--line);
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
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 12px 0 10px;
  }

  .toolbar .btn.active {
    color: var(--accent-ink);
    background: var(--accent);
    border-color: var(--accent);
    font-weight: 700;
  }

  .scope-strip {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 7px 0 9px;
    border-top: 1px solid var(--line);
  }

  .scope-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 24px;
    padding: 2px 8px;
    color: var(--muted);
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: 999px;
    font-size: 11px;
    font-weight: 550;
    cursor: pointer;
    transition:
      background var(--transition-fast),
      border-color var(--transition-fast),
      color var(--transition-fast);
  }

  .scope-pill:hover {
    color: var(--text);
    background: var(--card-hover);
    border-color: var(--line-strong);
  }

  .scope-pill.active {
    color: var(--accent-ink);
    background: var(--accent);
    border-color: var(--accent);
    font-weight: 700;
  }

  .scope-count {
    font-family: var(--mono);
    font-size: 10px;
    opacity: 0.78;
  }

  .scope-space-hint {
    margin-left: auto;
    color: var(--warn);
    font-size: 10.5px;
    line-height: 1.3;
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
      --left-col: min(var(--left-rail, 324px), 292px);
      --right-col: min(var(--right-rail, 312px), 280px);
    }
  }

  @media (max-width: 1120px) {
    .workspace {
      --left-col: min(var(--left-rail, 324px), 268px);
      --right-col: min(var(--right-rail, 312px), 258px);
    }
  }

  @media (max-width: 920px) {
    .workspace,
    .workspace.left-collapsed:not(.right-collapsed),
    .workspace.right-collapsed:not(.left-collapsed),
    .workspace.left-collapsed.right-collapsed {
      grid-template-columns: 1fr;
      overflow-y: auto;
    }
    .rail-resizer {
      display: none;
    }
    .rail-left,
    .rail-right {
      overflow-y: visible;
      border: 0;
      border-top: 1px solid var(--line);
    }
    .center {
      order: -1;
    }
  }
</style>
