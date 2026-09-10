// Thin IPC client for the Tauri backend.
//
// Every call goes through `invoke` on the Rust commands registered in
// src-tauri/src/lib.rs; solve outcomes arrive as `solve-result` /
// `solve-error` events, context changes as `contexts-changed`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Accessible,
  AppDocument,
  AppMessage,
  CatalogIndex,
  CatalogKind,
  ContextInfo,
  ContextList,
  DispatchResult,
  Milestone,
  PrototypeDetail,
  SolveResult,
} from "./types";

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function notInTauri(): string {
  return "不在 Tauri 环境中：请用 `pnpm tauri dev` 启动（纯浏览器没有 invoke）。";
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri()) throw new Error(notInTauri());
  return invoke<T>(command, args);
}

export async function dispatch(message: AppMessage): Promise<DispatchResult> {
  return call("dispatch", { message });
}

export async function getDocument(): Promise<AppDocument> {
  return call("get_document");
}

/** 项目可达性快照（选择器过滤用）：当前可达对象集合。 */
export async function accessibility(project: number): Promise<Accessible[]> {
  return call("accessibility", { project });
}

/** 里程碑节点按依赖拓扑排序（依赖在前），供 UI 按序展示。 */
export async function milestonesOrdered(project: number): Promise<Milestone[]> {
  return call("milestones_ordered", { project });
}

/** 产能视图：自动推算 + 用户覆盖，按来源区分。 */
export async function productivity(project: number): Promise<import("./types").ProductivityView> {
  return call("productivity", { project });
}

// ── Game contexts ─────────────────────────────────────────────────

export async function loadBundledDump(): Promise<ContextInfo> {
  return call("load_bundled_dump");
}

export async function loadGameContext(
  executablePath: string,
  modDir?: string | null,
): Promise<ContextInfo> {
  return call("load_game_context", { executablePath, modDir: modDir ?? null });
}

export async function loadDump(path: string): Promise<ContextInfo> {
  return call("load_dump", { path });
}

export async function listContexts(): Promise<ContextList> {
  return call("list_contexts");
}

// 上下文的切换/重命名/删除已收敛为 AppMessage（见 store.setActiveContext 等）：
// 它们改的是 app 层注册表，只有 app 层的命令执行器能碰，因此走 dispatch 后
// MCP agent 与 GUI 共享同一条路径（app 层 `activate_context` 等单一实现）。

export async function pickGameExecutable(): Promise<string | null> {
  return call("pick_game_executable");
}

export async function pickDumpFile(): Promise<string | null> {
  return call("pick_dump_file");
}

export async function pickModDir(): Promise<string | null> {
  return call("pick_mod_dir");
}

// ── Catalog & icons ───────────────────────────────────────────────

export async function loadIcon(
  type: string,
  name: string,
  contextId: string,
): Promise<number[] | null> {
  return call("icon", { ty: type, name, contextId: contextId });
}

export async function catalogIndex(contextId: string): Promise<CatalogIndex> {
  return call("catalog_index", { contextId: contextId });
}

export async function prototypeDetail(
  kind: string,
  name: string,
  contextId: string,
): Promise<PrototypeDetail | null> {
  return call("prototype_detail", { contextId: contextId, kind, name });
}

/** 建议系统：为一条流生成候选机制（配方/矿点/燃料/发电机）。 */
export async function suggest(
  flow: import("./types").DualVar,
  contextId: string,
): Promise<import("./types").Suggestion[]> {
  return call("suggest", { contextId: contextId, flow });
}

/** 每插件类别中 tier 最高的插件（"使用最佳插件"）。 */
export async function bestModules(contextId: string): Promise<import("./types").Suggestion[]> {
  return call("best_modules", { contextId: contextId });
}

/** 星球隐式可用输入（严格供给下也免费；被外部输入覆盖的不返回）。 */
export async function implicitSources(
  project: number,
  factory: number,
): Promise<import("./types").DualVar[]> {
  return call("implicit_sources", { project, factory });
}

/** 单个机制的展开流（系数 1 时每秒产/耗）；正值产出、负值消耗。 */
export async function mechanicFlow(
  project: number,
  factory: number,
  mechanic: number,
): Promise<[import("./types").DualVar, number][]> {
  return call("mechanic_flow", { project, factory, mechanic });
}

/** 太阳能机制的配平信息（平均出力 / 周期溢出总电量 / 蓄电器配比）。 */
export async function solarBalance(
  project: number,
  factory: number,
  mechanic: number,
): Promise<import("./types").SolarBalance | null> {
  return call("solar_balance", { project, factory, mechanic });
}

/**
 * 指定机器/插件塔允许的插件名列表（机制卡手动插件选择鉴权）。
 * machineKind: "machine" | "mining-machine" | "beacon"。
 * recipe: 可选配方名（recipe 机制传入；采矿/插件塔为 null）。
 */
export async function allowedModules(
  machineKind: string,
  machine: string,
  recipe: string | null,
  contextId: string,
): Promise<string[]> {
  return call("allowed_modules", { contextId: contextId, machineKind, machine, recipe });
}

// ── Persistence ───────────────────────────────────────────────────

export async function openProjectDialog(): Promise<AppDocument | null> {
  return call("open_project_dialog");
}

export async function saveProjectAsDialog(project: number): Promise<string | null> {
  return call("save_project_as_dialog", { project });
}

export async function saveProject(project: number): Promise<string | null> {
  return call("save_project", { project });
}

export async function projectSavePath(project: number): Promise<string | null> {
  return call("project_save_path", { project });
}

// ── Events ────────────────────────────────────────────────────────

export function onSolveResult(handler: (result: SolveResult) => void): Promise<() => void> {
  return listen<SolveResult>("solve-result", (event) => handler(event.payload));
}

export function onSolveError(handler: (message: string) => void): Promise<() => void> {
  return listen<string>("solve-error", (event) => handler(event.payload));
}

export function onContextsChanged(handler: (list: ContextList) => void): Promise<() => void> {
  return listen<ContextList>("contexts-changed", (event) => handler(event.payload));
}

export function onContextError(handler: (message: string) => void): Promise<() => void> {
  return listen<string>("context-error", (event) => handler(event.payload));
}

/**
 * 后端文档被外部修改（如 MCP 的 `dispatch` 工具）后广播，通知 GUI 重新拉取
 * 文档快照，实现「外部 agent + 用户在同一个界面上实时并存操作」。
 */
export function onDocumentChanged(handler: (revision: number) => void): Promise<() => void> {
  return listen<number>("document-changed", (event) => handler(event.payload));
}
