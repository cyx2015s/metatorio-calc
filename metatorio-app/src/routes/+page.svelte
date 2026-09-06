<script lang="ts">
  // Metatorio 主界面：紧凑布局，圆角卡片分区；游戏内物品一律图标按钮，
  // 一般操作一律文字按钮。
  import { onMount } from "svelte";
  import { dndzone } from "svelte-dnd-action";
  import { getVersion } from "@tauri-apps/api/app";
  import { check as updaterCheck } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { allowedModules, pickGameExecutable, pickModDir, suggest } from "$lib/runtime/client";
  import { dualVarLabel, flowQuality, itemOf, accessibleKind, accessibleName } from "$lib/runtime/types";
  import { signedCompactNumber } from "$lib/format";
  import type {
    Accessible,
    CatalogKind,
    DualVar,
    MechanicId,
    TargetId,
  } from "$lib/runtime/types";
  import HoverIcon from "$lib/ui/HoverIcon.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Selector from "$lib/ui/Selector.svelte";
  import MechanicCard from "$lib/ui/MechanicCard.svelte";

  const mechKinds: { kind: import("$lib/runtime/types").MechanicKind; label: string }[] = [
    { kind: "recipe", label: "配方" },
    { kind: "mining", label: "采矿" },
    { kind: "spoil", label: "变质" },
    { kind: "plant", label: "种植" },
    { kind: "item-fuel", label: "物品燃料" },
    { kind: "item-launch", label: "火箭发射" },
    { kind: "generator", label: "发电机" },
    { kind: "boiler", label: "锅炉" },
    { kind: "reactor", label: "反应堆" },
    { kind: "solar", label: "太阳能" },
    { kind: "fluid-fuel", label: "流体燃料" },
    { kind: "fluid-heat", label: "流体热" },
  ];

  onMount(() => {
    runtime.initTheme();
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
  let newProjectOpen = $state(false);
  let newProjectName = $state("新项目");
  let newFactoryOpen = $state(false);
  let newFactoryName = $state("新工厂");
  // 规划偏好弹窗（机器偏好/替代数量/枚举插件与信标）
  let prefsOpen = $state(false);
  // 建议系统弹窗
  let suggestions = $state<{
    flow: DualVar;
    items: import("$lib/runtime/types").Suggestion[];
    loading: boolean;
  } | null>(null);

  async function openSuggestions(flow: DualVar) {
    suggestions = { flow, items: [], loading: true };
    try {
      const items = await suggest(flow, runtime.effectiveContextId);
      suggestions = { flow, items, loading: false };
    } catch {
      suggestions = { flow, items: [], loading: false };
    }
  }

  function suggestionKindLabel(kind: string): string {
    return (
      {
        recipe: "配方",
        resource: "矿点",
        "item-fuel": "燃料",
        generator: "发电机",
      }[kind] ?? kind
    );
  }

  function suggestionIcon(kind: string): { type: string; detailKind: string } {
    if (kind === "recipe") return { type: "recipe", detailKind: "recipe" };
    if (kind === "resource") return { type: "entity", detailKind: "resource" };
    if (kind === "item-fuel") return { type: "item", detailKind: "item" };
    return { type: "entity", detailKind: "generator" };
  }

  function suggestionName(kind: string, name: string): string {
    const localizeKind =
      kind === "recipe" ? "recipe" : kind === "resource" ? "resource" : kind === "item-fuel" ? "item" : "generator";
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
    onSelectFlow: (flow: import("$lib/runtime/types").DualVar) => void;
  } | null>(null);

  // ── 左侧栏面板折叠状态 ───────────────────────────────────────────
  // 每个可设置项可独立下拉展开/收起；默认展开常用的（目标/工厂环境/
  // 项目设置），收起常为空的（目标表达式/外部输入）。
  let panels = $state({
    targets: true,
    expressions: false,
    inputs: false,
    env: true,
    settings: true,
    subMilestones: true,
    subProductivity: true,
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

  /** 来源展示缩短：只留每段最后一个路径片段（完整路径在 tooltip）。 */
  function shortSource(source: string): string {
    return source
      .split(", ")
      .map((part) => {
        const [label, path] = part.split(": ");
        if (!path) return part;
        const segment = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
        return `${label}: ${segment}`;
      })
      .join(" · ");
  }

  // ── 派生数据 ────────────────────────────────────────────────────
  let project = $derived(runtime.selectedProject);
  let planning = $derived(project?.planning ?? null);
  let factory = $derived(runtime.selectedFactory);
  let mechanics = $derived(factory?.mechanics ?? []);
  let targets = $derived(factory?.targets ?? []);
  let targetExpressions = $derived(factory?.target_expressions ?? []);
  let externalInputs = $derived(factory?.external_inputs ?? []);
  let implicitInputs = $derived(runtime.implicitSourcesCache);
  // 拖拽排序的本地镜像（文档变化时同步；拖拽期间由 dndzone 更新）
  let dragTargets = $state<import("$lib/runtime/types").FlowTarget[]>([]);
  let dragInputs = $state<import("$lib/runtime/types").ExternalInput[]>([]);
  let dragMechanics = $state<import("$lib/runtime/types").MechanicEntry[]>([]);
  // 机制列表始终聚合：同配方×机器合并成一张卡，品质变体紧凑排列。

  /** 组的显示名（首个条目的配方/资源/物品名）。 */
  function groupTitle(entries: import("$lib/runtime/types").MechanicEntry[]): string {
    const first = entries[0]?.mechanic;
    if (!first) return "";
    const name =
      first.recipe?.id ??
      first.item?.id ??
      first.seed?.id ??
      first.resource ??
      first.generator?.id ??
      first.boiler?.id ??
      first.reactor?.id ??
      first.fluid ??
      "";
    const kind = first.type;
    if (kind === "recipe") return runtime.localizedName("recipe", name) || name;
    if (kind === "mining") return runtime.localizedName("resource", name) || name;
    if (kind === "generator" || kind === "boiler" || kind === "reactor") {
      return runtime.localizedName("entity", name) || name;
    }
    if (kind === "fluid-fuel" || kind === "fluid-heat") {
      return runtime.localizedName("fluid", name) || name;
    }
    return runtime.localizedName("item", name) || name;
  }

  /** 组的机器名（配方/采矿有机器；其余无）。 */
  function groupMachine(entries: import("$lib/runtime/types").MechanicEntry[]): string {
    const machine = entries[0]?.mechanic.machine?.id;
    if (!machine) return "";
    return runtime.localizedName("machine", machine) || machine;
  }

  /** 信标原型的插件槽数（getDetail 异步；未知时默认 2，与 Factorio 一致）。 */
  async function beaconModuleSlots(beaconId: string | undefined): Promise<number> {
    if (!beaconId) return 2;
    const detail = await runtime.getDetail("beacon", beaconId);
    return detail?.beacon_module_slots ?? detail?.module_slots ?? 2;
  }

  /** 机器原型的插件槽数（catalogIndex；未知时 0）。 */
  function machineModuleSlots(machineId: string | undefined): number {
    if (!machineId) return 0;
    const entry = runtime.catalogIndex?.entries.find(
      (candidate) => candidate.kind === "machine" && candidate.name === machineId,
    );
    return entry?.module_slots ?? 0;
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

  /** 聚合键：合并开启时仅 recipe 按 (配方, 机器) 聚合；其余类型各自独立。 */
  function mechGroupKey(entry: import("$lib/runtime/types").MechanicEntry): string {
    if (!mergeRecipes) return `single:${entry.id}`;
    const m = entry.mechanic;
    if (m.type === "recipe") return `recipe:${m.recipe?.id ?? ""}:${m.machine?.id ?? ""}`;
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
      for (const entry of dragMechanics) {
        const key = mechGroupKey(entry);
        const existing = index.get(key);
        if (existing !== undefined) {
          groups[existing].entries.push(entry);
        } else {
          index.set(key, groups.length);
          groups.push({ key, entries: [entry] });
        }
      }
      return groups;
    })(),
  );
  $effect(() => {
    dragTargets = targets;
  });
  $effect(() => {
    dragInputs = externalInputs;
  });
  $effect(() => {
    dragMechanics = mechanics;
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
  function handleMechanicsConsider(event: CustomEvent<{ items: import("$lib/runtime/types").MechanicEntry[] }>) {
    dragMechanics = event.detail.items;
  }
  function handleMechanicsFinalize(event: CustomEvent<{ items: import("$lib/runtime/types").MechanicEntry[] }>) {
    dragMechanics = event.detail.items;
    runtime
      .reorderMechanics(event.detail.items.map((entry) => entry.id))
      .catch(() => {});
  }
  let solve = $derived(runtime.solve);
  let solveMap = $derived(
    new Map(
      (solve != null && "solved" in solve.status ? solve.status.solved.mechanics : []).map(
        (m) => [m.mechanic, { amount: m.amount, cost: m.cost }] as const,
      ),
    ),
  );
  let solved = $derived(solve != null && "solved" in solve.status);
  let notSolved = $derived(solve != null && "not-solved" in solve.status);

  function flowIcon(flow: DualVar): { type: string; name: string } {
    const item = itemOf(flow);
    if (item) return { type: "item", name: item.id };
    if (flow !== null && typeof flow === "object") {
      if ("Fluid" in flow) {
        const fluid = (flow as { Fluid: { name: string } }).Fluid;
        return { type: "fluid", name: fluid.name };
      }
      if ("Entity" in flow) {
        const entity = (flow as { Entity: { id: string } }).Entity;
        return { type: "entity", name: entity.id };
      }
    }
    return { type: "flow", name: dualVarLabel(flow) };
  }

  function flowDetailKind(icon: { type: string }): string | undefined {
    if (icon.type === "item") return "item";
    if (icon.type === "fluid") return "fluid";
    if (icon.type === "entity") return "entity";
    return undefined;
  }

  /** 能量类流（数值单位为瓦特 J/s）：Electricity / Heat / 抽象燃料热量。 */
  function isEnergyFlow(flow: DualVar): boolean {
    if (typeof flow === "string") {
      return flow === "Electricity" || flow === "Heat";
    }
    if (flow !== null && typeof flow === "object") {
      return "FluidHeat" in flow || "FluidFuel" in flow || "ItemFuel" in flow;
    }
    return false;
  }

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
    return signedCompactNumber(amount);
  }

  /** 流的显示名：物品/流体/实体优先本地化名，否则内部 id。 */
  function flowLabel(flow: DualVar): string {
    const icon = flowIcon(flow);
    if (icon.type === "item" || icon.type === "fluid" || icon.type === "entity") {
      return runtime.localizedName(icon.type, icon.name);
    }
    // 抽象能量流的中文名
    if (typeof flow === "string") {
      if (flow === "Electricity") return "电力";
      if (flow === "Heat") return "热量";
      if (flow === "RocketSlotCapacity") return "火箭运力（槽位）";
      if (flow === "RocketWeightCapacity") return "火箭运力（重量）";
    }
    if (flow !== null && typeof flow === "object") {
      if ("ItemFuel" in flow) return "物品燃料";
      if ("FluidFuel" in flow) return "流体燃料";
      if ("FluidHeat" in flow) return "流体热量";
      if ("Pollution" in flow) {
        const pollution = (flow as { Pollution: { name: string } }).Pollution;
        return `污染（${pollution.name}）`;
      }
    }
    return dualVarLabel(flow);
  }

  /** 抽象能量流的第二行说明（燃料类别/流体名）；无则空串。 */
  function flowLabelSub(flow: DualVar): string {
    if (flow !== null && typeof flow === "object") {
      if ("ItemFuel" in flow) {
        const itemFuel = flow as { ItemFuel: { category: string[] } };
        return itemFuel.ItemFuel.category.join(" / ");
      }
      if ("FluidFuel" in flow || "FluidHeat" in flow) {
        const inner = flow as { FluidFuel?: { filter: string }; FluidHeat?: { filter: string } };
        const filter = inner.FluidFuel?.filter ?? inner.FluidHeat?.filter ?? "";
        return filter ? runtime.localizedName("fluid", filter) || filter : "任意流体";
      }
    }
    return "";
  }

  /** 流 → 流选择器初始页签（编辑目标/外部输入时预选）。 */
  function flowTabOf(flow: DualVar): string {
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

  // ── 机制拾取器分发 ──────────────────────────────────────────────
  async function pickForMechanic(
    mechanic: MechanicId,
    kind: CatalogKind | "beacon-module",
    a?: number,
    b?: number,
  ) {
    const entry = mechanics.find((candidate) => candidate.id === mechanic);
    try {
      switch (kind) {
        case "recipe":
          openSelector(
            "recipe",
            "选择配方",
            (name, quality) => runtime.setRecipe(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.recipe?.id,
            entry?.mechanic.recipe?.quality,
          );
          break;
        case "machine": {
          // 机器列表按当前配方的 categories 过滤（组装机/电炉/火箭发射井等）
          const recipeId = entry?.mechanic.recipe?.id;
          const filter = recipeId
            ? (await runtime.getDetail("recipe", recipeId))?.categories ?? []
            : [];
          openSelector(
            "machine",
            "选择机器",
            (name, quality) => runtime.setMachine(mechanic, name, quality),
            [{ kind: "machine", label: "制造机" }],
            filter,
            undefined,
            entry?.mechanic.machine?.id,
            entry?.mechanic.machine?.quality,
          );
          break;
        }
        case "mining-machine": {
          // 采矿机列表按当前资源的 category 过滤
          const resource = entry?.mechanic.resource;
          const filter = resource
            ? (await runtime.getDetail("resource", resource))?.categories ?? []
            : [];
          openSelector(
            "mining-machine",
            "选择采矿机",
            (name, quality) => runtime.setMachine(mechanic, name, quality),
            [{ kind: "mining-machine", label: "采矿机" }],
            filter,
            undefined,
            entry?.mechanic.machine?.id,
            entry?.mechanic.machine?.quality,
          );
          break;
        }
        case "resource": {
          // 资源列表按当前采矿机的 resource_categories 过滤
          const machineId = entry?.mechanic.machine?.id;
          const filter = machineId
            ? (await runtime.getDetail("mining-machine", machineId))?.categories ?? []
            : [];
          openSelector(
            "resource",
            "选择资源",
            (name) => runtime.setResource(mechanic, name),
            [{ kind: "resource", label: "资源" }],
            filter,
            undefined,
            entry?.mechanic.resource,
          );
          break;
        }
        case "fluid": {
          // 按机制类型过滤：流体燃料/流体热有专用标签；发电机/锅炉按流体箱过滤
          const mechKind = entry?.mechanic.type;
          if (mechKind === "fluid-fuel") {
            openSelector(
              "fluid",
              "选择热值流体（燃料）",
              (name) => runtime.setFluidFuel(mechanic, name),
              [],
              ["fluid-fuel"],
            );
            break;
          }
          if (mechKind === "fluid-heat") {
            openSelector(
              "fluid",
              "选择提热流体",
              (name) => runtime.setFluidHeat(mechanic, name),
              [],
              ["fluid-heat"],
            );
            break;
          }
          const entityId =
            mechKind === "generator"
              ? entry?.mechanic.generator?.id
              : mechKind === "boiler"
                ? entry?.mechanic.boiler?.id
                : undefined;
          const filter = entityId ? await fluidFilterOf(mechKind, entityId) : [];
          openSelector(
            "fluid",
            "选择流体",
            (name) => runtime.setFluid(mechanic, name),
            [],
            [],
            filter,
          );
          break;
        }
        case "item": {
          // 按机制类型过滤物品：变质/种植/燃料/发射
          const mechKind = entry?.mechanic.type;
          const filter =
            mechKind === "spoil"
              ? ["spoilable"]
              : mechKind === "plant"
                ? ["plantable"]
                : mechKind === "item-fuel"
                  ? ["fuel"]
                  : mechKind === "item-launch"
                    ? ["launchable"]
                    : [];
          const title =
            mechKind === "spoil"
              ? "选择会变质的物品"
              : mechKind === "plant"
                ? "选择种子"
                : mechKind === "item-fuel"
                  ? "选择燃料"
                  : mechKind === "item-launch"
                    ? "选择发射物品"
                    : "选择物品";
          openSelector(
            "item",
            title,
            (name, quality) => runtime.setItem(mechanic, name, quality),
            [],
            filter,
            undefined,
            entry?.mechanic.item?.id,
            entry?.mechanic.item?.quality,
          );
          break;
        }
        case "generator":
          openSelector(
            "generator",
            "选择发电机",
            (name, quality) => runtime.setGenerator(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.generator?.id,
            entry?.mechanic.generator?.quality,
          );
          break;
        case "boiler":
          openSelector(
            "boiler",
            "选择锅炉",
            (name, quality) => runtime.setBoiler(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.boiler?.id,
            entry?.mechanic.boiler?.quality,
          );
          break;
        case "reactor":
          openSelector(
            "reactor",
            "选择反应堆",
            (name, quality) => runtime.setReactor(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.reactor?.id,
            entry?.mechanic.reactor?.quality,
          );
          break;
        case "solar-panel":
          openSelector(
            "solar-panel",
            "选择太阳能板",
            (name, quality) => runtime.setSolarPanel(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.solar_panel?.id,
            entry?.mechanic.solar_panel?.quality,
          );
          break;
        case "accumulator":
          openSelector(
            "accumulator",
            "选择蓄电器",
            (name, quality) => runtime.setAccumulator(mechanic, name, quality),
          );
          break;
        case "module": {
          // 机器插件：按机器允许的插件鉴权（类别 + 效果类型 + 配方开关）
          // 过滤；添加前校验槽数。
          const machineId = entry?.mechanic.machine?.id;
          const detailKind = entry?.mechanic.type === "mining" ? "mining-machine" : "machine";
          const slots = machineModuleSlots(machineId);
          const filled = entry?.mechanic.module_config?.modules.length ?? 0;
          if (a != null && a >= filled && slots > 0 && filled >= slots) {
            showNotice(`插件槽已满（${slots} 个）`);
            break;
          }
          const recipeId = entry?.mechanic.type === "recipe" ? (entry?.mechanic.recipe?.id ?? null) : null;
          const allowed = machineId
            ? await allowedModules(detailKind, machineId, recipeId, runtime.effectiveContextId)
            : [];
          // 已选机器但鉴权后无可用插件：用不可能匹配的哨兵让选择器显示空。
          const allowedNames =
            machineId && allowed.length === 0 ? [""] : allowed.length > 0 ? allowed : undefined;
          // 编辑已有槽位：预选当前插件（只改品质也可确认）。
          const currentModule =
            a != null ? entry?.mechanic.module_config?.modules[a] : undefined;
          openSelector(
            "module",
            machineId ? "选择插件" : "选择插件（先选择机器）",
            (name, quality) => runtime.setModuleSlot(mechanic, a ?? 0, name, quality),
            [],
            [],
            allowedNames,
            currentModule?.id,
            currentModule?.quality,
          );
          break;
        }
        case "beacon": {
          const beacon = a ?? 0;
          openSelector("beacon", "选择信标", (name, quality) =>
            runtime
              .moduleMessage(mechanic, {
                "set-beacon": { beacon, value: { id: name, quality } },
              })
              .catch(() => {}),
          );
          break;
        }
        case "beacon-module": {
          const beacon = a ?? 0;
          const moduleIdx = b ?? 0;
          const beaconCfg = entry?.mechanic.module_config?.beacons[beacon];
          const beaconModuleCount = beaconCfg?.modules.length ?? 0;
          // 添加新塔内插件前校验：总数量 ≤ 信标插件槽数 × 信标数量。
          if (moduleIdx >= beaconModuleCount && beaconCfg) {
            const beaconSlots =
              (await beaconModuleSlots(beaconCfg.beacon.id)) * beaconCfg.count;
            const total = beaconCfg.modules.reduce((sum, [, count]) => sum + count, 0);
            if (beaconSlots > 0 && total >= beaconSlots) {
              showNotice(`信标插件已满（${beaconSlots} 个槽位）`);
              break;
            }
          }
          // 塔内插件按该信标允许的插件鉴权过滤（类别 + 效果类型；与机器同理）
          const beaconId = beaconCfg?.beacon.id;
          const allowed = beaconId
            ? await allowedModules("beacon", beaconId, null, runtime.effectiveContextId)
            : [];
          // 已选信标但鉴权后无可用插件：用不可能匹配的哨兵让选择器显示空。
          const allowedNames =
            beaconId && allowed.length === 0 ? [""] : allowed.length > 0 ? allowed : undefined;
          openSelector(
            "module",
            beaconId ? "选择塔内插件" : "选择塔内插件（先选择信标）",
            (name, quality) => {
              const value = { id: name, quality };
              if (moduleIdx >= beaconModuleCount) {
                runtime
                  .moduleMessage(mechanic, { "add-beacon-module": { beacon, module: value } })
                  .catch(() => {});
              } else {
                runtime
                  .moduleMessage(mechanic, {
                    "set-beacon-module": { beacon, module: moduleIdx, value },
                  })
                  .catch(() => {});
              }
            },
            [],
            [],
            allowedNames,
          );
          break;
        }
        default:
          break;
      }
    } catch {
      /* 过滤信息拉取失败时仍打开选择器（不过滤） */
    }
  }

  /** 发电机/锅炉允许的流体（流体箱 filter；无则空 = 不限制）。 */
  async function fluidFilterOf(mechKind: string | undefined, entityId: string): Promise<string[]> {
    const detailKind = mechKind === "generator" ? "generator" : "boiler";
    const detail = await runtime.getDetail(detailKind, entityId);
    return detail?.fluid_filter ? [detail.fluid_filter] : [];
  }

  /** 指定机制燃料：流选择器（物品/流体页签）→ SetFuel。
   *  burner 机器只吃物品燃料：按机器的燃料类别（burner_fuel_categories）
   *  过滤物品的 fuel_category（无类别限制 → 全部燃料）；非 burner（流体/
   *  热能量源）不做物品限定，流体页签可选。锅炉/反应堆/采矿机与制造机同规则。 */
  async function pickFuel(mechanic: MechanicId) {
    const entry = mechanics.find((candidate) => candidate.id === mechanic);
    const m = entry?.mechanic;
    const type = m?.type;
    // 依机制类型确定要查的实体详情 kind + 设备 id（各组件字段不同）。
    let entityKind: CatalogKind | null = null;
    let entityId: string | null = null;
    if (type === "recipe") {
      entityKind = "machine";
      entityId = m?.machine?.id ?? null;
    } else if (type === "mining") {
      entityKind = "mining-machine";
      entityId = m?.machine?.id ?? null;
    } else if (type === "boiler") {
      entityKind = "boiler";
      entityId = m?.boiler?.id ?? null;
    } else if (type === "reactor") {
      entityKind = "reactor";
      entityId = m?.reactor?.id ?? null;
    }

    let allowedNames: string[] | undefined;
    let initialTab: string | undefined;
    if (entityKind && entityId) {
      const detail = await runtime.getDetail(entityKind, entityId);
      const categories = detail?.burner_fuel_categories ?? [];
      const energySource = detail?.machine_energy_source;
      if (energySource === "burner" || categories.length > 0) {
        // burner 机器：只允许物品燃料（有热值），且（无类别限制 或 类别匹配）。
        allowedNames = (runtime.catalogIndex?.entries ?? [])
          .filter(
            (candidate) =>
              candidate.kind === "item" &&
              candidate.fuel_value_j != null &&
              candidate.fuel_value_j > 0 &&
              (categories.length === 0 || categories.includes(candidate.fuel_category)),
          )
          .map((candidate) => candidate.name);
        initialTab = "item";
      } else if (energySource === "fluid") {
        // 流体能量源（如某些锅炉/发电机）：只允许有热值的流体燃料。
        allowedNames = (runtime.catalogIndex?.entries ?? [])
          .filter(
            (candidate) =>
              candidate.kind === "fluid" &&
              candidate.fuel_value_j != null &&
              candidate.fuel_value_j > 0,
          )
          .map((candidate) => candidate.name);
        initialTab = "fluid";
      }
    }
    flowSelector = {
      title: "选择燃料",
      initialTab,
      allowedNames,
      onSelectFlow: (flow) => {
        const item = itemOf(flow);
        if (item) {
          // 物品燃料：带品质。
          runtime.setFuel(mechanic, { kind: "item", item }).catch(() => {});
          return;
        }
        if (flow !== null && typeof flow === "object" && "Fluid" in flow) {
          const fluid = (flow as { Fluid: { name: string } }).Fluid.name;
          runtime.setFuel(mechanic, { kind: "fluid", fluid }).catch(() => {});
          return;
        }
        runtime.setFuel(mechanic, null).catch(() => {});
      },
    };
  }

  /** 添加信标：直接打开信标选择器（不再先加空配置行），重复信标拒绝。 */
  function addBeacon(mechanic: MechanicId) {
    const entry = mechanics.find((candidate) => candidate.id === mechanic);
    openSelector("beacon", "选择信标（添加）", (name, quality) => {
      const existing = entry?.mechanic.module_config?.beacons ?? [];
      // 重复按 IdWithQuality 判等：同种信标不同品质允许并存
      if (
        existing.some(
          (beacon) => beacon.beacon.id === name && beacon.beacon.quality === quality,
        )
      ) {
        showNotice(`信标 ${name}（${runtime.localizedName("quality", quality)}）已添加，不能重复`);
        return;
      }
      runtime
        .moduleMessage(mechanic, { "add-beacon": { beacon: { id: name, quality } } })
        .catch(() => {});
    });
  }
</script>

<svelte:head>
  <title>切向量化</title>
</svelte:head>

<div class="app">
  <!-- ══ 应用栏 ══ -->
  <header class="appbar">
    <div class="brand">
      <span class="brand-mark">切</span>
      <span class="brand-name">向量化</span>
    </div>

    <div class="menu-wrap">
      <button class="btn" onclick={openLoadGame} disabled={runtime.contextBusy}>
        加载游戏
      </button>
      <button class="btn ghost" onclick={checkForUpdate} disabled={updating}>
        {updating ? "检查中…" : "检查更新"}
      </button>
    </div>

    {#if runtime.contextBusy}
      <span class="chip warn">正在加载数据…</span>
    {:else if runtime.activeContext}
      <span class="chip ok">{runtime.activeContext.name}</span>
    {:else}
      <span class="chip">未加载游戏数据</span>
    {/if}
    {#if appVersion}
      <span class="chip mono" title="当前版本">v{appVersion}</span>
    {/if}
    <label class="theme-picker" title="主题">
      <span class="muted">主题</span>
      <select
        value={runtime.theme}
        onchange={(event) =>
          runtime.setTheme((event.currentTarget as HTMLSelectElement).value as "system" | "light" | "dark")}
      >
        <option value="system">跟随系统</option>
        <option value="light">亮色</option>
        <option value="dark">深色</option>
      </select>
    </label>
  </header>

  {#if runtime.contextError || runtime.lastError || notice}
    <div class="err-strip">
      {#if runtime.contextError}<span>数据：{runtime.contextError}</span>{/if}
      {#if runtime.lastError}<span>操作：{runtime.lastError}</span>{/if}
      {#if notice}<span class="notice">提示：{notice}</span>{/if}
    </div>
  {/if}

  <!-- ══ 项目 / 工厂页签 ══ -->
  <nav class="tabs">
    {#each runtime.document?.projects ?? [] as item (item.id)}
      {@const savePath = runtime.projectSavePath(item.id)}
      <div class:active={item.id === runtime.selectedProject?.id} class="tab-cluster">
        <button
          class="tab"
          title={savePath ? `保存位置：${savePath}` : "尚未保存"}
          onclick={() => runtime.selectProject(item.id).catch(() => {})}
        >{item.name}</button>
        {#if item.id === runtime.selectedProject?.id}
          <button class="tab-x" title="重命名项目" onclick={promptRenameProject}>✎</button>
        {/if}
      </div>
    {/each}
    <button class="tab add" title="新建项目" onclick={() => (newProjectOpen = true)}>+</button>
    <!-- 项目级操作（保存/导入/关闭都是对单个项目的，放在项目 tab 行） -->
    <span class="tabs-sep"></span>
    <button class="btn ghost" title="从文件导入项目（追加到当前）" onclick={() => runtime.openProject().catch(() => {})} disabled={runtime.busy}>
      导入项目
    </button>
    {#if project}
      {@const savePath = runtime.projectSavePath(project.id)}
      <button
        class="btn ghost"
        title={savePath ? `保存到 ${savePath}` : "保存（首次将选择位置）"}
        onclick={() => runtime.saveCurrentProject().catch(() => {})}
        disabled={runtime.busy}
      >保存</button>
      <button
        class="btn ghost"
        title="另存为（选择新位置）"
        onclick={() => runtime.saveProjectAs().catch(() => {})}
        disabled={runtime.busy}
      >另存为</button>
      <button
        class="btn ghost"
        title="关闭项目（不删除文件）"
        onclick={() =>
          (closeProjectState = { project: project.id, name: project.name })}
      >关闭</button>
    {/if}
  </nav>

  {#if project}
    <nav class="tabs sub">
      {#each project.factories as item (item.id)}
        <div class:active={item.id === runtime.selectedFactory?.id} class="tab-cluster">
          <button
            class="tab"
            onclick={() => runtime.selectFactory(item.id).catch(() => {})}
          >{item.name}</button>
          <button
            class="tab-x"
            title="重命名工厂"
            onclick={() => promptRenameFactory(item.name)}
          >✎</button>
          <button
            class="tab-x"
            title="克隆工厂"
            onclick={() => runtime.cloneFactory(item.id).catch(() => {})}
          >⧉</button>
          <button
            class="tab-x"
            title="删除工厂"
            onclick={() => runtime.removeFactory(item.id).catch(() => {})}
          >×</button>
        </div>
      {/each}
      <button class="tab add" title="新建工厂" onclick={() => (newFactoryOpen = true)}>+</button>
    </nav>
  {/if}

  <!-- ══ 工作区 ══ -->
  <main class="workspace">
    <!-- 左栏：目标 / 外部输入 / 项目设置 -->
    <aside class="col">
      {#if factory}
        <section class="panel">
          <button
            class="title panel-toggle"
            aria-expanded={panels.targets}
            onclick={() => (panels.targets = !panels.targets)}
          >
            <span class="toggle-label">
              <span class="chev">{panels.targets ? "▾" : "▸"}</span>
              目标 <span class="count">{targets.length}</span>
            </span>
          </button>
          {#if panels.targets}
          <div
            class="rows"
            use:dndzone={{ items: dragTargets, flipDurationMs: 120 }}
            onconsider={handleTargetsConsider}
            onfinalize={handleTargetsFinalize}
          >
            {#each dragTargets as target (target.id)}
              {@const icon = flowIcon(target.flow)}
              {@const q = flowQuality(target.flow)}
              <div class="row-item">
                <HoverIcon
                  type={icon.type}
                  name={icon.name}
                  size={26}
                  detailKind={flowDetailKind(icon)}
                  quality={q ?? undefined}
                  flow={target.flow}
                  onClick={() => openSuggestions(target.flow)}
                />
                <span class="row-name" title={dualVarLabel(target.flow)}>{flowLabel(target.flow)}</span>
                <input
                  class="num"
                  type="text"
                  inputmode="decimal"
                  value={isEnergyFlow(target.flow)
                    ? formatPowerValue(Math.abs(target.amount))
                    : String(target.amount)}
                  onchange={(event) => {
                    const raw = (event.currentTarget as HTMLInputElement).value;
                    const value = isEnergyFlow(target.flow) ? parsePowerInput(raw) : Number(raw);
                    if (value !== null && Number.isFinite(value)) {
                      runtime.setTargetAmount(target.id, value).catch(() => {});
                    }
                  }}
                />
                <button class="btn ghost" title="建议能产出该流的机制" onclick={() => openSuggestions(target.flow)}>建议</button>
                <button class="btn ghost" title="更改目标流" onclick={() => editTarget(target)}>更改</button>
                <button class="btn ghost" title="移除目标" onclick={() => runtime.removeTarget(target.id).catch(() => {})}>×</button>
              </div>
            {:else}
              <div class="empty-hint">还没有目标流</div>
            {/each}
          </div>
          <button
            class="btn"
            onclick={() =>
              (flowSelector = {
                title: "添加目标流",
                onSelectFlow: (flow) => runtime.addTargetFlow(flow, 1).catch(() => {}),
              })}
            disabled={!runtime.activeContext}
          >+ 添加目标</button>
          {/if}
        </section>

        <section class="panel">
          <button
            class="title panel-toggle"
            aria-expanded={panels.expressions}
            onclick={() => (panels.expressions = !panels.expressions)}
          >
            <span class="toggle-label">
              <span class="chev">{panels.expressions ? "▾" : "▸"}</span>
              目标表达式 <span class="count">{targetExpressions.length}</span>
            </span>
          </button>
          {#if panels.expressions}
          <div class="rows">
            {#each targetExpressions as expression, ei (expression.id)}
              <div class="expr-card">
                <div class="expr-head">
                  <label class="expr-const">
                    常数
                    <input
                      class="num"
                      type="number"
                      step="0.1"
                      value={String(expression.constant)}
                      onchange={(event) => {
                        const value = Number((event.currentTarget as HTMLInputElement).value);
                        if (Number.isFinite(value)) {
                          runtime
                            .setTargetExpressionConstant(expression.id, value)
                            .catch(() => {});
                        }
                      }}
                    />
                  </label>
                  <button
                    class="btn ghost"
                    title="移除表达式"
                    onclick={() => runtime.removeTargetExpression(expression.id).catch(() => {})}
                  >×</button>
                </div>
                <div class="expr-terms">
                  {#each expression.terms as term, ti (term.id)}
                    {@const icon = flowIcon(term.flow)}
                    <div class="expr-term">
                      <button
                        class="icon-btn"
                        title="更换流的种类"
                        onclick={() => {
                          flowSelector = {
                            title: "更改目标表达式的流",
                            initialTab: flowTabOf(term.flow),
                            ...flowInitialOf(term.flow),
                            onSelectFlow: (flow) =>
                              runtime
                                .setTargetExpressionTermFlow(expression.id, term.id, flow)
                                .catch(() => {}),
                          };
                        }}
                      >
                        <HoverIcon
                          type={icon.type}
                          name={icon.name}
                          size={20}
                          detailKind={flowDetailKind(icon)}
                          quality={flowQuality(term.flow) ?? undefined}
                          flow={term.flow}
                        />
                      </button>
                      <span class="row-name" title={dualVarLabel(term.flow)}>{flowLabel(term.flow)}</span>
                      <input
                        class="num"
                        type="number"
                        step="0.1"
                        value={String(term.coefficient)}
                        onchange={(event) => {
                          const value = Number((event.currentTarget as HTMLInputElement).value);
                          if (Number.isFinite(value)) {
                            runtime
                              .setTargetExpressionTermCoefficient(expression.id, term.id, value)
                              .catch(() => {});
                          }
                        }}
                      />
                      <button
                        class="btn ghost"
                        title="移除项"
                        onclick={() =>
                          runtime
                            .removeTargetExpressionTerm(expression.id, term.id)
                            .catch(() => {})}
                      >×</button>
                    </div>
                  {:else}
                    <span class="muted">还没有项（仅常数）</span>
                  {/each}
                </div>
                <button
                  class="btn"
                  onclick={() =>
                    (flowSelector = {
                      title: "添加目标表达式项",
                      onSelectFlow: (flow) =>
                        runtime.addTargetExpressionTerm(expression.id, flow).catch(() => {}),
                    })}
                >+ 添加项</button>
              </div>
            {:else}
              <div class="empty-hint">目标表达式 = 常数 + Σ(流 × 系数)</div>
            {/each}
          </div>
          <button
            class="btn"
            onclick={() => runtime.addTargetExpression().catch(() => {})}
            disabled={!runtime.activeContext}
          >+ 添加表达式</button>
          {/if}
        </section>

        <section class="panel">
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
          <div
            class="rows"
            use:dndzone={{ items: dragInputs, flipDurationMs: 120 }}
            onconsider={handleInputsConsider}
            onfinalize={handleInputsFinalize}
          >
            {#each dragInputs as input (input.id)}
              {@const icon = flowIcon(input.flow)}
              {@const q = flowQuality(input.flow)}
              <div class="row-item">
                <HoverIcon
                  type={icon.type}
                  name={icon.name}
                  size={26}
                  detailKind={flowDetailKind(icon)}
                  quality={q ?? undefined}
                  flow={input.flow}
                />
                <span class="row-name" title={dualVarLabel(input.flow)}>{flowLabel(input.flow)}</span>
                <input
                  class="num"
                  type="number"
                  step="0.1"
                  min="0"
                  value={String(input.penalty)}
                  onchange={(event) => {
                    const value = Number((event.currentTarget as HTMLInputElement).value);
                    if (Number.isFinite(value)) runtime.setExternalInputPenalty(input.id, value).catch(() => {});
                  }}
                />
                <button class="btn ghost" title="更改外部输入流" onclick={() => editExternalInput(input)}>更改</button>
                <button class="btn ghost" title="移除" onclick={() => runtime.removeExternalInput(input.id).catch(() => {})}>×</button>
              </div>
            {:else}
              <div class="empty-hint">还没有外部输入</div>
          {/each}
        </div>
        {#if implicitInputs.length > 0}
          <div class="implicit-hint">星球自带（隐式免费，外部输入可覆盖）</div>
          <div class="rows">
            {#each implicitInputs as flow (dualVarLabel(flow))}
              {@const icon = flowIcon(flow)}
              {@const q = flowQuality(flow)}
              <div class="row-item implicit" title="星球自动生成的可用资源，严格供给下也免费">
                <HoverIcon
                  type={icon.type}
                  name={icon.name}
                  size={24}
                  detailKind={flowDetailKind(icon)}
                  quality={q ?? undefined}
                  flow={flow}
                />
                <span class="row-name">{flowLabel(flow)}</span>
                <span class="chip">隐式</span>
              </div>
            {/each}
          </div>
        {/if}
        <button
          class="btn"
          onclick={() =>
            (flowSelector = {
              title: "添加外部输入流",
              onSelectFlow: (flow) => runtime.addExternalInputFlow(flow, 1).catch(() => {}),
            })}
          disabled={!runtime.activeContext}
        >+ 添加外部输入</button>
          {/if}
        </section>
      {/if}

      {#if factory}
        <section class="panel">
          <button
            class="title panel-toggle"
            aria-expanded={panels.env}
            onclick={() => (panels.env = !panels.env)}
          >
            <span class="toggle-label">
              <span class="chev">{panels.env ? "▾" : "▸"}</span>
              工厂环境
            </span>
          </button>
          {#if panels.env}
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
              {factory.settings.planet ? runtime.localizedName("planet", factory.settings.planet) : "星球：未选择"}
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
              {factory.settings.surface ? runtime.localizedName("surface", factory.settings.surface) : "地表：未选择"}
            </span>
            {#if factory.settings.surface}
              <button
                class="btn ghost"
                title="清除地表"
                onclick={() => runtime.setFactorySurface(null).catch(() => {})}
              >×</button>
            {/if}
          </div>
          <div class="env-row">
            <button
              class="icon-btn"
              title={`主品质：${runtime.localizedName("quality", factory.settings.major_quality || "normal")}`}
              onclick={() =>
                openSelector("quality", "选择主品质", (name) =>
                  runtime.setFactoryMajorQuality(name),
                )}
            >
              <HoverIcon
                type="quality"
                name={factory.settings.major_quality || "normal"}
                size={24}
                detailKind="quality"
              />
            </button>
            <span class="sub">主品质：{runtime.localizedName("quality", factory.settings.major_quality || "normal")}</span>
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
          {/if}
        </section>
      {/if}

      {#if project}
        <section class="panel project-settings">
          <button
            class="title panel-toggle"
            aria-expanded={panels.settings}
            onclick={() => (panels.settings = !panels.settings)}
          >
            <span class="toggle-label">
              <span class="chev">{panels.settings ? "▾" : "▸"}</span>
              项目设置 <span class="count">全局</span>
            </span>
          </button>
          {#if panels.settings}
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
            <label for="quality-limit">品质上限（超出会自动提升）</label>
            <select
              id="quality-limit"
              value={project.settings.quality_limit ?? ""}
              onchange={(event) => {
                const value = (event.currentTarget as HTMLSelectElement).value;
                runtime.setQualityLimit(value === "" ? null : value).catch(() => {});
              }}
            >
              <option value="">normal（当前上限）</option>
              {#each runtime.catalogIndex?.qualities ?? [] as q (q)}
                <option value={q}>{q}</option>
              {/each}
            </select>
          </div>
          <label class="check">
            <input
              type="checkbox"
              checked={project.settings.all_accessible}
              onchange={(event) =>
                runtime.setAllAccessible((event.currentTarget as HTMLInputElement).checked).catch(() => {})}
            />
            无视可达性（全部可用）
          </label>

          <div class="subpanel">
            <button
              class="title panel-toggle"
              aria-expanded={panels.subMilestones}
              onclick={() => (panels.subMilestones = !panels.subMilestones)}
            >
              <span class="toggle-label">
                <span class="chev">{panels.subMilestones ? "▾" : "▸"}</span>
                里程碑
                <span class="count">{runtime.orderedMilestones?.length ?? project.settings.milestones.length}</span>
              </span>
            </button>
            {#if panels.subMilestones}
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
                      {runtime.localizedName(accessibleKind(milestone.node), accessibleName(milestone.node))}
                    </span>
                    <label class="check" title="解锁/锁定">
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
                      class="btn ghost"
                      title="移除里程碑"
                      onclick={() =>
                        runtime.removeMilestone(milestone.node).catch(() => {})}
                    >×</button>
                  </div>
                {:else}
                  <span class="muted">还没有里程碑</span>
                {/each}
              </div>
              <div class="prefs-row">
                <button
                  class="btn"
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
                <button class="btn" onclick={() => runtime.setDefaultMilestones().catch(() => {})}>
                  设置默认里程碑
                </button>
              </div>
            </div>
            {/if}
          </div>

          <div class="subpanel">
            <button
              class="title panel-toggle"
              aria-expanded={panels.subProductivity}
              onclick={() => (panels.subProductivity = !panels.subProductivity)}
            >
              <span class="toggle-label">
                <span class="chev">{panels.subProductivity ? "▾" : "▸"}</span>
                产能（配方/采矿）
              </span>
            </button>
            {#if panels.subProductivity}
            <div class="field">
              <label class="check">
                <input
                  type="checkbox"
                  checked={project.settings.ignore_productivity}
                  onchange={(event) =>
                    runtime
                      .setIgnoreProductivity((event.currentTarget as HTMLInputElement).checked)
                      .catch(() => {})}
                />
                忽略产能加成
              </label>
            </div>

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
                    <span class="prefs-name">{runtime.localizedName("recipe", entry.recipe)}</span>
                    <input
                      class="prod-input"
                      type="number"
                      min="0"
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
                        class="btn ghost"
                        title="移除"
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
                class="btn"
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
                    <span class="prefs-name">{runtime.localizedName("technology", entry.tech)}</span>
                    <input
                      class="prod-input"
                      type="number"
                      step="1"
                      min={techInfo(entry.tech)?.base ?? 0}
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
                      class="btn ghost"
                      title="移除"
                      onclick={() =>
                        runtime.removeInfiniteTechLevel(entry.tech).catch(() => {})}
                    >×</button>
                  </div>
                {:else}
                  <span class="muted">还没有无限科技等级</span>
                {/each}
              </div>
              <button
                class="btn"
                onclick={() =>
                  openSelector(
                    "technology",
                    "选择无限科技",
                    (name) => {
                      // 默认从最低等级再研究一次（base + 1），有限上限会自动钳制。
                      const info = techInfo(name);
                      runtime.setInfiniteTechLevel(name, (info?.base ?? 0) + 1);
                    },
                    [],
                    [],
                    undefined,
                    undefined,
                    undefined,
                    undefined,
                    true, // 尊重可达性
                    true, // 只显示可多次研究的科技
                  )}
              >+ 添加无限科技</button>
            </div>
            {/if}
          </div>
          <button class="btn" onclick={() => (prefsOpen = true)}>规划偏好…</button>
          {/if}
        </section>
      {/if}
    </aside>

    <!-- 中栏：机制列表 -->
    <section class="col center">
      <div class="toolbar">
        <button class="btn" title="自动规划：迭代添加建议机制直至可解" onclick={() => runtime.autoPlan().catch(() => {})} disabled={runtime.autoPlanning || !factory}>
          {runtime.autoPlanning ? "规划中…" : "自动规划"}
        </button>
        <button class="btn ghost" title="移除求解中用量低于阈值的机制" onclick={() => runtime.cleanup("remove-unused").catch(() => {})} disabled={!solved}>
          移除未用
        </button>
        <button class="btn ghost" title="移除未参与求解的机制" onclick={() => runtime.cleanup("remove-unsolvable").catch(() => {})} disabled={!solved}>
          移除无解
        </button>
        <button class="btn ghost" title="按求解流量从大到小重排机制" onclick={() => runtime.cleanup("sort-by-solution-rate").catch(() => {})} disabled={!solved}>
          按流量排序
        </button>
        <button
          class="btn ghost"
          class:on={mergeRecipes}
          title="配方合并：同配方不同品质合并成一张卡"
          onclick={() => (mergeRecipes = !mergeRecipes)}
        >配方合并</button>
        {#if factory}
          <button
            class="btn"
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
        {#if !runtime.activeContext}
          <span class="muted">先加载游戏数据（左上角「游戏数据」）</span>
        {/if}
        <span class="spacer"></span>
        <span class="chip">{mechanics.length} 机制</span>
        <span class="chip">{targets.length} 目标</span>
        {#if solved}<span class="chip ok">已求解</span>{/if}
        {#if notSolved}<span class="chip warn">未求解</span>{/if}
        {#if runtime.solveError}<span class="chip warn">求解错误</span>{/if}
      </div>

      <div
        class="mech-list"
        use:dndzone={{ items: dragMechanics, flipDurationMs: 120 }}
        onconsider={handleMechanicsConsider}
        onfinalize={handleMechanicsFinalize}
      >
        {#each mechGroups as group (group.key)}
          {#if group.entries.length > 1}
            <!-- 配方合并组：标题（配方名 + 机器名），子机制完整显示 -->
            <div class="mech-group">
              <div class="mech-group-head">
                <span class="mech-group-title" title={groupTitle(group.entries)}>
                  {groupTitle(group.entries)}
                </span>
                {#if groupMachine(group.entries)}
                  <span class="chip">{groupMachine(group.entries)}</span>
                {/if}
                <span class="chip muted">{group.entries.length} 品质</span>
              </div>
              {#each group.entries as entry (entry.id)}
                <div class="mech-group-row">
                  <MechanicCard
                    {entry}
                    project={project?.id ?? 0}
                    factory={factory?.id ?? 0}
                    solution={solveMap.get(entry.id) ?? null}
                    onPick={(kind, a, b) => pickForMechanic(entry.id, kind, a, b)}
                    onToggleEnabled={() => runtime.setMechanicEnabled(entry.id, !entry.enabled).catch(() => {})}
                    onRemove={() => runtime.removeMechanic(entry.id).catch(() => {})}
                    onModuleSlot={(slot, module) => runtime.setModuleSlot(entry.id, slot, module).catch(() => {})}
                    onAddBeacon={() => addBeacon(entry.id)}
                    onPickFuel={() => pickFuel(entry.id)}
                    onClone={() => runtime.cloneMechanic(entry.id).catch(() => {})}
                  />
                </div>
              {/each}
            </div>
          {:else}
            <!-- 单条机制：不嵌套，直接完整卡片 -->
            {#each group.entries as entry (entry.id)}
              <MechanicCard
                {entry}
                project={project?.id ?? 0}
                factory={factory?.id ?? 0}
                solution={solveMap.get(entry.id) ?? null}
                onPick={(kind, a, b) => pickForMechanic(entry.id, kind, a, b)}
                onToggleEnabled={() => runtime.setMechanicEnabled(entry.id, !entry.enabled).catch(() => {})}
                onRemove={() => runtime.removeMechanic(entry.id).catch(() => {})}
                onModuleSlot={(slot, module) => runtime.setModuleSlot(entry.id, slot, module).catch(() => {})}
                onAddBeacon={() => addBeacon(entry.id)}
                onPickFuel={() => pickFuel(entry.id)}
                onClone={() => runtime.cloneMechanic(entry.id).catch(() => {})}
              />
            {/each}
          {/if}
        {:else}
          <div class="empty-state">
            {#if factory}
              <span class="muted">还没有机制，点下方「添加机制」</span>
            {:else}
              <span class="muted">选择或新建一个工厂</span>
            {/if}
          </div>
        {/each}
      </div>
    </section>

    {#if addMechMenuPos}
      <div class="menu-catcher" aria-hidden="true" onclick={() => (addMechMenuPos = null)}></div>
      <div class="menu fixed" style={`top:${addMechMenuPos.top}px;right:${addMechMenuPos.right}px`}>
        {#each mechKinds as option (option.kind)}
          <button
            onclick={() => {
              addMechMenuPos = null;
              runtime.addMechanic(option.kind).catch(() => {});
            }}
          >{option.label}</button>
        {/each}
      </div>
    {/if}

    <!-- 右栏：求解结果 / 数据上下文 -->
    <aside class="col">
      <section class="panel">
        <div class="title">求解结果</div>
        {#if runtime.solveError}
          <div class="err-box prominent" title="最近一次求解/自动规划失败">
            <strong>求解失败</strong>：{runtime.solveError}
          </div>
        {/if}
        {#if solve && "solved" in solve.status}
          {@const status = solve.status.solved}
          <div class="kv">
            <span>总成本</span><strong class="mono">{status.cost.toFixed(3)}</strong>
          </div>
          <div class="subtitle">总流平衡</div>
          <div class="rows compact">
            {#each status.flows.filter((b) => Math.abs(b.amount) / Math.max(b.scale ?? 1, 1e-12) > 1e-9) as balance (balance.flow)}
              {@const icon = flowIcon(balance.flow)}
              {@const q = flowQuality(balance.flow)}
              <div class="row-item">
                <HoverIcon
                  type={icon.type}
                  name={icon.name}
                  size={22}
                  detailKind={flowDetailKind(icon)}
                  quality={q ?? undefined}
                  flow={balance.flow}
                  onClick={() => openSuggestions(balance.flow)}
                />
                <span class="row-name" title={dualVarLabel(balance.flow)}>
                  <span class="flow-name">{flowLabel(balance.flow)}</span>
                  {#if flowLabelSub(balance.flow)}
                    <span class="flow-sub">{flowLabelSub(balance.flow)}</span>
                  {/if}
                </span>
                <strong class:amount-pos={balance.amount > 0} class="mono amount">{formatFlowAmount(balance.flow, balance.amount)}</strong>
                <button class="btn ghost up" title="建议能产出该流的机制" onclick={() => openSuggestions(balance.flow)}>建议</button>
              </div>
            {/each}
          </div>
        {:else if solve && "not-solved" in solve.status}
          {@const status = solve.status["not-solved"]}
          <div class="err-box">
            <div><strong>未求解</strong>：{status.description}</div>
            {#if status.no_provider.length > 0}
              <div>无供给：{status.no_provider.map(dualVarLabel).join(", ")}</div>
            {/if}
            {#if status.no_consumer.length > 0}
              <div>无消耗：{status.no_consumer.map(dualVarLabel).join(", ")}</div>
            {/if}
          </div>
        {:else}
          <div class="empty-hint">改动数据会自动重新求解</div>
        {/if}
      </section>

      <section class="panel">
        <div class="title">游戏上下文 <span class="count">{runtime.contexts.length}</span></div>
        {#if project && project.context_id && !runtime.contexts.some((c) => c.id === project.context_id)}
          <div class="warn-strip">当前项目引用的游戏上下文已删除或不存在，请从下方列表选一个并点「应用」重新绑定。</div>
        {/if}
        {#if runtime.contexts.length === 0}
          <div class="empty-hint">尚未加载任何游戏上下文。请先加载一个（左上角「加载游戏」或「从文件导入 Dump」）以创建。</div>
        {:else}
          <div class="rows compact">
            {#each runtime.contexts as context (context.id)}
              <div
                class="ctx-row"
                class:active={project
                  ? project?.context_id === context.id
                  : runtime.activeContext?.id === context.id}
              >
                <div class="ctx-main">
                  <div class="ctx-name">
                    {context.name}
                    {#if project?.context_id === context.id}<span class="chip ok">项目上下文</span>{/if}
                    {#if !context.loaded}<span class="chip">未载入</span>{/if}
                  </div>
                  <div class="ctx-meta" title={context.source}>{shortSource(context.source)}</div>
                </div>
                <div class="ctx-actions">
                  <button
                    class="btn ghost"
                    title={project
                      ? (project?.context_id === context.id
                          ? "当前项目已用此上下文"
                          : "把选中项目改用此上下文")
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
                  <button
                    class="btn ghost"
                    title="重命名"
                    onclick={() => renameContextPrompt(context)}
                  >改名</button>
                  <button
                    class="btn ghost danger"
                    title="删除缓存"
                    onclick={() =>
                      (confirmState = {
                        message: `删除上下文「${context.name}」的缓存？`,
                        confirmLabel: "删除",
                        danger: true,
                        action: () => runtime.deleteContext(context.id).catch(() => {}),
                      })}
                  >删除</button>
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
      </section>
    </aside>
  </main>
</div>

<!-- ══ 选择器 ══ -->
{#if selector}
  <Selector
    kind={selector.kind}
    title={selector.title}
    kindOptions={selector.kinds}
    categoryFilter={selector.categoryFilter}
    allowedNames={selector.allowedNames}
    initialName={selector.initialName}
    initialQuality={selector.initialQuality}
    onSelect={selector.onSelect}
    onPickKind={selector.onPickKind}
    respectAccessibility={selector.respectAccessibility}
    multiLevelTechOnly={selector.multiLevelTechOnly}
    onClose={() => (selector = null)}
  />
{/if}

{#if flowSelector}
  <Selector
    kind="item"
    title={flowSelector.title}
    flowMode
    initialTab={flowSelector.initialTab}
    initialName={flowSelector.initialName}
    initialQuality={flowSelector.initialQuality}
    allowedNames={flowSelector.allowedNames}
    onSelectFlow={flowSelector.onSelectFlow}
    onClose={() => (flowSelector = null)}
  />
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
              <span class="prefs-name">{runtime.localizedName("machine", pref.id)}</span>
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
          class="btn"
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
              <span class="prefs-name">{runtime.localizedName("module", module.id)}</span>
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
          class="btn"
          onclick={() =>
            openSelector("module", "添加枚举插件", (name, quality) =>
              runtime.addEnumeratedModule(name, quality),
            )}
        >+ 添加插件</button>
        <button
          class="btn"
          title="用每插件类别中最高 tier 的插件（工厂主品质）替换枚举列表"
          onclick={() =>
            runtime
              .applyBestModules(factory?.settings.major_quality || "normal")
              .catch(() => {})}
        >使用最佳插件</button>
      </div>

      <div class="prefs-section">
        <div class="prefs-title">枚举信标（自动规划叠加的信标方案）</div>
        <div class="prefs-list">
          {#each planning.enumerate_beacons as plan, i (i)}
            {@const beacon = plan.module_config?.beacons?.[0]?.beacon}
            {@const beaconConfig = plan.module_config?.beacons?.[0]}
            <div class="prefs-item column">
              <div class="prefs-row">
                <button
                  class="icon-btn"
                  title="选择信标"
                  onclick={() =>
                    openSelector("beacon", "选择枚举信标", (name, quality) =>
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
                    <Icon type="entity" name="beacon" size={24} />
                  {/if}
                </button>
                <span class="prefs-name">{beacon ? runtime.localizedName("beacon", beacon.id) : "未选信标"}</span>
                <button
                  class="btn ghost danger"
                  title="移除"
                  onclick={() => runtime.removeEnumeratedBeacon(i).catch(() => {})}
                >×</button>
              </div>
              {#if beaconConfig}
                <div class="prefs-row sub">
                  <label class="me-num">
                    数量
                    <input
                      type="number"
                      min="1"
                      value={String(beaconConfig.count)}
                      onchange={(event) => {
                        const value = Number((event.currentTarget as HTMLInputElement).value);
                        if (Number.isFinite(value) && value > 0) {
                          runtime
                            .enumeratedBeaconModule(i, { "set-beacon-count": { beacon: 0, count: value } })
                            .catch(() => {});
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
                  <div class="prefs-row wrap">
                    {#each beaconConfig.modules as [module, count], mi (mi)}
                      <span class="prefs-chip">
                        <HoverIcon type="item" name={module.id} size={16} detailKind="module" quality={module.quality} />
                        <input
                          class="chip-num"
                          type="number"
                          min="1"
                          value={String(count)}
                          onchange={(event) => {
                            const value = Number((event.currentTarget as HTMLInputElement).value);
                            if (!Number.isFinite(value) || value < 1) return;
                            void (async () => {
                              const slots =
                                (await beaconModuleSlots(beacon?.id)) * beaconConfig.count;
                              const total =
                                beaconConfig.modules.reduce(
                                  (sum, [, c], index) => sum + (index === mi ? 0 : c),
                                  0,
                                ) + value;
                              if (slots > 0 && total > slots) {
                                showNotice(`信标插件槽位不足（${slots} 个）`);
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
            <span class="muted">还没有枚举信标</span>
          {/each}
        </div>
        <button
          class="btn"
          onclick={() =>
            openSelector("beacon", "添加枚举信标", (name, quality) =>
              runtime.addEnumeratedBeacon({ id: name, quality }),
            )}
        >+ 添加信标</button>
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
      <div class="mini-title">建议：{flowLabel(suggestions.flow)}</div>
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
                      .addSuggestion(candidate)
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
                      .addSuggestion(candidate)
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
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .appbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-right: 6px;
  }

  .brand-mark {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    color: #0f1f19;
    background: var(--accent);
    border-radius: 6px;
    font-size: 11px;
    font-weight: 800;
  }

  .brand-name {
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.14em;
  }

  .spacer {
    flex: 1;
  }

  .menu-wrap {
    position: relative;
  }

  .menu {
    position: absolute;
    z-index: 30;
    top: calc(100% + 6px);
    left: 0;
    min-width: 210px;
    padding: 5px;
    display: grid;
    gap: 2px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  }

  .menu button {
    padding: 7px 9px;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    font-size: 11px;
    cursor: pointer;
  }

  .menu button:hover {
    background: var(--card-hover);
  }

  .add-wrap {
    padding-top: 2px;
  }

  /* fixed 定位的弹出菜单：完全脱离布局，不参与任何滚动区域 */
  .menu-catcher {
    position: fixed;
    z-index: 29;
    inset: 0;
  }

  .menu.fixed {
    position: fixed;
    z-index: 30;
    /* 覆盖基类 .menu 的 left: 0：fixed 定位用 right 锚定，left 必须 auto，
       否则 left:0 + right 会同时生效把面板拉伸到整个视口宽。 */
    left: auto;
    width: max-content;
    min-width: 190px;
    max-width: min(320px, calc(100vw - 16px));
    max-height: min(420px, 60vh);
    overflow-y: auto;
    padding: 5px;
    display: grid;
    gap: 2px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  }

  .err-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    padding: 6px 12px;
    color: var(--danger);
    background: var(--danger-dim);
    border-bottom: 1px solid var(--danger-line);
    font-size: 11px;
  }

  .warn-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 6px 10px;
    margin: 0 12px 8px;
    color: var(--warn);
    background: var(--warn-dim);
    border: 1px solid var(--warn-line);
    border-radius: var(--radius-sm);
    font-size: 11px;
  }

  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 12px 0;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }

  .tabs.sub {
    background: var(--bg);
  }

  .tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    color: var(--muted);
    background: transparent;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    font-size: 11px;
    cursor: pointer;
    max-width: 180px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .tab:hover {
    color: var(--text);
  }

  .tab.active {
    color: var(--text);
    background: var(--bg);
    border-color: var(--line);
  }

  .tab-cluster {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    background: transparent;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    max-width: 200px;
  }

  .tab-cluster.active {
    background: var(--bg);
    border-color: var(--line);
  }

  .tab-cluster .tab {
    max-width: 160px;
    border: none;
    border-radius: 0;
  }

  .tab-cluster .tab:hover {
    background: transparent;
  }

  .tabs:not(.sub) .tab.active {
    background: var(--bg);
  }

  .tab.add {
    color: var(--faint);
    font-size: 14px;
  }

  .tab-x {
    padding: 0 2px;
    color: var(--faint);
    background: transparent;
    border: none;
    font-size: 12px;
    cursor: pointer;
  }

  .tab-x:hover {
    color: var(--danger);
  }

  .workspace {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 330px minmax(420px, 1fr) 300px;
    gap: 0;
    padding: 10px 0;
    overflow: hidden;
  }

  .col {
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    display: grid;
    align-content: start;
    gap: 10px;
    padding: 0 12px;
  }

  /* 分区竖分隔线 */
  .col + .col {
    border-left: 1px solid var(--line);
  }

  .rows {
    display: grid;
    gap: 4px;
    margin-bottom: 8px;
  }

  .rows.compact {
    gap: 2px;
  }

  .row-item {
    min-width: 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    min-height: 32px;
    padding: 3px 6px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .row-item .row-name {
    flex: 1 1 90px;
    display: flex;
    flex-direction: column;
    gap: 0;
    min-width: 0;
  }

  .flow-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .flow-sub {
    color: var(--faint);
    font-size: 9px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-item.implicit {
    border-style: dashed;
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--card) 82%, var(--accent) 6%);
    opacity: 1;
  }

  .implicit-hint {
    margin-top: 6px;
    color: var(--faint);
    font-size: 9px;
    letter-spacing: 0.05em;
  }

  .btn.up {
    padding: 2px 6px;
    line-height: 1;
  }

  .row-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .num {
    width: 68px;
    min-height: 24px;
    padding: 0 6px;
    text-align: right;
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
  }

  .empty-hint {
    padding: 14px 4px;
    color: var(--faint);
    font-size: 11px;
  }

  .center {
    display: flex;
    flex-direction: column;
  }

  .toolbar {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0 8px;
    margin-bottom: 8px;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
  }

  .mech-list {
    display: grid;
    align-content: start;
    gap: 0;
  }

  /* 机制组：由内嵌卡片改为「分组标题 + 底部分隔线」的扁平 section。 */
  .mech-group {
    display: grid;
    gap: 0;
    padding: 0 0 6px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--line);
  }

  .mech-group-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 4px 6px 4px;
  }

  .mech-group-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mech-group-row + .mech-group-row {
    border-top: 1px solid var(--line);
  }

  .mech-group-row .mech-card {
    padding: 6px 8px;
    background: transparent;
    border: none;
    border-radius: 0;
  }

  /* 单条机制（未合并）：同样扁平化为行式，避免浮起卡片。 */
  .mech-list > .mech-card {
    padding: 8px;
    margin: 0;
    background: transparent;
    border: none;
    border-radius: 0;
    border-bottom: 1px solid var(--line);
  }

  .panel.project-settings {
    /* 项目级设置与工厂级生产设置视觉区分：左侧品牌色强调条 */
    border-left: 3px solid var(--accent-line);
    background: color-mix(in srgb, var(--panel) 94%, var(--accent) 4%);
  }

  .empty-state {
    padding: 30px;
    text-align: center;
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius);
  }

  .kv {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    color: var(--muted);
    font-size: 11px;
  }

  .kv strong {
    color: var(--text);
  }

  .kv .small {
    overflow: hidden;
    max-width: 180px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .subtitle {
    margin: 10px 0 5px;
    color: var(--muted);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .amount {
    font-size: 10px;
  }

  .amount-pos {
    color: var(--accent);
  }

  .ctx-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .ctx-row.active {
    border-color: var(--accent-line);
    background: var(--accent-dim);
  }

  .ctx-main {
    min-width: 0;
    flex: 1;
    overflow: hidden;
  }

  .ctx-name {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
    font-size: 11px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-meta {
    min-width: 0;
    overflow: hidden;
    margin-top: 2px;
    color: var(--faint);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctx-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 2px;
    flex: 0 0 auto;
    max-width: 50%;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 10px 0;
    color: var(--text);
    font-size: 11px;
    cursor: pointer;
  }

  .backdrop {
    position: fixed;
    z-index: 50;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(4, 7, 8, 0.72);
  }

  /* 低于选择器（z 40）的弹层：允许从其中再打开选择器 */
  .backdrop.low {
    z-index: 30;
  }

  .prefs-modal {
    width: min(430px, 100%);
    max-height: min(680px, calc(100vh - 48px));
    display: grid;
    gap: 10px;
    padding: 14px;
    overflow-y: auto;
    background: var(--panel);
    border: 1px solid var(--accent-line);
    border-radius: var(--radius);
  }

  .prefs-section {
    display: grid;
    gap: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }

  .prefs-title {
    color: var(--muted);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.05em;
  }

  .prefs-list {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  .prefs-item {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 2px 6px 2px 2px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    min-width: 0;
    overflow: hidden;
  }

  .prefs-name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prefs-item.column {
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    padding: 5px 6px;
  }

  /* 嵌套可折叠子面板（里程碑 / 产能）。 */
  .subpanel {
    margin-top: 10px;
    padding-top: 6px;
    border-top: 1px solid var(--line);
  }

  .subpanel .panel-toggle {
    margin-bottom: 6px;
  }

  /* 推断（自动）产能项：虚线边框（与外部输入面板的隐式行一致）。 */
  .prefs-item.inferred {
    border-style: dashed;
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--card) 84%, var(--accent) 8%);
  }

  /* 用户指定的产能项：实线，辅以强调色表示是自己指定的。 */
  .prefs-item.user-specified {
    border-style: solid;
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--card) 92%, var(--accent) 6%);
  }

  .badge {
    flex: 0 0 auto;
    font-size: 9px;
    letter-spacing: 0.05em;
    color: var(--faint);
    padding: 1px 5px;
    border-radius: calc(var(--radius-sm) - 2px);
    border: 1px solid var(--line);
    white-space: nowrap;
    max-width: 48px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .prefs-row {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .prefs-add-row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 5px;
  }

  /* 左侧栏可折叠面板标题（复用 .title 基类的 flex / 间距）。 */
  .panel-toggle {
    width: 100%;
    border: none;
    background: none;
    padding: 0;
    margin: 0 0 8px;
    cursor: pointer;
    text-align: left;
    font: inherit;
    color: var(--text);
  }

  .panel-toggle:hover {
    color: var(--accent);
  }

  .panel-toggle .count {
    margin-left: auto;
  }

  .toggle-label {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
  }

  .chev {
    color: var(--muted);
    font-size: 10px;
    width: 12px;
    flex: 0 0 auto;
  }

  .prefs-row.sub {
    flex-wrap: wrap;
    gap: 6px;
  }

  .prefs-row.wrap {
    flex-wrap: wrap;
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
    min-height: 22px;
    padding: 0 4px;
    text-align: right;
    background: var(--card);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
  }

  .prefs-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 5px 1px 2px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 10px;
  }

  .prefs-chip-x {
    width: 13px;
    height: 13px;
    display: grid;
    place-items: center;
    padding: 0;
    color: var(--danger);
    background: transparent;
    border: none;
    font-size: 9px;
    line-height: 1;
    cursor: pointer;
  }

  .env-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .expr-card {
    display: grid;
    gap: 6px;
    padding: 7px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .expr-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .expr-const {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
    font-size: 10px;
  }

  .expr-terms {
    display: grid;
    gap: 3px;
  }

  .expr-term {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .prod-input {
    width: 56px;
    min-height: 22px;
    padding: 0 4px;
    text-align: right;
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
  }

  .mini-modal {
    width: min(340px, 100%);
    display: grid;
    gap: 10px;
    padding: 14px;
    background: var(--panel);
    border: 1px solid var(--accent-line);
    border-radius: var(--radius);
  }

  .mini-modal.load-game {
    width: min(460px, 100%);
  }

  .path-field {
    display: grid;
    gap: 4px;
  }

  .path-label {
    font-size: 11px;
    color: var(--muted);
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
    color: var(--danger, #e5484d);
    font-size: 11px;
    line-height: 1.4;
  }

  .mini-title {
    font-size: 12px;
    font-weight: 700;
  }

  .confirm-text {
    color: var(--muted);
    font-size: 11px;
    line-height: 1.5;
  }

  .mini-modal input {
    min-height: 30px;
    padding: 0 9px;
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  .mini-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
</style>
