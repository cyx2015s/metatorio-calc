// Thin IPC client for the Tauri backend.
//
// Every call goes through `invoke` on the Rust commands registered in
// src-tauri/src/lib.rs; solve outcomes arrive as `solve-result` /
// `solve-error` events emitted by the backend worker.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppDocument,
  AppMessage,
  CatalogEntry,
  CatalogKind,
  ContextInfo,
  DispatchResult,
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

export async function getContext(): Promise<ContextInfo> {
  return call("get_context");
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

export async function loadIcon(type: string, name: string): Promise<number[] | null> {
  return call("icon", { ty: type, name });
}

export async function catalog(
  kind: CatalogKind,
  query: string,
  limit: number,
): Promise<CatalogEntry[]> {
  return call("catalog", { kind, query, limit });
}

export async function openProjectDialog(): Promise<AppDocument | null> {
  return call("open_project_dialog");
}

export async function saveProjectAsDialog(): Promise<string | null> {
  return call("save_project_as_dialog");
}

export async function saveProject(): Promise<string | null> {
  return call("save_project");
}

// ── Events ────────────────────────────────────────────────────────

export function onSolveResult(handler: (result: SolveResult) => void): Promise<() => void> {
  return listen<SolveResult>("solve-result", (event) => handler(event.payload));
}

export function onSolveError(handler: (message: string) => void): Promise<() => void> {
  return listen<string>("solve-error", (event) => handler(event.payload));
}

export function onContextLoaded(handler: (info: ContextInfo) => void): Promise<() => void> {
  return listen<ContextInfo>("context-loaded", (event) => handler(event.payload));
}

export function onContextError(handler: (message: string) => void): Promise<() => void> {
  return listen<string>("context-error", (event) => handler(event.payload));
}
