// Svelte 5 rune-based store bridging the frontend to the Tauri runtime.
//
// `runtime` is a singleton: the UI reads its $state fields reactively and
// calls its methods to send AppMessages.  After every dispatch the store
// refreshes the document + UI snapshots, so the UI never keeps its own copy
// of backend data.  Game context (prototype store + icons) and catalog
// results are cached here as well.

import {
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
  implicitSources,
} from "./client";
import type {
  AppDocument,
  AppMessage,
  CatalogIndex,
  ContextInfo,
  FactoryId,
  MechanicId,
  MechanicKind,
  ProjectId,
  PrototypeDetail,
  SolveResult,
  TargetId,
  TimeScale,
  ExternalInputId,
} from "./types";

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
  /** 图标对象 URL 列表，用于卸载时释放。 */
  private iconUrls: string[] = [];

  /** 当前上下文的全量目录索引（前端本地筛选）。 */
  catalogIndex = $state<CatalogIndex | null>(null);

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
    } catch (error) {
      this.lastError = String(error);
    }
    this.ready = true;
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
    try {
      const result = await dispatch(message);
      this.revision = result.revision;
      await this.refresh();
    } catch (error) {
      this.lastError = String(error);
      throw error;
    } finally {
      this.busy = false;
    }
    this.refreshImplicitSources().catch(() => {});
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

  /** 拉取上下文列表；激活上下文变化时清空图标/目录缓存。 */
  async refreshContexts(): Promise<void> {
    const list = await listContexts();
    this.contexts = list.contexts;
    this.activeContext = list.contexts.find((entry) => entry.id === list.active) ?? null;
    if (this.activeContextId !== list.active) {
      this.activeContextId = list.active ?? "";
      this.clearIconCache();
      this.clearCatalogCache();
    }
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
  async loadCatalogIndex(): Promise<CatalogIndex | null> {
    if (this.catalogIndex?.context_id === this.activeContextId && this.activeContextId) {
      return this.catalogIndex;
    }
    try {
      this.catalogIndex = await catalogIndex();
      return this.catalogIndex;
    } catch (error) {
      this.lastError = String(error);
      return null;
    }
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

  /** 本地化显示名（无翻译/未加载索引时回退内部 id）。 */
  localizedName(kind: string, name: string): string {
    const entry = this.catalogIndex?.entries.find(
      (candidate) => candidate.kind === kind && candidate.name === name,
    );
    return entry?.localized_name || name;
  }

  /** 换上下文后清空索引/详情缓存。 */
  clearCatalogCache(): void {
    this.catalogIndex = null;
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

  async selectProject(project: ProjectId | null): Promise<void> {
    await this.send({ scope: "ui", action: { "select-project": { project } } });
    this.restoreSolveForSelection();
  }

  async selectFactory(factory: FactoryId | null): Promise<void> {
    await this.send({ scope: "ui", action: { "select-factory": { factory } } });
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

  // ── 科技里程碑 / 配方产能 ────────────────────────────────────────

  async addTechnologyMilestone(technology: string): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: {
        project,
        action: { "add-technology-milestone": { milestone: { technology, unlocked: true } } },
      },
    });
  }

  async setTechnologyUnlocked(technology: string, unlocked: boolean): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-technology-unlocked": { technology, unlocked } } },
    });
  }

  async removeTechnologyMilestone(technology: string): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "remove-technology-milestone": { technology } } },
    });
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
  async setFuel(mechanic: MechanicId, fuel: string | null): Promise<void> {
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

  /** 锅炉工作模式；null = 使用锅炉原型自带模式（缺省 heat-fluid-inside）。 */
  async setBoilerMode(
    mechanic: MechanicId,
    mode: "heat-fluid-inside" | "output-to-separate-pipe" | null,
  ): Promise<void> {
    await this.mechanicMessage(mechanic, { boiler: { "set-mode": { mode } } });
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

  async addMachinePreference(machine: string): Promise<void> {
    await this.planningMessage({
      "add-machine-preference": { machine: { id: machine, quality: "normal" } },
    });
  }

  async removeMachinePreference(machine: string): Promise<void> {
    await this.planningMessage({
      "remove-machine-preference": { machine: { id: machine, quality: "normal" } },
    });
  }

  async addEnumeratedModule(module: string, quality = "normal"): Promise<void> {
    await this.planningMessage({
      "add-enumerated-module": { module: { id: module, quality } },
    });
  }

  async removeEnumeratedModule(module: string): Promise<void> {
    await this.planningMessage({
      "remove-enumerated-module": { module: { id: module, quality: "normal" } },
    });
  }

  /** 使用最佳插件：用每插件类别中最高 tier 的插件（指定品质）替换枚举列表。 */
  async applyBestModules(quality: string): Promise<void> {
    const { bestModules } = await import("./client");
    const modules = await bestModules();
    const existing = [...(this.selectedProject?.planning.enumerate_modules ?? [])];
    for (const module of existing) {
      await this.removeEnumeratedModule(module.id);
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
