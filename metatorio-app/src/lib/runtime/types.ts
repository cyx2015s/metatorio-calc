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
export type TargetExpressionId = number;
export type TargetTermId = number;
export type ExternalInputId = number;

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

export type ApplicationAction =
  | { "new-project": { name: string } }
  | { "load-game-context": { executable_path: string; mod_path: string | null } }
  | { "load-cached-context": null }
  | { "save-project": { project: ProjectId } }
  | { "save-project-as": { project: ProjectId; path: string } };

export type FactoryTemplate = "empty" | "default-mechanics";

export type TimeScale = "seconds" | "minutes" | "hours";

export type ProjectAction =
  | { "add-factory": { name: string; template: FactoryTemplate } }
  | { "remove-factory": { factory: FactoryId } }
  | { "reorder-factory": { factory: FactoryId; position: number } }
  | { "set-time-scale": { time_scale: TimeScale } }
  | { "set-all-accessible": { enabled: boolean } }
  | { "set-quality-limit": { quality: string | null } }
  | { "set-mining-productivity": { productivity: number } }
  | { "set-context": { context: string | null } }
  | { planning: PlanningAction };

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

export type MechanicListAction =
  | { add: { kind: MechanicKind } }
  | { remove: { mechanic: MechanicId } }
  | { clone: { mechanic: MechanicId } }
  | { reorder: { mechanic: MechanicId; position: number } }
  | { "set-enabled": { mechanic: MechanicId; enabled: boolean } };

// MechanicAction is tagged by mechanic kind: each variant carries exactly
// the operations that kind supports.  The reducer rejects a kind mismatch.
export type RecipeMechanicAction =
  | { "set-recipe": { recipe: IdWithQuality } }
  | { "set-machine": { machine: IdWithQuality } }
  | { "set-fuel": { fuel: string | null } }
  | { "set-fuel-temperature": { temperature: number | null } }
  | { module: ModuleAction };

export type MiningMechanicAction =
  | { "set-resource": { resource: string } }
  | { "set-machine": { machine: IdWithQuality } }
  | { "set-fuel": { fuel: string | null } }
  | { "set-fuel-temperature": { temperature: number | null } }
  | { module: ModuleAction };

export type SpoilMechanicAction = { "set-item": { item: IdWithQuality } };
export type PlantMechanicAction = { "set-seed": { seed: IdWithQuality } };
export type ItemFuelMechanicAction = { "set-item": { item: IdWithQuality } };
export type ItemLaunchMechanicAction =
  | { "set-item": { item: IdWithQuality } }
  | { "set-weight-mode": { weight_mode: boolean } };

export type GeneratorMechanicAction =
  | { "set-generator": { generator: IdWithQuality } }
  | { "set-fluid": { fluid: string } }
  | { "set-temperature": { temperature: number | null } };

export type BoilerMechanicAction =
  | { "set-boiler": { boiler: IdWithQuality } }
  | { "set-fluid": { fluid: string } }
  | { "set-temperature": { temperature: number | null } }
  | { "set-fuel": { fuel: string | null } }
  | { "set-fuel-temperature": { temperature: number | null } };

export type ReactorMechanicAction =
  | { "set-reactor": { reactor: IdWithQuality } }
  | { "set-fuel": { fuel: string | null } }
  | { "set-neighbours": { neighbours: number } };

export type MechanicAction =
  | { recipe: RecipeMechanicAction }
  | { mining: MiningMechanicAction }
  | { spoil: SpoilMechanicAction }
  | { plant: PlantMechanicAction }
  | { "item-fuel": ItemFuelMechanicAction }
  | { "item-launch": ItemLaunchMechanicAction }
  | { generator: GeneratorMechanicAction }
  | { boiler: BoilerMechanicAction }
  | { reactor: ReactorMechanicAction };

export type ModuleAction =
  | { "set-module-slot": { slot: number; module: IdWithQuality | null } }
  | { "clear-modules": null }
  | { "add-beacon": null }
  | { "remove-beacon": { beacon: number } }
  | { "set-beacon": { beacon: number; value: IdWithQuality } }
  | { "set-beacon-count": { beacon: number; count: number } }
  | { "set-beacon-share": { beacon: number; share: number } };

// 项目级自动规划偏好（全局，不绑定到单个机制）。
export type PlanningAction =
  | { "set-alternative-count": { count: number } }
  | { "add-machine-preference": { machine: IdWithQuality } }
  | { "remove-machine-preference": { machine: IdWithQuality } }
  | { "reorder-machine-preference": { machine: IdWithQuality; position: number } }
  | { "add-enumerated-module": { module: IdWithQuality } }
  | { "remove-enumerated-module": { module: IdWithQuality } }
  | { "use-best-modules": null }
  | { "add-enumerated-beacon": null }
  | { "remove-enumerated-beacon": { beacon: number } }
  | { "set-enumerated-beacon": { beacon: number; plan: AutoBeaconPlan } }
  | { "enumerated-beacon-module": { beacon: number; action: ModuleAction } };

export type TargetAction =
  | { add: { target: FlowTarget } }
  | { remove: { target: TargetId } }
  | { "set-flow": { target: TargetId; flow: DualVar } }
  | { "set-amount": { target: TargetId; amount: number } }
  | { reorder: { target: TargetId; position: number } };

export type ExternalInputAction =
  | { add: { input: ExternalInput } }
  | { remove: { input: ExternalInputId } }
  | { "set-flow": { input: ExternalInputId; flow: DualVar } }
  | { "set-penalty": { input: ExternalInputId; penalty: number } }
  | { reorder: { input: ExternalInputId; position: number } };

export type FlowAction =
  | { "add-to-target": { flow: DualVar; amount: number } }
  | { "add-to-external-input": { flow: DualVar; penalty: number } };

export type SolveAction = "recompute" | "auto-plan";

export type FactoryAction =
  | { "set-name": { name: string } }
  | { "set-strict-source": { strict: boolean } }
  | { "set-strict-sink": { strict: boolean } }
  | { target: TargetAction }
  | { "mechanic-list": MechanicListAction }
  | { mechanic: { mechanic: MechanicId; action: MechanicAction } }
  | { flow: FlowAction }
  | { solve: SolveAction }
  | { "external-input": ExternalInputAction };

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
  planning: PlanningPreferences;
  /** 项目绑定的游戏上下文（缓存 id）；null = 跟随应用当前激活的上下文。 */
  context_id: string | null;
  factories: FactoryDocument[];
}

export interface PlanningPreferences {
  alternative_count: number;
  machine_preferences: IdWithQuality[];
  enumerate_modules: IdWithQuality[];
  enumerate_beacons: AutoBeaconPlan[];
}

export interface AutoBeaconPlan {
  module_config: { modules: IdWithQuality[]; beacons: unknown[] };
}

export interface ProjectSettings {
  time_scale: TimeScale;
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
  external_inputs: ExternalInput[];
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

export interface ExternalInput {
  id: ExternalInputId;
  flow: DualVar;
  penalty: number;
}

export interface MechanicEntry {
  id: MechanicId;
  enabled: boolean;
  mechanic: Mechanic;
}

// Mechanic is internally tagged with "type" (kebab-case kind).
export interface Mechanic {
  type: MechanicKind;
  recipe?: IdWithQuality;
  machine?: IdWithQuality;
  resource?: string;
  item?: IdWithQuality;
  seed?: IdWithQuality;
  generator?: IdWithQuality;
  boiler?: IdWithQuality;
  reactor?: IdWithQuality;
  fluid?: string;
  temperature?: number | null;
  fuel?: string | null;
  neighbours?: number;
  weight_mode?: boolean;
  module_config?: { modules: IdWithQuality[]; beacons: unknown[] };
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

// ── Game context & catalog ────────────────────────────────────────

export interface ContextInfo {
  id: string;
  name: string;
  source: string;
  created_at: number;
  loaded: boolean;
  groups: { name: string; count: number }[];
  icon_root: string | null;
  active: boolean;
}

export interface ContextList {
  active: string | null;
  contexts: ContextInfo[];
}

export interface IndexEntry {
  kind: string;
  name: string;
  group: string;
  subgroup: string;
  icon_type: string;
  module_slots: number | null;
}

export interface CatalogIndex {
  context_id: string;
  entries: IndexEntry[];
}

export interface FlowAmount {
  kind: "item" | "fluid";
  name: string;
  amount: number;
}

/** 悬停详情（按需拉取 + 前端缓存）。 */
export interface PrototypeDetail {
  name: string;
  kind: string;
  subgroup: string | null;
  order: string;
  hidden: boolean;
  stack_size: number | null;
  category: string | null;
  energy_required: number | null;
  ingredients: FlowAmount[];
  results: FlowAmount[];
  crafting_speed: number | null;
  module_slots: number | null;
  /** 焦耳/刻（功率）；前端换算为 W。 */
  energy_usage_j: number | null;
  default_temperature: number | null;
}

export type CatalogKind =
  | "item"
  | "fluid"
  | "recipe"
  | "module"
  | "machine"
  | "mining-machine"
  | "generator"
  | "boiler"
  | "reactor"
  | "beacon"
  | "resource"
  | "entity"
  | "technology"
  | "planet"
  | "surface"
  | "quality";
