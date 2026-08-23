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
  UiState,
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

export async function getUiState(): Promise<UiState> {
  return call("get_ui_state");
}

/** 项目可达性快照（选择器过滤用）：当前可达对象集合。 */
export async function accessibility(project: number): Promise<Accessible[]> {
  return call("accessibility", { project });
}

/** 把项目的里程碑重置为默认（实验室输入的科技瓶物品，全部解锁）。 */
export async function setDefaultMilestones(project: number): Promise<void> {
  await call("set_default_milestones", { project });
}

/** 里程碑节点按依赖拓扑排序（依赖在前），供 UI 按序展示。 */
export async function milestonesOrdered(project: number): Promise<Milestone[]> {
  return call("milestones_ordered", { project });
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

export async function setActiveContext(id: string | null): Promise<ContextList> {
  return call("set_active_context", { id });
}

export async function renameContext(id: string, name: string): Promise<ContextList> {
  return call("rename_context", { id, name });
}

export async function deleteContext(id: string): Promise<ContextList> {
  return call("delete_context", { id });
}

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

export async function loadIcon(type: string, name: string): Promise<number[] | null> {
  return call("icon", { ty: type, name });
}

export async function catalogIndex(): Promise<CatalogIndex> {
  return call("catalog_index");
}

export async function prototypeDetail(
  kind: string,
  name: string,
): Promise<PrototypeDetail | null> {
  return call("prototype_detail", { kind, name });
}

/** 建议系统：为一条流生成候选机制（配方/矿点/燃料/发电机）。 */
export async function suggest(flow: import("./types").DualVar): Promise<import("./types").Suggestion[]> {
  return call("suggest", { flow });
}

/** 每插件类别中 tier 最高的插件（"使用最佳插件"）。 */
export async function bestModules(): Promise<import("./types").Suggestion[]> {
  return call("best_modules");
}

/** 星球隐式可用输入（严格供给下也免费；被外部输入覆盖的不返回）。 */
export async function implicitSources(factory: number): Promise<import("./types").DualVar[]> {
  return call("implicit_sources", { factory });
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
 * 指定机器/信标允许的插件名列表（机制卡手动插件选择鉴权）。
 * machineKind: "machine" | "mining-machine" | "beacon"。
 * recipe: 可选配方名（recipe 机制传入；采矿/信标为 null）。
 */
export async function allowedModules(
  machineKind: string,
  machine: string,
  recipe: string | null,
): Promise<string[]> {
  return call("allowed_modules", { machineKind, machine, recipe });
}

// ── Persistence ───────────────────────────────────────────────────

export async function openProjectDialog(): Promise<AppDocument | null> {
  return call("open_project_dialog");
}

export async function saveProjectAsDialog(): Promise<string | null> {
  return call("save_project_as_dialog");
}

export async function saveProject(): Promise<string | null> {
  return call("save_project");
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
