// Rust owns the wire contract. Only browser guards and UI argument adapters live here.
import { commands as generatedCommands, events } from "./bindings.ts";
import type { AutoPlanProgress, ContextList, DualVar, RuntimeDiagnostic, SolveResult, UniverseSolveResult } from "./types";
export type { UniversePreview_Serialize as UniversePreview, ProjectFileOpen, ExportInfo as NetworkPngExport } from "./bindings.ts";

const commands = new Proxy(generatedCommands, {
  get(target, key, receiver) {
    const command = Reflect.get(target, key, receiver);
    if (typeof command !== "function") return command;
    return (...args: unknown[]) => {
      if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
        return Promise.reject(new Error("不在 Tauri 环境中：请用 `pnpm tauri dev` 启动（纯浏览器没有 invoke）。"));
      }
      return Reflect.apply(command, undefined, args);
    };
  },
});

export const dispatch = commands.dispatch;
export const getDocument = commands.getDocument;
export const universePreview = commands.universePreview;
export const solveUniverse = commands.solveUniverse;
export const autoPlanUniverse = commands.autoPlanUniverse;
export const beginNetworkPng = commands.beginNetworkPng;
export const finishNetworkPng = commands.finishNetworkPng;
export const abortNetworkPng = commands.abortNetworkPng;
export const historyState = commands.historyState;
export const accessibility = commands.accessibility;
export const milestonesOrdered = commands.milestonesOrdered;
export const productivity = commands.productivity;
export const loadBundledDump = commands.loadBundledDump;
export const loadDump = commands.loadDump;
export const listContexts = commands.listContexts;
export const pickGameExecutable = commands.pickGameExecutable;
export const pickDumpFile = commands.pickDumpFile;
export const pickModDir = commands.pickModDir;
export const catalogIndex = commands.catalogIndex;
export const implicitSources = commands.implicitSources;
export const mechanicFlow = commands.mechanicFlow;
export const solarBalance = commands.solarBalance;
export const openProjectFile = commands.openProjectFile;
export const takeProjectOpenRequests = commands.takeProjectOpenRequests;
export const pickProjectFile = commands.pickProjectFile;
export const pickProjectSavePath = commands.pickProjectSavePath;
export const projectSavePath = commands.projectSavePath;
export const loadIcon = commands.icon;

// These adapters retain the existing UI's parameter order and optional mod directory.
export const loadGameContext = (executablePath: string, modDir?: string | null) => commands.loadGameContext(executablePath, modDir ?? null);
export const prototypeDetail = (kind: string, name: string, contextId: string) => commands.prototypeDetail(contextId, kind, name);
export const suggest = (flow: DualVar, contextId: string) => commands.suggest(contextId, flow);
export const allowedModules = (machineKind: string, machine: string, recipe: string | null, contextId: string) => commands.allowedModules(contextId, machineKind, machine, recipe);

export async function writeNetworkPngTile(exportId: number, x: number, y: number, png: Blob): Promise<void> {
  const imageBase64 = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve((reader.result as string).split(",")[1]);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(png);
  });
  await commands.writeNetworkPngTile(exportId, x, y, imageBase64);
}

export const onProjectOpenRequested = (handler: () => void) => events.projectOpenRequested.listen(handler);
export const onAutoPlanProgress = (handler: (progress: AutoPlanProgress) => void) => events.autoPlanProgress.listen(event => handler(event.payload));
export const onSolveResult = (handler: (result: SolveResult) => void) => events.solveResult.listen(event => handler(event.payload));
export const onSolveError = (handler: (message: string | RuntimeDiagnostic) => void) => events.solveError.listen(event => handler(event.payload));
export const onContextsChanged = (handler: (list: ContextList) => void) => events.contextsChanged.listen(event => handler(event.payload));
export const onContextError = (handler: (message: string) => void) => events.contextError.listen(event => handler(event.payload));
export const onDocumentChanged = (handler: (revision: number | null) => void) => events.documentChanged.listen(event => handler(event.payload));
export const onCommandError = (handler: (errors: (string | RuntimeDiagnostic)[]) => void) => events.commandError.listen(event => handler(event.payload));
export const onUniverseSolveResult = (handler: (result: UniverseSolveResult) => void) => events.universeSolveResult.listen(event => handler(event.payload));
export const onUniverseSolveError = (handler: (error: import("./bindings").UniverseSolveError) => void) => events.universeSolveError.listen(event => handler(event.payload));
