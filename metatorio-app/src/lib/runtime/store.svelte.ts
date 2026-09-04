// Svelte 5 rune-based store bridging the frontend to the Tauri runtime.
//
// `runtime` is a singleton: the UI reads its $state fields reactively and
// calls its methods to send AppMessages.  After every dispatch the store
// refreshes the document + UI snapshots, so the UI never keeps its own copy
// of backend data.  Game context (prototype store + icons) and catalog
// results are cached here as well.

import {
  accessibility,
  catalogIndex,
  deleteContext,
  dispatch,
  getDocument,
  getUiState,
  listContexts,
  loadBundledDump,
  loadDump,
  loadGameContext,
  loadIcon,
  onContextError,
  onContextsChanged,
  onSolveError,
  onSolveResult,
  openProjectDialog,
  prototypeDetail,
  projectSavePath,
  renameContext,
  saveProject,
  saveProjectAsDialog,
  setActiveContext,
  setDefaultMilestones,
  milestonesOrdered,
  productivity,
  implicitSources,
  mechanicFlow,
} from "./client";
import type {
  Accessible,
  AppDocument,
  AppMessage,
  CatalogIndex,
  ContextInfo,
  FactoryId,
  IdWithQuality,
  InfiniteTechLevel,
  MechanicId,
  MechanicKind,
  Milestone,
  ProductivityView,
  ProjectId,
  PrototypeDetail,
  SolveResult,
  TargetId,
  TimeScale,
  ExternalInputId,
} from "./types";

// 会修改里程碑集合/可达性的项目动作（重拉有序里程碑用）。与后端
// message_affects_accessibility 对齐。
const MILESTONE_KEYS = new Set([
  "add-milestone",
  "set-milestone-unlocked",
  "remove-milestone",
  "set-all-accessible",
  "set-context",
]);
// 会修改配方/采矿产能（自动推算 + 用户覆盖）的项目动作。
const PRODUCTIVITY_KEYS = new Set([
  "set-all-accessible",
  "set-context",
  "set-mining-productivity",
  "set-ignore-productivity",
  "set-recipe-productivity",
  "remove-recipe-productivity",
  "set-infinite-tech-level",
  "remove-infinite-tech-level",
]);

class RuntimeStore {
  document = $state<AppDocument | null>(null);
  ui = $state<import("./types").UiState | null>(null);
  /** 项目 id → 保存路径（记忆路径，未保存过无条目）。 */
  projectPaths = $state<Map<ProjectId, string>>(new Map());
  solve = $state<SolveResult | null>(null);
  solveError = $state<string | null>(null);
  /** 求解结果缓存：`${project}:${factory}` → 结果，切换项目/工厂时恢复。 */
  private solveCache = new Map<string, SolveResult>();
  revision = $state(0);
  busy = $state(false);
  solving = $state(false);
  lastError = $state<string | null>(null);
  ready = $state(false);

  contexts = $state<ContextInfo[]>([]);
  activeContext = $state<ContextInfo | null>(null);
  contextBusy = $state(false);
  contextError = $state<string | null>(null);

  /** 图标缓存：`type/name` → blob URL（或 null 表示无图标）。 */
  private icons = new Map<string, Promise<string | null>>();
  /** 上次刷新时的激活上下文 id（用于判断索引/图标缓存是否失效）。 */
  private activeContextId = "";
  /** 悬停详情缓存：`kind/name` → 详情。 */
  private detailCache = new Map<string, PrototypeDetail>();
  /** 机制展开流缓存：`contextId:project:factory:mechanicId` → {机制内容hash, 流}。
   *  每次 dispatch 后 refresh() 会用全新对象引用刷新机制列表，导致每个
   *  MechanicCard 都重新拉取展开流。按机制**内容哈希**缓存，内容不变时不重发
   *  IPC（py 大厂几百机制时，这是主线程卡顿/滞后的主要来源之一）。 */
  private mechanicFlowCache = new Map<
    string,
    { hash: string; flows: { flow: import("./types").DualVar; amount: number }[] }
  >();
  /** 图标对象 URL 列表，用于卸载时释放。 */
  private iconUrls: string[] = [];

  /** 当前上下文的全量目录索引（前端本地筛选）。 */
  catalogIndex = $state<CatalogIndex | null>(null);

  /**
   * 当前选中项目的可达性快照（选择器过滤用）。
   * `null` = 未拉取/不可用（未绑定上下文等）；任意 dispatch 后失效，
   * 打开选择器时经 `ensureAccessibility()` 惰性重拉。
   */
  accessibility = $state<Accessible[] | null>(null);

  /** Subscribe to backend events; call once at app start. */
  async init(): Promise<void> {
    onSolveResult((result) => {
      this.solve = result;
      this.solveError = null;
      this.solving = false;
      // 缓存到当前工厂：切换回来时直接恢复，不重复求解。
      const [project, factory] = this.currentFactoryKey();
      if (project != null && factory != null) {
        this.solveCache.set(`${project}:${factory}`, result);
      }
    });
    onSolveError((message) => {
      this.solveError = message;
      this.solving = false;
    });
    onContextsChanged(() => {
      this.contextBusy = false;
      this.contextError = null;
      this.refreshContexts().catch(() => {});
    });
    onContextError((message) => {
      this.contextError = message;
      this.contextBusy = false;
    });
    try {
      const [document, ui] = await Promise.all([getDocument(), getUiState()]);
      this.document = document;
      this.ui = ui;
      await this.refreshContexts();
      // 及早拉取目录索引（含本地化显示名），保证产能/机制等面板在
      // 首个渲染就能用上译名，而不是回退到内部 id。
      await this.loadCatalogIndex();
    } catch (error) {
      this.lastError = String(error);
    }
    this.ready = true;
    this.refreshOrderedMilestones().catch(() => {});
  }

  async refresh(): Promise<void> {
    const [document, ui] = await Promise.all([getDocument(), getUiState()]);
    this.document = document;
    this.ui = ui;
    this.refreshProjectPaths().catch(() => {});
  }

  /** Send one AppMessage to the Rust runtime and refresh the snapshot. */
  async send(message: AppMessage): Promise<void> {
    this.busy = true;
    this.lastError = null;
    const prevProject = this.ui?.selected_project;
    try {
      const result = await dispatch(message);
      this.revision = result.revision;
      // 任何交互都可能改变可达性（显式标记/里程碑/无视开关/换上下文），
      // 整体失效缓存；选择器打开时按需重拉。
      this.accessibility = null;
      await this.refresh();
    } catch (error) {
      this.lastError = String(error);
      throw error;
    } finally {
      this.busy = false;
    }
    // 只有会改动里程碑/产能的消息才重拉它们（py 下 milestonesOrdered 是
    // 重 BFS ~1.8s，不应在每次无关键交互后都跑）；换选项目也要重拉。
    const keys = RuntimeStore.innerActionKeys(message);
    const domain =
      keys.some((k) => MILESTONE_KEYS.has(k)) || prevProject !== this.ui?.selected_project;
    if (domain) this.refreshOrderedMilestones().catch(() => {});
    if (
      keys.some((k) => PRODUCTIVITY_KEYS.has(k)) ||
      keys.some((k) => MILESTONE_KEYS.has(k)) ||
      prevProject !== this.ui?.selected_project
    ) {
      this.refreshProductivity().catch(() => {});
    }
    this.refreshImplicitSources().catch(() => {});
  }

  /** 提取 AppMessage 内层动作键（scope=project/factory 时在 action.action，
   *  scope=application 时直接是 action）。用于判断是否需要重拉里程碑/产能。 */
  private static innerActionKeys(message: AppMessage): string[] {
    const action = (message as { action?: unknown }).action as
      | { action?: Record<string, unknown> }
      | Record<string, unknown>
      | undefined;
    if (!action) return [];
    const inner = (action as { action?: Record<string, unknown> }).action ?? action;
    return Object.keys(inner);
  }

  // ── 可达性（选择器过滤） ────────────────────────────────────────

  /**
   * 确保当前选中项目的可达性快照已拉取；返回可达对象集合。
   * 无选中项目 / 未绑定上下文 / 拉取失败时返回 `null`（表示"不可用，
   * 不做可达性过滤"），避免把全部条目标记为不可达而误滤。
   */
  async ensureAccessibility(): Promise<Accessible[] | null> {
    if (this.accessibility != null) return this.accessibility;
    const project = this.ui?.selected_project;
    if (project == null) return null;
    try {
      const nodes = await accessibility(project);
      this.accessibility = nodes;
      return nodes;
    } catch {
      this.accessibility = null;
      return null;
    }
  }

  /** 选中项目是否"无视可达性"（全可达）。 */
  get allAccessible(): boolean {
    return this.selectedProject?.settings.all_accessible ?? false;
  }

  // ── 星球隐式可用输入（外部输入面板的虚线行） ────────────────────

  /** 当前工厂的星球隐式输入（被外部输入覆盖的已剔除）。 */
  implicitSourcesCache = $state<import("./types").DualVar[]>([]);

  async refreshImplicitSources(): Promise<void> {
    const factory = this.ui?.selected_factory;
    if (factory == null) {
      this.implicitSourcesCache = [];
      return;
    }
    try {
      this.implicitSourcesCache = await implicitSources(factory);
    } catch {
      this.implicitSourcesCache = [];
    }
  }

  // ── 游戏上下文 ──────────────────────────────────────────────────

  /** 拉取上下文列表；激活上下文变化时清空图标/目录缓存并重拉目录索引。 */
  async refreshContexts(): Promise<void> {
    const list = await listContexts();
    this.contexts = list.contexts;
    this.activeContext = list.contexts.find((entry) => entry.id === list.active) ?? null;
    if (this.activeContextId !== list.active) {
      this.activeContextId = list.active ?? "";
      this.clearIconCache();
      this.clearCatalogCache();
      this.clearMechanicFlowCache();
    }
    // 激活上下文就绪后立即加载目录索引（含本地化显示名），
    // 使产能/机制等面板的译名尽早生效。
    this.loadCatalogIndex().catch(() => {});
  }
  async loadDemoData(): Promise<void> {
    await this.runContext(async () => {
      await loadBundledDump();
      await this.refreshContexts();
    });
  }

  async loadContextFromDump(path: string): Promise<void> {
    await this.runContext(async () => {
      await loadDump(path);
      await this.refreshContexts();
    });
  }

  async loadContextFromExecutable(exe: string, modDir?: string | null): Promise<void> {
    await this.runContext(async () => {
      await loadGameContext(exe, modDir);
      await this.refreshContexts();
    });
  }

  async setActiveContext(id: string | null): Promise<void> {
    await this.runContext(async () => {
      await setActiveContext(id);
      await this.refreshContexts();
    });
  }

  async renameContext(id: string, name: string): Promise<void> {
    await this.runContext(async () => {
      await renameContext(id, name);
      await this.refreshContexts();
    });
  }

  async deleteContext(id: string): Promise<void> {
    await this.runContext(async () => {
      await deleteContext(id);
      await this.refreshContexts();
    });
  }

  /** 把项目绑定到某个上下文；null = 跟随激活上下文。 */
  async setProjectContext(project: ProjectId, context: string | null): Promise<void> {
    await this.send({
      scope: "project",
      action: { project, action: { "set-context": { context } } },
    });
  }

  private async runContext(action: () => Promise<void>): Promise<void> {
    this.contextBusy = true;
    this.contextError = null;
    try {
      await action();
    } catch (error) {
      this.contextError = String(error);
      throw error;
    } finally {
      // 成功路径不能依赖后端事件兜底复位：事件可能被错过，
      // 否则 UI 会永久卡在“正在加载数据…”。
      this.contextBusy = false;
    }
  }

  // ── 文件 ────────────────────────────────────────────────────────

  async openProject(): Promise<boolean> {
    this.busy = true;
    try {
      const document = await openProjectDialog();
      if (document) {
        // 后端已把文件项目导入当前文档；这里用返回值整体刷新界面。
        this.document = document;
        const ui = await getUiState();
        this.ui = ui;
        return true;
      }
      return false;
    } catch (error) {
      this.lastError = String(error);
      throw error;
    } finally {
      this.busy = false;
    }
  }

  async saveCurrentProject(): Promise<boolean> {
    this.busy = true;
    try {
      const path = await saveProject();
      if (path != null) return true;
      return await this.saveProjectAs();
    } catch (error) {
      this.lastError = String(error);
      throw error;
    } finally {
      this.busy = false;
    }
  }

  async saveProjectAs(): Promise<boolean> {
    this.busy = true;
    try {
      const path = await saveProjectAsDialog();
      return path != null;
    } catch (error) {
      this.lastError = String(error);
      throw error;
    } finally {
      this.busy = false;
    }
  }

  // ── 图标与目录 ──────────────────────────────────────────────────

  /** 取物品图标（blob URL）；无图标时返回 null。带缓存。 */
  getIcon(type: string, name: string): Promise<string | null> {
    const key = `${type}/${name}`;
    let entry = this.icons.get(key);
    if (!entry) {
      entry = loadIcon(type, name).then((bytes) => {
        if (!bytes || bytes.length === 0) return null;
        const blob = new Blob([new Uint8Array(bytes)], { type: "image/png" });
        const url = URL.createObjectURL(blob);
        this.iconUrls.push(url);
        return url;
      });
      this.icons.set(key, entry);
    }
    return entry;
  }

  private clearIconCache(): void {
    for (const url of this.iconUrls) URL.revokeObjectURL(url);
    this.iconUrls = [];
    this.icons.clear();
  }

  /** 目录索引：一次拉取当前上下文的全量目录（前端本地筛选/分组）。 */
  private catalogLoadPromise: Promise<CatalogIndex | null> | null = null;

  async loadCatalogIndex(): Promise<CatalogIndex | null> {
    if (this.catalogIndex?.context_id === this.activeContextId && this.activeContextId) {
      return this.catalogIndex;
    }
    // 合并并发加载（init 的 await 与上下文切换的 fire-and-forget 可能同时触发）。
    if (this.catalogLoadPromise) return this.catalogLoadPromise;
    this.catalogLoadPromise = (async () => {
      try {
        this.catalogIndex = await catalogIndex();
        return this.catalogIndex;
      } catch (error) {
        this.lastError = String(error);
        return null;
      } finally {
        this.catalogLoadPromise = null;
      }
    })();
    return this.catalogLoadPromise;
  }

  /** 悬停详情：按 (kind, name) 缓存。 */
  async getDetail(kind: string, name: string): Promise<PrototypeDetail | null> {
    const key = `${kind}/${name}`;
    const cached = this.detailCache.get(key);
    if (cached) return cached;
    try {
      const detail = await prototypeDetail(kind, name);
      if (detail) this.detailCache.set(key, detail);
      return detail;
    } catch {
      return null;
    }
  }

  /**
   * 单个机制的展开流，带内容哈希缓存。`mechanic` 为机制配置对象（按当前
   * 内容判断是否需要重拉）。上下文/项目/工厂/机制 id 与**配置内容**都参与
   * 缓存键：配置不变（仅引用刷新）时直接复用，避免每次 dispatch 后所有
   * MechanicCard 重发 `mechanicFlow` IPC。
   */
  async getMechanicFlow(
    project: number,
    factory: number,
    mechanicId: number,
    mechanic: unknown,
  ): Promise<{ flow: import("./types").DualVar; amount: number }[]> {
    const key = `${this.activeContextId || ""}:${project}:${factory}:${mechanicId}`;
    const hash = JSON.stringify(mechanic);
    const cached = this.mechanicFlowCache.get(key);
    if (cached && cached.hash === hash) return cached.flows;
    try {
      const flows = await mechanicFlow(project, factory, mechanicId);
      const result = flows.map(([flow, amount]) => ({ flow, amount }));
      this.mechanicFlowCache.set(key, { hash, flows: result });
      return result;
    } catch {
      return [];
    }
  }

  /** 换上下文/项目后清空机制流缓存（同一 id 在不同上下文含义可能不同）。
   *  具体缓存按 (项目, 工厂, id) 隔离，故仅在幂等上有益。 */
  clearMechanicFlowCache(): void {
    this.mechanicFlowCache.clear();
  }

  /** 本地化显示名（无翻译/未加载索引时回退内部 id）。O(1) Map 查找，避免大
   * 目录下每次重渲染对 entries 做 O(N) 线性 .find。 */
  private localizedNameMap = new Map<string, string>();
  private localizedNameContext = "";

  private ensureLocalizedNameMap(): void {
    const ci = this.catalogIndex;
    if (!ci) {
      this.localizedNameMap.clear();
      this.localizedNameContext = "";
      return;
    }
    // 目录/上下文变化时重建一次；按 context_id 判断是否需要重建。
    const key = ci.context_id;
    if (key === this.localizedNameContext) return;
    const map = new Map<string, string>();
    for (const entry of ci.entries) {
      map.set(`${entry.kind}/${entry.name}`, entry.localized_name);
    }
    this.localizedNameMap = map;
    this.localizedNameContext = key;
  }

  localizedName(kind: string, name: string): string {
    this.ensureLocalizedNameMap();
    const localized = this.localizedNameMap.get(`${kind}/${name}`);
    return localized || name;
  }

  /** 换上下文后清空索引/详情缓存。 */
  clearCatalogCache(): void {
    this.catalogIndex = null;
    this.localizedNameMap.clear();
    this.localizedNameContext = "";
    this.detailCache.clear();
  }

  // ── 项目 / 工厂 ─────────────────────────────────────────────────

  async newProject(name: string): Promise<void> {
    await this.send({ scope: "application", action: { "new-project": { name } } });
  }

  /** 关闭项目：decision = "discard"（不保存关闭）| "save"（先保存再关闭）。 */
  async closeProject(decision: "discard" | "save" = "discard"): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "application",
      action: { "close-project": { project, decision } },
    });
  }

  /** 项目保存位置（记忆路径；null = 尚未保存过）。 */
  projectSavePath(project: ProjectId): string | null {
    return this.projectPaths.get(project) ?? null;
  }

  /** 刷新全部项目的保存路径缓存（保存/导入后调用）。 */
  async refreshProjectPaths(): Promise<void> {
    const projects = this.document?.projects ?? [];
    const entries = await Promise.all(
      projects.map(async (project) => [project.id, await projectSavePath(project.id)] as const),
    );
    this.projectPaths = new Map(
      entries.filter((entry): entry is readonly [ProjectId, string] => entry[1] != null),
    );
  }

  async addFactory(name: string): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "add-factory": { name, template: "empty" } } },
    });
  }

  async removeFactory(factory: FactoryId): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "remove-factory": { factory } } },
    });
  }

  /** 重命名项目。 */
  async setProjectName(name: string): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-name": { name } } },
    });
  }

  /** 重命名工厂。 */
  async setFactoryName(name: string): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "set-name": { name } } },
    });
  }

  /** 轻量 UI 动作：只同步后端的选择态（供 effective_context_id / 上下文解析），
   *  不刷新整份 document、不置 busy。前端选择态直接本地设置，省去 getDocument 回程。 */
  private async uiDispatch(message: AppMessage): Promise<void> {
    const result = await dispatch(message);
    this.revision = result.revision;
  }

  async selectProject(project: ProjectId | null): Promise<void> {
    if (!this.ui) this.ui = { selected_project: null, selected_factory: null, selected_mechanic: null };
    this.ui = { ...this.ui, selected_project: project, selected_factory: null, selected_mechanic: null };
    this.accessibility = null;
    // 后端选择态保持同步（图标/上下文解析用），但无需整份刷新。
    void this.uiDispatch({ scope: "ui", action: { "select-project": { project } } });
    this.restoreSolveForSelection();
  }

  async selectFactory(factory: FactoryId | null): Promise<void> {
    if (!this.ui) this.ui = { selected_project: null, selected_factory: null, selected_mechanic: null };
    this.ui = { ...this.ui, selected_factory: factory, selected_mechanic: null };
    // 后端选择态保持同步；无需整份刷新。
    void this.uiDispatch({ scope: "ui", action: { "select-factory": { factory } } });
    this.restoreSolveForSelection();
  }

  /** 当前选中 (project, factory) 键；无选中返回 (null, null)。 */
  private currentFactoryKey(): [ProjectId | null, FactoryId | null] {
    const project = this.ui?.selected_project ?? null;
    const factory = this.ui?.selected_factory ?? null;
    return [project, factory];
  }

  /** 切换后恢复该工厂的缓存求解结果；无缓存则自动重新求解。 */
  private async restoreSolveForSelection(): Promise<void> {
    const [project, factory] = this.currentFactoryKey();
    if (project == null || factory == null) {
      this.solve = null;
      return;
    }
    const cached = this.solveCache.get(`${project}:${factory}`);
    if (cached) {
      this.solve = cached;
      this.solveError = null;
    } else {
      this.solve = null;
      this.recompute().catch(() => {});
    }
  }

  // ── 项目设置 ────────────────────────────────────────────────────

  async setTimeScale(time_scale: TimeScale): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-time-scale": { time_scale } } },
    });
  }

  async setAllAccessible(enabled: boolean): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-all-accessible": { enabled } } },
    });
  }

  async setQualityLimit(quality: string | null): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-quality-limit": { quality } } },
    });
  }

  async setMiningProductivity(productivity: number): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-mining-productivity": { productivity } } },
    });
  }

  // ── 里程碑 / 配方产能 ────────────────────────────────────────────

  async addMilestone(node: Accessible, unlocked = true): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "add-milestone": { node, unlocked } } },
    });
  }

  async setMilestoneUnlocked(node: Accessible, unlocked: boolean): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-milestone-unlocked": { node, unlocked } } },
    });
  }

  async removeMilestone(node: Accessible): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "remove-milestone": { node } } },
    });
  }

  /** 把里程碑重置为默认（实验室输入的科技瓶物品，全部解锁）。 */
  async setDefaultMilestones(): Promise<void> {
    const project = this.requireProject();
    await setDefaultMilestones(project);
    // 走的是 Tauri 命令（不经过 send），需手动失效可达性缓存并刷新
    // 有序里程碑（否则里程碑列表不立刻更新，要手动添加一次才刷新）。
    this.accessibility = null;
    await this.refresh();
    this.refreshOrderedMilestones().catch(() => {});
    this.refreshProductivity().catch(() => {});
  }

  /** 里程碑节点按依赖拓扑排序（依赖在前）的当前结果；供里程碑列表按序展示。 */
  orderedMilestones = $state<Milestone[] | null>(null);
  async refreshOrderedMilestones(): Promise<void> {
    const project = this.ui?.selected_project;
    if (project == null) {
      this.orderedMilestones = null;
      return;
    }
    try {
      this.orderedMilestones = await milestonesOrdered(project);
    } catch {
      this.orderedMilestones = null;
    }
  }

  /** 产能视图（自动 + 用户覆盖），供项目设置面板按来源区分展示。 */
  productivityInfo = $state<ProductivityView | null>(null);
  async refreshProductivity(): Promise<void> {
    const project = this.ui?.selected_project;
    if (project == null) {
      this.productivityInfo = null;
      return;
    }
    try {
      this.productivityInfo = await productivity(project);
    } catch {
      this.productivityInfo = null;
    }
  }

  async setIgnoreProductivity(ignore: boolean): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-ignore-productivity": { ignore } } },
    });
  }

  async setRecipeProductivity(recipe: string, productivity: number): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: {
        project,
        action: { "set-recipe-productivity": { productivity: { recipe, productivity } } },
      },
    });
  }

  async removeRecipeProductivity(recipe: string): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "remove-recipe-productivity": { recipe } } },
    });
  }

  async setInfiniteTechLevel(tech: string, level: number): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-infinite-tech-level": { level: { tech, level } } } },
    });
  }

  async removeInfiniteTechLevel(tech: string): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "remove-infinite-tech-level": { tech } } },
    });
  }

  // ── 工厂环境（星球/地表/主品质） ────────────────────────────────

  async setFactoryPlanet(planet: string | null): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { context: { "set-planet": { planet } } } },
    });
  }

  async setFactorySurface(surface: string | null): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { context: { "set-surface": { surface } } } },
    });
  }

  async setFactoryMajorQuality(quality: string): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { context: { "set-major-quality": { quality } } } },
    });
  }

  async setStrictSource(strict: boolean): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "set-strict-source": { strict } } },
    });
  }

  async setStrictSink(strict: boolean): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "set-strict-sink": { strict } } },
    });
  }

  /** 克隆机制（追加到列表末尾）。 */
  async cloneMechanic(mechanic: MechanicId): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "mechanic-list": { clone: { mechanic } } } },
    });
  }

  /** 求解后清理：移除未用/无解机制，或按流量重排。 */
  async cleanup(action: import("./types").CleanupAction): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { cleanup: action } },
    });
  }

  /** 采纳建议：新增对应机制并设置主项（机器由兼容回退自动推断）。 */
  async addSuggestion(candidate: { kind: string; name: string }): Promise<void> {
    const kindMap: Record<string, import("./types").MechanicKind> = {
      recipe: "recipe",
      resource: "mining",
      "item-fuel": "item-fuel",
      generator: "generator",
    };
    const kind = kindMap[candidate.kind];
    if (!kind) throw new Error(`未知建议类型 ${candidate.kind}`);
    await this.addMechanic(kind);
    const id = this.selectedFactory?.mechanics.at(-1)?.id;
    if (id == null) return;
    if (candidate.kind === "recipe") await this.setRecipe(id, candidate.name);
    else if (candidate.kind === "resource") await this.setResource(id, candidate.name);
    else if (candidate.kind === "item-fuel") await this.setItem(id, candidate.name);
    else if (candidate.kind === "generator") await this.setGenerator(id, candidate.name);
  }

  // ── 目标表达式（常数 + 线性项求和） ──────────────────────────────

  async addTargetExpression(): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: {
          "target-expression": {
            add: { expression: { id: 0, constant: 1, terms: [] } },
          },
        },
      },
    });
  }

  async removeTargetExpression(expression: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "target-expression": { remove: { expression } } },
      },
    });
  }

  async setTargetExpressionConstant(expression: number, constant: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "target-expression": { "set-constant": { expression, constant } } },
      },
    });
  }

  async addTargetExpressionTerm(expression: number, flow: import("./types").DualVar): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: {
          "target-expression": {
            "add-term": { expression, term: { id: 0, flow, coefficient: 1 } },
          },
        },
      },
    });
  }

  async removeTargetExpressionTerm(expression: number, term: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "target-expression": { "remove-term": { expression, term } } },
      },
    });
  }

  async setTargetExpressionTermCoefficient(
    expression: number,
    term: number,
    coefficient: number,
  ): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "target-expression": { "set-term-coefficient": { expression, term, coefficient } } },
      },
    });
  }

  async setTargetExpressionTermFlow(
    expression: number,
    term: number,
    flow: import("./types").DualVar,
  ): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "target-expression": { "set-term-flow": { expression, term, flow } } },
      },
    });
  }

  /** 克隆工厂（整份追加为新工厂）。 */
  async cloneFactory(factory: FactoryId): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "clone-factory": { factory } } },
    });
  }

  // ── 目标 ────────────────────────────────────────────────────────

  async addTarget(itemId: string, amount: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: {
          flow: {
            "add-to-target": { flow: { Item: { id: itemId, quality: "normal" } }, amount },
          },
        },
      },
    });
  }

  async addTargetFlow(flow: import("./types").DualVar, amount: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { flow: { "add-to-target": { flow, amount } } } },
    });
  }

  async setTargetAmount(target: TargetId, amount: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { target: { "set-amount": { target, amount } } } },
    });
  }

  async removeTarget(target: TargetId): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { target: { remove: { target } } } },
    });
  }

  /** 更改已有目标的流（不改变数量）。 */
  async setTargetFlow(target: TargetId, flow: import("./types").DualVar): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { target: { "set-flow": { target, flow } } } },
    });
  }

  /** 目标排序（position 为目标位置）。 */
  async reorderTarget(target: TargetId, position: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { target: { reorder: { target, position } } } },
    });
  }

  /** 拖拽后的整表重排（从末尾往前逐个移动到目标位置，保证稳定）。 */
  async reorderTargets(order: TargetId[]): Promise<void> {
    for (let i = order.length - 1; i >= 0; i--) {
      const current = this.selectedFactory?.targets ?? [];
      const currentIndex = current.findIndex((target) => target.id === order[i]);
      if (currentIndex >= 0 && currentIndex !== i) {
        await this.reorderTarget(order[i], i);
      }
    }
  }

  // ── 外部输入 ────────────────────────────────────────────────────

  async addExternalInput(itemId: string, penalty: number): Promise<void> {
    await this.addExternalInputFlow({ Item: { id: itemId, quality: "normal" } }, penalty);
  }

  /** 外部输入支持任意 DualVar 流（物品/流体/实体/电/热/火箭运力…）。 */
  async addExternalInputFlow(flow: import("./types").DualVar, penalty: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: {
          "external-input": {
            add: { input: { id: 0, flow, penalty } },
          },
        },
      },
    });
  }

  async removeExternalInput(input: ExternalInputId): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "external-input": { remove: { input } } },
      },
    });
  }

  /** 更改已有外部输入的流（不改变惩罚系数）。 */
  async setExternalInputFlow(
    input: ExternalInputId,
    flow: import("./types").DualVar,
  ): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "external-input": { "set-flow": { input, flow } } },
      },
    });
  }

  /** 外部输入排序（position 为目标位置）。 */
  async reorderExternalInput(input: ExternalInputId, position: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "external-input": { reorder: { input, position } } },
      },
    });
  }

  /** 拖拽后的整表重排（从末尾往前逐个移动到目标位置，保证稳定）。 */
  async reorderExternalInputs(order: ExternalInputId[]): Promise<void> {
    for (let i = order.length - 1; i >= 0; i--) {
      const current = this.selectedFactory?.external_inputs ?? [];
      const currentIndex = current.findIndex((input) => input.id === order[i]);
      if (currentIndex >= 0 && currentIndex !== i) {
        await this.reorderExternalInput(order[i], i);
      }
    }
  }

  async setExternalInputPenalty(input: ExternalInputId, penalty: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "external-input": { "set-penalty": { input, penalty } } },
      },
    });
  }

  // ── 机制 ────────────────────────────────────────────────────────

  async addMechanic(kind: MechanicKind): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "mechanic-list": { add: { kind } } } },
    });
  }

  async removeMechanic(mechanic: MechanicId): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "mechanic-list": { remove: { mechanic } } } },
    });
  }

  /** 机制排序（position 为目标位置）。 */
  async reorderMechanic(mechanic: MechanicId, position: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "mechanic-list": { reorder: { mechanic, position } } } },
    });
  }

  /** 拖拽后的整表重排（从末尾往前逐个移动到目标位置，保证稳定）。 */
  async reorderMechanics(order: MechanicId[]): Promise<void> {
    for (let i = order.length - 1; i >= 0; i--) {
      const current = this.selectedFactory?.mechanics ?? [];
      const currentIndex = current.findIndex((entry) => entry.id === order[i]);
      if (currentIndex >= 0 && currentIndex !== i) {
        await this.reorderMechanic(order[i], i);
      }
    }
  }

  async setMechanicEnabled(mechanic: MechanicId, enabled: boolean): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "mechanic-list": { "set-enabled": { mechanic, enabled } } },
      },
    });
  }

  async mechanicMessage(mechanic: MechanicId, action: import("./types").MechanicAction): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { mechanic: { mechanic, action } } },
    });
  }

  /** 当前机制的类型（来自文档快照）；用于构造按类型标签的 action。 */
  mechanicKind(mechanic: MechanicId): import("./types").MechanicKind {
    const kind = this.selectedFactory?.mechanics.find((entry) => entry.id === mechanic)?.mechanic
      .type;
    if (!kind) throw new Error(`机制 #${mechanic} 不存在`);
    return kind;
  }

  async setRecipe(mechanic: MechanicId, recipe: string, quality = "normal"): Promise<void> {
    const recipeId = { id: recipe, quality } as import("./types").IdWithQuality;
    await this.mechanicMessage(mechanic, { recipe: { "set-recipe": { recipe: recipeId } } });
  }

  async setMachine(mechanic: MechanicId, machine: string, quality = "normal"): Promise<void> {
    const machineId = { id: machine, quality } as import("./types").IdWithQuality;
    switch (this.mechanicKind(mechanic)) {
      case "recipe":
        return this.mechanicMessage(mechanic, { recipe: { "set-machine": { machine: machineId } } });
      case "mining":
        return this.mechanicMessage(mechanic, { mining: { "set-machine": { machine: machineId } } });
      default:
        throw new Error(`${this.mechanicKind(mechanic)} 机制不支持设置机器`);
    }
  }

  async setResource(mechanic: MechanicId, resource: string): Promise<void> {
    await this.mechanicMessage(mechanic, { mining: { "set-resource": { resource } } });
  }

  async setItem(mechanic: MechanicId, item: string, quality = "normal"): Promise<void> {
    const itemId = { id: item, quality } as import("./types").IdWithQuality;
    switch (this.mechanicKind(mechanic)) {
      case "spoil":
        return this.mechanicMessage(mechanic, { spoil: { "set-item": { item: itemId } } });
      case "item-fuel":
        return this.mechanicMessage(mechanic, { "item-fuel": { "set-item": { item: itemId } } });
      case "item-launch":
        return this.mechanicMessage(mechanic, { "item-launch": { "set-item": { item: itemId } } });
      case "plant":
        return this.mechanicMessage(mechanic, { plant: { "set-seed": { seed: itemId } } });
      default:
        throw new Error(`${this.mechanicKind(mechanic)} 机制不支持设置物品`);
    }
  }

  async setGenerator(mechanic: MechanicId, generator: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      generator: { "set-generator": { generator: { id: generator, quality } } },
    });
  }

  async setBoiler(mechanic: MechanicId, boiler: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      boiler: { "set-boiler": { boiler: { id: boiler, quality } } },
    });
  }

  /** 流体燃料机制：选择热值流体。 */
  async setFluidFuel(mechanic: MechanicId, fluid: string): Promise<void> {
    await this.mechanicMessage(mechanic, { "fluid-fuel": { "set-fluid": { fluid } } });
  }

  /** 流体热机制：选择提热流体。 */
  async setFluidHeat(mechanic: MechanicId, fluid: string): Promise<void> {
    await this.mechanicMessage(mechanic, { "fluid-heat": { "set-fluid": { fluid } } });
  }

  /** 设置流体类机制的温度（流体燃料/流体热/发电机/锅炉）。 */
  async setMechanicTemperature(mechanic: MechanicId, temperature: number | null): Promise<void> {
    const kind = this.mechanicKind(mechanic);
    if (kind === "fluid-fuel") {
      return this.mechanicMessage(mechanic, {
        "fluid-fuel": { "set-temperature": { temperature } },
      });
    }
    if (kind === "fluid-heat") {
      return this.mechanicMessage(mechanic, {
        "fluid-heat": { "set-temperature": { temperature } },
      });
    }
    if (kind === "generator") {
      return this.mechanicMessage(mechanic, { generator: { "set-temperature": { temperature } } });
    }
    if (kind === "boiler") {
      return this.mechanicMessage(mechanic, { boiler: { "set-temperature": { temperature } } });
    }
    throw new Error(`${kind} 机制不支持设置温度`);
  }

  /** 指定机制燃料（配方/采矿/锅炉/反应堆）；null = 自动（燃料类别抽象）。 */
  async setFuel(mechanic: MechanicId, fuel: import("./types").Fuel | null): Promise<void> {
    const kind = this.mechanicKind(mechanic);
    if (kind === "recipe") {
      return this.mechanicMessage(mechanic, { recipe: { "set-fuel": { fuel } } });
    }
    if (kind === "mining") {
      return this.mechanicMessage(mechanic, { mining: { "set-fuel": { fuel } } });
    }
    if (kind === "boiler") {
      return this.mechanicMessage(mechanic, { boiler: { "set-fuel": { fuel } } });
    }
    if (kind === "reactor") {
      return this.mechanicMessage(mechanic, { reactor: { "set-fuel": { fuel } } });
    }
    throw new Error(`${kind} 机制不支持指定燃料`);
  }

  /** 指定机制燃料温度（配方/采矿/锅炉）；null = 默认温度。 */
  async setFuelTemperature(mechanic: MechanicId, temperature: number | null): Promise<void> {
    const kind = this.mechanicKind(mechanic);
    if (kind === "recipe") {
      return this.mechanicMessage(mechanic, { recipe: { "set-fuel-temperature": { temperature } } });
    }
    if (kind === "mining") {
      return this.mechanicMessage(mechanic, { mining: { "set-fuel-temperature": { temperature } } });
    }
    if (kind === "boiler") {
      return this.mechanicMessage(mechanic, { boiler: { "set-fuel-temperature": { temperature } } });
    }
    throw new Error(`${kind} 机制不支持设置燃料温度`);
  }

  /** 反应堆相邻数（0-8）。 */
  async setNeighbours(mechanic: MechanicId, neighbours: number): Promise<void> {
    await this.mechanicMessage(mechanic, { reactor: { "set-neighbours": { neighbours } } });
  }

  /** 火箭发射重量模式（true = 按重量；false = 按堆叠槽位）。 */
  async setWeightMode(mechanic: MechanicId, weight_mode: boolean): Promise<void> {
    await this.mechanicMessage(mechanic, { "item-launch": { "set-weight-mode": { weight_mode } } });
  }

  async setReactor(mechanic: MechanicId, reactor: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      reactor: { "set-reactor": { reactor: { id: reactor, quality } } },
    });
  }

  async setSolarPanel(mechanic: MechanicId, solarPanel: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      solar: { "set-solar-panel": { solar_panel: { id: solarPanel, quality } } },
    });
  }

  async setAccumulator(mechanic: MechanicId, accumulator: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      solar: { "set-accumulator": { accumulator: { id: accumulator, quality } } },
    });
  }

  async setFluid(mechanic: MechanicId, fluid: string): Promise<void> {
    switch (this.mechanicKind(mechanic)) {
      case "generator":
        return this.mechanicMessage(mechanic, { generator: { "set-fluid": { fluid } } });
      case "boiler":
        return this.mechanicMessage(mechanic, { boiler: { "set-fluid": { fluid } } });
      default:
        throw new Error(`${this.mechanicKind(mechanic)} 机制不支持设置流体`);
    }
  }

  /** 模块配置消息（按机制类型包装 recipe/mining）。 */
  async moduleMessage(mechanic: MechanicId, action: import("./types").ModuleAction): Promise<void> {
    const kind = this.mechanicKind(mechanic);
    switch (kind) {
      case "recipe":
        return this.mechanicMessage(mechanic, { recipe: { module: action } });
      case "mining":
        return this.mechanicMessage(mechanic, { mining: { module: action } });
      default:
        throw new Error(`${kind} 机制不支持模块配置`);
    }
  }

  async setModuleSlot(
    mechanic: MechanicId,
    slot: number,
    module: string | null,
    quality = "normal",
  ): Promise<void> {
    const moduleId = module ? { id: module, quality } : null;
    await this.moduleMessage(mechanic, {
      "set-module-slot": { slot, module: moduleId },
    });
  }

  async clearModules(mechanic: MechanicId): Promise<void> {
    await this.moduleMessage(mechanic, "clear-modules");
  }

  // ── 自动规划偏好（项目级全局） ──────────────────────────────────

  async planningMessage(action: import("./types").PlanningAction): Promise<void> {
    const project = this.requireProject();
    await this.send({ scope: "project", action: { project, action: { planning: action } } });
  }

  async setAlternativeCount(count: number): Promise<void> {
    await this.planningMessage({ "set-alternative-count": { count } });
  }

  async addMachinePreference(machine: IdWithQuality): Promise<void> {
    await this.planningMessage({
      "add-machine-preference": { machine: { id: machine.id, quality: machine.quality } },
    });
  }

  async removeMachinePreference(machine: IdWithQuality): Promise<void> {
    // 必须传完整品质：runtime 按 IdWithQuality 精确匹配删除
    // （历史 bug：硬编码 normal 导致带品质的机器偏好删不掉）。
    await this.planningMessage({
      "remove-machine-preference": { machine: { id: machine.id, quality: machine.quality } },
    });
  }

  async addEnumeratedModule(module: string, quality = "normal"): Promise<void> {
    await this.planningMessage({
      "add-enumerated-module": { module: { id: module, quality } },
    });
  }

  async removeEnumeratedModule(module: IdWithQuality): Promise<void> {
    // 必须传完整品质：runtime 按 IdWithQuality 精确匹配删除。
    // 历史 bug：硬编码 quality="normal" 导致带品质的插件（如 applyBestModules
    // 用主品质添加的 legendary）永远删不掉。
    await this.planningMessage({
      "remove-enumerated-module": { module: { id: module.id, quality: module.quality } },
    });
  }

  /** 使用最佳插件：用每插件类别中最高 tier 的插件（指定品质）替换枚举列表。 */
  async applyBestModules(quality: string): Promise<void> {
    const { bestModules } = await import("./client");
    const modules = await bestModules();
    const existing = [...(this.selectedProject?.planning.enumerate_modules ?? [])];
    for (const module of existing) {
      await this.removeEnumeratedModule(module);
    }
    for (const module of modules) {
      await this.addEnumeratedModule(module.name, quality);
    }
  }

  /** 添加枚举信标方案：先加空方案，再把所选信标写入新方案的插件配置。 */
  async addEnumeratedBeacon(beacon: { id: string; quality: string }): Promise<void> {
    await this.planningMessage("add-enumerated-beacon");
    const index = (this.selectedProject?.planning.enumerate_beacons.length ?? 1) - 1;
    await this.planningMessage({
      "enumerated-beacon-module": { beacon: index, action: { "add-beacon": { beacon } } },
    });
  }

  async removeEnumeratedBeacon(index: number): Promise<void> {
    await this.planningMessage({ "remove-enumerated-beacon": { beacon: index } });
  }

  /** 编辑枚举信标方案的插件配置（信标数量/共享/塔内插件等，完整 ModuleAction）。 */
  async enumeratedBeaconModule(
    index: number,
    action: import("./types").ModuleAction,
  ): Promise<void> {
    await this.planningMessage({
      "enumerated-beacon-module": { beacon: index, action },
    });
  }

  async useBestModules(): Promise<void> {
    await this.planningMessage("use-best-modules");
  }

  async recompute(): Promise<void> {
    const { project, factory } = this.requireFactory();
    this.solving = true;
    try {
      await this.send({
        scope: "factory",
        action: { project, factory, action: { solve: "recompute" } },
      });
    } catch (error) {
      this.solving = false;
      // 求解失败：写入 solveError 让求解面板醒目显示
      // （后端失败会发 solve-error 事件覆盖；dispatch 校验失败走这里）。
      this.solveError = String(error);
      throw error;
    }
  }

  /** 自动规划：迭代添加建议机制直至可解（后端 AutoPlan）。 */
  async autoPlan(): Promise<void> {
    const { project, factory } = this.requireFactory();
    this.solving = true;
    try {
      await this.send({
        scope: "factory",
        action: { project, factory, action: { solve: "auto-plan" } },
      });
    } catch (error) {
      this.solving = false;
      // 自动规划失败：同时写入 solveError，让求解面板醒目显示
      // （后端失败会发 solve-error 事件覆盖；dispatch 校验失败走这里）。
      this.solveError = String(error);
      throw error;
    }
  }

  // ── 派生访问 ────────────────────────────────────────────────────

  get selectedProject() {
    return (
      this.document?.projects.find((project) => project.id === this.ui?.selected_project) ?? null
    );
  }

  get selectedFactory() {
    const project = this.selectedProject;
    return (
      project?.factories.find((factory) => factory.id === this.ui?.selected_factory) ?? null
    );
  }

  private requireProject(): ProjectId {
    const project = this.ui?.selected_project;
    if (project == null) throw new Error("没有选中的项目");
    return project;
  }

  private requireFactory(): { project: ProjectId; factory: FactoryId } {
    const project = this.requireProject();
    const factory = this.ui?.selected_factory;
    if (factory == null) throw new Error("没有选中的工厂");
    return { project, factory };
  }
}

export const runtime = new RuntimeStore();