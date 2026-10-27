import {onUniverseSolveResult,onUniverseSolveError} from './client';
import {universeSolveSignatures} from '$lib/mechanics/universe-signature';
import {factoryImplicitSignature, factorySolveSignatures} from '$lib/mechanics/factory-signature';
import { universeView } from "$lib/mechanics/located";
import { universePreview, type UniversePreview } from "./client";
import { MechanicCommands, type MechanicScope } from "./mechanic-commands";
import { IconCache } from "./icon-cache";
import { richTextPlain } from "$lib/rich-text";
import { lookupName } from "$lib/display-labels";
import { MechanicFlowCache } from "$lib/mechanic-filter";
import { diagnosticFrom } from "$lib/diagnostic";
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
  dispatch,
  getDocument,
  historyState,
  listContexts,
  loadBundledDump,
  loadDump,
  loadGameContext,
  loadIcon,
  onContextError,
  onCommandError,
  onContextsChanged,
  onDocumentChanged,
  onSolveError,
  onSolveResult,
  onAutoPlanProgress,
  pickProjectFile,
  openProjectFile,
  onProjectOpenRequested,
  takeProjectOpenRequests,
  pickProjectSavePath,
  prototypeDetail,
  projectSavePath,
  milestonesOrdered,
  productivity,
  implicitSources,
  mechanicFlow,
  solveUniverse,
  autoPlanUniverse,
} from "./client";
import type {
  Accessible,
  AutoPlanProgress,
  AppDocument,
  AppMessage,
  CatalogIndex,
  ContextInfo,
  DispatchResult,
  FactoryId,
  UniverseId,
  UniverseDocument,
  UniverseSolveResult,
  UniverseAction,
  LocatedDualVar,
  Location,
  HistoryFocus,
  HistoryStatus,
  IdWithQuality,
  InfiniteTechLevel,
  LpSolver,
  MechanicId,
  MechanicKind,
  Milestone,
  ProductivityView,
  ProjectId,
  ProjectDocument,
  PrototypeDetail,
  SolveResult,
  RuntimeDiagnostic,
  TargetId,
  TargetExpressionId,
  TargetTermId,
  DualVar,
  TimeScale,
  ExternalInputId,
} from "./types";

function projectReadKeys(project: ProjectDocument | null) {
  const settings = project?.settings;
  const domain = JSON.stringify([project?.id, project?.context_id, settings?.all_accessible, settings?.milestones, settings?.quality_limit]);
  return {domain, productivity: JSON.stringify([domain, settings?.mining_productivity, settings?.ignore_productivity, settings?.recipe_productivity, settings?.infinite_levels])};
}

const UI_FONT_FALLBACK = `"Inter", "IBM Plex Sans", "Segoe UI", system-ui, -apple-system, sans-serif`;
const MONO_FONT_FALLBACK = `"JetBrains Mono", "SFMono-Regular", Consolas, "Cascadia Mono", monospace`;
const GENERIC_FONT_FAMILIES = new Set([
  "serif",
  "sans-serif",
  "monospace",
  "cursive",
  "fantasy",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
  "math",
  "emoji",
  "fangsong",
]);

/** 把用户输入的字体族转成安全的 CSS 字体栈。允许直接粘贴完整 stack（含逗号）。 */
function fontStack(name: string, fallback: string): string {
  const raw = name.replace(/[;{}]/g, "").trim();
  if (!raw) return "";
  const family = raw.includes(",")
    ? raw
    : GENERIC_FONT_FAMILIES.has(raw.toLowerCase())
      ? raw
      : `"${raw.replace(/"/g, "")}"`;
  return `${family}, ${fallback}`;
}

class RuntimeStore {
  document = $state<AppDocument | null>(null);
  // 前端持有 UI 选择态（后端不再存储任何选择）。选中 id 为 0 时表示未选；
  // 用独立的 *Id 字段承担 id，`selectedProject`/`selectedFactory` getter 仍
  // 返回文档对象，便于模板直接读取。
  selectedProjectId = $state<ProjectId | null>(null);
  selectedFactoryId = $state<FactoryId | null>(null);
  selectedUniverseId = $state<UniverseId | null>(null);
  private previousFactoryId: FactoryId | null = null;
  selectedMechanic = $state<MechanicId | null>(null);
  universeSolve = $state<UniverseSolveResult | null>(null);
  universeSolveError = $state<string | null>(null);
  universeSolveDiagnostic=$state<RuntimeDiagnostic|null>(null);
  private universeSolveCache = new Map<string, UniverseSolveResult>();
  private universeSolveGeneration = 0;
  /** 项目 id → 保存路径（记忆路径，未保存过无条目）。 */
  projectPaths = $state<Map<ProjectId, string>>(new Map());
  solve = $state<SolveResult | null>(null);
  solveError = $state<string | null>(null);
  solveDiagnostic = $state<RuntimeDiagnostic | null>(null);
  /** 求解结果缓存：`${project}:${factory}` → 结果，切换项目/工厂时恢复。 */
  private solveCache = new Map<string, SolveResult>();
  private factoryInputRevisions = new Map<string, number>();
  private universeInputRevisions = new Map<string, number>();
  private refreshGeneration = 0;
  private refreshTask: Promise<void> | null = null;
  private pendingSends = 0;
  private implicitGeneration = 0;
  private implicitScope = '';
  private milestonesGeneration = 0;
  private productivityGeneration = 0;
  private accessibilityGeneration = 0;
  private historyGeneration = 0;
  private pathsGeneration = 0;
  revision = $state(0);
  busy = $state(false);
  openingProjectPath = $state<string | null>(null);
  fileOpenNotice = $state<string | null>(null);
  private fileOpenQueue: Promise<unknown> = Promise.resolve();
  private nativeOpenTask: Promise<void> | null = null;
  private nativeOpenPending = false;
  /** 正在求解/自动规划的 (project, factory)；null = 空闲。按工厂区分，避免
   *  切换工厂时误判"当前在算"而锁死按钮。 */
  solving = $state<{ project: ProjectId; factory: FactoryId } | null>(null);
  /** 自动规划是否正在进行。它与全局 `busy`（任何交互都会短暂置位）解耦，
   *  只在真正执行自动规划时置为 true，避免后台自动重算把「自动规划」按钮误禁用。 */
  private autoPlanningLocally = $state(false);
  private autoPlanProgressByFactory = $state<Record<string, AutoPlanProgress>>({});
  private latestAutoPlanRuns = new Map<string, { id: number; finished: boolean }>();

  get autoPlanProgress(): AutoPlanProgress | null {
    return this.autoPlanProgressByFactory[`${this.selectedUniverseId!=null?"universe:":""}${this.selectedProjectId}:${this.selectedUniverseId??this.selectedFactoryId}`] ?? null;
  }

  get autoPlanning(): boolean {
    return this.autoPlanningLocally || this.autoPlanProgress !== null;
  }
  lastError = $state<string | null>(null);
  lastDiagnostic = $state<RuntimeDiagnostic | null>(null);

  private setOperationError(error: unknown): void {
    this.lastDiagnostic = diagnosticFrom(error);
    this.lastError = this.lastDiagnostic.message;
  }
  ready = $state(false);
  /** 撤销/重做可用性（启动时 history_state 初始化，之后每次 dispatch 回执同步）。 */
  history = $state<HistoryStatus>({
    can_undo: false,
    can_redo: false,
    undo_label: null,
    redo_label: null,
  });

  /** 主题模式：system(跟随系统) / light / dark。不会写死，用户可选。 */
  theme = $state<"system" | "light" | "dark">("system");
  private themeMedia: MediaQueryList | null = null;
  private static readonly THEME_KEY = "metatorio-theme";
  /** 依主题模式解析实际亮/暗，并写入 <html data-theme>。 */
  private applyTheme(): void {
    const resolved =
      this.theme === "system"
        ? typeof window !== "undefined" &&
          window.matchMedia("(prefers-color-scheme: light)").matches
          ? "light"
          : "dark"
        : this.theme;
    document.documentElement.dataset.theme = resolved;
  }
  /** 启动时读回存储的主题并监听系统偏好变化（system 模式下实时跟随）。 */
  initTheme(): void {
    const saved = localStorage.getItem(RuntimeStore.THEME_KEY) as
      | "system"
      | "light"
      | "dark"
      | null;
    this.theme = saved === "light" || saved === "dark" || saved === "system" ? saved : "system";
    if (typeof window === "undefined") return;
    this.themeMedia = window.matchMedia("(prefers-color-scheme: light)");
    this.themeMedia.addEventListener("change", () => {
      if (this.theme === "system") this.applyTheme();
    });
    this.applyTheme();
  }
  setTheme(mode: "system" | "light" | "dark"): void {
    this.theme = mode;
    localStorage.setItem(RuntimeStore.THEME_KEY, mode);
    this.applyTheme();
  }

  /** 界面 / 代码字体：纯前端偏好，保存在 localStorage，不进入项目文档或后端。 */
  uiFont = $state("");
  monoFont = $state("");
  private static readonly UI_FONT_KEY = "metatorio-ui-font";
  private static readonly MONO_FONT_KEY = "metatorio-mono-font";

  private applyFonts(): void {
    if (typeof document === "undefined") return;
    const root = document.documentElement.style;
    const ui = fontStack(this.uiFont, UI_FONT_FALLBACK);
    const mono = fontStack(this.monoFont, MONO_FONT_FALLBACK);
    if (ui) root.setProperty("--sans", ui);
    else root.removeProperty("--sans");
    if (mono) root.setProperty("--mono", mono);
    else root.removeProperty("--mono");
  }

  /** 启动时读回本地字体设置；空字符串表示使用 app.css 的默认字体。 */
  initFonts(): void {
    this.uiFont = localStorage.getItem(RuntimeStore.UI_FONT_KEY) ?? "";
    this.monoFont = localStorage.getItem(RuntimeStore.MONO_FONT_KEY) ?? "";
    this.applyFonts();
  }

  setUiFont(name: string): void {
    this.uiFont = name.replace(/[;{}]/g, "").trim();
    if (this.uiFont) localStorage.setItem(RuntimeStore.UI_FONT_KEY, this.uiFont);
    else localStorage.removeItem(RuntimeStore.UI_FONT_KEY);
    this.applyFonts();
  }

  setMonoFont(name: string): void {
    this.monoFont = name.replace(/[;{}]/g, "").trim();
    if (this.monoFont) localStorage.setItem(RuntimeStore.MONO_FONT_KEY, this.monoFont);
    else localStorage.removeItem(RuntimeStore.MONO_FONT_KEY);
    this.applyFonts();
  }

  resetFonts(): void {
    this.uiFont = "";
    this.monoFont = "";
    localStorage.removeItem(RuntimeStore.UI_FONT_KEY);
    localStorage.removeItem(RuntimeStore.MONO_FONT_KEY);
    this.applyFonts();
  }

  contexts = $state<ContextInfo[]>([]);
  activeContext = $state<ContextInfo | null>(null);
  contextBusy = $state(false);
  contextError = $state<string | null>(null);

  /** 图标缓存：`type/name` → blob URL（或 null 表示无图标）。 */
  private icons = new IconCache(loadIcon);
  /** 图标缓存失效版本：缓存清空/目录加载后自增，驱动 <Icon> 组件重新拉取，
   *  修复"总流平衡等面板偶发首字母占位"（瞬时 null 被缓存后不再刷新）。 */
  iconRevision = $state(0);
  /** 上次刷新时的激活上下文 id（用于判断索引/图标缓存是否失效）。 */
  private activeContextId = "";
  /** 上次同步过的"项目有效上下文"id：项目上下文切换后用它驱动目录/上下文列表
   *  刷新，使后端 store 载入后"未载入"徽标消失。 */
  private syncedContextId = "";
  /** 悬停详情缓存：`kind/name` → 详情。 */
  private detailCache = new Map<string, PrototypeDetail>();
  private detailRequests = new Map<string, Promise<PrototypeDetail | null>>();
  private detailGeneration = 0;
  /** 机制展开流缓存：上下文/项目/工厂/机制身份 + 配置/项目设置/环境签名。
   *  每次 dispatch 后 refresh() 会用全新对象引用刷新机制列表，导致每个
   *  MechanicCard 都重新拉取展开流。按机制**内容哈希**缓存，内容不变时不重发
   *  IPC；卡片和筛选共享已完成及进行中的请求，失败可重试。 */
  private mechanicFlowCache = new MechanicFlowCache();

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
    await onProjectOpenRequested(() => { if (this.ready) void this.consumeProjectOpenRequests(); });
    await onAutoPlanProgress((progress) => {
      const key = `${progress.universe!=null?"universe:":""}${progress.project}:${progress.universe??progress.factory}`;
      const latest = this.latestAutoPlanRuns.get(key);
      if (latest && (progress.run_id < latest.id ||
          (progress.run_id === latest.id && latest.finished))) return;
      this.latestAutoPlanRuns.set(key, { id: progress.run_id, finished: !progress.running });
      if (progress.running) this.autoPlanProgressByFactory[key] = progress;
      else delete this.autoPlanProgressByFactory[key];
    });
    onUniverseSolveResult(async result=>{
      const key = `${result.project}:${result.universe}`;
      if(result.revision!=null && result.revision<(this.universeInputRevisions.get(key)??0))return;
      this.revision=Math.max(this.revision,result.revision??0);
      await this.refresh();
      if(result.revision!=null && result.revision<(this.universeInputRevisions.get(key)??0))return;
      this.universeSolveCache.set(`${result.project}:${result.universe}`,result);
      if(this.selectedProjectId===result.project&&this.selectedUniverseId===result.universe){this.universeSolve=result;this.universeSolveError=null;this.universeSolveDiagnostic=null;this.universeSolving=false;}
    });
    onUniverseSolveError(error=>{if(this.selectedProjectId===error.project&&this.selectedUniverseId===error.universe&&(error.revision==null||error.revision>=(this.universeInputRevisions.get(`${error.project}:${error.universe}`)??0))){this.universeSolveDiagnostic=diagnosticFrom(error.message);this.universeSolveError=this.universeSolveDiagnostic.message;this.universeSolving=false;}});
    onSolveResult(async (result) => {
      const key = `${result.project}:${result.factory}`;
      if (result.revision != null && result.revision < (this.factoryInputRevisions.get(key) ?? 0)) return;
      await this.refresh();
      if (result.revision != null && result.revision < (this.factoryInputRevisions.get(key) ?? 0)) return;
      this.solveCache.set(key, result);
      if (this.selectedUniverseId == null && this.selectedProjectId === result.project && this.selectedFactoryId === result.factory) {
        this.solve = result;
        this.solveError = null;
        this.solveDiagnostic = null;
      }
      if (this.solving?.project === result.project && this.solving?.factory === result.factory) this.solving = null;
    });
    onSolveError((message) => {
      if (typeof message === 'object' && message !== null && 'project' in message) {
        const scoped = message as RuntimeDiagnostic & {project: number; factory: number; revision?: number};
        if (scoped.project !== this.selectedProjectId || scoped.factory !== this.selectedFactoryId || this.selectedUniverseId != null || (scoped.revision != null && scoped.revision < (this.factoryInputRevisions.get(`${scoped.project}:${scoped.factory}`)??0))) return;
      }
      this.solveDiagnostic = diagnosticFrom(message);
      this.solveError = this.solveDiagnostic.message;
      this.solving = null;
    });
    onContextsChanged(() => {
      this.clearMechanicFlowCache();
      this.clearIconCache();
      this.contextBusy = false;
      this.contextError = null;
      this.refreshContexts().then(()=>{if(this.selectedUniverseId!=null)void this.solveSelectedUniverse();}).catch(() => {});
    });
    onContextError((message) => {
      this.contextError = message;
      this.contextBusy = false;
    });
    // 外部（MCP dispatch）改动文档后，拉取最新快照，实现「外部 agent + 用户」
    // 在同一个界面上实时并存协同。
    onDocumentChanged((revision) => {
      if(typeof revision==='number')this.revision=Math.max(this.revision,revision);
      this.refresh().catch(() => {});
      // 外部（MCP）也可能撤销/重做，同步历史可用性。
      this.refreshHistory().catch(() => {});
    });
    // 命令阶段失败（自动保存写盘失败等）：没有专属事件，统一进「操作」错误条，
    // 否则失败只写 stderr，界面上完全看不出来。
    onCommandError((errors) => {
      const diagnostics = errors.map(diagnosticFrom);
      this.setOperationError(diagnostics.length === 1 ? diagnostics[0] : {code: "command-failed", message: diagnostics.map(error => error.message).join("；"),
        no_provider: diagnostics.flatMap(error => error.no_provider), no_consumer: diagnostics.flatMap(error => error.no_consumer), flows: diagnostics.flatMap(error => error.flows)});
    });
    try {
      this.document = await getDocument();
      // 前端持有选择态：初始选中第一个项目（如有）+ 其第一个工厂。
      const firstProject = this.document?.projects[0] ?? null;
      this.selectedProjectId = firstProject?.id ?? null;
      this.selectedFactoryId = firstProject?.factories[0]?.id ?? null;
      this.selectedMechanic = null;
      await this.refreshHistory();
      await this.refreshContexts();
      // 及早拉取目录索引（含本地化显示名），保证产能/机制等面板在
      // 首个渲染就能用上译名，而不是回退到内部 id。
      await this.loadCatalogIndex();
    } catch (error) {
      this.setOperationError(error);
    }
    this.ready = true;
    this.refreshOrderedMilestones().catch(() => {});
    // 首屏也要拉取与项目/工厂相关、但只在 dispatch 后刷新的派生数据；
    // 否则重新打开应用后「环境自带」外部输入与产能视图会直到第一次编辑才出现。
    this.refreshProductivity().catch(() => {});
    this.refreshImplicitSources().catch(() => {});
    // 首屏目录/上下文刷新后同步一次：目录索引会触发后端载入 store，
    // 再刷新上下文列表以清除「未载入」徽标（初始化时序里列表先于 store 载入）。
    await this.syncEffectiveContext();
    await this.consumeProjectOpenRequests();
  }

  refresh(): Promise<void> {
    this.refreshGeneration++;
    if (!this.refreshTask) {
      this.refreshTask = (async () => {
        try {
          let generation: number;
          do {
            generation = this.refreshGeneration;
            await this.refreshSnapshot(generation);
          } while (generation !== this.refreshGeneration);
        } finally {
          this.refreshTask = null;
        }
      })();
    }
    return this.refreshTask;
  }

  private async refreshSnapshot(generation: number): Promise<void> {
    const document = await getDocument();
    if (generation !== this.refreshGeneration) return;
    // Events and dispatch completion can request the same snapshot. Preserve
    // component identity/focus instead of redrawing an unchanged document.
    if (JSON.stringify(document) === JSON.stringify(this.document)) return;
    const beforeFactory = factorySolveSignatures(this.document);
    const beforeImplicit = factoryImplicitSignature(this.document, this.selectedProjectId, this.selectedFactoryId);
    const beforeReads = projectReadKeys(this.selectedProject);
    const beforeUniverse = universeSolveSignatures(this.document);
    const previousProject = this.selectedProjectId;
    const previousFactory = this.selectedFactoryId;
    this.document = document;
    const afterFactory = factorySolveSignatures(this.document);
    for (const key of new Set([...beforeFactory.keys(), ...afterFactory.keys()])) if (beforeFactory.get(key) !== afterFactory.get(key)) {
      this.solveCache.delete(key);
      if (afterFactory.has(key)) this.factoryInputRevisions.set(key, this.revision);
      else this.factoryInputRevisions.delete(key);
    }
    const factoryChanged = beforeFactory.get(`${previousProject}:${previousFactory}`) !== afterFactory.get(`${previousProject}:${previousFactory}`);
    if (factoryChanged) { this.solve = null; this.solveError = null; this.solveDiagnostic = null; }
    const afterUniverse = universeSolveSignatures(this.document);
    for(const key of new Set([...beforeUniverse.keys(), ...afterUniverse.keys()]))if(beforeUniverse.get(key)!==afterUniverse.get(key)){
      this.universeSolveCache.delete(key);
      if(afterUniverse.has(key))this.universeInputRevisions.set(key,this.revision);
      else this.universeInputRevisions.delete(key);
    }
    const selectedKey=`${this.selectedProjectId}:${this.selectedUniverseId}`;
    const universeChanged = beforeUniverse.get(selectedKey)!==afterUniverse.get(selectedKey);
    if (universeChanged) {
      this.universeSolveGeneration++;
      this.universeSolve = null; this.universeSolving = false;
      this.universeSolveError = null; this.universeSolveDiagnostic = null;
    }

    // 保留有效选择；删除、导入或外部编辑后自动补全缺失选择。
    this.pruneSelection();
    const afterReads = projectReadKeys(this.selectedProject);
    if (beforeReads.domain !== afterReads.domain) {
      this.accessibilityGeneration++;
      this.accessibility = null;
      this.refreshOrderedMilestones().catch(() => {});
    }
    if (beforeReads.productivity !== afterReads.productivity) this.refreshProductivity().catch(() => {});
    if (previousProject !== this.selectedProjectId || previousFactory !== this.selectedFactoryId) {
      this.restoreSolveForSelection();
      this.refreshImplicitSources().catch(() => {});
    }
    if (previousProject !== this.selectedProjectId) {
      this.syncEffectiveContext().catch(() => {});
    }
    if (beforeImplicit !== factoryImplicitSignature(this.document, this.selectedProjectId, this.selectedFactoryId)
      && previousProject === this.selectedProjectId && previousFactory === this.selectedFactoryId) {
      this.refreshImplicitSources().catch(() => {});
    }
    this.refreshProjectPaths().catch(() => {});

  }

  /** Send one AppMessage to the Rust runtime and refresh the snapshot.
   *
   * 返回本次 dispatch 的结果（revision / changed / created / commands），供需要
   * 「刚刚创建了什么」的调用方使用（如 openProject 选出新导入的项目）。
   */
  async send(message: AppMessage): Promise<DispatchResult> {
    this.universeOperationVersion++;
    this.pendingSends++;
    this.busy = true;
    this.lastError = null;
    this.lastDiagnostic = null;
    let result: DispatchResult;
    try {
      result = await dispatch(message);
      if (result.revision >= this.revision) {
        this.historyGeneration++;
        this.history = result.history;
      }
      this.revision = Math.max(this.revision,result.revision);
      await this.refresh();
      if(result.universe_solve && (result.universe_solve.revision??result.revision)>=(this.universeInputRevisions.get(`${result.universe_solve.project}:${result.universe_solve.universe}`)??0)){
        const solved=result.universe_solve;this.universeSolveCache.set(`${solved.project}:${solved.universe}`,solved);
        if(this.selectedProjectId===solved.project&&this.selectedUniverseId===solved.universe){this.universeSolve=solved;this.universeSolveError=null;this.universeSolveDiagnostic=null;this.universeSolving=false;}
      }
    } catch (error) {
      this.setOperationError(error);
      throw error;
    } finally {
      this.busy = --this.pendingSends > 0;
    }
    // 项目上下文切换（设置上下文/换绑等）后同步目录与上下文列表，
    // 使后端 store 载入后「未载入」徽标消失。幂等：上下文未变时不处理。
    this.syncEffectiveContext().catch(() => {});
    return result;
  }

  /** 刷新撤销/重做可用性（启动时与外部改动后调用）。 */
  async refreshHistory(): Promise<void> {
    const generation = ++this.historyGeneration;
    try {
      const history = await historyState();
      if (generation === this.historyGeneration) this.history = history;
    } catch {
      /* 后端不可用时保持原状态 */
    }
  }

  /** 撤销一步；成功后按后端回传的 focus 聚焦到受影响的项目/工厂。 */
  async undo(): Promise<void> {
    await this.applyHistory("undo");
  }

  /** 重做一步。 */
  async redo(): Promise<void> {
    await this.applyHistory("redo");
  }

  private async applyHistory(action: "undo" | "redo"): Promise<void> {
    if (this.busy) return;
    if (action === "undo" && !this.history.can_undo) return;
    if (action === "redo" && !this.history.can_redo) return;
    const result = await this.send({ scope: "history", action });
    // 无步可退/无步可进时后端返回 changed=false：不要动选中态。
    if (result.changed) await this.applyHistoryFocus(result.history.focus);
  }

  /** 撤销/重做后把选中态落到受影响对象上（删除类则落回第一个项目）。 */
  private async applyHistoryFocus(focus: HistoryFocus | undefined): Promise<void> {
    const projects = this.document?.projects ?? [];
    if (focus?.project != null && projects.some((p) => p.id === focus.project)) {
      await this.selectProject(focus.project);
      if(focus.universe!=null && this.selectedProject?.universes.some(u=>u.id===focus.universe)){await this.selectUniverse(focus.universe);return;}
      if (
        focus.factory != null &&
        this.selectedProject?.factories.some((f) => f.id === focus.factory)
      ) {
        await this.selectFactory(focus.factory);
      }
      return;
    }
    const first = projects[0] ?? null;
    await this.selectProject(first?.id ?? null);
    if (first?.factories[0]) await this.selectFactory(first.factories[0].id);
  }

  // ── 可达性（选择器过滤） ────────────────────────────────────────

  /**
   * 确保当前选中项目的可达性快照已拉取；返回可达对象集合。
   * 无选中项目 / 未绑定上下文 / 拉取失败时返回 `null`（表示"不可用，
   * 不做可达性过滤"），避免把全部条目标记为不可达而误滤。
   */
  async ensureAccessibility(): Promise<Accessible[] | null> {
    if (this.accessibility != null) return this.accessibility;
    const project = this.selectedProjectId;
    const generation = ++this.accessibilityGeneration;
    if (project == null) return null;
    try {
      const nodes = await accessibility(project);
      if (generation !== this.accessibilityGeneration || project !== this.selectedProjectId) return null;
      this.accessibility = nodes;
      return nodes;
    } catch {
      if (generation === this.accessibilityGeneration && project === this.selectedProjectId) this.accessibility = null;
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
    const generation = ++this.implicitGeneration;
    const scope = factoryImplicitSignature(this.document, this.selectedProjectId, this.selectedFactoryId);
    if (scope !== this.implicitScope) { this.implicitScope = scope; this.implicitSourcesCache = []; }
    const project = this.selectedProjectId;
    const factory = this.selectedFactoryId;
    if (project == null || factory == null) {
      this.implicitSourcesCache = [];
      return;
    }
    try {
      const sources = await implicitSources(project, factory);
      if (generation === this.implicitGeneration && project === this.selectedProjectId && factory === this.selectedFactoryId) this.implicitSourcesCache = sources;
    } catch {
      if (generation === this.implicitGeneration && project === this.selectedProjectId && factory === this.selectedFactoryId) this.implicitSourcesCache = [];
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

  /**
   * 项目上下文变化后同步前端状态：重载目录索引（会触发后端载入该上下文的
   * store）并刷新上下文列表，使「未载入」徽标在 store 实际载入后消失。
   * 项目上下文未变时无操作（幂等；可按当前项目切换/新建触发）。
   */
  private async syncEffectiveContext(): Promise<void> {
    const ctxId = this.effectiveContextId || this.activeContext?.id || "";
    if (ctxId === this.syncedContextId) return;
    this.syncedContextId = ctxId;
    try {
      // 先载入 store（catalog_index 后端会 ensure_context_loaded），
      // 再刷新列表：列表里的 loaded 才会反映真实载入状态。
      if (ctxId && !await this.loadCatalogIndex()) this.syncedContextId = "";
      await this.refreshContexts();
    } catch {
      /* 刷新失败不阻塞界面 */
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
      this.clearIconCache();
      await this.refreshContexts();
    });
  }

  async loadContextFromExecutable(exe: string, modDir?: string | null): Promise<void> {
    await this.runContext(async () => {
      await loadGameContext(exe, modDir);
      this.clearIconCache();
      await this.refreshContexts();
    });
  }

  async setActiveContext(id: string | null): Promise<void> {
    await this.runContext(async () => {
      // 走消息而不是 Tauri 命令：上下文动作进入 dispatch 管线后，MCP agent 与
      // GUI 走同一条路径（app 层共用 activate_context 实现）。
      await this.send({
        scope: "application",
        action: { "set-active-context": { context: id } },
      });
      await this.refreshContexts();
    });
  }

  async renameContext(id: string, name: string): Promise<void> {
    await this.runContext(async () => {
      await this.send({
        scope: "application",
        action: { "rename-context": { id, name } },
      });
      await this.refreshContexts();
    });
  }

  async deleteContext(id: string): Promise<void> {
    await this.runContext(async () => {
      await this.send({
        scope: "application",
        action: { "delete-context": { id } },
      });
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

  /** Requests are queued natively until listeners and initial state are ready. */
  private consumeProjectOpenRequests(): Promise<void> {
    this.nativeOpenPending = true;
    if (this.nativeOpenTask) return this.nativeOpenTask;
    this.nativeOpenTask = (async () => {
      while (this.nativeOpenPending) {
        this.nativeOpenPending = false;
        const paths = await takeProjectOpenRequests();
        const errors: string[] = [];
        for (const path of paths) {
          try { await this.openProjectPath(path); } catch (error) { errors.push(String(error)); }
        }
        if (errors.length) this.setOperationError(errors.join('\n'));
      }
    })().catch(error => { this.setOperationError(error); }).finally(() => { this.nativeOpenTask = null; });
    return this.nativeOpenTask;
  }

  /** Dialogs, startup arguments and OS drop/open events share one serial queue. */
  openProjectPath(path: string): Promise<boolean> {
    const task = this.fileOpenQueue.then(async () => {
      this.openingProjectPath = path;
      this.fileOpenNotice = null;
      this.busy = true;
      this.lastError = null; this.lastDiagnostic = null;
      try {
        const opened = await openProjectFile(path);
        await this.refresh();
        await this.refreshHistory();
        await this.refreshProjectPaths();
        const project = opened.projects.at(-1);
        if (project == null) throw new Error(`工程「${path}」没有可打开的项目`);
        await this.selectProject(project);
        if (opened.reused) this.fileOpenNotice = `此工程已打开，已切换到现有项目：${opened.path}`;
        else if (opened.projects.length > 1) this.fileOpenNotice = `已导入 ${opened.projects.length} 个项目；请分别另存为，避免覆盖原工程合集。`;
        return true;
      } catch (error) {
        this.setOperationError(error);
        throw error;
      } finally {
        this.openingProjectPath = null;
        this.busy = false;
      }
    });
    this.fileOpenQueue = task.catch(() => {});
    return task;
  }

  async openProject(): Promise<boolean> {
    this.busy = true;
    try {
      const path = await pickProjectFile();
      if (!path) return false;
      return await this.openProjectPath(path);
    } catch (error) {
      this.setOperationError(error);
      throw error;
    } finally {
      this.busy = false;
    }
  }

  async saveCurrentProject(): Promise<boolean> {
    this.busy = true;
    const project = this.requireProject();
    try {
      // 没有记忆路径时先走「另存为」（后端 SaveProject 命令对无路径会报错，
      // 不再像过去那样靠 Tauri 命令返回 null 来区分）。
      if (this.projectSavePath(project) == null) return await this.saveProjectAs();
      await this.send({ scope: "application", action: { "save-project": { project } } });
      await this.refreshProjectPaths();
      return true;
    } catch (error) {
      this.setOperationError(error);
      throw error;
    } finally {
      this.busy = false;
    }
  }

  async saveProjectAs(): Promise<boolean> {
    this.busy = true;
    const project = this.requireProject();
    try {
      const path = await pickProjectSavePath();
      if (!path) return false;
      await this.send({
        scope: "application",
        action: { "save-project-as": { project, path } },
      });
      await this.refreshProjectPaths();
      return true;
    } catch (error) {
      this.setOperationError(error);
      throw error;
    } finally {
      this.busy = false;
    }
  }

  // ── 图标与目录 ──────────────────────────────────────────────────

  /** 取物品图标（blob URL）；无图标时返回 null。带缓存（按上下文 id 区分）。 */
  getIcon(type: string, name: string): Promise<string | null> {
    return this.icons.get(this.effectiveContextId, type, name);
  }

  private clearIconCache(): void {
    this.icons.clear();
    this.iconRevision++;
  }

  /** 目录索引：一次拉取当前上下文的全量目录（前端本地筛选/分组）。 */
  private catalogLoads = new Map<string, Promise<CatalogIndex | null>>();

  async loadCatalogIndex(): Promise<CatalogIndex | null> {
    const contextId = this.effectiveContextId || this.activeContext?.id || "";
    if (this.catalogIndex?.context_id === contextId && contextId) {
      return this.catalogIndex;
    }
    // 合并并发加载（init 的 await 与上下文切换的 fire-and-forget 可能同时触发）。
    const pending = this.catalogLoads.get(contextId);
    if (pending) return pending;
    const request = (async () => {
      try {
        const index = await catalogIndex(contextId);
        // 目录（含该上下文图标）就绪后，让已渲染的 <Icon> 重新拉取，
        // 修复"首屏/上下文未就绪时图标被瞬时 null 缓存"导致的占位。
        if ((this.effectiveContextId || this.activeContext?.id || "") === contextId) {
          this.catalogIndex = index;
          this.iconRevision++;
        }
        return index;
      } catch (error) {
        if ((this.effectiveContextId || this.activeContext?.id || "") === contextId) this.setOperationError(error);
        return null;
      } finally {
        this.catalogLoads.delete(contextId);
      }
    })();
    this.catalogLoads.set(contextId, request);
    return request;
  }

  /** 悬停详情：按 (context, kind, name) 缓存（上下文项目级绑定）。 */
  async getDetail(kind: string, name: string): Promise<PrototypeDetail | null> {
    const context = this.effectiveContextId;
    const key = `${context}/${kind}/${name}`;
    const cached = this.detailCache.get(key);
    if (cached) return cached;
    const pending = this.detailRequests.get(key);
    if (pending) return pending;
    const generation = this.detailGeneration;
    const request = prototypeDetail(kind, name, context).then((detail) => {
      if (detail && generation === this.detailGeneration) this.detailCache.set(key, detail);
      return detail;
    }).catch(() => null).finally(() => {
      if (this.detailRequests.get(key) === request) this.detailRequests.delete(key);
    });
    this.detailRequests.set(key, request);
    return request;
  }

  /**
   * 单个机制的展开流，带内容哈希缓存。`mechanic` 为机制配置对象（按当前
   * 内容判断是否需要重拉）。上下文/项目/工厂/机制 id 与配置、项目设置、环境都参与
   * 缓存键：配置不变（仅引用刷新）时直接复用，避免每次 dispatch 后所有
   * MechanicCard 重发 `mechanicFlow` IPC。
   */
  async getMechanicFlow(
    project: number,
    factory: number,
    mechanicId: number,
    mechanic: unknown,
  ): Promise<{ flow: import("./types").DualVar; amount: number }[]> {
    try {
      return await this.loadMechanicFlow(project, factory, mechanicId, mechanic);
    } catch {
      return [];
    }
  }

  /** Same cache as mechanism cards, but preserve failures for filter progress/retry. */
  loadMechanicFlow(project: number, factory: number, mechanicId: number, mechanic: unknown) {
    const document = this.document?.projects.find(entry => entry.id === project);
    const environment = document?.factories.find(entry => entry.id === factory)?.settings;
    const key = JSON.stringify([document?.context_id ?? this.effectiveContextId, project, factory, mechanicId]);
    const hash = JSON.stringify([mechanic, document?.settings, environment]);
    return this.mechanicFlowCache.get(key, hash, async () => {
      const flows = await mechanicFlow(project, factory, mechanicId);
      return flows.map(([flow, amount]) => ({flow, amount}));
    });
  }

  /** 换上下文/项目后清空机制流缓存（同一 id 在不同上下文含义可能不同）。
   *  具体缓存按 (项目, 工厂, id) 隔离，故仅在幂等上有益。 */
  clearMechanicFlowCache(): void {
    this.universePreviewCache.clear(); this.universeSolveCache.clear();
    this.universeOperationVersion++; this.universeSolveGeneration++;
    this.universeSolve = null; this.universeSolving = false;
    this.mechanicFlowCache.clear();
  }

  /** 本地化显示名（无翻译/未加载索引时回退内部 id）。O(1) Map 查找，避免大
   * 目录下每次重渲染对 entries 做 O(N) 线性 .find。 */
  private localizedNameMap = new Map<string, string>();
  private localizedNameIndex: CatalogIndex | null = null;

  private ensureLocalizedNameMap(): void {
    const ci = this.catalogIndex;
    if (!ci) {
      this.localizedNameMap.clear();
      this.localizedNameIndex = null;
      return;
    }
    // 同一上下文重新加载翻译时也必须重建，不能只比较 context_id。
    if (ci === this.localizedNameIndex) return;
    const map = new Map<string, string>();
    for (const entry of ci.entries) {
      map.set(`${entry.kind}/${entry.name}`, entry.localized_name);
    }
    // 非条目的名字：分组、类别、表面属性等，保留完整命名空间。
    for (const [name, localized] of Object.entries(ci.names ?? {})) {
      map.set(name, localized);
    }
    this.localizedNameMap = map;
    this.localizedNameIndex = ci;
  }

  /** Raw Factorio markup, for RichText only. IPC/prototype IDs remain unchanged. */
  richName(kind: string, name: string): string {
    this.ensureLocalizedNameMap();
    return lookupName(this.localizedNameMap, kind, name);
  }

  /** Plain visible label, safe for titles, aria labels and ordinary controls. */
  localizedName(kind: string, name: string): string {
    return richTextPlain(this.richName(kind, name), (kind, name) => richTextPlain(this.richName(kind, name))).trim();
  }

  /** 换上下文后清空索引/详情缓存。 */
  clearCatalogCache(): void {
    this.catalogIndex = null;
    this.localizedNameMap.clear();
    this.localizedNameIndex = null;
    this.detailCache.clear();
    this.detailGeneration++;
    this.detailRequests.clear();
  }

  // ── 项目 / 工厂 ─────────────────────────────────────────────────

  async newProject(name: string): Promise<void> {
    const result = await this.send({ scope: "application", action: { "new-project": { name } } });
    const project = result.created.projects[0];
    if (project != null) await this.selectProject(project);
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
    const generation = ++this.pathsGeneration;
    const projects = this.document?.projects ?? [];
    const entries = await Promise.all(
      projects.map(async (project) => [project.id, await projectSavePath(project.id)] as const),
    );
    if (generation !== this.pathsGeneration) return;
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
    // 前端持有选择态：新建工厂后选中它（项目里最后一个工厂）。
    const factories = this.selectedProject?.factories ?? [];
    this.selectedFactoryId = factories[factories.length - 1]?.id ?? null;
    this.selectedMechanic = null;
    this.restoreSolveForSelection();
    this.refreshImplicitSources().catch(() => {});
  }


  async addUniverse(name: string): Promise<void> {
    const project = this.requireProject();
    const previousFactory = this.selectedFactoryId;
    const result = await this.send({
      scope: "project",
      action: { project, action: { "add-universe": { name } } },
    });
    this.previousFactoryId = previousFactory;
    this.selectedUniverseId = result.created.universes.at(-1) ?? null;
    this.selectedFactoryId = null;
    this.universeSolve = null;
  }

  async removeUniverse(universe: UniverseId): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "remove-universe": { universe } } },
    });
    if (this.selectedUniverseId === universe) await this.selectUniverse(null);
  }

  async selectUniverse(universe: UniverseId | null): Promise<void> {
    this.universeSolving = false;
    if (universe != null && this.selectedUniverseId == null) {
      this.previousFactoryId = this.selectedFactoryId;
    }
    this.selectedUniverseId = universe;
    this.selectedMechanic = null;
    this.universeSolveError = null;
    this.universeSolveDiagnostic = null;
    if (universe != null) {
      this.selectedFactoryId = null;
      const key = `${this.selectedProjectId}:${universe}`;
      this.universeSolve = this.universeSolveCache.get(key) ?? null;
      if (!this.universeSolve) void this.solveSelectedUniverse();
    } else {
      this.selectedFactoryId = this.previousFactoryId;
      this.previousFactoryId = null;
      this.universeSolve = null;
      await this.restoreSolveForSelection();
    }
  }

  private requireUniverse(): { project: ProjectId; universe: UniverseId } {
    const project = this.requireProject();
    const universe = this.selectedUniverseId;
    if (universe == null) throw new Error("没有选中的宇宙工厂");
    return { project, universe };
  }

  private async universeAction(action: UniverseAction): Promise<void> {
    const {project, universe} = this.requireUniverse();
    await this.dispatchUniverse(project, universe, action);
  }

  async addUniverseLocal(location: Location, kind: MechanicKind): Promise<void> {
    await this.universeAction({ "add-local-mechanic": { location, kind } });
  }

  async addUniverseOrbitalDrop(location: Location, item: IdWithQuality): Promise<void> {
    await this.universeAction({ "add-orbital-drop": { location, item } });
  }
  async addUniverseRocketExport(location: Location, item: IdWithQuality): Promise<void> {
    await this.universeAction({ "add-rocket-export": { location, item } });
  }

  async removeUniverseMechanic(mechanic: MechanicId): Promise<void> {
    await this.universeAction({ "remove-mechanic": { mechanic } });
  }

  async setUniverseMechanicEnabled(mechanic: MechanicId, enabled: boolean): Promise<void> {
    await this.universeAction({ "set-mechanic-enabled": { mechanic, enabled } });
  }

  async updateUniverseMechanic(mechanic: MechanicId, action: import("./types").MechanicAction): Promise<void> {
    await this.universeAction({ mechanic: { mechanic, action } });
  }

  async setUniverseMechanicJson(mechanic: MechanicId, mechanicJson: string): Promise<void> {
    await this.universeAction({ "set-local-mechanic": { mechanic, mechanic_json: mechanicJson } });
  }

  async setUniverseStrictSource(strict: boolean): Promise<void> {
    await this.universeAction({ "set-strict-source": { strict } });
  }

  async setUniverseStrictSink(strict: boolean): Promise<void> {
    await this.universeAction({ "set-strict-sink": { strict } });
  }

  async setUniverseMechanicLocation(mechanic: MechanicId, location: Location): Promise<void> {
    await this.universeAction({ "set-mechanic-location": { mechanic, location } });
  }

  async addUniverseTarget(flow: LocatedDualVar, amount: number): Promise<void> {
    await this.universeAction({ "add-target": { flow, amount } });
  }

  async removeUniverseTarget(target: TargetId): Promise<void> {
    await this.universeAction({ "remove-target": { target } });
  }

  async setUniverseTargetAmount(target: TargetId, amount: number): Promise<void> {
    await this.universeAction({ "set-target-amount": { target, amount } });
  }

  async addUniverseTargetExpression(): Promise<void> {
    await this.universeAction("add-target-expression");
  }

  async removeUniverseTargetExpression(expression: TargetExpressionId): Promise<void> {
    await this.universeAction({ "remove-target-expression": { expression } });
  }

  async setUniverseTargetExpressionConstant(expression: TargetExpressionId, constant: number): Promise<void> {
    await this.universeAction({ "set-target-expression-constant": { expression, constant } });
  }

  async addUniverseTargetExpressionTerm(expression: TargetExpressionId, flow: LocatedDualVar): Promise<void> {
    await this.universeAction({ "add-target-expression-term": { expression, flow } });
  }

  async removeUniverseTargetExpressionTerm(expression: TargetExpressionId, term: TargetTermId): Promise<void> {
    await this.universeAction({ "remove-target-expression-term": { expression, term } });
  }

  async setUniverseTargetExpressionTermFlow(expression: TargetExpressionId, term: TargetTermId, flow: LocatedDualVar): Promise<void> {
    await this.universeAction({ "set-target-expression-term-flow": { expression, term, flow } });
  }

  async setUniverseTargetExpressionTermCoefficient(expression: TargetExpressionId, term: TargetTermId, coefficient: number): Promise<void> {
    await this.universeAction({ "set-target-expression-term-coefficient": { expression, term, coefficient } });
  }

  async autoPlanSelectedUniverse(): Promise<void> {
    const { project, universe } = this.requireUniverse();
    const generation = ++this.universeSolveGeneration;
    const operation = ++this.universeOperationVersion;
    this.universeSolveError = null;
    this.busy = true;
    let planned: UniverseSolveResult | null = null;
    try {
      const result = await autoPlanUniverse(project, universe);
      if (generation !== this.universeSolveGeneration) return;
      if (this.selectedProjectId !== project || this.selectedUniverseId !== universe) return;
      planned = result;
      this.universeSolve = result;
    } catch (error) {
      if (generation !== this.universeSolveGeneration || this.selectedProjectId!==project || this.selectedUniverseId!==universe) return;
      this.universeSolveDiagnostic=diagnosticFrom(error);this.universeSolveError=this.universeSolveDiagnostic.message;
    } finally {
      this.busy = false;
      const current = operation === this.universeOperationVersion;
      await this.refresh();await this.refreshHistory();
      if (current && operation === this.universeOperationVersion && planned && this.selectedProjectId === project && this.selectedUniverseId === universe) {
        this.universeSolve = planned;
        this.universeSolveCache.set(`${project}:${universe}`, planned);
      }
    }
  }

  async solveSelectedUniverse(): Promise<void> {
    this.universeOperationVersion++;
    const { project, universe } = this.requireUniverse();
    const generation = ++this.universeSolveGeneration;
    this.universeSolveError = null;
    this.universeSolving = true;
    try {
      const result = await solveUniverse(project, universe);
      if (generation !== this.universeSolveGeneration) return;
      if (this.selectedProjectId !== project || this.selectedUniverseId !== universe) return;
      this.universeSolve = result;
      this.universeSolveCache.set(`${project}:${universe}`, result);
    } catch (error) {
      if (generation !== this.universeSolveGeneration) return;
      if (this.selectedProjectId !== project || this.selectedUniverseId !== universe) return;
      this.universeSolve = null;
      this.universeSolveDiagnostic=diagnosticFrom(error);this.universeSolveError=this.universeSolveDiagnostic.message;
    } finally {
      if (generation === this.universeSolveGeneration) this.universeSolving = false;
    }
  }

  async removeFactory(factory: FactoryId): Promise<void> {
    if (this.selectedUniverseId === factory) return this.removeUniverse(factory);
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
    if (this.selectedUniverseId != null) {
      await this.send({scope:'project',action:{project:this.requireProject(),action:{'rename-universe':{universe:this.selectedUniverseId,name}}}}); return;
    }
    const { project, factory } = this.requireFactory();
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "set-name": { name } } },
    });
  }

  async selectProject(project: ProjectId | null): Promise<void> {
    // 前端持有选择态：只更新本地 `$state`，不再向后端同步（后端无 UI 状态）。
    const projects = this.document?.projects ?? [];
    const next = projects.find((entry) => entry.id === project) ?? projects[0];
    const changed = this.selectedProjectId !== (next?.id ?? null);
    if (!changed) return;
    this.selectedProjectId = next?.id ?? null;
    if (changed) this.selectedFactoryId = null;
    this.pruneSelection();
    this.selectedMechanic = null;
    this.accessibility = null;
    this.accessibilityGeneration++;
    this.restoreSolveForSelection();
    if (changed) {
      // 选择态由前端驱动：换选项目后重拉其里程碑/产能/隐式输入。
      this.refreshOrderedMilestones().catch(() => {});
      this.refreshProductivity().catch(() => {});
      this.refreshImplicitSources().catch(() => {});
      // 切换项目可能绑定不同上下文：同步目录/上下文列表（清「未载入」）。
      this.syncEffectiveContext().catch(() => {});
    }
  }

  async selectFactory(factory: FactoryId | null): Promise<void> {
    const factories = this.selectedProject?.factories ?? [];
    const next = factories.find((entry) => entry.id === factory) ?? factories[0];
    const changed = this.selectedFactoryId !== (next?.id ?? null);
    if (!changed && this.selectedUniverseId == null) return;
    this.selectedFactoryId = next?.id ?? null;
    this.selectedUniverseId = null;
    this.selectedMechanic = null;
    this.restoreSolveForSelection();
    if (changed) this.refreshImplicitSources().catch(() => {});
  }

  /** 当前选中 (project, factory) 键；无选中返回 (null, null)。 */
  private currentFactoryKey(): [ProjectId | null, FactoryId | null] {
    const project = this.selectedProjectId;
    const factory = this.selectedFactoryId;
    return [project, factory];
  }

  /** 当前选中 (project, factory) 是否正在求解/自动规划（按工厂区分忙状态）。 */
  get isSolvingCurrent(): boolean {
    const s = this.solving;
    return !!s && s.project === this.selectedProjectId && s.factory === this.selectedFactoryId;
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
      this.solveDiagnostic = null;
    } else {
      this.solve = null;
      this.solveError = null;
      this.solveDiagnostic = null;
      if (this.solving?.project === project && this.solving?.factory === factory) return;
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

  async setSolver(solver: LpSolver): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-solver": { solver } } },
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
    // 走消息而不是 Tauri 命令：后端由 Runtime 解析默认集合后统一走
    // finish（递增 revision、落盘、重解全部工厂），前端因此不需要手写
    // 「失效可达性缓存 + 重拉里程碑」的补偿逻辑。
    await this.send({
      scope: "project",
      action: { project, action: "set-default-milestones" },
    });
  }

  /** 里程碑节点按依赖拓扑排序（依赖在前）的当前结果；供里程碑列表按序展示。 */
  orderedMilestones = $state<Milestone[] | null>(null);
  async refreshOrderedMilestones(): Promise<void> {
    const generation = ++this.milestonesGeneration;
    const project = this.selectedProjectId;
    if (project == null) {
      this.orderedMilestones = null;
      return;
    }
    try {
      const milestones = await milestonesOrdered(project);
      if (generation === this.milestonesGeneration && project === this.selectedProjectId) this.orderedMilestones = milestones;
    } catch {
      if (generation === this.milestonesGeneration && project === this.selectedProjectId) this.orderedMilestones = null;
    }
  }

  /** 产能视图（自动 + 用户覆盖），供项目设置面板按来源区分展示。 */
  productivityInfo = $state<ProductivityView | null>(null);
  async refreshProductivity(): Promise<void> {
    const generation = ++this.productivityGeneration;
    const project = this.selectedProjectId;
    if (project == null) {
      this.productivityInfo = null;
      return;
    }
    try {
      const info = await productivity(project);
      if (generation === this.productivityGeneration && project === this.selectedProjectId) this.productivityInfo = info;
    } catch {
      if (generation === this.productivityGeneration && project === this.selectedProjectId) this.productivityInfo = null;
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

  async setProjectMajorQuality(quality: string): Promise<void> {
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "set-major-quality": { quality } } },
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
    return this.mechanicCommands().cloneMechanic(mechanic);
  }

  /** 求解后清理：移除未用/无解机制，或按流量重排。 */
  async cleanup(action: import("./types").CleanupAction): Promise<void> {
    return this.mechanicCommands().cleanup(action);
  }

  /** 采纳建议：新增对应机制并设置主项（机器由兼容回退自动推断）。 */
  async addSuggestion(candidate:{kind:string;name:string},destination?:MechanicScope,location?:Location):Promise<void>{
    const scope:MechanicScope=destination??(this.selectedUniverseId!=null?{project:this.requireProject(),factory:this.selectedUniverseId,kind:'universe' as const}:this.requireFactory());
    const kind=({recipe:'recipe',resource:'mining','item-fuel':'item-fuel','virtual-science':'virtual-science',generator:'generator','fusion-reactor':'fusion-reactor','fusion-generator':'fusion-generator'} as Record<string,import('./types').MechanicKind>)[candidate.kind];
    if(!kind)throw new Error(`未知建议类型 ${candidate.kind}`);
    const result=await this.send(scope.kind==='universe'?{scope:'universe',action:{project:scope.project,universe:scope.factory,action:{'add-local-mechanic':{location:location??this.universeAddLocation,kind}}}}:{scope:'factory',action:{project:scope.project,factory:scope.factory,action:{'mechanic-list':{add:{kind}}}}});
    const id=result.created.mechanics.at(-1);if(id==null)return;
    const commands=this.mechanicCommands(scope);
    if(candidate.kind==='recipe')await commands.setRecipe(id,candidate.name);
    else if(candidate.kind==='resource')await commands.setResource(id,candidate.name);
    else if(candidate.kind==='generator'||candidate.kind==='fusion-generator')await commands.setGenerator(id,candidate.name);
    else if(candidate.kind==='fusion-reactor')await commands.setReactor(id,candidate.name);
    else await commands.setItem(id,candidate.name);
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
    if (this.selectedUniverseId === factory) {
      const result = await this.send({scope:'project',action:{project:this.requireProject(),action:{'clone-universe':{universe:factory}}}});
      await this.selectUniverse(result.created.universes.at(-1) ?? factory); return;
    }
    const project = this.requireProject();
    await this.send({
      scope: "project",
      action: { project, action: { "clone-factory": { factory } } },
    });
    // 前端持有选择态：克隆工厂后选中克隆（项目里最后一个工厂）。
    const factories = this.selectedProject?.factories ?? [];
    this.selectedFactoryId = factories[factories.length - 1]?.id ?? null;
    this.selectedMechanic = null;
    this.restoreSolveForSelection();
    this.refreshImplicitSources().catch(() => {});
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

  universeSolving = $state(false);
  private universeOperationVersion = 0;

  universeAddLocation = $state<Location>({kind: 'planet', name: 'nauvis'});
  private universePreviewCache = new Map<string, Promise<UniversePreview>>();
  loadUniversePreview(project: number, universe: number): Promise<UniversePreview> {
    const key = JSON.stringify([project, universe, this.effectiveContextId, universeSolveSignatures(this.document).get(`${project}:${universe}`)]);
    let pending = this.universePreviewCache.get(key);
    if (!pending) {
      if (this.universePreviewCache.size > 8) this.universePreviewCache.clear();
      pending = universePreview(project, universe).catch(error => {this.universePreviewCache.delete(key); throw error;});
      this.universePreviewCache.set(key, pending);
    }
    return pending;
  }
  mechanicFactory({project, factory, kind}: MechanicScope) {
    const p = this.document?.projects.find(p => p.id === project);
    if (kind === 'universe') { const u = p?.universes?.find(u => u.id === factory); return u ? universeView(u) : null; }
    return p?.factories.find(f => f.id === factory) ?? null;
  }
  async dispatchUniverse(project:number,universe:number,action:UniverseAction){
    this.universeSolveGeneration++;
    await this.send({scope:'universe',action:{project,universe,action}});
  }
  /** Scope captured once; a universe projection never becomes the selected factory. */
  mechanicCommands(scope: MechanicScope = this.requireFactory()): MechanicCommands {
    scope = Object.freeze({...scope});
    return new MechanicCommands({
      getFactory: scope => this.mechanicFactory(scope),
      send: async message => {
        if (scope.kind !== 'universe') return this.send(message);
        if (message.scope !== 'factory') throw new Error('无效的机制操作');
        const action = message.action.action;
        const send = (action: UniverseAction) => this.dispatchUniverse(scope.project, scope.factory, action);
        if (typeof action === 'string') throw new Error('宇宙工厂不支持该操作');
        if ('mechanic' in action) {
          const {mechanic, action: edit} = action.mechanic;
          const entry = this.mechanicFactory(scope)?.mechanics.find(e => e.id === mechanic);
          if (entry?.transport && typeof edit === 'object' && 'item-launch' in edit && typeof edit['item-launch'] === 'object' && 'set-item' in edit['item-launch']) {
            return send({'set-transport-item': {mechanic, item: edit['item-launch']['set-item'].item}});
          }
          return send({mechanic: action.mechanic});
        }
        if ('cleanup' in action) return send({cleanup:action.cleanup});
        if ('mechanic-list' in action) {
          const list = action['mechanic-list'];
          if ('add' in list) return send({'add-local-mechanic': {location: this.universeAddLocation, kind: list.add.kind}});
          if ('clone' in list) return send({'clone-mechanic': list.clone});
          if ('remove' in list) return send({'remove-mechanic': list.remove});
          if ('reorder' in list) return send({'reorder-mechanic': list.reorder});
          if ('set-enabled' in list) return send({'set-mechanic-enabled': list['set-enabled']});
        }
        throw new Error('宇宙工厂不支持该操作');
      },
    }, scope);
  }

  // ── 机制 ────────────────────────────────────────────────────────

  async addMechanic(kind: MechanicKind): Promise<void> {
    return this.mechanicCommands().addMechanic(kind);
  }

  async removeMechanic(mechanic: MechanicId): Promise<void> {
    return this.mechanicCommands().removeMechanic(mechanic);
  }

  /** 机制排序（position 为目标位置）。 */
  async reorderMechanic(mechanic: MechanicId, position: number): Promise<void> {
    return this.mechanicCommands().reorderMechanic(mechanic, position);
  }

  /** 拖拽后的整表重排（从末尾往前逐个移动到目标位置，保证稳定）。 */
  async reorderMechanics(order: MechanicId[]): Promise<void> {
    return this.mechanicCommands().reorderMechanics(order);
  }

  async setMechanicEnabled(mechanic: MechanicId, enabled: boolean): Promise<void> {
    return this.mechanicCommands().setMechanicEnabled(mechanic, enabled);
  }

  async mechanicMessage(mechanic: MechanicId, action: import("./types").MechanicAction): Promise<void> {
    return this.mechanicCommands().mechanicMessage(mechanic, action);
  }

  /** 当前机制的类型（来自文档快照）；用于构造按类型标签的 action。 */
  mechanicKind(mechanic: MechanicId): import("./types").MechanicKind {
    return this.mechanicCommands().mechanicKind(mechanic);
  }

  async setRecipe(mechanic: MechanicId, recipe: string, quality = "normal"): Promise<void> {
    return this.mechanicCommands().setRecipe(mechanic, recipe, quality);
  }

  async setMachine(mechanic: MechanicId, machine: string, quality = "normal"): Promise<void> {
    return this.mechanicCommands().setMachine(mechanic, machine, quality);
  }

  async setResource(mechanic: MechanicId, resource: string): Promise<void> {
    return this.mechanicCommands().setResource(mechanic, resource);
  }

  async setItem(mechanic: MechanicId, item: string, quality = "normal"): Promise<void> {
    return this.mechanicCommands().setItem(mechanic, item, quality);
  }

  async setGenerator(mechanic: MechanicId, generator: string, quality = "normal"): Promise<void> {
    return this.mechanicCommands().setGenerator(mechanic, generator, quality);
  }

  async setBoiler(mechanic: MechanicId, boiler: string, quality = "normal"): Promise<void> {
    return this.mechanicCommands().setBoiler(mechanic, boiler, quality);
  }

  /** 锅炉 heat-fluid-inside 模式的输出温度；null = 流体最高温度。 */
  async setBoilerOutputTemperature(mechanic: MechanicId, temperature: number | null): Promise<void> {
    return this.mechanicCommands().setBoilerOutputTemperature(mechanic, temperature);
  }

  /** 流体燃料机制：选择热值流体。 */
  async setFluidFuel(mechanic: MechanicId, fluid: string): Promise<void> {
    return this.mechanicCommands().setFluidFuel(mechanic, fluid);
  }

  /** 流体热机制：选择提热流体。 */
  async setFluidHeat(mechanic: MechanicId, fluid: string): Promise<void> {
    return this.mechanicCommands().setFluidHeat(mechanic, fluid);
  }

  /** 地格抽取：选择地格（**地格决定产出哪种流体**）。 */
  async setTileExtractTile(mechanic: MechanicId, tile: string): Promise<void> {
    return this.mechanicCommands().setTileExtractTile(mechanic, tile);
  }

  /** 地格抽取：选择抽取机械（决定速率 pumping_speed）。 */
  async setTileExtractMachine(
    mechanic: MechanicId,
    machine: string,
    quality = "normal",
  ): Promise<void> {
    return this.mechanicCommands().setTileExtractMachine(mechanic, machine, quality);
  }

  /** 地格销毁：选择可销毁物品的地格（后端要求 destroys_dropped_items）。 */
  async setTileDisposeTile(mechanic: MechanicId, tile: string): Promise<void> {
    return this.mechanicCommands().setTileDisposeTile(mechanic, tile);
  }

  /** 地格销毁：选择被销毁的物品。 */
  async setTileDisposeItem(
    mechanic: MechanicId,
    item: string,
    quality = "normal",
  ): Promise<void> {
    return this.mechanicCommands().setTileDisposeItem(mechanic, item, quality);
  }

  /** 设置流体类机制的温度（流体热/发电机/锅炉）。流体燃料只看热值，没有温度可编辑。 */
  async setMechanicTemperature(mechanic: MechanicId, temperature: number | null): Promise<void> {
    return this.mechanicCommands().setMechanicTemperature(mechanic, temperature);
  }

  /** 指定机制燃料（配方/采矿/锅炉/反应堆）；null = 自动（燃料类别抽象）。 */
  async setFuel(mechanic: MechanicId, fuel: import("./types").Fuel | null): Promise<void> {
    return this.mechanicCommands().setFuel(mechanic, fuel);
  }

  /** 指定机制燃料温度（配方/采矿/锅炉）；null = 默认温度。 */
  async setFuelTemperature(mechanic: MechanicId, temperature: number | null): Promise<void> {
    return this.mechanicCommands().setFuelTemperature(mechanic, temperature);
  }

  /** 反应堆相邻数；null 继承项目估计。 */
  async setNeighbours(mechanic: MechanicId, neighbours: number | null): Promise<void> {
    return this.mechanicCommands().setNeighbours(mechanic, neighbours);
  }

  /** 火箭发射重量模式（true = 按重量；false = 按堆叠槽位）。 */
  async setWeightMode(mechanic: MechanicId, weight_mode: boolean): Promise<void> {
    return this.mechanicCommands().setWeightMode(mechanic, weight_mode);
  }

  async setReactor(mechanic: MechanicId, reactor: string, quality = "normal"): Promise<void> {
    return this.mechanicCommands().setReactor(mechanic, reactor, quality);
  }

  async setSolarPanel(mechanic: MechanicId, solarPanel: string, quality = "normal"): Promise<void> {
    return this.mechanicCommands().setSolarPanel(mechanic, solarPanel, quality);
  }

  async setAccumulator(mechanic: MechanicId, accumulator: string, quality = "normal"): Promise<void> {
    return this.mechanicCommands().setAccumulator(mechanic, accumulator, quality);
  }

  async setFluid(mechanic: MechanicId, fluid: string): Promise<void> {
    return this.mechanicCommands().setFluid(mechanic, fluid);
  }

  /** 模块配置消息（按机制类型包装 recipe/mining）。 */
  async moduleMessage(mechanic: MechanicId, action: import("./types").ModuleAction): Promise<void> {
    return this.mechanicCommands().moduleMessage(mechanic, action);
  }

  async setModuleSlot(
    mechanic: MechanicId,
    slot: number,
    module: string | null,
    quality = "normal",
  ): Promise<void> {
    return this.mechanicCommands().setModuleSlot(mechanic, slot, module, quality);
  }

  async clearModules(mechanic: MechanicId): Promise<void> {
    return this.mechanicCommands().clearModules(mechanic);
  }

  // ── 自动规划偏好（项目级全局） ──────────────────────────────────

  async planningMessage(action: import("./types").PlanningAction): Promise<void> {
    const project = this.requireProject();
    await this.send({ scope: "project", action: { project, action: { planning: action } } });
  }

  async setAlternativeCount(count: number): Promise<void> {
    await this.planningMessage({ "set-alternative-count": { count } });
  }

  async setPreserveExistingMechanics(preserve: boolean): Promise<void> {
    await this.planningMessage({ "set-preserve-existing-mechanics": { preserve } });
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

  /**
   * 使用最佳插件：用「每个插件类别中 tier 最高的插件」整体替换枚举插件列表。
   *
   * 走一条项目级消息而不是前端编排（旧实现是 `best_modules` 只读命令 +
   * 可达性过滤 + 逐条 add/remove 消息，N 次 dispatch）：后端解析最佳集合、
   * 按可达性过滤并统一 finish（递增 revision、落盘、重解全部工厂）。
   *
   * `quality` 必须由调用方给出，后端不做推断——品质是特殊维度，「解锁某品质」
   * 不等于「能大规模量产该品质的插件」。界面按钮传当前工厂的主品质。
   */
  async applyBestModules(quality: string): Promise<void> {
    await this.planningMessage({ "use-best-modules": { quality } });
  }

  /** 添加枚举插件塔方案：先加空方案，再把所选插件塔写入新方案的插件配置。 */
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

  /** 编辑枚举插件塔方案的插件配置（插件塔数量/共享/塔内插件等，完整 ModuleAction）。 */
  async enumeratedBeaconModule(
    index: number,
    action: import("./types").ModuleAction,
  ): Promise<void> {
    await this.planningMessage({
      "enumerated-beacon-module": { beacon: index, action },
    });
  }

  async recompute(): Promise<void> {
    const { project, factory } = this.requireFactory();
    this.solving = { project, factory };
    try {
      await this.send({
        scope: "factory",
        action: { project, factory, action: { solve: "recompute" } },
      });
    } catch (error) {
      // 求解失败：写入 solveError 让求解面板醒目显示
      // （后端失败会发 solve-error 事件覆盖；dispatch 校验失败走这里）。
      if (this.solving?.project === project && this.solving?.factory === factory) this.solving = null;
      if (project === this.selectedProjectId && factory === this.selectedFactoryId && this.selectedUniverseId == null) {
        this.solveDiagnostic = diagnosticFrom(error);
        this.solveError = this.solveDiagnostic.message;
      }
      throw error;
    }
  }

  /** 自动规划：迭代添加建议机制直至可解（后端 AutoPlan）。 */
  async autoPlan(): Promise<void> {
    const { project, factory } = this.requireFactory();
    this.solving = { project, factory };
    this.autoPlanningLocally = true;
    try {
      await this.send({
        scope: "factory",
        action: { project, factory, action: { solve: "auto-plan" } },
      });
    } catch (error) {
      // 自动规划失败：同时写入 solveError，让求解面板醒目显示
      // （后端失败会发 solve-error 事件覆盖；dispatch 校验失败走这里）。
      if (this.solving?.project === project && this.solving?.factory === factory) this.solving = null;
      if (project === this.selectedProjectId && factory === this.selectedFactoryId && this.selectedUniverseId == null) {
        this.solveDiagnostic = diagnosticFrom(error);
        this.solveError = this.solveDiagnostic.message;
      }
      throw error;
    } finally {
      this.autoPlanningLocally = false;
    }
  }

  // ── 派生访问 ────────────────────────────────────────────────────

  get selectedProject() {
    return (
      this.document?.projects.find((project) => project.id === this.selectedProjectId) ?? null
    );
  }

  get selectedFactory() {
    const project = this.selectedProject;
    return (
      project?.factories.find((factory) => factory.id === this.selectedFactoryId) ?? null
    );
  }

  get selectedUniverse() {
    const project = this.selectedProject;
    return (
      project?.universes?.find((universe) => universe.id === this.selectedUniverseId) ?? null
    );
  }

  /** 前端解析的"有效上下文 id"：当前选中项目绑定的上下文（项目级上下文唯一来源）。
   *  用于图标/目录/详情等需按上下文取数据的命令（后端不再持有该选择）。 */
  get effectiveContextId(): string {
    return this.selectedProject?.context_id ?? "";
  }

  /** 保留有效选择；有项目/工厂时始终补全选择，只有列表为空时才为 null。 */
  private pruneSelection(): void {
    const projects = this.document?.projects ?? [];
    const project = projects.find((entry) => entry.id === this.selectedProjectId) ?? projects[0];
    const projectId = project?.id ?? null;
    const factories = project?.factories ?? [];
    const universes = project?.universes ?? [];
    const universeSelected = universes.some((entry) => entry.id === this.selectedUniverseId);
    const factory = universeSelected
      ? null
      : factories.find((entry) => entry.id === this.selectedFactoryId) ?? factories[0];
    const factoryId = factory?.id ?? null;
    if (projectId !== this.selectedProjectId || factoryId !== this.selectedFactoryId) {
      this.selectedMechanic = null;
    }
    this.selectedProjectId = projectId;
    this.selectedFactoryId = factoryId;
    if (!universeSelected) {
      this.selectedUniverseId = null;
    }
  }

  private requireProject(): ProjectId {
    const project = this.selectedProjectId;
    if (project == null) throw new Error("没有选中的项目");
    return project;
  }

  private requireFactory(): { project: ProjectId; factory: FactoryId } {
    const project = this.requireProject();
    const factory = this.selectedFactoryId;
    if (factory == null) throw new Error("没有选中的工厂");
    return { project, factory };
  }
}

export const runtime = new RuntimeStore();
