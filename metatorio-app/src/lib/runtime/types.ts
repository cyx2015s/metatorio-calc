// Type mirrors of the serde JSON shapes produced by
// crates/metatorio-runtime and crates/metatorio-core.
//
// Keep in sync with the Rust side: rename_all = "kebab-case" on messages,
// internally tagged AppMessage (tag "scope", content "action"), PascalCase
// externally tagged DualVar, internally tagged Mechanic (tag "type").
//
// Only the shapes the current UI uses are fully typed; grow this file as
// more of the app lands.

export type ProjectId = number;
export type FactoryId = number;
export type MechanicId = number;
export type TargetId = number;

export interface IdWithQuality {
  id: string;
  quality: string;
}

// DualVar: externally tagged; unit variants serialize as bare strings.
export type DualVar =
  | "Unknown"
  | { Item: IdWithQuality }
  | { Fluid: { name: string; temperature: [number, number] } }
  | { Entity: IdWithQuality }
  | "Heat"
  | "Electricity"
  | { FluidHeat: { filter: string } }
  | { ItemFuel: { category: string[]; has_burnt_result?: boolean } }
  | "RocketSlotCapacity"
  | "RocketWeightCapacity"
  | { Pollution: { name: string } }
  | { Custom: { name: string } }
  | { [key: string]: unknown };

export function itemOf(flow: DualVar): IdWithQuality | undefined {
  if (flow !== null && typeof flow === "object" && "Item" in flow) {
    return flow.Item as IdWithQuality;
  }
  return undefined;
}

export function dualVarLabel(flow: DualVar): string {
  const item = itemOf(flow);
  if (item) return item.id;
  if (typeof flow === "string") return flow;
  if (flow !== null && typeof flow === "object") {
    const record = flow as Record<string, unknown>;
    const key = Object.keys(record)[0];
    const value = record[key];
    if (value && typeof value === "object" && "name" in value) {
      return (value as { name: string }).name;
    }
    return key;
  }
  return String(flow);
}

// ── Messages ──────────────────────────────────────────────────────

export type ApplicationAction = {
  "new-project": { name: string };
};
// Grow later: open-project, save-project, save-project-as, close-project,
// delete-project, reorder-project, load-game-context, ...

export type FactoryTemplate = "empty" | "default-mechanics";

export type ProjectAction = {
  "add-factory": { name: string; template: FactoryTemplate };
};

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

export type MechanicListAction = { add: { kind: MechanicKind } };

export type MechanicAction =
  | { "set-recipe": { recipe: IdWithQuality } }
  | { "set-machine": { machine: IdWithQuality } };

export type FlowAction = { "add-to-target": { flow: DualVar; amount: number } };

export type SolveAction = "recompute" | "auto-plan";

export type FactoryAction =
  | { "mechanic-list": MechanicListAction }
  | { mechanic: { mechanic: MechanicId; action: MechanicAction } }
  | { flow: FlowAction }
  | { solve: SolveAction };

export type UiAction =
  | { "select-project": { project: ProjectId | null } }
  | { "select-factory": { factory: FactoryId | null } };

// AppMessage is internally tagged (tag "scope", content "action").
// serde wraps struct-variant fields under the content key:
//   { scope: "project", action: { project: 1, action: {...} } }
// (newtype variants like Application/Ui put the payload directly under
// "action").  Keep in sync with crates/metatorio-runtime/src/message.rs.
export type AppMessage =
  | { scope: "application"; action: ApplicationAction }
  | { scope: "project"; action: { project: ProjectId; action: ProjectAction } }
  | {
      scope: "factory";
      action: { project: ProjectId; factory: FactoryId; action: FactoryAction };
    }
  | { scope: "ui"; action: UiAction };

// ── Document snapshot ─────────────────────────────────────────────

export interface AppDocument {
  schema_version: number;
  projects: ProjectDocument[];
}

export interface ProjectDocument {
  id: ProjectId;
  name: string;
  settings: ProjectSettings;
  factories: FactoryDocument[];
}

export interface ProjectSettings {
  time_scale: "seconds" | "minutes" | "hours";
  tech_milestones: unknown[];
  recipe_productivity: unknown[];
  ignore_productivity: boolean;
  mining_productivity: number;
  all_accessible: boolean;
  quality_limit: string | null;
}

export interface FactoryDocument {
  id: FactoryId;
  name: string;
  settings: FactorySettings;
  targets: FlowTarget[];
  target_expressions: unknown[];
  external_inputs: unknown[];
  mechanics: MechanicEntry[];
  strict_source: boolean;
  strict_sink: boolean;
}

export interface FactorySettings {
  planet: string | null;
  surface: string | null;
  major_quality: string;
  debug: boolean;
}

export interface FlowTarget {
  id: TargetId;
  flow: DualVar;
  amount: number;
}

export interface MechanicEntry {
  id: MechanicId;
  enabled: boolean;
  mechanic: Mechanic;
  planning: unknown;
}

// Mechanic is internally tagged with "type" (kebab-case kind).
export interface Mechanic {
  type: MechanicKind;
  recipe?: IdWithQuality;
  machine?: IdWithQuality;
  resource?: string;
  module_config?: unknown;
  [key: string]: unknown;
}

// ── Runtime replies ───────────────────────────────────────────────

export interface DispatchResult {
  revision: number;
  changed: boolean;
  commands: unknown[];
}

export interface UiState {
  selected_project: ProjectId | null;
  selected_factory: FactoryId | null;
  selected_mechanic: MechanicId | null;
  page: "preferences" | { factory: FactoryId };
  selector: unknown;
  suggestion_mechanic: number;
  suggestion_filter: string;
  logs_open: boolean;
  font_filter: string;
  font: string | null;
  locale: string | null;
  close_requested: boolean;
}

// ── Solver output ─────────────────────────────────────────────────

export interface SolveResult {
  project: ProjectId;
  factory: FactoryId;
  status: SolveStatus;
}

export type SolveStatus =
  | {
      solved: {
        cost: number;
        mechanics: MechanicSolution[];
        flows: FlowBalance[];
      };
    }
  | {
      "not-solved": {
        no_provider: DualVar[];
        no_consumer: DualVar[];
        description: string;
      };
    };

export interface MechanicSolution {
  mechanic: MechanicId;
  variant: number;
  amount: number;
}

export interface FlowBalance {
  flow: DualVar;
  amount: number;
}
