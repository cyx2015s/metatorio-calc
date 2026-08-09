import { invoke } from "@tauri-apps/api/core";

// ---- Phase 2: dispatch 闭环的类型化封装 ----

export type MechanicKind =
  | "recipe"
  | "mining"
  | "spoil"
  | "plant"
  | "item-fuel"
  | "item-launch"
  | "generator"
  | "boiler"
  | "reactor"
  | "unsupported";

export interface MechanicView {
  id: number;
  kind: MechanicKind;
  summary: string;
}

export interface AppViewState {
  factory_name: string;
  mechanics: MechanicView[];
  selected: number | null;
}

/** 与 Rust `message::AppMessage` 对应的意图枚举（kebab-case tag）。 */
export type AppMessage =
  | { type: "set-factory-name"; name: string }
  | { type: "add-mechanic"; kind: MechanicKind }
  | { type: "remove-mechanic"; id: number }
  | { type: "select-mechanic"; id: number }
  | { type: "toggle-mechanic"; id: number };

/** 分发一个意图给 Rust reducer，返回最新渲染投影。 */
export function dispatch(message: AppMessage): Promise<AppViewState> {
  return invoke<AppViewState>("dispatch", { message });
}

/** 无副作用的当前投影（初始加载用）。 */
export function getView(): Promise<AppViewState> {
  return invoke<AppViewState>("get_view");
}
