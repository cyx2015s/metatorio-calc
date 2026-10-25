<script lang="ts">
  import AutoPlanProgress from "$lib/ui/AutoPlanProgress.svelte";
  import OptimizationSummary from "$lib/ui/OptimizationSummary.svelte";
  import RichText from "$lib/ui/RichText.svelte";
  import { flowName } from "$lib/display-labels";
  // Metatorio 主界面：紧凑布局，圆角卡片分区；游戏内物品一律图标按钮，
  // 一般操作一律文字按钮。
  import { onMount } from "svelte";
  import { dndzone } from "svelte-dnd-action";
  import { getVersion } from "@tauri-apps/api/app";
  import { check as updaterCheck } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { pickGameExecutable, pickModDir, suggest } from "$lib/runtime/client";
  import { itemOf, accessibleKind, accessibleName } from "$lib/runtime/types";
  import { signedCompactNumber } from "$lib/format";
  import type {
    Accessible,
    CatalogKind,
    ContextInfo,
    DualVar,
    MechanicId,
    TargetId,
  } from "$lib/runtime/types";
  import HoverIcon from "$lib/ui/HoverIcon.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Selector from "$lib/ui/Selector.svelte";
  import MechanicWorkspace from "$lib/ui/MechanicWorkspace.svelte";
  import FactorySidebar from "$lib/ui/FactorySidebar.svelte";
  import FactoryTargetsPanel from "$lib/ui/FactoryTargetsPanel.svelte";
  import SolvePanel from "$lib/ui/SolvePanel.svelte";
  import LocationPicker from "$lib/ui/LocationPicker.svelte";
  import UniverseInputsPanel from "$lib/ui/UniverseInputsPanel.svelte";
  import {universeView} from "$lib/mechanics/located";
  import { createExternalInputShortcuts, DISABLED_RESOURCE_PENALTY, FREE_ENERGY_FLOWS } from "$lib/external-input-shortcuts";
  import { createMechanicActions } from "$lib/mechanics/actions";
  import { DEFAULT_MECHANIC_VIEW, type MechanicViewMode } from "$lib/mechanics/workspace";
  import FlowRef from "$lib/ui/FlowRef.svelte";
  import DiagnosticView from "$lib/ui/DiagnosticView.svelte";
  import { diagnosticFrom, diagnosticText } from "$lib/diagnostic";
  import { isEnergyFlow, presentFlow } from "$lib/dual-var";

  const mechKinds: { kind: import("$lib/runtime/types").MechanicKind; label: string }[] = [
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
    getVersion().then((v) => (appVersion = v)).catch(() => {});
  });

  // 应用版本号（来自 tauri.conf.json version，无则空串）
  let appVersion = $state("");

  // ── 应用栏状态 ──────────────────────────────────────────────────
  // 添加机制菜单用 fixed 定位（视口坐标），脱离滚动容器，避免开合时
  // 改变机制列表滚动区域的高度。
  let addMechMenuPos = $state<{ top: number; right: number } | null>(null);
  // 二级菜单：项目、工厂、机制工具、游戏数据。
  // 主界面只保留高频操作，低频/批量操作收进这些菜单。
  let projectMenuOpen = $state(false);
  let factoryMenuOpen = $state(false);
  let mechMenuOpen = $state(false);
  let appMenuOpen = $state(false);
  let dataOpen = $state(false);
  /** 字体设置弹窗（纯前端偏好，存 localStorage）。 */
  let fontSettingsOpen = $state(false);
  /** 左栏编辑范围：工厂（目标/输入/环境）或项目（全局设置）。 */
  let leftTab = $state<"factory" | "project">("factory");
  let newProjectOpen = $state(false);
  let newProjectName = $state("新项目");
  let newFactoryOpen = $state(false);
  let newFactoryName = $state("新工厂");
  let newUniverseOpen = $state(false);
  let newUniverseName = $state("新宇宙工厂");
  // 规划偏好弹窗（机器偏好/替代数量/枚举插件与插件塔）
  let prefsOpen = $state(false);
  // 建议系统弹窗
  let suggestions = $state<{
    flow: DualVar;
    location?:import("$lib/runtime/types").Location;
    scope:import("$lib/runtime/mechanic-commands").MechanicScope;
    items: import("$lib/runtime/types").Suggestion[];
    loading: boolean;
  } | null>(null);

  let suggestionGeneration=0;
  async function openSuggestions(flow:DualVar,location?:import('$lib/runtime/types').Location){
    if(!project||!actions)return;
    const scope={...actions.commands.scope},generation=++suggestionGeneration;
    suggestions={flow,location,scope,items:[],loading:true};
    try{const items=await suggest(flow,runtime.effectiveContextId);if(generation===suggestionGeneration&&suggestions)suggestions={flow,location,scope,items,loading:false};}
    catch{if(generation===suggestionGeneration&&suggestions)suggestions={flow,location,scope,items:[],loading:false};}
  }

  function suggestionKindLabel(kind: string): string {
    return (
      {
        recipe: "配方",
        resource: "矿点",
        "item-fuel": "燃料",
        "virtual-science": "虚拟科研",
        generator: "发电机",
      }[kind] ?? kind
    );
  }

  function suggestionIcon(kind: string): { type: string; detailKind: string } {
    if (kind === "recipe") return { type: "recipe", detailKind: "recipe" };
    if (kind === "resource") return { type: "entity", detailKind: "resource" };
    if (kind === "item-fuel" || kind === "virtual-science") return { type: "item", detailKind: "item" };
    return { type: "entity", detailKind: "generator" };
  }

  function suggestionName(kind: string, name: string): string {
    const localizeKind =
      kind === "recipe" ? "recipe" : kind === "resource" ? "resource" : kind === "item-fuel" || kind === "virtual-science" ? "item" : "generator";
    return runtime.localizedName(localizeKind, name);
  }

  // 应用内确认/重命名弹窗（Tauri WebView2 不支持 window.confirm/prompt，会阻塞）
  let confirmState = $state<{
    message: string;
    action: () => void;
    confirmLabel?: string;
    danger?: boolean;
  } | null>(null);
  let renameCtx = $state<{ id: string; name: string } | null>(null);
  let renameName = $state("");
  // 轻量提示（自动消失）
  let notice = $state<string | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function showNotice(message: string) {
    notice = message;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), 4000);
  }

  // 顶端报错条：最多 3 行，过长省略；被截断时给一个复制按钮把完整内容取回来。
  // 求解器的报错会把缺供给的物品全列出来，原文能把顶部撑满、挤掉下面的内容。
  let errLines = $state<HTMLElement | null>(null);
  let errTruncated = $state(false);

  function errorStripText(): string {
    return [
      runtime.contextError ? `数据：${runtime.contextError}` : "",
      runtime.lastError ? `操作：${diagnosticText(runtime.lastDiagnostic?.message === runtime.lastError ? runtime.lastDiagnostic : diagnosticFrom(runtime.lastError), (kind, name) => runtime.localizedName(kind, name))}` : "",
      notice ? `提示：${notice}` : "",
    ]
      .filter(Boolean)
      .join("\n");
  }

  async function copyErrorStrip() {
    try {
      await navigator.clipboard.writeText(errorStripText());
      showNotice("已复制完整报错");
    } catch {
      showNotice("复制失败：剪贴板不可用");
    }
  }

  // 量「有没有被截断」：纵向溢出（超过 3 行）或任一行横向溢出（省略号生效）。
  // 三个数据源都读一遍，任一变化就重新量；resize 时窗口宽度变了也要重量。
  $effect(() => {
    const node = errLines;
    void runtime.contextError;
    void runtime.lastError;
    void notice;
    if (!node) {
      errTruncated = false;
      return;
    }
    const measure = () => {
      const vertical = node.scrollHeight > node.clientHeight + 1;
      const horizontal = Array.from(node.children).some(
        (child) => (child as HTMLElement).scrollWidth > (child as HTMLElement).clientWidth + 1,
      );
      errTruncated = vertical || horizontal;
    };
    const frame = requestAnimationFrame(measure);
    window.addEventListener("resize", measure);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", measure);
    };
  });

  // ── 应用更新 ────────────────────────────────────────────────────
  let updating = $state(false);
  async function checkForUpdate() {
    if (updating) return;
    updating = true;
    try {
      const update = await updaterCheck();
      if (!update) {
        showNotice("当前已是最新版本");
        return;
      }
      confirmState = {
        message: `发现新版本 ${update.version}，是否立即下载并重启更新？`,
        confirmLabel: "立即更新",
        action: () => {
          showNotice("正在下载更新…");
          update
            .downloadAndInstall()
            .then(() => relaunch())
            .catch((error) => showNotice(`更新失败：${String(error)}`));
        },
      };
    } catch (error) {
      showNotice(`检查更新失败：${String(error)}`);
    } finally {
      updating = false;
    }
  }

  /** 遮罩/可点击背景的键盘访达：Enter/Space/Esc 与点击一致（a11y）。 */
  function backdropKeydown(event: KeyboardEvent, close: () => void): void {
    if (event.key === "Enter" || event.key === " " || event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  // ── 选择器状态 ──────────────────────────────────────────────────
  let selector = $state<{
    kind: CatalogKind;
    title: string;
    kinds: { kind: CatalogKind; label: string }[];
    categoryFilter?: string[];
    tilePurpose?: "extract" | "dispose";
    allowedNames?: string[];
    /** 编辑模式：预选当前条目/品质（允许只改品质或只改条目后确认）。 */
    initialName?: string;
    initialQuality?: string;
    onSelect: (name: string, quality: string) => void;
    /** 目录多分类页签选中时返回分类 kind（选择里程碑/标记可达等）。 */
    onPickKind?: (kind: CatalogKind, name: string, quality: string) => void;
    /** 是否按项目可达性过滤；需要列出全部可选对象的调用方置 false（如里程碑）。 */
    respectAccessibility?: boolean;
    /** 仅显示可多次研究的科技（无限科技选择器用）。 */
    multiLevelTechOnly?: boolean;
  } | null>(null);

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
    onSelectFlow: (flow: import("$lib/runtime/types").DualVar) => void;
  } | null>(null);

  // ── 左侧栏面板折叠状态 ───────────────────────────────────────────
  // 工厂 tab 内可独立折叠的面板；默认展开目标与外部输入（生产 → 供给的
  // 主信息流），收起低频的表达式。环境与项目设置由左栏 tab 承载。
  let panels = $state({
    targets: true,
    expressions: false,
    inputs: true,
  });

  function openSelector(
    kind: CatalogKind,
    title: string,
    onSelect: (name: string, quality: string) => void,
    kinds: { kind: CatalogKind; label: string }[] = [],
    categoryFilter?: string[],
    allowedNames?: string[],
    initialName?: string,
    initialQuality?: string,
    onPickKind?: (kind: CatalogKind, name: string, quality: string) => void,
    respectAccessibility = true,
    multiLevelTechOnly = false,
  ) {
    selector = {
      kind,
      title,
      kinds,
      categoryFilter,
      allowedNames,
      initialName,
      initialQuality,
      onSelect,
      onPickKind,
      respectAccessibility,
      multiLevelTechOnly,
    };
  }

  function renameContextPrompt(context: import("$lib/runtime/types").ContextInfo) {
    renameCtx = { id: context.id, name: context.name };
    renameName = context.name;
  }

  // ── 可达性选择分类 ──────────────────────────────────────────────
  // 里程碑 / 显式标记可达 / 不可达统一的分类页签（与可达性对象一一对应；
  // 目录无 space-location 条目，故不含太空地点）。
  const markKinds: { kind: CatalogKind; label: string }[] = [
    { kind: "item", label: "物品" },
    { kind: "recipe", label: "配方" },
    { kind: "entity", label: "实体" },
    { kind: "technology", label: "科技" },
    { kind: "fluid", label: "流体" },
    { kind: "quality", label: "品质" },
    { kind: "planet", label: "星球" },
  ];

  function makeAccessible(kind: CatalogKind, name: string): Accessible {
    switch (kind) {
      case "technology":
        return { Tech: name };
      case "recipe":
        return { Recipe: name };
      case "quality":
        return { Quality: name };
      case "fluid":
        return { Fluid: name };
      case "planet":
        return { Planet: name };
      case "entity":
        return { Entity: name };
      default:
        return { Item: name };
    }
  }

  function confirmRename() {
    const name = renameName.trim();
    if (renameCtx && name) {
      runtime.renameContext(renameCtx.id, name).catch(() => {});
    }
    renameCtx = null;
  }

  // 项目 / 工厂重命名（复用同一弹窗，target 区分作用对象）
  let renameTarget = $state<{ kind: "project" | "factory"; name: string } | null>(null);
  // 关闭项目确认
  let closeProjectState = $state<{ project: number; name: string } | null>(null);
  function promptRenameProject() {
    const item = runtime.selectedProject;
    if (!item) return;
    renameTarget = { kind: "project", name: item.name };
    renameName = item.name;
  }
  function promptRenameFactory(name: string) {
    renameTarget = { kind: "factory", name };
    renameName = name;
  }
  function confirmRenameTarget() {
    const name = renameName.trim();
    if (!renameTarget || !name) {
      renameTarget = null;
      return;
    }
    if (renameTarget.kind === "project") {
      runtime.setProjectName(name).catch(() => {});
    } else {
      runtime.setFactoryName(name).catch(() => {});
    }
    renameTarget = null;
  }

  /** 上下文来源摘要：游戏版本 + 启用的 mod 数（完整清单在 tooltip）。 */
  function contextSummary(context: ContextInfo): string {
    const version = context.game_version ?? "版本未知";
    const mods = context.mods.length ? `${context.mods.length} 个 mod` : "无 mod";
    return `${version} · ${mods}`;
  }

  /** 上下文 tooltip：逐条列出启用的 mod 与版本。 */
  function contextTitle(context: ContextInfo): string {
    const lines = [`游戏版本：${context.game_version ?? "未知"}`];
    if (context.mods.length === 0) {
      lines.push("启用 mod：无（或这份缓存没有记录）");
    } else {
      lines.push(`启用 mod（${context.mods.length}）：`);
      for (const mod of context.mods) {
        lines.push(`  ${mod.name}${mod.version ? ` ${mod.version}` : ""}`);
      }
    }
    return lines.join("\n");
  }

  // ── 派生数据 ────────────────────────────────────────────────────
  let project = $derived(runtime.selectedProject);
  let planning = $derived(project?.planning ?? null);
  const universe = $derived(runtime.selectedUniverse);
  let factory = $derived(universe ? universeView(universe) : runtime.selectedFactory);
  let universeBusy = $state(false);
  async function autoPlanUniverse() {
    if (universeBusy) return;
    universeBusy=true;
    try {await runtime.autoPlanSelectedUniverse();}
    finally {universeBusy=false;}
  }
  /** 当前项目绑定的游戏上下文（未绑定时回退到全局激活上下文）。 */
  let projectContext = $derived(project?.context_id
    ? runtime.contexts.find((context) => context.id === project?.context_id) ?? null
    : runtime.activeContext);
  const missingProjectContext = $derived(!!project?.context_id && !projectContext);
  /** 当前主题的可读标签（应用菜单里显示当前选中项）。 */
  let themeLabel = $derived(
    runtime.theme === "system" ? "跟随系统" : runtime.theme === "light" ? "亮色" : "深色",
  );
  function closeMenus() {
    projectMenuOpen = false;
    factoryMenuOpen = false;
    mechMenuOpen = false;
    appMenuOpen = false;
    addMechMenuPos = null;
  }
  let mechanics = $derived(factory?.mechanics ?? []);
  let targets = $derived(factory?.targets ?? []);
  let targetExpressions = $derived(factory?.target_expressions ?? []);
  let externalInputs = $derived(factory?.external_inputs ?? []);
  let implicitInputs = $derived(runtime.implicitSourcesCache);
  const inputShortcuts = $derived(project && factory ? createExternalInputShortcuts({
    send: message => runtime.send(message),
    getFactory: scope => runtime.document?.projects.find(project => project.id === scope.project)?.factories.find(factory => factory.id === scope.factory) ?? null,
  }, {project: project.id, factory: factory.id}) : null);
  let addingFreeEnergy = $state(false);
  const hasFreeEnergy = $derived(FREE_ENERGY_FLOWS.every(flow => externalInputs.some(input => input.flow === flow && input.penalty === 0)));
  async function addFreeEnergy() {
    if (addingFreeEnergy || !inputShortcuts) return;
    addingFreeEnergy = true;
    try { await inputShortcuts.addFreeEnergy(); }
    finally { addingFreeEnergy = false; }
  }
  let implicitPromotions = $state<Record<string, boolean>>({});
  const implicitPromotionKey = (flow: DualVar) => `${project?.id}:${factory?.id}:${JSON.stringify(flow)}`;
  async function promoteImplicitInput(flow: DualVar, penalty: number) {
    const key = implicitPromotionKey(flow), shortcuts = inputShortcuts;
    if (!shortcuts || implicitPromotions[key]
      || factory?.external_inputs.some((input) => JSON.stringify(input.flow) === JSON.stringify(flow))) return;
    implicitPromotions[key] = true;
    try {
      await shortcuts.setCost(flow, penalty);
    } finally {
      delete implicitPromotions[key];
    }
  }
  let mechanicProjectId = $derived(project?.id ?? null);
  let mechanicFactoryId = $derived(factory?.id ?? null);
  let actions = $derived(mechanicProjectId !== null && mechanicFactoryId !== null ? createMechanicActions(runtime,
    {project: mechanicProjectId, factory: mechanicFactoryId, ...(universe ? {kind: "universe" as const} : {})}, {
      select: request => selector = request,
      selectFlow: request => flowSelector = request,
      notice: showNotice,
    }) : null);
  // 拖拽排序的本地镜像（文档变化时同步；拖拽期间由 dndzone 更新）
  let dragTargets = $state<import("$lib/runtime/types").FlowTarget[]>([]);
  let dragInputs = $state<import("$lib/runtime/types").ExternalInput[]>([]);
  /** 插件塔原型的插件槽数（getDetail 异步；未知时默认 2，与 Factorio 一致）。 */
  async function beaconModuleSlots(beaconId: string | undefined): Promise<number> {
    if (!beaconId) return 2;
    const detail = await runtime.getDetail("beacon", beaconId);
    return detail?.beacon_module_slots ?? detail?.module_slots ?? 2;
  }

  /** 科技等级信息（catalogIndex）：base = 名字后缀最低等级，max = 有效上限
   *  （null = 无限）。未知科技返回 null。 */
  function techInfo(tech: string): { base: number; max: number | null } | null {
    const entry = runtime.catalogIndex?.entries.find(
      (candidate) => candidate.kind === "technology" && candidate.name === tech,
    );
    if (!entry) return null;
    return { base: entry.technology_base_level, max: entry.technology_max_level };
  }

  // 机制列表：配方合并开关（同配方×机器合并成一张卡，品质变体并列）；
  // 其他机制类型始终单条展示，不额外嵌套。
  let mergeRecipes = $state(true);
  let groupLocations = $state(false);
  let mechanicView = $state<MechanicViewMode>(DEFAULT_MECHANIC_VIEW);
  let filterOpen = $state(false);
  let filterActive = $state(false);

  $effect(() => {
    dragTargets = targets;
  });
  $effect(() => {
    dragInputs = externalInputs;
  });
  function handleTargetsConsider(event: CustomEvent<{ items: import("$lib/runtime/types").FlowTarget[] }>) {
    dragTargets = event.detail.items;
  }
  function handleTargetsFinalize(event: CustomEvent<{ items: import("$lib/runtime/types").FlowTarget[] }>) {
    dragTargets = event.detail.items;
    runtime
      .reorderTargets(event.detail.items.map((target) => target.id))
      .catch(() => {});
  }
  function handleInputsConsider(event: CustomEvent<{ items: import("$lib/runtime/types").ExternalInput[] }>) {
    dragInputs = event.detail.items;
  }
  function handleInputsFinalize(event: CustomEvent<{ items: import("$lib/runtime/types").ExternalInput[] }>) {
    dragInputs = event.detail.items;
    runtime
      .reorderExternalInputs(event.detail.items.map((input) => input.id))
      .catch(() => {});
  }
  let solve = $derived(universe ? null : runtime.solve);
  let solveMap = $derived(new Map(
    universe && runtime.universeSolve?.status.status === "solved" ? runtime.universeSolve.status.mechanics.map(m => [m.mechanic,{amount:m.amount,cost:m.cost}] as const)
    : solve && "solved" in solve.status ? solve.status.solved.mechanics.map(m => [m.mechanic,{amount:m.amount,cost:m.cost}] as const) : []
  ));
  let solved = $derived(universe ? runtime.universeSolve?.status.status === "solved" : solve != null && "solved" in solve.status);
  let notSolved = $derived(solve != null && "not-solved" in solve.status);

  /** 瓦特 → 可读功率文本（数值为 J/s，直接换算，不乘 60）。 */
  function formatPowerValue(watts: number): string {
    if (watts >= 1e6) return `${(watts / 1e6).toFixed(2)} MW`;
    if (watts >= 1e3) return `${(watts / 1e3).toFixed(1)} kW`;
    return `${watts.toFixed(0)} W`;
  }

  /** 解析功率输入：支持 "1.5MW" / "90kW" / "500"（纯数字 = 瓦特）；无法解析返回 null。 */
  function parsePowerInput(raw: string): number | null {
    const text = raw.trim();
    const match = /^([+-]?\d*\.?\d+)\s*(MW|kW|W)?$/i.exec(text);
    if (!match) return null;
    const value = Number(match[1]);
    if (!Number.isFinite(value)) return null;
    const unit = (match[2] ?? "W").toUpperCase();
    if (unit === "MW") return value * 1e6;
    if (unit === "KW") return value * 1e3;
    return value;
  }

  /** 能量流数值显示：能量流用功率文本，否则普通数字（复刻 egui compact）。 */
  function formatFlowAmount(flow: DualVar, amount: number): string {
    if (isEnergyFlow(flow)) return formatPowerValue(Math.abs(amount));
    // 非能量流：按项目时间刻度（秒/分/时）缩放，并带单位（/s /min /h）。
    const scale = project?.settings.time_scale ?? "seconds";
    const factor = scale === "minutes" ? 60 : scale === "hours" ? 3600 : 1;
    const unit = scale === "minutes" ? "/min" : scale === "hours" ? "/h" : "/s";
    return `${signedCompactNumber(amount * factor)}${unit}`;
  }

  /** 项目时间刻度的数值因子（秒=1、分=60、时=3600）。内部一律按每秒，显示按当前刻度。 */
  function currentTimeFactor(): number {
    const s = project?.settings.time_scale ?? "seconds";
    return s === "minutes" ? 60 : s === "hours" ? 3600 : 1;
  }

  /** 流的显示名：物品/流体/实体优先本地化名，否则内部 id。 */
  function flowLabel(flow: DualVar): string {
    return presentFlow(flow, (kind, name) => runtime.localizedName(kind, name)).description;
  }


  /** 流 → 流选择器初始页签（编辑目标/外部输入时预选）。 */
  function flowTabOf(flow: DualVar): string {
    if (typeof flow === "object" && "VirtualScience" in flow) return "virtual-science";
    const item = itemOf(flow);
    if (item) return "item";
    if (flow !== null && typeof flow === "object") {
      if ("Fluid" in flow) return "fluid";
      if ("Entity" in flow) return "entity";
      if ("Electricity" in flow) return "electricity";
      if ("Heat" in flow) return "heat";
      if ("RocketSlotCapacity" in flow) return "rocket-slot";
      if ("RocketWeightCapacity" in flow) return "rocket-weight";
      if ("Pollution" in flow || "Custom" in flow) return "custom";
    }
    return "item";
  }

  /** 流 → 初始选中条目/品质（物品/实体/流体类）。 */
  function flowInitialOf(flow: DualVar): { initialName?: string; initialQuality?: string } {
    if (typeof flow === "object" && "VirtualScience" in flow) return { initialName: String(flow.VirtualScience) };
    const item = itemOf(flow);
    if (item) return { initialName: item.id, initialQuality: item.quality };
    if (flow !== null && typeof flow === "object") {
      if ("Entity" in flow) {
        const entity = (flow as { Entity: { id: string; quality?: string } }).Entity;
        return { initialName: entity.id, initialQuality: entity.quality };
      }
      if ("Fluid" in flow) {
        const fluid = (flow as { Fluid: { name: string } }).Fluid;
        return { initialName: fluid.name };
      }
    }
    return {};
  }

  /** 流体流的期望温度区间；非流体返回 null。 */
  function fluidRangeOf(flow: DualVar): [number, number] | null {
    if (flow !== null && typeof flow === "object" && "Fluid" in flow) {
      return (flow as { Fluid: { temperature: [number, number] } }).Fluid.temperature;
    }
    return null;
  }

  /** 解析温度输入；非法返回 null（不把 NaN 写进文档）。 */
  function parseTemperature(raw: string): number | null {
    const value = Number(raw.trim());
    return Number.isFinite(value) ? Math.round(value) : null;
  }

  /**
   * 只改流体流的温度区间，流体名不动。`apply` 由调用方给出（目标 / 外部输入各走
   * 自己的消息），两处行为因此不会漂移。
   *
   * 区间是**期望**而不是既成事实：下游只要产出落在 [lo, hi] 内就能满足它。默认区间
   * 是流体的 [default_temperature, max_temperature]，所以新建流不会钉死在某个点上。
   */
  function setFluidRange(
    flow: DualVar,
    low: number,
    high: number,
    apply: (flow: DualVar) => void,
  ) {
    if (fluidRangeOf(flow) === null) return;
    const name = (flow as { Fluid: { name: string } }).Fluid.name;
    apply({ Fluid: { name, temperature: [Math.min(low, high), Math.max(low, high)] } });
  }

  function setTargetFluidRange(
    target: import("$lib/runtime/types").FlowTarget,
    low: number,
    high: number,
  ) {
    setFluidRange(target.flow, low, high, (flow) =>
      runtime.setTargetFlow(target.id, flow).catch(() => {}),
    );
  }

  function setExternalInputFluidRange(
    input: import("$lib/runtime/types").ExternalInput,
    low: number,
    high: number,
  ) {
    setFluidRange(input.flow, low, high, (flow) =>
      runtime.setExternalInputFlow(input.id, flow).catch(() => {}),
    );
  }

  function editTarget(target: import("$lib/runtime/types").FlowTarget) {
    flowSelector = {
      title: "更改目标流",
      initialTab: flowTabOf(target.flow),
      ...flowInitialOf(target.flow),
      onSelectFlow: (flow) => runtime.setTargetFlow(target.id, flow).catch(() => {}),
    };
  }

  function editExternalInput(input: import("$lib/runtime/types").ExternalInput) {
    flowSelector = {
      title: "更改外部输入流",
      respectAccessibility: false,
      initialTab: flowTabOf(input.flow),
      ...flowInitialOf(input.flow),
      onSelectFlow: (flow) => runtime.setExternalInputFlow(input.id, flow).catch(() => {}),
    };
  }

  // ── 游戏数据加载 ────────────────────────────────────────────────
  let loadGameOpen = $state(false);
  let gameExePath = $state("");
  let gameModDir = $state("");
  let loadGameError = $state<string | null>(null);
  let pickingExe = $state(false);
  let pickingMod = $state(false);

  async function openLoadGame() {
    loadGameError = null;
    gameExePath = "";
    gameModDir = "";
    loadGameOpen = true;
  }

  async function browseGameExe() {
    pickingExe = true;
    try {
      const picked = await pickGameExecutable();
      if (picked) gameExePath = picked;
    } finally {
      pickingExe = false;
    }
  }

  async function browseGameMod() {
    pickingMod = true;
    try {
      const picked = await pickModDir();
      if (picked) gameModDir = picked;
    } finally {
      pickingMod = false;
    }
  }

  async function submitLoadGame() {
    const exe = gameExePath.trim();
    if (!exe) {
      loadGameError = "请选择游戏可执行文件";
      return;
    }
    loadGameError = null;
    try {
      await runtime.loadContextFromExecutable(exe, gameModDir.trim() || null);
      loadGameOpen = false;
    } catch (error) {
      loadGameError = String(error);
    }
  }

  // ── 项目 / 工厂 ─────────────────────────────────────────────────
  async function createProject() {
    const name = newProjectName.trim();
    if (!name) return;
    newProjectOpen = false;
    try {
      await runtime.newProject(name);
    } catch {
      /* lastError 已展示 */
    }
  }

  async function createUniverse() {
    const name = newUniverseName.trim();
    if (!name) return;
    newUniverseOpen = false;
    try {
      await runtime.addUniverse(name);
    } catch {
      // the runtime store already exposes the command error.
    }
  }

  async function createFactory() {
    const name = newFactoryName.trim();
    if (!name) return;
    newFactoryOpen = false;
    try {
      await runtime.addFactory(name);
    } catch {
      /* lastError 已展示 */
    }
  }

  /** 全局快捷键：Ctrl/Cmd+Z 撤销；Ctrl/Cmd+Shift+Z 或 Ctrl+Y 重做。
   *  焦点在输入框/文本域/可编辑元素时交给浏览器处理文本撤销，不拦截。 */
  function handleGlobalKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && (projectMenuOpen || factoryMenuOpen || mechMenuOpen || appMenuOpen)) {
      closeMenus();
      return;
    }
    if (event.key === "Escape" && fontSettingsOpen) {
      fontSettingsOpen = false;
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
  <header class="appbar">
    <div class="brand" title="切向量化 · 异星工厂产线求解器">
      <span class="brand-mark">切</span>
      <span class="brand-name">向量化</span>
    </div>

    <!-- 项目：切换 / 新建 / 导入 / 保存 / 关闭都收进一个菜单 -->
    <div class="menu-wrap">
      <button
        class="project-trigger"
        aria-expanded={projectMenuOpen}
        title={project ? `当前项目：${project.name} · 点击切换或管理项目` : "选择或新建项目"}
        onclick={() => {
          projectMenuOpen = !projectMenuOpen;
          factoryMenuOpen = false;
          appMenuOpen = false;
        }}
      >
        <span class="trigger-label">项目</span>
        <span class="trigger-value">{project?.name ?? "未选择项目"}</span>
        <span class="caret">{projectMenuOpen ? "▴" : "▾"}</span>
      </button>
      {#if projectMenuOpen}
        <div class="menu-catcher" aria-hidden="true" onclick={() => (projectMenuOpen = false)}></div>
        <div class="menu project-menu">
          <div class="menu-label">切换项目</div>
          {#each runtime.document?.projects ?? [] as item (item.id)}
            <button
              class:active={item.id === runtime.selectedProject?.id}
              title={item.id === runtime.selectedProject?.id ? "当前项目" : `切换到项目：${item.name}`}
              onclick={() => {
                projectMenuOpen = false;
                runtime.selectProject(item.id).catch(() => {});
              }}
            >
              <span class="menu-item-main">{item.name}</span>
              <span class="menu-meta">{item.factories.length} 工厂</span>
            </button>
          {:else}
            <div class="menu-empty">还没有项目</div>
          {/each}
          <div class="menu-sep"></div>
          <button onclick={() => { projectMenuOpen = false; newProjectOpen = true; }}>新建项目…</button>
          <button
            disabled={runtime.busy}
            onclick={() => { projectMenuOpen = false; runtime.openProject().catch(() => {}); }}
          >导入项目…</button>
          {#if project}
            <div class="menu-sep"></div>
            <button onclick={() => { projectMenuOpen = false; promptRenameProject(); }}>重命名项目…</button>
            <button
              disabled={runtime.busy}
              title={runtime.projectSavePath(project.id) ? `保存到 ${runtime.projectSavePath(project.id)}` : "保存项目（首次保存会询问位置）"}
              onclick={() => { projectMenuOpen = false; runtime.saveCurrentProject().catch(() => {}); }}
            >保存项目</button>
            <button
              disabled={runtime.busy}
              onclick={() => { projectMenuOpen = false; runtime.saveProjectAs().catch(() => {}); }}
            >另存为…</button>
            <button
              class="danger"
              onclick={() => {
                projectMenuOpen = false;
                closeProjectState = { project: project.id, name: project.name };
              }}
            >关闭项目…</button>
          {/if}
        </div>
      {/if}
    </div>

    <!-- 工厂切换与当前工厂操作 -->
    {#if project}
      <div class="factory-bar">
        <span class="bar-label">工厂</span>
        <div class="factory-tabs">
          {#each project.factories as item (item.id)}
            <button
              class="ftab"
              class:active={item.id === runtime.selectedFactory?.id}
              title={item.id === runtime.selectedFactory?.id ? `当前工厂：${item.name}` : `切换到工厂：${item.name}`}
              onclick={() => runtime.selectFactory(item.id).catch(() => {})}
            >{item.name}</button>
          {/each}
          {#each project.universes ?? [] as item (item.id)}
            <button
              class="ftab universe"
              class:active={item.id === runtime.selectedUniverseId}
              title={item.id === runtime.selectedUniverseId ? `当前宇宙工厂：${item.name}` : `切换到宇宙工厂：${item.name}`}
              onclick={() => runtime.selectUniverse(item.id).catch(() => {})}
            >{item.name}</button>
          {/each}
          <button class="ftab add" title="新建工厂" onclick={() => (newFactoryOpen = true)}>+</button>
          <button class="ftab add universe-add" title="新建宇宙工厂" onclick={() => (newUniverseOpen = true)}>✦</button>
        </div>
        {#if factory}
          <div class="menu-wrap">
            <button
              class="icon-btn subtle"
              title={`当前工厂操作：${factory.name}（重命名 / 克隆 / 删除）`}
              aria-expanded={factoryMenuOpen}
              onclick={() => {
                factoryMenuOpen = !factoryMenuOpen;
                projectMenuOpen = false;
                appMenuOpen = false;
              }}
            >⋯</button>
            {#if factoryMenuOpen}
              <div class="menu-catcher" aria-hidden="true" onclick={() => (factoryMenuOpen = false)}></div>
              <div class="menu factory-menu">
                <div class="menu-label">{factory.name}</div>
                <button onclick={() => { factoryMenuOpen = false; promptRenameFactory(factory.name); }}>重命名工厂…</button>
                <button onclick={() => { factoryMenuOpen = false; runtime.cloneFactory(factory.id).catch(() => {}); }}>克隆工厂</button>
                <button class="danger" onclick={() => { factoryMenuOpen = false; runtime.removeFactory(factory.id).catch(() => {}); }}>删除工厂…</button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    <span class="spacer"></span>

    <!-- 数据状态：点击打开游戏数据/上下文管理 -->
    <button
      class="status-pill"
      class:busy={runtime.contextBusy}
      title={projectContext
        ? `游戏数据：${projectContext.name}（${contextSummary(projectContext)}）\n点击管理数据上下文`
        : "尚未加载游戏数据 · 点击载入或选择上下文"}
      onclick={() => { dataOpen = true; closeMenus(); }}
    >
      <span class="status-dot"></span>
      <span class="status-text">
        {runtime.contextBusy ? "正在加载数据…" : projectContext?.name ?? "未加载游戏数据"}
      </span>
      <span class="status-action">数据</span>
    </button>

    <div class="undo-group">
      <button
        class="icon-btn"
        title={runtime.history.undo_label
          ? `撤销：${runtime.history.undo_label}（Ctrl+Z）`
          : "没有可撤销的操作"}
        aria-label="撤销"
        onclick={() => runtime.undo().catch(() => {})}
        disabled={!runtime.history.can_undo || runtime.busy}
      >↶</button>
      <button
        class="icon-btn"
        title={runtime.history.redo_label
          ? `重做：${runtime.history.redo_label}（Ctrl+Shift+Z / Ctrl+Y）`
          : "没有可重做的操作"}
        aria-label="重做"
        onclick={() => runtime.redo().catch(() => {})}
        disabled={!runtime.history.can_redo || runtime.busy}
      >↷</button>
    </div>

    <!-- 应用菜单：外观 / 加载数据 / 更新 / 版本 -->
    <div class="menu-wrap">
      <button
        class="icon-btn"
        aria-label="应用菜单"
        aria-expanded={appMenuOpen}
        title="应用菜单：外观、字体、加载数据、检查更新、版本"
        onclick={() => {
          appMenuOpen = !appMenuOpen;
          projectMenuOpen = false;
          factoryMenuOpen = false;
        }}
      >⋯</button>
      {#if appMenuOpen}
        <div class="menu-catcher" aria-hidden="true" onclick={() => (appMenuOpen = false)}></div>
        <div class="menu app-menu">
          <div class="menu-label">外观</div>
          <div class="theme-switch" role="group" aria-label="主题">
            <button
              class:active={runtime.theme === "system"}
              title="跟随系统深色 / 亮色外观"
              onclick={() => runtime.setTheme("system")}
            >系统</button>
            <button
              class:active={runtime.theme === "light"}
              title="使用亮色外观"
              onclick={() => runtime.setTheme("light")}
            >亮色</button>
            <button
              class:active={runtime.theme === "dark"}
              title="使用深色外观"
              onclick={() => runtime.setTheme("dark")}
            >深色</button>
          </div>
          <button
            title="分别选择界面字体与代码字体，设置只保存在本机"
            onclick={() => { appMenuOpen = false; fontSettingsOpen = true; }}
          >字体设置…</button>
          <div class="menu-sep"></div>
          <button onclick={() => { appMenuOpen = false; openLoadGame(); }}>加载游戏数据…</button>
          <button disabled={updating} onclick={() => { appMenuOpen = false; checkForUpdate(); }}>
            {updating ? "正在检查更新…" : "检查更新…"}
          </button>
          <div class="menu-note">
            切向量化 v{appVersion || "—"} · 当前 {themeLabel}
          </div>
        </div>
      {/if}
    </div>
  </header>

  {#if runtime.openingProjectPath || runtime.fileOpenNotice || missingProjectContext}
    <div class="file-feedback" role="status" aria-live="polite">
      {#if runtime.openingProjectPath}<span title={runtime.openingProjectPath}>正在打开工程：{runtime.openingProjectPath}</span>
      {:else if runtime.fileOpenNotice}<span>{runtime.fileOpenNotice}</span><button class="icon-btn" aria-label="关闭文件提示" onclick={() => runtime.fileOpenNotice = null}>×</button>{/if}
      {#if missingProjectContext}
        <span>工程已打开，但引用的游戏数据不在本机缓存中。请选择或加载对应的数据。</span>
        <button class="btn" onclick={() => dataOpen = true}>选择游戏数据</button>
      {/if}
    </div>
  {/if}

  {#if runtime.contextError || runtime.lastError || notice}
    <div class="err-strip">
      <div class="err-lines" class:structured={runtime.lastDiagnostic && (runtime.lastDiagnostic.flows.length + runtime.lastDiagnostic.no_provider.length + runtime.lastDiagnostic.no_consumer.length > 0)} bind:this={errLines}>
        {#if runtime.contextError}
          <span class="err-line" title={runtime.contextError}>数据：{runtime.contextError}</span>
        {/if}
        {#if runtime.lastError}
          <div><strong>操作失败</strong><DiagnosticView diagnostic={runtime.lastDiagnostic?.message === runtime.lastError ? runtime.lastDiagnostic : diagnosticFrom(runtime.lastError)} onFlowClick={openSuggestions} /></div>
        {/if}
        {#if notice}
          <span class="err-line notice" title={notice}>提示：{notice}</span>
        {/if}
      </div>
      {#if errTruncated}
        <button class="err-copy" title="复制完整报错" onclick={copyErrorStrip}>复制</button>
      {/if}
    </div>
  {/if}

  <!-- ══ 工作区 ══ -->
  <main class="workspace">
    <!-- 左栏：生产目标 / 外部输入 / 目标表达式（低频设置收进右上角抽屉） -->
    <!-- 左栏：工厂 / 项目 双 tab -->
    <aside class="col rail rail-left">
      <FactorySidebar bind:tab={leftTab} factoryName={factory?.name ?? "未选择工厂"} projectName={project?.name ?? "未选择项目"}>
      {#snippet factoryContent()}

        {#if factory}
          {#if universe}
          <section class="panel universe-settings"><div class="section-head"><span>环境与约束</span><span class="count">跨星球 · 太空</span></div>
            <div class="env-row"><span>新增条目地点</span><LocationPicker value={runtime.universeAddLocation} onChange={location=>runtime.universeAddLocation=location} /></div>
            <label class="check"><input type="checkbox" checked={universe.strict_source} onchange={e=>runtime.dispatchUniverse(project!.id,universe.id,{'set-strict-source':{strict:e.currentTarget.checked}})} />严格供给（只允许环境资源作为外部来源）</label>
            <label class="check"><input type="checkbox" checked={universe.strict_sink} onchange={e=>runtime.dispatchUniverse(project!.id,universe.id,{'set-strict-sink':{strict:e.currentTarget.checked}})} />严格消耗（未出现在目标中的物品必须配平）</label>
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
                openSelector("planet", "选择星球", (name) =>
                  runtime.setFactoryPlanet(name),
                )}
            >
              <HoverIcon
                type="planet"
                name={factory.settings.planet || "planet"}
                size={24}
                detailKind={factory.settings.planet ? "planet" : undefined}
              />
            </button>
            <span class="sub">
              {factory.settings.planet ? `星球：${runtime.localizedName("planet", factory.settings.planet)}` : "星球：未选择"}
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
                openSelector("surface", "选择地表", (name) =>
                  runtime.setFactorySurface(name),
                )}
            >
              <HoverIcon
                type="surface"
                name={factory.settings.surface || "surface"}
                size={24}
                detailKind={factory.settings.surface ? "surface" : undefined}
              />
            </button>
            <span class="sub">
              {factory.settings.surface ? `地表：${runtime.localizedName("surface", factory.settings.surface)}` : "地表：未选择"}
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
                runtime.setStrictSource((event.currentTarget as HTMLInputElement).checked).catch(() => {})}
            />
            严格供给（只允许从外部输入获得未配平物品）
          </label>
          <label class="check">
            <input
              type="checkbox"
              checked={factory.strict_sink}
              onchange={(event) =>
                runtime.setStrictSink((event.currentTarget as HTMLInputElement).checked).catch(() => {})}
            />
            严格消耗（未出现在目标中的物品必须配平）
          </label>
          </section>
          {/if}
        {#if actions}<FactoryTargetsPanel scope={actions.commands.scope} {targets} expressions={targetExpressions} selectFlow={request=>flowSelector=request} onSuggest={openSuggestions} />{/if}


        {#if universe && project}<UniverseInputsPanel project={project.id} {universe} selectFlow={request=>flowSelector={...request,respectAccessibility:false}} />{/if}
        {#if !universe}<section class="panel" aria-label="外部输入">
          <button
            class="title panel-toggle"
            aria-expanded={panels.inputs}
            onclick={() => (panels.inputs = !panels.inputs)}
          >
            <span class="toggle-label">
              <span class="chev">{panels.inputs ? "▾" : "▸"}</span>
              外部输入 <span class="count">{externalInputs.length}</span>
            </span>
          </button>
          {#if panels.inputs}
          <div class="external-shortcuts">
            <button class="btn" onclick={() => addFreeEnergy().catch(() => {})}
              disabled={addingFreeEnergy || hasFreeEnergy || !runtime.activeContext}
              title={hasFreeEnergy ? "电力和热量已配置为零成本外部输入" : "将电力和热量设为零成本外部输入，复用已有条目"}>
              + 免费电力与热量
            </button>
          </div>
          <div
            class="rows"
            use:dndzone={{ items: dragInputs, flipDurationMs: 120 }}
            onconsider={handleInputsConsider}
            onfinalize={handleInputsFinalize}
          >
            {#each dragInputs as input (input.id)}

              {@const fluidRange = fluidRangeOf(input.flow)}
              <div class="row-item">
                <FlowRef flow={input.flow} grow showQualifiers={false} size={26} showBadge={!fluidRange} title="外部输入 · 点击更改" onClick={() => editExternalInput(input)} />
                {#if input.penalty === DISABLED_RESOURCE_PENALTY}<span class="chip" title="使用极高成本抑制该来源">已禁用</span>{/if}
                {#if fluidRange}
                  <!-- 与目标行同一套语义：区间内任何温度的外部输入都算满足。
                       外部输入本来就是"从外面买"，指定可接受温度区间比钉死一个温度更准。 -->
                  <span class="temp-range">
                    <input
                      class="num temp"
                      type="text"
                      inputmode="numeric"
                      value={String(fluidRange[0])}
                      title="期望温度下限（℃）"
                      onchange={(event) => {
                        const next = parseTemperature((event.currentTarget as HTMLInputElement).value);
                        if (next !== null) setExternalInputFluidRange(input, next, fluidRange[1]);
                      }}
                    />
                    <span class="temp-sep">~</span>
                    <input
                      class="num temp"
                      type="text"
                      inputmode="numeric"
                      value={String(fluidRange[1])}
                      title="期望温度上限（℃）"
                      onchange={(event) => {
                        const next = parseTemperature((event.currentTarget as HTMLInputElement).value);
                        if (next !== null) setExternalInputFluidRange(input, fluidRange[0], next);
                      }}
                    />
                  </span>
                {/if}
                <input
                  class="num"
                  type="number"
                  step="0.1"
                  min="0"
                  title={`外部输入成本：获得 1 个「${flowLabel(input.flow)}」的代价（按项目时间刻度）`}
                  aria-label={`外部输入成本：${flowLabel(input.flow)}`}
                  value={isEnergyFlow(input.flow)
                    ? String(Number((input.penalty * 1e6).toFixed(6)))
                    : String(Number((input.penalty / currentTimeFactor()).toFixed(6)))}
                  onchange={(event) => {
                    const raw = (event.currentTarget as HTMLInputElement).value;
                    const value = Number(raw);
                    if (!Number.isFinite(value)) return;
                    // 能量流：输入为"每 MW 成本"，内部存"每 W 成本"（除以 1e6）。
                    // 非能量：输入为"当前单位下产出 1 个的代价"，内部存"每秒产出 1 个的代价"。
                    const internal = isEnergyFlow(input.flow)
                      ? value / 1e6
                      : value * currentTimeFactor();
                    runtime.setExternalInputPenalty(input.id, internal).catch(() => {});
                  }}
                />
                <span class="row-actions">
                  {#if input.penalty === DISABLED_RESOURCE_PENALTY}
                    <button class="icon-btn" aria-label={`撤销禁用：${flowLabel(input.flow)}`} title="移除极高成本覆盖，当前环境若提供该资源则恢复免费输入"
                      onclick={() => runtime.removeExternalInput(input.id).catch(() => {})}>↶</button>
                  {/if}
                  <button
                    class="icon-btn danger"
                    title={`移除外部输入「${flowLabel(input.flow)}」`}
                    aria-label="移除外部输入"
                    onclick={() => runtime.removeExternalInput(input.id).catch(() => {})}
                  >×</button>
                </span>
              </div>
            {:else}
              <div class="empty-hint">还没有外部输入</div>
          {/each}
        </div>
        {#if implicitInputs.length > 0}
          <div class="implicit-hint">环境自带（隐式免费，编辑成本可转为手动输入）</div>
          <div class="rows">
            {#each implicitInputs as flow (JSON.stringify(flow))}

              <div class="row-item one-line implicit" title="当前地表或轨道的可用资源，严格供给下也免费">
                <FlowRef flow={flow} grow showQualifiers={false} size={24} title="环境自带 · 严格供给下也免费" />
                <span class="chip">隐式</span>
                <input
                  class="num"
                  type="number"
                  step="0.1"
                  min="0"
                  value="0"
                  aria-label={`外部成本：${flowLabel(flow)}`}
                  title="编辑成本后保存为手动外部输入"
                  disabled={implicitPromotions[implicitPromotionKey(flow)]}
                  onchange={(event) => {
                    const raw = event.currentTarget.value.trim();
                    const value = Number(raw);
                    if (raw === "" || !Number.isFinite(value) || value < 0) return;
                    const internal = isEnergyFlow(flow) ? value / 1e6 : value * currentTimeFactor();
                    promoteImplicitInput(flow, internal).catch(() => {});
                  }}
                />
                <span class="row-actions">
                  <button
                    class="icon-btn"
                    title={`以零成本把「${flowLabel(flow)}」保存为手动外部输入`}
                    aria-label="设为手动外部输入"
                    disabled={implicitPromotions[implicitPromotionKey(flow)]}
                    onclick={() => promoteImplicitInput(flow, 0).catch(() => {})}
                  >+</button>
                  <button class="icon-btn danger" aria-label={`禁用环境资源：${flowLabel(flow)}`}
                    title="将外部输入成本设为 10¹²，优先使用其他来源"
                    disabled={implicitPromotions[implicitPromotionKey(flow)]}
                    onclick={() => promoteImplicitInput(flow, DISABLED_RESOURCE_PENALTY).catch(() => {})}>⊘</button>
                </span>
              </div>
            {/each}
          </div>
        {/if}
        <button
          class="btn block"
          onclick={() =>
            (flowSelector = {
              title: "添加外部输入流",
              respectAccessibility: false,
              onSelectFlow: (flow) => runtime.addExternalInputFlow(flow, 1).catch(() => {}),
            })}
          disabled={!runtime.activeContext}
        >+ 添加外部输入</button>
          {/if}
        </section>{/if}



        {:else}
          <div class="rail-empty">
            <strong>还没有选中的工厂</strong>
            <span>在顶部选择一个工厂，或点「+」新建工厂开始规划。</span>
          </div>
        {/if}
      {/snippet}
      {#snippet projectContent()}
      {#if project}
        <section class="panel">
          <div class="section-head">
            <span>显示与精度</span>
            <span class="count">对所有工厂生效</span>
          </div>
          <div class="field">
            <label for="time-scale">时间刻度</label>
            <select
              id="time-scale"
              value={project.settings.time_scale}
              onchange={(event) =>
                runtime
                  .setTimeScale((event.currentTarget as HTMLSelectElement).value as import("$lib/runtime/types").TimeScale)
                  .catch(() => {})}
            >
              <option value="seconds">秒</option>
              <option value="minutes">分钟</option>
              <option value="hours">小时</option>
            </select>
          </div>
          <div class="field">
            <label for="major-quality">主品质（项目级）</label>
            <button
              id="major-quality"
              class="btn block"
              onclick={() =>
                openSelector("quality", "选择主品质", (name) =>
                  runtime.setProjectMajorQuality(name),
                )}
            >
              {runtime.localizedName("quality", project?.settings.major_quality || "normal")}
            </button>
            <span class="muted">所有普通工厂与宇宙工厂共享。</span>
          </div>
          <div class="field">
            <label for="lp-solver">线性规划后端</label>
            <select
              id="lp-solver"
              value={project.settings.solver}
              onchange={(event) =>
                runtime
                  .setSolver((event.currentTarget as HTMLSelectElement).value as import("$lib/runtime/types").LpSolver)
                  .catch(() => {})}
            >
              <option value="highs">HiGHS（推荐）</option>
              <option value="soplex">SoPlex</option>
              <option value="clarabel">Clarabel</option>
              <option value="microlp">microlp</option>
              <option value="rust-certified">Rust based</option>
            </select>
            <span class="muted">项目级设置；切换后重新求解全部工厂。</span>
          </div>
          <div class="field">
            <label for="quality-limit">品质上限（超出会自动提升）</label>
            <select
              id="quality-limit"
              value={project.settings.quality_limit ?? ""}
              onchange={(event) => {
                const value = (event.currentTarget as HTMLSelectElement).value;
                runtime.setQualityLimit(value === "" ? null : value).catch(() => {});
              }}
            >
              <option value="">{runtime.localizedName("quality", "normal")}（当前上限）</option>
              {#each runtime.catalogIndex?.qualities ?? [] as q (q)}
                <option value={q}>{runtime.localizedName("quality", q)}</option>
              {/each}
            </select>
          </div>
          <label class="check" title="忽略里程碑与科技依赖，所有原型都可用于计算">
            <input
              type="checkbox"
              checked={project.settings.all_accessible}
              onchange={(event) =>
                runtime.setAllAccessible((event.currentTarget as HTMLInputElement).checked).catch(() => {})}
            />
            无视可达性（全部可用）
          </label>
        </section>

        <section class="panel">
          <div class="section-head">
            <span>里程碑</span>
            <span class="count">{runtime.orderedMilestones?.length ?? project.settings.milestones.length}</span>
          </div>
          <div class="field">
            <span class="field-label">取消勾选 = 禁用，含依赖内容</span>
            <div class="prefs-list">
              {#each runtime.orderedMilestones ?? project.settings.milestones as milestone, i (i)}
                <div class="prefs-item">
                  <HoverIcon
                    type={accessibleKind(milestone.node)}
                    name={accessibleName(milestone.node)}
                    size={22}
                    detailKind={accessibleKind(milestone.node)}
                  />
                  <span class="prefs-name">
                    <RichText text={typeof milestone.node === "string" ? flowName(milestone.node, (kind,name)=>runtime.richName(kind,name)) : runtime.richName(accessibleKind(milestone.node), accessibleName(milestone.node))} />
                  </span>
                  <label class="check" title={milestone.unlocked ? "点击锁定该里程碑及其依赖" : "点击解锁该里程碑"}>
                    <input
                      type="checkbox"
                      checked={milestone.unlocked}
                      onchange={(event) =>
                        runtime
                          .setMilestoneUnlocked(
                            milestone.node,
                            (event.currentTarget as HTMLInputElement).checked,
                          )
                          .catch(() => {})}
                    />
                  </label>
                  <button
                    class="icon-btn danger"
                    title="移除里程碑"
                    aria-label="移除里程碑"
                    onclick={() =>
                      runtime.removeMilestone(milestone.node).catch(() => {})}
                  >×</button>
                </div>
              {:else}
                <span class="muted">还没有里程碑</span>
              {/each}
            </div>
          </div>
          <div class="button-row two">
            <button
              class="btn"
              title="选择一个物品、科技或配方作为里程碑"
              onclick={() =>
                openSelector(
                  "item",
                  "选择里程碑",
                  () => {},
                  markKinds,
                  undefined,
                  undefined,
                  undefined,
                  undefined,
                  (kind, name) => runtime.addMilestone(makeAccessible(kind, name)),
                  false,
                )}
            >+ 添加里程碑</button>
            <button
              class="btn"
              title="用当前上下文中的默认里程碑替换列表"
              onclick={() => runtime.setDefaultMilestones().catch(() => {})}
            >设置默认里程碑</button>
          </div>
        </section>

        <section class="panel">
          <div class="section-head">
            <span>产能</span>
            <span class="count">配方 / 采矿 / 无限科技</span>
          </div>
          <label class="check" title="丢弃自动推算的产能加成，只保留用户手动指定的值">
            <input
              type="checkbox"
              checked={project.settings.ignore_productivity}
              onchange={(event) =>
                runtime.setIgnoreProductivity((event.currentTarget as HTMLInputElement).checked).catch(() => {})}
            />
            忽略产能加成
          </label>

          <div class="field">
            <span class="field-label">采矿产能加成</span>
            <div class="prefs-list">
              <div class="prefs-item {project.settings.mining_productivity !== 0 ? "user-specified" : "inferred"}">
                <span class="prefs-name">采矿</span>
                <input
                  class="prod-input"
                  type="number"
                  step="0.1"
                  min="0"
                  title="采矿产出加成百分比；0 表示使用自动推断值"
                  aria-label="采矿产能加成百分比"
                  value={String(100 * (
                    project.settings.mining_productivity !== 0
                      ? project.settings.mining_productivity
                      : (runtime.productivityInfo?.auto_mining ?? 0)
                  ))}
                  onchange={(event) => {
                    const value = Number((event.currentTarget as HTMLInputElement).value);
                    if (Number.isFinite(value)) runtime.setMiningProductivity(value / 100).catch(() => {});
                  }}
                />
                <span class="muted badge">{project.settings.mining_productivity !== 0 ? "用户" : "推断"}</span>
              </div>
            </div>
          </div>

          <div class="field">
            <span class="field-label">配方产能加成（%）</span>
            <div class="prefs-list">
              {#each runtime.productivityInfo?.recipes ?? [] as entry, i (entry.recipe)}
                <div class="prefs-item {entry.source === "user" ? "user-specified" : "inferred"}">
                  <HoverIcon type="recipe" name={entry.recipe} size={22} detailKind="recipe" />
                  <span class="prefs-name"><RichText text={runtime.richName("recipe", entry.recipe)} /></span>
                  <input
                    class="prod-input"
                    type="number"
                    min="0"
                    title={`配方产能加成：${runtime.localizedName("recipe", entry.recipe)}（%）`}
                    aria-label={`配方产能加成：${runtime.localizedName("recipe", entry.recipe)}`}
                    value={String(entry.value * 100)}
                    onchange={(event) => {
                      const value = Number((event.currentTarget as HTMLInputElement).value);
                      if (Number.isFinite(value)) {
                        runtime.setRecipeProductivity(entry.recipe, value / 100).catch(() => {});
                      }
                    }}
                  />
                  <span class="muted">%</span>
                  {#if entry.source === "user"}
                    <button
                      class="icon-btn danger"
                      title="移除该配方产能加成"
                      aria-label="移除配方产能加成"
                      onclick={() =>
                        runtime.removeRecipeProductivity(entry.recipe).catch(() => {})}
                    >×</button>
                  {:else}
                    <span class="muted badge">推断</span>
                  {/if}
                </div>
              {:else}
                <span class="muted">还没有配方产能加成</span>
              {/each}
            </div>
            <button
              class="btn block"
              title="选择配方并添加产能加成"
              onclick={() =>
                openSelector("recipe", "选择配方", (name) =>
                  runtime.setRecipeProductivity(name, 0.5),
                )}
            >+ 添加配方产能</button>
          </div>

          <div class="field">
            <span class="field-label">无限科技研究次数</span>
            <div class="prefs-list">
              {#each project.settings.infinite_levels as entry (entry.tech)}
                <div class="prefs-item user-specified">
                  <HoverIcon type="technology" name={entry.tech} size={22} detailKind="technology" />
                  <span class="prefs-name"><RichText text={runtime.richName("technology", entry.tech)} /></span>
                  <input
                    class="prod-input"
                    type="number"
                    step="1"
                    min={techInfo(entry.tech)?.base ?? 0}
                    title={`研究次数：${runtime.localizedName("technology", entry.tech)}`}
                    aria-label={`研究次数：${runtime.localizedName("technology", entry.tech)}`}
                    value={String(entry.level)}
                    onchange={(event) => {
                      const raw = (event.currentTarget as HTMLInputElement).value;
                      let value = Number(raw);
                      // 钳制到科技实际范围 [base, max]（无限无上限）：
                      // 低于最低等级或无上限时按 base/自身取值。
                      const info = techInfo(entry.tech);
                      const base = info?.base ?? 0;
                      const max = info?.max ?? null;
                      if (max != null) value = Math.min(value, max);
                      value = Math.max(value, base);
                      if (Number.isInteger(value)) {
                        runtime.setInfiniteTechLevel(entry.tech, value).catch(() => {});
                      }
                    }}
                  />
                  <span class="muted">级</span>
                  <button
                    class="icon-btn danger"
                    title="移除无限科技研究次数"
                    aria-label="移除无限科技研究次数"
                    onclick={() =>
                      runtime.removeInfiniteTechLevel(entry.tech).catch(() => {})}
                  >×</button>
                </div>
              {:else}
                <span class="muted">还没有无限科技等级</span>
              {/each}
            </div>
            <button
              class="btn block"
              title="选择可多次研究的科技并设置研究次数"
              onclick={() =>
                openSelector(
                  "technology",
                  "选择无限科技",
                  (name) => {
                    const info = techInfo(name);
                    runtime.setInfiniteTechLevel(name, (info?.base ?? 0) + 1);
                  },
                  [],
                  [],
                  undefined,
                  undefined,
                  undefined,
                  undefined,
                  true,
                  true,
                )}
            >+ 添加无限科技</button>
          </div>
        </section>

        <section class="panel">
          <div class="section-head">
            <span>规划与插件</span>
            <span class="count">自动规划</span>
          </div>
          <div class="planning-summary">
            <div class="planning-stat" title="自动规划时优先选择的机器">
              <span>机器偏好</span>
              <strong>{planning?.machine_preferences.length ?? 0}</strong>
            </div>
            <div class="planning-stat" title="自动规划参与枚举的插件">
              <span>枚举插件</span>
              <strong>{planning?.enumerate_modules.length ?? 0}</strong>
            </div>
            <div class="planning-stat" title="自动规划叠加的插件塔方案">
              <span>枚举插件塔</span>
              <strong>{planning?.enumerate_beacons.length ?? 0}</strong>
            </div>
          </div>
          <p class="planning-note">
            每配方枚举 {planning?.alternative_count ?? 1} 个备选机器；{planning?.preserve_existing_mechanics ? "保留已有机制" : "不保留已有机制"}。机器偏好、枚举插件与插件塔组合在二级弹窗中编辑。
          </p>
          <button class="btn block" title="编辑机器偏好、替代数量、枚举插件与插件塔" onclick={() => (prefsOpen = true)}>编辑规划偏好…</button>
        </section>
      {:else}
        <div class="rail-empty">
          <strong>还没有选中的项目</strong>
          <span>在左上角「项目」菜单中新建或打开项目。</span>
        </div>
      {/if}
      {/snippet}
      </FactorySidebar>
    </aside>

    <!-- 中栏：机制列表 -->
    <section class="col center">
      <div class="toolbar">
        <button
          class="btn primary"
          title="自动规划：按规划偏好的枚举列表生成候选机制 → 选优 → 回写机制列表。总是按严格供给求解，并在完成后自动开启严格供给"
          onclick={() => universe ? autoPlanUniverse() : runtime.autoPlan().catch(() => {})}
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
        <button class="btn" class:active={filterOpen || filterActive} aria-expanded={filterOpen}
          title="按消耗或产出对象筛选机制；仅影响显示，不改变工厂或求解"
          onclick={() => filterOpen = !filterOpen} disabled={!factory}>筛选{filterActive ? " · 已启用" : ""}</button>
        {#if universe}
          {#each ['rocket-export','orbital-drop'] as direction}
            <button class="btn" disabled={runtime.universeAddLocation.kind !== 'planet'}
              title={direction==='rocket-export'?`从左栏所选星球发往太空；消耗物品与火箭载荷；每件成本：重量（kg）× ${project?.settings.orbital_transport_costs?.rocket_export_per_kg??1}`:`从太空投送到左栏所选星球；每件成本：${project?.settings.orbital_transport_costs?.orbital_drop_per_stack??64} ÷ 堆叠数量；变质物品额外成本：${project?.settings.orbital_transport_costs?.orbital_drop_spoil_penalty??216000} ÷ 有效变质时间（tick）`}
              onclick={()=>{const location=runtime.universeAddLocation;if(location.kind!=='planet')return;selector={kind:"item",kinds:[],title:direction==='rocket-export'?"选择火箭发运物品":"选择轨道投送物品",onSelect:(name,quality)=>direction==='rocket-export'?runtime.addUniverseRocketExport(location,{id:name,quality}):runtime.addUniverseOrbitalDrop(location,{id:name,quality})}}}>
              + {direction==='rocket-export'?'火箭发运':'轨道投送'}
            </button>
          {/each}
        {/if}
        <div class="view-switch" role="group" aria-label="机制展示方式">
          <button class="btn" class:active={mechanicView === 'list'} aria-pressed={mechanicView === 'list'} onclick={() => mechanicView = 'list'}>列表</button>
          <button class="btn" class:active={mechanicView === 'network'} aria-pressed={mechanicView === 'network'} onclick={() => mechanicView = 'network'}>网络</button>
        </div>
        {#if universe}<label class="check"><input type="checkbox" bind:checked={groupLocations}/>按地点聚类</label>{/if}
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
              projectMenuOpen = false;
              factoryMenuOpen = false;
              appMenuOpen = false;
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
                    onchange={(event) => runtime.setPreserveExistingMechanics(event.currentTarget.checked).catch(() => {})}
                  />
                  保留已有机制
                </label>
              {/if}
              <button
                disabled={!solved}
                title="移除整个工厂中本次求解用量为 0 的机制，不受显示筛选影响"
                onclick={() => { mechMenuOpen = false; actions?.commands.cleanup("remove-unused").catch(() => {}); }}
              >移除未用</button>
              <button
                disabled={!solved}
                title="按求解流量从大到小重排整个工厂的机制，不受显示筛选影响"
                onclick={() => { mechMenuOpen = false; actions?.commands.cleanup("sort-by-solution-rate").catch(() => {}); }}
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

      <MechanicWorkspace project={project?.id ?? null} factory={factory?.id ?? null}
        entries={mechanics} solutions={solveMap} {solved} {actions} {filterOpen} {mergeRecipes} groupLocations={!!universe && groupLocations} view={mechanicView}
        onFilterActiveChange={active => filterActive = active} onFilterClose={() => filterOpen = false} />

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
      <SolvePanel solve={universe ? null : solve} universeSolve={universe ? runtime.universeSolve : null} error={universe ? runtime.universeSolveError : runtime.solveError} diagnostic={universe?runtime.universeSolveDiagnostic:null} busy={universe ? universeBusy || runtime.universeSolving : !!runtime.solving} onSuggest={openSuggestions} />
    </aside>

  </main>
</div>

  <!-- ══ 游戏数据（上下文与缓存管理） ══ -->
  {#if dataOpen}
    <div
      class="backdrop"
      role="button"
      tabindex="0"
      onclick={(event) => { if (event.target === event.currentTarget) dataOpen = false; }}
      onkeydown={(event) => backdropKeydown(event, () => (dataOpen = false))}
    >
      <div class="modal data-modal" role="dialog" aria-modal="true" aria-label="游戏数据与上下文">
        <header class="modal-head">
          <div class="modal-heading">
            <span class="eyebrow">游戏数据</span>
            <strong>上下文与缓存</strong>
          </div>
          <button class="icon-btn" title="关闭游戏数据" aria-label="关闭游戏数据" onclick={() => (dataOpen = false)}>×</button>
        </header>

        <div class="data-actions">
          <button class="btn primary" onclick={() => { dataOpen = false; openLoadGame(); }}>加载游戏数据…</button>
          <!-- <button
            class="btn"
            title="载入应用内置的示例数据（若当前构建包含）"
            onclick={() => runtime.loadDemoData().catch(() => {})}
          >载入内置示例</button> -->
        </div>

        {#if project && project.context_id && !runtime.contexts.some((c) => c.id === project.context_id)}
          <div class="warn-strip">当前项目引用的游戏上下文已删除或不存在，请从下方列表选一个并点「应用」重新绑定。</div>
        {/if}

        {#if runtime.contexts.length === 0}
          <div class="empty-state">
            <strong>还没有游戏上下文</strong>
            <span class="muted">加载 Factorio 可执行文件，或导入一份数据缓存后即可选择。</span>
            <button class="btn primary" onclick={() => { dataOpen = false; openLoadGame(); }}>加载游戏数据</button>
          </div>
        {:else}
          <div class="context-list">
            {#each runtime.contexts as context (context.id)}
              <div
                class="ctx-row data-row"
                class:active={project
                  ? project?.context_id === context.id
                  : runtime.activeContext?.id === context.id}
              >
                <div class="ctx-main">
                  <div class="ctx-name">
                    <span class="ctx-name-text">{context.name}</span>
                    {#if project?.context_id === context.id}<span class="chip ok">项目使用中</span>{/if}
                    {#if runtime.activeContext?.id === context.id}<span class="chip">全局激活</span>{/if}
                    {#if !context.loaded}<span class="chip">未载入</span>{/if}
                  </div>
                  <div class="ctx-meta" title={contextTitle(context)}>{contextSummary(context)}</div>
                  {#if context.mods.length > 0}
                    <div class="ctx-mods" title={contextTitle(context)}>
                      {context.mods.slice(0, 5).map((mod) => mod.name).join(" · ")}{context.mods.length > 5 ? ` 等 ${context.mods.length} 个` : ""}
                    </div>
                  {/if}
                </div>
                <div class="ctx-actions">
                  <button
                    class="btn"
                    title={project
                      ? (project?.context_id === context.id
                          ? "当前项目已使用此上下文"
                          : "把当前项目改用此上下文")
                      : (runtime.activeContext?.id === context.id
                          ? "当前为新项目默认上下文"
                          : "设为新项目默认上下文")}
                    disabled={project
                      ? project?.context_id === context.id || runtime.contextBusy
                      : runtime.activeContext?.id === context.id || runtime.contextBusy}
                    onclick={() => {
                      if (project) {
                        runtime.setProjectContext(project.id, context.id).catch(() => {});
                      } else {
                        runtime.setActiveContext(context.id).catch(() => {});
                      }
                    }}
                  >应用</button>
                  <button class="icon-btn" title="重命名上下文" aria-label="重命名上下文" onclick={() => renameContextPrompt(context)}>✎</button>
                  <button
                    class="icon-btn danger"
                    title="删除该上下文的缓存"
                    aria-label="删除上下文缓存"
                    onclick={() =>
                      (confirmState = {
                        message: `删除上下文「${context.name}」的缓存？`,
                        confirmLabel: "删除",
                        danger: true,
                        action: () => runtime.deleteContext(context.id).catch(() => {}),
                      })}
                  >×</button>
                </div>
              </div>
            {/each}
          </div>
          {#if project}
            {@const projCtx = runtime.contexts.find((c) => c.id === project.context_id)}
            {#if projCtx && projCtx.icon_root}
              <div class="kv"><span>图标目录</span><span class="mono small" title={projCtx.icon_root}>{projCtx.icon_root}</span></div>
            {/if}
          {/if}
        {/if}

        <footer class="modal-foot">
          <span class="muted">上下文只决定可用原型与图标；项目文档独立保存。</span>
          <button class="btn ghost" onclick={() => (dataOpen = false)}>关闭</button>
        </footer>
      </div>
    </div>
  {/if}

  <!-- ══ 字体设置（纯前端，localStorage） ══ -->
  {#if fontSettingsOpen}
    <div
      class="backdrop"
      role="button"
      tabindex="0"
      onclick={(event) => { if (event.target === event.currentTarget) fontSettingsOpen = false; }}
      onkeydown={(event) => backdropKeydown(event, () => (fontSettingsOpen = false))}
    >
      <div class="mini-modal font-modal" role="dialog" aria-modal="true" aria-label="字体设置">
        <div class="mini-title">字体设置</div>
        <p class="font-hint">分别指定界面文字与数值/代码使用的字体。设置只保存在本机浏览器中，不写入项目文件。</p>

        <div class="font-section">
          <label for="ui-font">界面字体</label>
          <input
            id="ui-font"
            type="text"
            list="ui-font-presets"
            value={runtime.uiFont}
            placeholder="默认：Inter / 系统 UI"
            spellcheck="false"
            oninput={(event) => runtime.setUiFont(event.currentTarget.value)}
          />
          <datalist id="ui-font-presets">
            <option value="system-ui"></option>
            <option value="Microsoft YaHei"></option>
            <option value="Noto Sans SC"></option>
            <option value="PingFang SC"></option>
            <option value="LXGW WenKai"></option>
            <option value="Sarasa Gothic SC"></option>
            <option value="Inter"></option>
            <option value="IBM Plex Sans"></option>
            <option value="Segoe UI"></option>
          </datalist>
          <div class="font-preview ui">
            <strong>切向量化</strong>
            <span>生产目标 5.0/s · 电磁科技包（粉瓶）· 已启用 · 114.514</span>
          </div>
        </div>

        <div class="font-section">
          <label for="mono-font">代码字体</label>
          <input
            id="mono-font"
            type="text"
            list="mono-font-presets"
            value={runtime.monoFont}
            placeholder="默认：JetBrains Mono / 等宽"
            spellcheck="false"
            oninput={(event) => runtime.setMonoFont(event.currentTarget.value)}
          />
          <datalist id="mono-font-presets">
            <option value="JetBrains Mono"></option>
            <option value="Cascadia Code"></option>
            <option value="Fira Code"></option>
            <option value="Sarasa Term SC Nerd"></option>
            <option value="Consolas"></option>
            <option value="SF Mono"></option>
            <option value="Sarasa Mono SC"></option>
            <option value="ui-monospace"></option>
          </datalist>
          <div class="font-preview mono">
            <span>SC 128.426 · iron-plate 43.8/s · 1,234.56 · 0x1F600</span>
          </div>
        </div>

        <p class="font-hint">输入系统已安装的字体名；留空恢复默认。也可以直接粘贴完整的 font-family 列表（逗号分隔）。</p>
        <div class="mini-actions">
          <button class="btn ghost" onclick={() => runtime.resetFonts()}>恢复默认</button>
          <button class="btn primary" onclick={() => (fontSettingsOpen = false)}>完成</button>
        </div>
      </div>
    </div>
  {/if}

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

<!-- ══ 新建项目 / 新建工厂 ══ -->
{#if newProjectOpen}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) newProjectOpen = false; }} onkeydown={(e) => backdropKeydown(e, () => (newProjectOpen = false))}>
    <div class="mini-modal">
      <div class="mini-title">新建项目</div>
      <input
        bind:value={newProjectName}
        onkeydown={(event) => {
          if (event.key === "Enter") createProject();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (newProjectOpen = false)}>取消</button>
        <button class="btn primary" onclick={createProject}>创建</button>
      </div>
    </div>
  </div>
{/if}

{#if loadGameOpen}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) loadGameOpen = false; }} onkeydown={(e) => backdropKeydown(e, () => (loadGameOpen = false))}>
    <div class="mini-modal load-game">
      <div class="mini-title">加载游戏数据</div>
      <div class="path-field">
        <div class="path-label">游戏可执行文件（必选）</div>
        <div class="path-row">
          <input
            bind:value={gameExePath}
            placeholder="游戏可执行文件路径"
            onkeydown={(event) => {
              if (event.key === "Enter") submitLoadGame();
            }}
          />
          <button class="btn" onclick={browseGameExe} disabled={pickingExe}>
            {pickingExe ? "…" : "浏览"}
          </button>
        </div>
      </div>
      <div class="path-field">
        <div class="path-label">Mod 目录（可选，留空 = 原版）</div>
        <div class="path-row">
          <input
            bind:value={gameModDir}
            placeholder="留空使用原版"
            onkeydown={(event) => {
              if (event.key === "Enter") submitLoadGame();
            }}
          />
          <button class="btn" onclick={browseGameMod} disabled={pickingMod}>
            {pickingMod ? "…" : "浏览"}
          </button>
        </div>
      </div>
      {#if loadGameError}
        <div class="err-text">{loadGameError}</div>
      {/if}
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (loadGameOpen = false)}>取消</button>
        <button class="btn primary" onclick={submitLoadGame} disabled={runtime.contextBusy}>
          {runtime.contextBusy ? "加载中…" : "加载"}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if newUniverseOpen}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) newUniverseOpen = false; }} onkeydown={(e) => backdropKeydown(e, () => (newUniverseOpen = false))}>
    <div class="mini-modal">
      <div class="mini-title">新建宇宙工厂</div>
      <input
        bind:value={newUniverseName}
        onkeydown={(event) => {
          if (event.key === "Enter") createUniverse();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (newUniverseOpen = false)}>取消</button>
        <button class="btn primary" onclick={createUniverse}>创建</button>
      </div>
    </div>
  </div>
{/if}

{#if newFactoryOpen}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) newFactoryOpen = false; }} onkeydown={(e) => backdropKeydown(e, () => (newFactoryOpen = false))}>
    <div class="mini-modal">
      <div class="mini-title">新建工厂</div>
      <input
        bind:value={newFactoryName}
        onkeydown={(event) => {
          if (event.key === "Enter") createFactory();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (newFactoryOpen = false)}>取消</button>
        <button class="btn primary" onclick={createFactory}>创建</button>
      </div>
    </div>
  </div>
{/if}

{#if confirmState}
  {@const confirm = confirmState}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) confirmState = null; }} onkeydown={(e) => backdropKeydown(e, () => (confirmState = null))}>
    <div class="mini-modal">
      <div class="mini-title">确认</div>
      <div class="confirm-text">{confirm.message}</div>
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (confirmState = null)}>取消</button>
        <button
          class="btn {confirm.danger ? 'danger' : 'primary'}"
          onclick={() => {
            confirm.action();
            confirmState = null;
          }}
        >{confirm.confirmLabel ?? "确定"}</button>
      </div>
    </div>
  </div>
{/if}

{#if renameCtx}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) renameCtx = null; }} onkeydown={(e) => backdropKeydown(e, () => (renameCtx = null))}>
    <div class="mini-modal">
      <div class="mini-title">重命名上下文</div>
      <input
        bind:value={renameName}
        onkeydown={(event) => {
          if (event.key === "Enter") confirmRename();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (renameCtx = null)}>取消</button>
        <button class="btn primary" onclick={confirmRename}>确定</button>
      </div>
    </div>
  </div>
{/if}

{#if renameTarget}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) renameTarget = null; }} onkeydown={(e) => backdropKeydown(e, () => (renameTarget = null))}>
    <div class="mini-modal">
      <div class="mini-title">{renameTarget.kind === "project" ? "重命名项目" : "重命名工厂"}</div>
      <input
        bind:value={renameName}
        onkeydown={(event) => {
          if (event.key === "Enter") confirmRenameTarget();
        }}
      />
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (renameTarget = null)}>取消</button>
        <button class="btn primary" onclick={confirmRenameTarget}>确定</button>
      </div>
    </div>
  </div>
{/if}

{#if closeProjectState}
  {@const closing = closeProjectState}
  <div class="backdrop" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) closeProjectState = null; }} onkeydown={(e) => backdropKeydown(e, () => (closeProjectState = null))}>
    <div class="mini-modal">
      <div class="mini-title">关闭项目「{closing.name}」</div>
      <div class="confirm-text">
        关闭后项目从当前工作区移除（文件不会删除）。未保存的改动将丢失。
      </div>
      <div class="mini-actions">
        <button class="btn ghost" onclick={() => (closeProjectState = null)}>取消</button>
        <button
          class="btn"
          onclick={() => {
            runtime.closeProject("save").catch(() => {});
            closeProjectState = null;
          }}
        >保存并关闭</button>
        <button
          class="btn danger"
          onclick={() => {
            runtime.closeProject("discard").catch(() => {});
            closeProjectState = null;
          }}
        >不保存关闭</button>
      </div>
    </div>
  </div>
{/if}

{#if prefsOpen && planning}
  <!-- z-index 低于选择器（40），从弹窗内打开选择器时选择器叠在上层 -->
  <div class="backdrop low" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) prefsOpen = false; }} onkeydown={(e) => backdropKeydown(e, () => (prefsOpen = false))}>
    <div class="prefs-modal">
      <div class="mini-title">规划偏好</div>

      <div class="field">
        <label for="alt-count">替代数量（自动规划时每个配方枚举几个备选机器）</label>
        <input
          id="alt-count"
          type="number"
          min="1"
          max="3"
          value={String(planning.alternative_count)}
          onchange={(event) => {
            const value = Number((event.currentTarget as HTMLInputElement).value);
            if (Number.isFinite(value) && value >= 1 && value <= 3) {
              runtime.setAlternativeCount(value).catch(() => {});
            }
          }}
        />
      </div>

      <div class="prefs-section">
        <div class="prefs-title">机器偏好（自动选机时优先）</div>
        <div class="prefs-list">
          {#each planning.machine_preferences as pref, i (i)}
            <div class="prefs-item">
              <HoverIcon type="entity" name={pref.id} size={22} detailKind="machine" quality={pref.quality} />
              <span class="prefs-name"><RichText text={runtime.richName("machine", pref.id)} /></span>
              <button
                class="btn ghost"
                title="移除"
                onclick={() => runtime.removeMachinePreference(pref).catch(() => {})}
              >×</button>
            </div>
          {:else}
            <span class="muted">还没有机器偏好</span>
          {/each}
        </div>
        <button
          class="btn block"
          onclick={() =>
            openSelector("machine", "添加机器偏好", (name, quality) =>
              runtime.addMachinePreference({ id: name, quality }),
            )}
        >+ 添加机器</button>
      </div>

      <div class="prefs-section">
        <div class="prefs-title">枚举插件（自动规划参与组合的插件）</div>
        <div class="prefs-list">
          {#each planning.enumerate_modules as module, i (i)}
            <div class="prefs-item">
              <HoverIcon type="item" name={module.id} size={22} detailKind="module" quality={module.quality} />
              <span class="prefs-name"><RichText text={runtime.richName("module", module.id)} /></span>
              <button
                class="btn ghost"
                title="移除"
                onclick={() => runtime.removeEnumeratedModule(module).catch(() => {})}
              >×</button>
            </div>
          {:else}
            <span class="muted">还没有枚举插件</span>
          {/each}
        </div>
        <button
          class="btn block"
          onclick={() =>
            openSelector("module", "添加枚举插件", (name, quality) =>
              runtime.addEnumeratedModule(name, quality),
            )}
        >+ 添加插件</button>
        <button
          class="btn block"
          title="用每插件类别中最高 tier 的插件（工厂主品质）替换枚举列表"
          onclick={() =>
            runtime
              .applyBestModules(project?.settings.major_quality || "normal")
              .catch(() => {})}
        >使用最佳插件</button>
      </div>

      <div class="prefs-section">
        <div class="prefs-title">枚举插件塔（自动规划叠加的插件塔方案）</div>
        <div class="prefs-list">
          {#each planning.enumerate_beacons as plan, i (i)}
            {@const beacon = plan.module_config?.beacons?.[0]?.beacon}
            {@const beaconConfig = plan.module_config?.beacons?.[0]}
            <div class="prefs-item column">
              <div class="prefs-row">
                <button
                  class="icon-btn"
                  title="选择插件塔"
                  onclick={() =>
                    openSelector("beacon", "选择枚举插件塔", (name, quality) =>
                      runtime
                        .enumeratedBeaconModule(i, {
                          "add-beacon": { beacon: { id: name, quality } },
                        })
                        .catch(() => {}),
                    )}
                >
                  {#if beacon}
                    <HoverIcon type="entity" name={beacon.id} size={24} detailKind="beacon" quality={beacon.quality} />
                  {:else}
                    <Icon type="entity" name="beacon" size={24} title="选择插件塔" />
                  {/if}
                </button>
                <span class="prefs-name">{beacon ? runtime.localizedName("beacon", beacon.id) : "未选插件塔"}</span>
                <button
                  class="btn ghost danger"
                  title="移除"
                  onclick={() => runtime.removeEnumeratedBeacon(i).catch(() => {})}
                >×</button>
              </div>
              {#if beaconConfig}
                <div class="prefs-row sub">
                  <label class="me-num" title="插件塔的座数（不是塔内插件数）：塔内插件上限 = 塔数 × 每塔插件槽数">
                    塔数
                    <input
                      type="number"
                      min="1"
                      value={String(beaconConfig.count)}
                      title="插件塔的座数（不是塔内插件数）"
                      onchange={(event) => {
                        const input = event.currentTarget as HTMLInputElement;
                        const value = Number(input.value);
                        if (Number.isFinite(value) && value > 0) {
                          runtime
                            .enumeratedBeaconModule(i, { "set-beacon-count": { beacon: 0, count: value } })
                            .catch(() => {});
                        } else {
                          input.value = String(beaconConfig.count);
                          showNotice("插件塔座数必须是大于 0 的整数");
                        }
                      }}
                    />
                  </label>
                  <label class="me-num">
                    共享
                    <input
                      type="number"
                      min="0.1"
                      step="0.1"
                      value={String(beaconConfig.share)}
                      onchange={(event) => {
                        const value = Number((event.currentTarget as HTMLInputElement).value);
                        if (Number.isFinite(value) && value > 0) {
                          runtime
                            .enumeratedBeaconModule(i, { "set-beacon-share": { beacon: 0, share: value } })
                            .catch(() => {});
                        }
                      }}
                    />
                  </label>
                  <button
                    class="btn"
                    title="添加塔内插件"
                    onclick={() =>
                      openSelector("module", "选择塔内插件", (name, quality) =>
                        runtime
                          .enumeratedBeaconModule(i, {
                            "add-beacon-module": {
                              beacon: 0,
                              module: { id: name, quality },
                            },
                          })
                          .catch(() => {}),
                      )}
                  >+ 插件</button>
                </div>
                {#if beaconConfig.modules.length > 0}
                  <div class="prefs-hint">
                    插件数量 = 全部插件塔加起来的总数（例：10 座塔 × 每塔 2 槽 → 填 20），不是每座塔的插件数
                  </div>
                  <div class="prefs-row wrap">
                    {#each beaconConfig.modules as [module, count], mi (mi)}
                      <span class="prefs-chip">
                        <HoverIcon type="item" name={module.id} size={16} detailKind="module" quality={module.quality} />
                        <input
                          class="chip-num"
                          type="number"
                          min="1"
                          value={String(count)}
                          title="该插件在全部插件塔中的总数；上限 = 塔数 × 每塔插件槽数"
                          onchange={(event) => {
                            const input = event.currentTarget as HTMLInputElement;
                            const value = Number(input.value);
                            if (!Number.isFinite(value) || value < 1) {
                              input.value = String(count);
                              showNotice("插件数量必须是大于 0 的整数");
                              return;
                            }
                            void (async () => {
                              const perBeacon = await beaconModuleSlots(beacon?.id);
                              const slots = perBeacon * beaconConfig.count;
                              const total =
                                beaconConfig.modules.reduce(
                                  (sum, [, c], index) => sum + (index === mi ? 0 : c),
                                  0,
                                ) + value;
                              if (slots > 0 && total > slots) {
                                input.value = String(count);
                                showNotice(
                                  `插件数量超出槽位上限：${beaconConfig.count} 座塔 × 每塔 ${perBeacon} 槽 = ${slots}`,
                                );
                                return;
                              }
                              runtime
                                .enumeratedBeaconModule(i, {
                                  "set-beacon-module-count": { beacon: 0, module: mi, count: value },
                                })
                                .catch(() => {});
                            })();
                          }}
                        />
                        <button
                          class="prefs-chip-x"
                          title="移除塔内插件"
                          onclick={() =>
                            runtime
                              .enumeratedBeaconModule(i, { "remove-beacon-module": { beacon: 0, module: mi } })
                              .catch(() => {})}
                        >×</button>
                      </span>
                    {/each}
                  </div>
                {/if}
              {/if}
            </div>
          {:else}
            <span class="muted">还没有枚举插件塔</span>
          {/each}
        </div>
        <button
          class="btn block"
          onclick={() =>
            openSelector("beacon", "添加枚举插件塔", (name, quality) =>
              runtime.addEnumeratedBeacon({ id: name, quality }),
            )}
        >+ 添加插件塔</button>
      </div>

      <div class="mini-actions">
        <button class="btn primary" onclick={() => (prefsOpen = false)}>完成</button>
      </div>
    </div>
  </div>
{/if}

{#if suggestions}
  <div class="backdrop low" role="button" tabindex="0" onclick={(e) => { if (e.target === e.currentTarget) suggestions = null; }} onkeydown={(e) => backdropKeydown(e, () => (suggestions = null))}>
    <div class="prefs-modal">
      <div class="mini-title">建议：<FlowRef flow={suggestions.flow} /></div>
      {#if suggestions.loading}
        <span class="muted">生成中…</span>
      {:else if suggestions.items.length === 0}
        <span class="muted">没有找到与该流相关的候选机制</span>
      {:else}
        {@const producers = suggestions.items.filter((s) => s.role !== "consumer")}
        {@const consumers = suggestions.items.filter((s) => s.role === "consumer")}
        {#if producers.length > 0}
          <div class="prefs-title">产出该流</div>
          <div class="prefs-list">
            {#each producers as candidate, i (candidate.kind + candidate.name)}
              {@const icon = suggestionIcon(candidate.kind)}
              <div class="prefs-item">
                <HoverIcon
                  type={icon.type}
                  name={candidate.name}
                  size={22}
                  detailKind={icon.detailKind}
                />
                <span class="prefs-name" title={candidate.name}>
                  {suggestionName(candidate.kind, candidate.name)}
                  <span class="muted">（{suggestionKindLabel(candidate.kind)}）</span>
                </span>
                <button
                  class="btn"
                  title="添加该机制"
                  onclick={() =>
                    runtime
                      .addSuggestion(candidate,suggestions?.scope,suggestions?.location)
                      .catch(() => {})
                      .then(() => (suggestions = null))}
                >添加</button>
              </div>
            {/each}
          </div>
        {/if}
        {#if consumers.length > 0}
          <div class="prefs-title">消耗该流</div>
          <div class="prefs-list">
            {#each consumers as candidate, i (candidate.kind + candidate.name)}
              {@const icon = suggestionIcon(candidate.kind)}
              <div class="prefs-item">
                <HoverIcon
                  type={icon.type}
                  name={candidate.name}
                  size={22}
                  detailKind={icon.detailKind}
                />
                <span class="prefs-name" title={candidate.name}>
                  {suggestionName(candidate.kind, candidate.name)}
                  <span class="muted">（{suggestionKindLabel(candidate.kind)}）</span>
                </span>
                <button
                  class="btn"
                  title="添加该机制"
                  onclick={() =>
                    runtime
                      .addSuggestion(candidate,suggestions?.scope,suggestions?.location)
                      .catch(() => {})
                      .then(() => (suggestions = null))}
                >添加</button>
              </div>
            {/each}
          </div>
        {/if}
      {/if}
      <div class="mini-actions">
        <button class="btn primary" onclick={() => (suggestions = null)}>关闭</button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── 应用骨架 ───────────────────────────────────────────────── */

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
    background: var(--bg);
  }

  /* ── 顶栏：项目 / 工厂 / 数据 / 全局动作 ─────────────────────── */

  .appbar {
    position: relative;
    z-index: 20;
    flex: 0 0 auto;
    height: 52px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 0 auto;
    user-select: none;
  }

  .brand-mark {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    color: var(--accent-ink);
    background: var(--accent);
    border-radius: 4px;
    font-size: 12px;
    font-weight: 800;
    line-height: 1;
  }

  .brand-name {
    color: var(--text);
    font-size: 11px;
    font-weight: 750;
    letter-spacing: 0.2em;
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

  /* 项目切换按钮：把项目名做成可点的主入口。 */
  .project-trigger {
    height: 34px;
    max-width: 290px;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 0 9px 0 10px;
    color: var(--text);
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: border-color 90ms ease, background 90ms ease;
  }

  .project-trigger:hover {
    background: var(--card-hover);
    border-color: var(--line-strong);
  }

  .trigger-label {
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }

  .trigger-value {
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .caret {
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 9px;
  }

  /* 工厂段：与左侧项目区分，用竖线隔开。 */
  .factory-bar {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    padding-left: 10px;
    border-left: 1px solid var(--line);
  }

  .bar-label {
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
  }

  .factory-tabs {
    min-width: 0;
    max-width: min(42vw, 620px);
    display: flex;
    align-items: center;
    gap: 2px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .factory-tabs::-webkit-scrollbar {
    display: none;
  }

  .ftab {
    height: 32px;
    max-width: 190px;
    display: inline-flex;
    align-items: center;
    padding: 0 11px;
    overflow: hidden;
    color: var(--muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
    transition: color 90ms ease, background 90ms ease, border-color 90ms ease;
  }

  .ftab:hover {
    color: var(--text);
    background: var(--card-hover);
  }

  .ftab.active {
    color: var(--text);
    background: var(--accent-dim);
    border-color: var(--line-strong);
    font-weight: 700;
  }

  .ftab.add {
    width: 28px;
    padding: 0;
    justify-content: center;
    color: var(--faint);
    font-size: 15px;
  }

  .ftab.add:hover {
    color: var(--text);
  }

  .icon-btn.subtle {
    color: var(--faint);
  }

  /* 数据状态 pill */
  .status-pill {
    height: 32px;
    max-width: 260px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 0 10px;
    color: var(--text);
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: 999px;
    cursor: pointer;
    transition: border-color 90ms ease, background 90ms ease;
  }

  .status-pill:hover {
    background: var(--card-hover);
    border-color: var(--line-strong);
  }

  .status-pill .status-dot {
    background: var(--ok);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 16%, transparent);
  }

  .status-pill.busy .status-dot {
    background: var(--warn);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--warn) 16%, transparent);
  }

  .status-text {
    min-width: 0;
    overflow: hidden;
    font-size: 11px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-action {
    flex: 0 0 auto;
    padding-left: 7px;
    color: var(--faint);
    border-left: 1px solid var(--line);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
  }

  .undo-group {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .undo-group .icon-btn {
    width: 28px;
    height: 30px;
    border: none;
    border-radius: 0;
    font-size: 15px;
  }

  .undo-group .icon-btn + .icon-btn {
    border-left: 1px solid var(--line);
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
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .menu.app-menu {
    left: auto;
    right: 0;
    width: 238px;
  }

  .menu.project-menu {
    width: 292px;
  }

  .menu.factory-menu,
  .menu.toolbar-menu {
    left: auto;
    right: 0;
    width: 220px;
  }

  .menu-label {
    padding: 5px 8px 4px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.11em;
    text-transform: uppercase;
  }

  .menu button {
    width: 100%;
    min-height: 30px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 8px;
    color: var(--text);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover {
    background: var(--card-hover);
  }

  .menu button.active {
    background: var(--accent-dim);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .menu button.danger {
    color: var(--danger);
  }

  .menu button.danger:hover {
    background: var(--danger-dim);
  }

  .menu button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .menu-item-main {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .menu-meta {
    flex: 0 0 auto;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9px;
  }

  .menu-empty {
    padding: 7px 8px;
    color: var(--faint);
    font-size: 11px;
  }

  .menu-sep {
    height: 1px;
    margin: 5px 4px;
    background: var(--line);
  }

  .menu-note {
    padding: 7px 8px 3px;
    margin-top: 4px;
    color: var(--faint);
    border-top: 1px solid var(--line);
    font-family: var(--mono);
    font-size: 9px;
    line-height: 1.5;
  }

  .theme-switch {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 2px;
    padding: 2px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .theme-switch button {
    min-height: 26px;
    justify-content: center;
    padding: 0 6px;
    color: var(--muted);
    font-size: 10.5px;
    text-align: center;
  }

  .theme-switch button.active {
    color: var(--accent-ink);
    background: var(--accent);
    box-shadow: none;
    font-weight: 700;
  }

  .menu-check {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 30px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    color: var(--text);
    font-size: 11.5px;
    cursor: pointer;
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

  /* ── 顶栏错误条 ─────────────────────────────────────────────── */

  .err-strip {
    flex: 0 0 auto;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 6px 14px;
    color: var(--danger);
    background: var(--danger-dim);
    border-bottom: 1px solid var(--danger-line);
    font-size: 11px;
  }

  .err-lines {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: calc(3 * 1.45em);
    overflow: hidden;
  }

  .err-line {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .err-line.notice {
    color: var(--muted);
  }
  .err-lines.structured { max-height: min(260px, 35vh); overflow: auto; }

  .err-copy {
    flex: 0 0 auto;
    height: 22px;
    padding: 0 7px;
    color: inherit;
    background: transparent;
    border: 1px solid var(--danger-line);
    border-radius: var(--radius-sm);
    font-size: 10px;
    cursor: pointer;
  }

  .err-copy:hover {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }

  .warn-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 8px 10px;
    margin: 0 0 10px;
    color: var(--warn);
    background: var(--warn-dim);
    border: 1px solid var(--warn-line);
    border-radius: var(--radius-sm);
    font-size: 11px;
    line-height: 1.45;
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

  .rail .panel > .field + .field {
    margin-top: 10px;
  }

  /* 配方/无限科技等“字段 + 整行动作按钮”的垂直间距。 */
  .field > .btn.block {
    margin-top: 7px;
  }

  /* 并排动作行：两枚按钮等宽居中；单独动作行由 .btn.block 撑满。 */
  .button-row {
    display: grid;
    gap: 6px;
    margin-top: 10px;
  }

  .button-row.two {
    grid-template-columns: 1fr 1fr;
  }

  .button-row .btn {
    width: 100%;
  }

  .planning-summary {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
    margin: 9px 0 8px;
  }

  .planning-stat {
    min-width: 0;
    display: grid;
    gap: 2px;
    padding: 7px 8px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .planning-stat span {
    overflow: hidden;
    color: var(--faint);
    font-size: 9px;
    letter-spacing: 0.04em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .planning-stat strong {
    color: var(--text);
    font-family: var(--mono);
    font-size: 16px;
    font-weight: 600;
    line-height: 1.2;
  }

  .planning-note {
    margin: 0 0 8px;
    color: var(--muted);
    font-size: 10px;
    line-height: 1.5;
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

  .external-shortcuts { display: flex; gap: 6px; margin: 0 0 8px; }
  .external-shortcuts .btn { flex: 1; font-size: 11px; }
  .file-feedback { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 8px; padding: 4px 16px;
    border-bottom: 1px solid var(--line); color: var(--muted); font-size: 11px; }
  .file-feedback > span { min-width: 0; overflow-wrap: anywhere; }

  .center {
    display: flex;
    flex-direction: column;
    padding: 0 16px 28px;
  }

  .view-switch { display: inline-flex; gap: 2px; }
  .view-switch .active { background: var(--accent-dim); border-color: var(--accent); font-weight: 600; }

  /* ── 面板标题 / 折叠 ────────────────────────────────────────── */

  .panel-toggle {
    width: 100%;
    display: block;
    margin: 0 0 10px;
    padding: 0;
    color: var(--text);
    background: none;
    border: 0;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .panel-toggle:hover {
    color: var(--text);
  }

  .panel-toggle:hover .chev {
    color: var(--text);
  }

  .toggle-label {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
  }

  .chev {
    width: 9px;
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 9px;
    transition: color 90ms ease;
  }

  .panel-toggle .count {
    margin-left: auto;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9px;
    font-weight: 500;
    letter-spacing: 0;
  }

  /* ── 列表行 ─────────────────────────────────────────────────── */

  .rows {
    display: grid;
    gap: 1px;
    margin-bottom: 10px;
  }

  .row-item {
    min-width: 0;
    min-height: 32px;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 7px;
    padding: 3px 6px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    transition: background 80ms ease, border-color 80ms ease;
  }

  .row-item:hover,
  .row-item:focus-within {
    background: var(--card-hover);
    border-color: var(--line);
  }

  .row-item.one-line {
    flex-wrap: nowrap;
  }





  .row-actions {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 1px;
    opacity: 0;
    transform: translateX(2px);
    transition: opacity 90ms ease, transform 90ms ease;
  }

  .row-item:hover .row-actions,
  .row-item:focus-within .row-actions {
    opacity: 1;
    transform: none;
  }

  .row-actions .icon-btn {
    width: 20px;
    height: 22px;
    font-size: 11px;
  }

  .row-item.implicit {
    border-style: dashed;
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--panel) 92%, var(--accent) 4%);
  }

  .implicit-hint {
    margin: 10px 0 4px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .num {
    width: 68px;
    min-height: 26px;
    padding: 0 6px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10.5px;
    text-align: right;
  }

  .num:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .temp-range {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    flex: 0 0 auto;
  }

  .num.temp {
    width: 38px;
    text-align: center;
  }

  .temp-sep {
    color: var(--faint);
    font-size: 9px;
  }

  .empty-hint {
    padding: 14px 4px;
    color: var(--faint);
    font-size: 11px;
    line-height: 1.55;
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
    background: color-mix(in srgb, var(--bg) 92%, transparent);
    border-bottom: 1px solid var(--line);
    backdrop-filter: blur(8px);
  }

  .toolbar-stats {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .toolbar .menu-wrap {
    margin-left: 2px;
  }

  /* ── 机制列表 ───────────────────────────────────────────────── */

  .empty-state {
    display: grid;
    justify-items: center;
    gap: 6px;
    padding: 56px 24px;
    color: var(--muted);
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius);
    text-align: center;
  }

  .empty-state strong {
    color: var(--text);
    font-size: 12.5px;
  }

  .empty-state .muted {
    max-width: 360px;
    font-size: 11.5px;
    line-height: 1.6;
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

  /* ── 游戏数据模态 ───────────────────────────────────────────── */

  .backdrop {
    position: fixed;
    z-index: 50;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--backdrop);
    backdrop-filter: blur(4px);
  }

  .backdrop.low {
    z-index: 30;
  }

  .modal {
    width: min(560px, 100%);
    max-height: min(760px, calc(100vh - 48px));
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    overflow: hidden;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .modal.data-modal {
    width: min(780px, 100%);
  }

  .modal-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--line);
  }

  .modal-heading {
    display: grid;
    gap: 3px;
  }

  .modal-heading strong {
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .data-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .context-list {
    display: grid;
    gap: 6px;
    min-height: 0;
    overflow-y: auto;
    padding-right: 2px;
  }

  .ctx-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .ctx-row.active {
    background: var(--accent-dim);
    border-color: var(--accent-line);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .ctx-main {
    min-width: 0;
    flex: 1;
    display: grid;
    gap: 3px;
  }

  .ctx-name {
    min-width: 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    overflow: hidden;
    font-size: 12px;
    font-weight: 650;
  }

  .ctx-name-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-meta {
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 9.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-mods {
    min-width: 0;
    overflow: hidden;
    color: var(--faint);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-actions {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .modal-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
    font-size: 10px;
  }

  .kv {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 5px 0;
    color: var(--muted);
    font-size: 10.5px;
  }

  .kv .small {
    overflow: hidden;
    max-width: 220px;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  /* ── 设置项列表（里程碑 / 产能 / 插件塔） ───────────────────── */

  .prefs-list {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .prefs-item {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 4px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
  }

  .prefs-item:hover {
    background: var(--card-hover);
    border-color: var(--line);
  }

  .prefs-item .check {
    margin: 0;
  }

  .prefs-item.column {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    padding: 8px;
    border-color: var(--line);
    background: var(--input-bg);
  }

  .prefs-name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prefs-row {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .prefs-row.sub,
  .prefs-row.wrap {
    flex-wrap: wrap;
  }

  .prefs-title {
    margin: 6px 0 2px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .prefs-hint {
    color: var(--faint);
    font-size: 9.5px;
    line-height: 1.45;
  }

  .prefs-item.inferred {
    border-style: dashed;
    border-color: var(--accent-line);
  }

  .prefs-item.user-specified {
    border-style: solid;
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--input-bg) 94%, var(--accent) 4%);
  }

  .prefs-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 5px 1px 2px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 10px;
  }

  .prefs-chip-x {
    width: 14px;
    height: 14px;
    display: grid;
    place-items: center;
    padding: 0;
    color: var(--danger);
    background: transparent;
    border: none;
    font-size: 10px;
    line-height: 1;
    cursor: pointer;
  }

  .prod-input,
  .me-num input,
  .chip-num {
    min-height: 24px;
    padding: 0 5px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
    text-align: right;
  }

  .prod-input {
    width: 58px;
  }

  .me-num {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-size: 10px;
  }

  .me-num input {
    width: 52px;
  }

  .chip-num {
    width: 42px;
  }

  .badge {
    flex: 0 0 auto;
    max-width: 54px;
    padding: 1px 5px;
    overflow: hidden;
    color: var(--faint);
    border: 1px solid var(--line);
    border-radius: 3px;
    font-size: 9px;
    letter-spacing: 0.04em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── 通用弹窗 ───────────────────────────────────────────────── */

  .mini-modal {
    width: min(352px, 100%);
    display: grid;
    gap: 12px;
    padding: 16px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .mini-modal.load-game {
    width: min(480px, 100%);
  }

  .font-modal {
    width: min(520px, 100%);
  }

  .font-section {
    display: grid;
    gap: 5px;
  }

  .font-section > label {
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .font-section input {
    width: 100%;
    font-size: 12px;
  }

  .font-preview {
    min-width: 0;
    display: grid;
    gap: 3px;
    padding: 9px 11px;
    overflow: hidden;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .font-preview.ui {
    font-family: var(--sans);
  }

  .font-preview.mono {
    font-family: var(--mono);
    font-size: 12px;
  }

  .font-preview strong {
    overflow: hidden;
    font-size: 14px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .font-preview span {
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .font-hint {
    margin: 0;
    color: var(--muted);
    font-size: 10px;
    line-height: 1.55;
  }

  .mini-title {
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .confirm-text {
    color: var(--muted);
    font-size: 11.5px;
    line-height: 1.55;
  }

  .mini-modal input {
    min-height: 32px;
    padding: 0 9px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  .mini-modal input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .mini-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .path-field {
    display: grid;
    gap: 5px;
  }

  .path-label {
    color: var(--faint);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .path-row {
    display: flex;
    gap: 6px;
  }

  .path-row input {
    flex: 1;
    min-width: 0;
  }

  .err-text {
    color: var(--danger);
    font-size: 11px;
    line-height: 1.45;
  }

  /* 规划偏好弹窗：略高于设置抽屉、低于选择器。 */
  .prefs-modal {
    width: min(460px, 100%);
    max-height: min(720px, calc(100vh - 48px));
    display: grid;
    gap: 12px;
    padding: 16px;
    overflow-y: auto;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .prefs-section {
    display: grid;
    gap: 6px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }

  /* ── 响应式：窄窗口先压缩两侧栏，再让行内动作常显 ──────────── */

  @media (max-width: 1360px) {
    .workspace {
      grid-template-columns: 292px minmax(400px, 1fr) 280px;
    }
    .factory-tabs {
      max-width: 34vw;
    }
    .status-text {
      max-width: 150px;
    }
    /* 侧栏变窄后给目标名称留出空间，避免「石油气」被温度区间挤成省略号；
       较宽的界面字体（如 Microsoft YaHei）下也优先保证名称完整。 */
    .row-item {
      gap: 5px;
    }
    .num {
      width: 60px;
    }
    .num.temp {
      width: 32px;
    }
    .row-actions .icon-btn {
      width: 18px;
    }
    .temp-range {
      gap: 1px;
    }
  }

  @media (max-width: 1120px) {
    .workspace {
      grid-template-columns: 270px minmax(360px, 1fr) 260px;
    }
    .trigger-label,
    .bar-label {
      display: none;
    }
    .project-trigger {
      max-width: 180px;
    }
    .row-item {
      gap: 5px;
    }
    .num {
      width: 60px;
    }
    .num.temp {
      width: 32px;
    }
    .temp-range {
      gap: 1px;
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
