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

export async function dispatch(message: AppMessage): Promise<DispatchResult> {
  if (!inTauri()) throw new Error(notInTauri());
  return invoke("dispatch", { message });
}

export async function loadBundledDump(): Promise<void> {
  if (!inTauri()) throw new Error(notInTauri());
  return invoke("load_bundled_dump");
}

export async function getDocument(): Promise<AppDocument> {
  if (!inTauri()) throw new Error(notInTauri());
  return invoke("get_document");
}

export async function getUiState(): Promise<UiState> {
  if (!inTauri()) throw new Error(notInTauri());
  return invoke("get_ui_state");
}

export function onSolveResult(handler: (result: SolveResult) => void): Promise<() => void> {
  return listen<SolveResult>("solve-result", (event) => handler(event.payload));
}

export function onSolveError(handler: (message: string) => void): Promise<() => void> {
  return listen<string>("solve-error", (event) => handler(event.payload));
}
