// Svelte 5 rune-based store bridging the frontend to the Tauri runtime.
//
// `runtime` is a singleton: the UI reads its $state fields reactively and
// calls its methods to send AppMessages.  After every dispatch the store
// refreshes the document + UI snapshots, so the UI never keeps its own copy
// of backend data.  Game context (prototype store + icons) and catalog
// results are cached here as well.

import {
  catalog,
  dispatch,
  getContext,
  getDocument,
  getUiState,
  loadBundledDump,
  loadDump,
  loadGameContext,
  loadIcon,
  onContextError,
  onContextLoaded,
  onSolveError,
  onSolveResult,
  openProjectDialog,
  saveProject,
  saveProjectAsDialog,
} from "./client";
import type {
  AppDocument,
  AppMessage,
  CatalogEntry,
  CatalogKind,
  ContextInfo,
  FactoryId,
  MechanicId,
  MechanicKind,
  ProjectId,
  SolveResult,
  TargetId,
  TimeScale,
  ExternalInputId,
} from "./types";

class RuntimeStore {
  document = $state<AppDocument | null>(null);
  ui = $state<import("./types").UiState | null>(null);
  solve = $state<SolveResult | null>(null);
  solveError = $state<string | null>(null);
  revision = $state(0);
  busy = $state(false);
  solving = $state(false);
  lastError = $state<string | null>(null);
  ready = $state(false);

  context = $state<ContextInfo | null>(null);
  contextBusy = $state(false);
  contextError = $state<string | null>(null);

  /** 图标缓存：`type/name` → blob URL（或 null 表示无图标）。 */
  private icons = new Map<string, Promise<string | null>>();
  /** 目录缓存：`kind|query` → 条目。 */
  private catalogCache = new Map<string, CatalogEntry[]>();
  /** 图标对象 URL 列表，用于卸载时释放。 */
  private iconUrls: string[] = [];

  /** Subscribe to backend events; call once at app start. */
  async init(): Promise<void> {
    onSolveResult((result) => {
      this.solve = result;
      this.solveError = null;
      this.solving = false;
    });
    onSolveError((message) => {
      this.solveError = message;
      this.solving = false;
    });
    onContextLoaded((info) => {
      this.context = info;
      this.contextBusy = false;
      this.contextError = null;
      this.clearIconCache();
    });
    onContextError((message) => {
      this.contextError = message;
      this.contextBusy = false;
    });
    try {
      const [document, ui, context] = await Promise.all([
        getDocument(),
        getUiState(),
        getContext(),
      ]);
      this.document = document;
      this.ui = ui;
      this.context = context;
    } catch (error) {
      this.lastError = String(error);
    }
    this.ready = true;
  }

  async refresh(): Promise<void> {
    const [document, ui] = await Promise.all([getDocument(), getUiState()]);
    this.document = document;
    this.ui = ui;
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
  }

  // ── 游戏上下文 ──────────────────────────────────────────────────

  async loadDemoData(): Promise<void> {
    await this.runContext(async () => {
      this.context = await loadBundledDump();
    });
  }

  async loadContextFromDump(path: string): Promise<void> {
    await this.runContext(async () => {
      this.context = await loadDump(path);
    });
  }

  async loadContextFromExecutable(exe: string, modDir?: string | null): Promise<void> {
    await this.runContext(async () => {
      this.context = await loadGameContext(exe, modDir);
    });
  }

  private async runContext(action: () => Promise<void>): Promise<void> {
    this.contextBusy = true;
    this.contextError = null;
    try {
      await action();
    } catch (error) {
      this.contextError = String(error);
      this.contextBusy = false;
      throw error;
    }
  }

  // ── 文件 ────────────────────────────────────────────────────────

  async openProject(): Promise<boolean> {
    this.busy = true;
    try {
      const document = await openProjectDialog();
      if (document) {
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

  /** 目录搜索，按 (kind, query) 缓存。 */
  async searchCatalog(kind: CatalogKind, query: string): Promise<CatalogEntry[]> {
    const key = `${kind}|${query.trim()}`;
    const cached = this.catalogCache.get(key);
    if (cached) return cached;
    const entries = await catalog(kind, query, 300);
    this.catalogCache.set(key, entries);
    return entries;
  }

  /** 清空目录缓存（换上下文后调用）。 */
  clearCatalogCache(): void {
    this.catalogCache.clear();
  }

  // ── 项目 / 工厂 ─────────────────────────────────────────────────

  async newProject(name: string): Promise<void> {
    await this.send({ scope: "application", action: { "new-project": { name } } });
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

  async selectProject(project: ProjectId | null): Promise<void> {
    await this.send({ scope: "ui", action: { "select-project": { project } } });
  }

  async selectFactory(factory: FactoryId | null): Promise<void> {
    await this.send({ scope: "ui", action: { "select-factory": { factory } } });
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

  // ── 外部输入 ────────────────────────────────────────────────────

  async addExternalInput(itemId: string, penalty: number): Promise<void> {
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: {
          "external-input": {
            add: {
              input: {
                id: 0,
                flow: { Item: { id: itemId, quality: "normal" } },
                penalty,
              },
            },
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

  async setRecipe(mechanic: MechanicId, recipe: string): Promise<void> {
    const recipeId = { id: recipe, quality: "normal" } as import("./types").IdWithQuality;
    await this.mechanicMessage(mechanic, { recipe: { "set-recipe": { recipe: recipeId } } });
  }

  async setMachine(mechanic: MechanicId, machine: string): Promise<void> {
    const machineId = { id: machine, quality: "normal" } as import("./types").IdWithQuality;
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

  async setItem(mechanic: MechanicId, item: string): Promise<void> {
    const itemId = { id: item, quality: "normal" } as import("./types").IdWithQuality;
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

  async setGenerator(mechanic: MechanicId, generator: string): Promise<void> {
    await this.mechanicMessage(mechanic, {
      generator: { "set-generator": { generator: { id: generator, quality: "normal" } } },
    });
  }

  async setBoiler(mechanic: MechanicId, boiler: string): Promise<void> {
    await this.mechanicMessage(mechanic, {
      boiler: { "set-boiler": { boiler: { id: boiler, quality: "normal" } } },
    });
  }

  async setReactor(mechanic: MechanicId, reactor: string): Promise<void> {
    await this.mechanicMessage(mechanic, {
      reactor: { "set-reactor": { reactor: { id: reactor, quality: "normal" } } },
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

  async setModuleSlot(mechanic: MechanicId, slot: number, module: string | null): Promise<void> {
    const moduleId = module ? { id: module, quality: "normal" } : null;
    const inner: import("./types").ModuleAction = { "set-module-slot": { slot, module: moduleId } };
    switch (this.mechanicKind(mechanic)) {
      case "recipe":
        return this.mechanicMessage(mechanic, { recipe: { module: inner } });
      case "mining":
        return this.mechanicMessage(mechanic, { mining: { module: inner } });
      default:
        throw new Error(`${this.mechanicKind(mechanic)} 机制不支持模块配置`);
    }
  }

  async clearModules(mechanic: MechanicId): Promise<void> {
    const inner: import("./types").ModuleAction = { "clear-modules": null };
    switch (this.mechanicKind(mechanic)) {
      case "recipe":
        return this.mechanicMessage(mechanic, { recipe: { module: inner } });
      case "mining":
        return this.mechanicMessage(mechanic, { mining: { module: inner } });
      default:
        throw new Error(`${this.mechanicKind(mechanic)} 机制不支持模块配置`);
    }
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

  async addEnumeratedModule(module: string): Promise<void> {
    await this.planningMessage({
      "add-enumerated-module": { module: { id: module, quality: "normal" } },
    });
  }

  async removeEnumeratedModule(module: string): Promise<void> {
    await this.planningMessage({
      "remove-enumerated-module": { module: { id: module, quality: "normal" } },
    });
  }

  async useBestModules(): Promise<void> {
    await this.planningMessage({ "use-best-modules": null });
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
