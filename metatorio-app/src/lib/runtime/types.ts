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
export type ExternalInputId = number;
export type TargetExpressionId = number;
export type TargetTermId = number;

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
    if (value !== null && typeof value === "object") {
      const inner = value as Record<string, unknown>;
      if ("name" in inner) return String(inner.name);
      if ("id" in inner) return String(inner.id);
    }
    return key;
  }
  return String(flow);
}

/** 流的品质（Item/Entity 且非 normal 时返回；否则 null）。 */
export function flowQuality(flow: DualVar): string | null {
  if (flow !== null && typeof flow === "object") {
    const record = flow as Record<string, unknown>;
    const key = Object.keys(record)[0];
    const value = record[key];
    if (value !== null && typeof value === "object") {
      const inner = value as { quality?: string };
      if (inner.quality && inner.quality !== "normal") return inner.quality;
    }
  }
  return null;
}

// ── Messages ──────────────────────────────────────────────────────

export type ApplicationAction =
  | { "new-project": { name: string } }
  | { "load-game-context": { executable_path: string; mod_path: string | null } }
  | "load-cached-context"
  | { "save-project": { project: ProjectId } }
  | { "save-project-as": { project: ProjectId; path: string } }
  | { "close-project": { project: ProjectId; decision: "cancel" | "discard" | "save" } }
  | { "delete-project": { project: ProjectId; decision: "cancel" | "confirm" } };

export type FactoryTemplate = "empty" | "default-mechanics";

export type TimeScale = "seconds" | "minutes" | "hours";

// 可达性对象（镜像 metatorio-core::Accessible 的 serde 外部标签：
// 结构变体 {"Tech":"..."}，单元变体裸字符串 "Electricity"/"Heat"）。
export type Accessible =
  | { Tech: string }
  | { Recipe: string }
  | { Quality: string }
  | { Space: string }
  | { Item: string }
  | { Fluid: string }
  | { Entity: string }
  | { Planet: string }
  | "Electricity"
  | "Heat";

/** 可达性对象的目录 kind（与选择器 kind 对齐：technology/recipe/…）。 */
export function accessibleKind(node: Accessible): string {
  switch (node) {
    case "Electricity":
      return "electricity";
    case "Heat":
      return "heat";
  }
  if ("Tech" in node) return "technology";
  if ("Space" in node) return "space-location";
  return Object.keys(node)[0].toLowerCase();
}

/** 可达性对象的名称（电/热无名称，返回 ""）。 */
export function accessibleName(node: Accessible): string {
  if (typeof node === "string") return "";
  return Object.values(node)[0];
}

/** 目录 kind（item/recipe/machine/resource/…）→ 可达性 kind；
 *  机器/资源等按实体判断；surface 等无对应的返回 null（不做可达性过滤）。 */
export function accessibleKindFor(kind: string): string | null {
  switch (kind) {
    case "item":
    case "module":
      return "item";
    case "fluid":
      return "fluid";
    case "recipe":
      return "recipe";
    case "technology":
      return "technology";
    case "planet":
      return "planet";
    case "quality":
      return "quality";
    case "space-location":
      return "space-location";
    case "entity":
    case "machine":
    case "mining-machine":
    case "generator":
    case "boiler":
    case "reactor":
    case "solar-panel":
    case "accumulator":
    case "beacon":
    case "resource":
      return "entity";
    default:
      return null;
  }
}

export type ProjectAction =
  | { "set-name": { name: string } }
  | { "add-factory": { name: string; template: FactoryTemplate } }
  | { "clone-factory": { factory: FactoryId } }
  | { "remove-factory": { factory: FactoryId } }
  | { "reorder-factory": { factory: FactoryId; position: number } }
  | { "set-time-scale": { time_scale: TimeScale } }
  | { "set-all-accessible": { enabled: boolean } }
  | { "set-quality-limit": { quality: string | null } }
  | { "set-mining-productivity": { productivity: number } }
  | { "set-context": { context: string | null } }
  | { "add-milestone": { node: Accessible; unlocked: boolean } }
  | { "set-milestone-unlocked": { node: Accessible; unlocked: boolean } }
  | { "remove-milestone": { node: Accessible } }
  | { "set-ignore-productivity": { ignore: boolean } }
  | { "set-recipe-productivity": { productivity: RecipeProductivity } }
  | { "remove-recipe-productivity": { recipe: string } }
  | { "set-infinite-tech-level": { level: InfiniteTechLevel } }
  | { "remove-infinite-tech-level": { tech: string } }
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
  | "solar"
  | "fluid-fuel"
  | "fluid-heat"
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
  | { "set-fuel": { fuel: Fuel | null } }
  | { "set-fuel-temperature": { temperature: number | null } }
  | { module: ModuleAction };

export type MiningMechanicAction =
  | { "set-resource": { resource: string } }
  | { "set-machine": { machine: IdWithQuality } }
  | { "set-fuel": { fuel: Fuel | null } }
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
  | { "set-fuel": { fuel: Fuel | null } }
  | { "set-fuel-temperature": { temperature: number | null } };

export type ReactorMechanicAction =
  | { "set-reactor": { reactor: IdWithQuality } }
  | { "set-fuel": { fuel: Fuel | null } }
  | { "set-neighbours": { neighbours: number } };

export type SolarMechanicAction =
  | { "set-solar-panel": { solar_panel: IdWithQuality } }
  | { "set-accumulator": { accumulator: IdWithQuality } };

export type FluidFuelMechanicAction =
  | { "set-fluid": { fluid: string } }
  | { "set-temperature": { temperature: number | null } };

export type FluidHeatMechanicAction =
  | { "set-fluid": { fluid: string } }
  | { "set-temperature": { temperature: number | null } };

export type MechanicAction =
  | { recipe: RecipeMechanicAction }
  | { mining: MiningMechanicAction }
  | { spoil: SpoilMechanicAction }
  | { plant: PlantMechanicAction }
  | { "item-fuel": ItemFuelMechanicAction }
  | { "item-launch": ItemLaunchMechanicAction }
  | { generator: GeneratorMechanicAction }
  | { boiler: BoilerMechanicAction }
  | { reactor: ReactorMechanicAction }
  | { solar: SolarMechanicAction }
  | { "fluid-fuel": FluidFuelMechanicAction }
  | { "fluid-heat": FluidHeatMechanicAction };

// 注意：unit 变体（clear-modules）在 serde 外部标签下序列化为裸字符串。
export type ModuleAction =
  | { "set-module-slot": { slot: number; module: IdWithQuality | null } }
  | "clear-modules"
  | { "add-beacon": { beacon: IdWithQuality } }
  | { "remove-beacon": { beacon: number } }
  | { "set-beacon": { beacon: number; value: IdWithQuality } }
  | { "set-beacon-count": { beacon: number; count: number } }
  | { "set-beacon-share": { beacon: number; share: number } }
  | { "add-beacon-module": { beacon: number; module: IdWithQuality } }
  | { "remove-beacon-module": { beacon: number; module: number } }
  | { "set-beacon-module": { beacon: number; module: number; value: IdWithQuality } }
  | { "set-beacon-module-count": { beacon: number; module: number; count: number } }
  | { "clamp-modules": { max: number } };

// 项目级自动规划偏好（全局，不绑定到单个机制）。
export type PlanningAction =
  | { "set-alternative-count": { count: number } }
  | { "add-machine-preference": { machine: IdWithQuality } }
  | { "remove-machine-preference": { machine: IdWithQuality } }
  | { "reorder-machine-preference": { machine: IdWithQuality; position: number } }
  | { "add-enumerated-module": { module: IdWithQuality } }
  | { "remove-enumerated-module": { module: IdWithQuality } }
  | "use-best-modules"
  | "add-enumerated-beacon"
  | { "remove-enumerated-beacon": { beacon: number } }
  | { "set-enumerated-beacon": { beacon: number; plan: AutoBeaconPlan } }
  | { "enumerated-beacon-module": { beacon: number; action: ModuleAction } };

export type TargetAction =
  | { add: { target: FlowTarget } }
  | { remove: { target: TargetId } }
  | { "set-flow": { target: TargetId; flow: DualVar } }
  | { "set-amount": { target: TargetId; amount: number } }
  | { reorder: { target: TargetId; position: number } };

export type TargetExpressionAction =
  | { add: { expression: TargetExpression } }
  | { remove: { expression: TargetExpressionId } }
  | { "set-constant": { expression: TargetExpressionId; constant: number } }
  | { "add-term": { expression: TargetExpressionId; term: TargetTerm } }
  | { "remove-term": { expression: TargetExpressionId; term: TargetTermId } }
  | { "set-term-flow": { expression: TargetExpressionId; term: TargetTermId; flow: DualVar } }
  | {
      "set-term-coefficient": {
        expression: TargetExpressionId;
        term: TargetTermId;
        coefficient: number;
      };
    }
  | { reorder: { expression: TargetExpressionId; position: number } }
  | { "reorder-term": { expression: TargetExpressionId; term: TargetTermId; position: number } };

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

export type CleanupAction = "remove-unused" | "remove-unsolvable" | "sort-by-solution-rate";

export type FactoryContextAction =
  | { "set-planet": { planet: string | null } }
  | { "set-surface": { surface: string | null } }
  | { "set-major-quality": { quality: string } };

export type FactoryAction =
  | { "set-name": { name: string } }
  | { "set-strict-source": { strict: boolean } }
  | { "set-strict-sink": { strict: boolean } }
  | { context: FactoryContextAction }
  | { cleanup: CleanupAction }
  | { target: TargetAction }
  | { "target-expression": TargetExpressionAction }
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
  module_config: ModuleConfig;
}

export interface Milestone {
  /** 里程碑节点（默认是科技瓶物品；也支持科技/配方等可达性对象）。 */
  node: Accessible;
  /** true = 已解锁；false = 未解锁（剪枝自身并阻断依赖它的对象）。 */
  unlocked: boolean;
}

export interface RecipeProductivity {
  recipe: string;
  productivity: number;
}

/** 无限科技的研究次数覆盖（2.b）。 */
export interface InfiniteTechLevel {
  tech: string;
  level: number;
}

/** 面向前端的产能视图：区分自动推算与用户指定（用户值有虚线边框）。 */
export interface ProductivityView {
  /** 每个配方产能项：source = "auto"（自动推算）| "user"（用户指定）。 */
  recipes: RecipeProductivityView[];
  /** 自动推算的采矿产出加成。 */
  auto_mining: number;
  /** 最终采矿产出加成（用户覆盖后）。 */
  mining: number;
  /** 用户对无限科技的研究次数覆盖（2.b）。 */
  infinite_levels: InfiniteTechLevel[];
  /** 是否忽略自动推算（2.c）。 */
  ignore: boolean;
}

/** 单个配方产能项。 */
export interface RecipeProductivityView {
  recipe: string;
  value: number;
  source: "auto" | "user";
}

export interface ProjectSettings {
  time_scale: TimeScale;
  /** 里程碑（可达性节点级；unlocked 是强制覆盖——false 强制不可达并阻断依赖，true 强制可达）。 */
  milestones: Milestone[];
  /** 用户手动固定的配方产能（2.a，替换自动值）。 */
  recipe_productivity: RecipeProductivity[];
  /** 用户对无限科技的研究次数覆盖（2.b）。 */
  infinite_levels: InfiniteTechLevel[];
  /** 忽略产能加成（2.c：丢弃自动推算，保留用户值）。 */
  ignore_productivity: boolean;
  /** 用户手动固定的采矿产出加成（替换自动值）。 */
  mining_productivity: number;
  all_accessible: boolean;
  quality_limit: string | null;
}

export interface FactoryDocument {
  id: FactoryId;
  name: string;
  settings: FactorySettings;
  targets: FlowTarget[];
  target_expressions: TargetExpression[];
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

export interface TargetTerm {
  id: TargetTermId;
  flow: DualVar;
  coefficient: number;
}

export interface TargetExpression {
  id: TargetExpressionId;
  constant: number;
  terms: TargetTerm[];
}

export interface ExternalInput {
  id: ExternalInputId;
  flow: DualVar;
  penalty: number;
}

/** 建议候选：kind ∈ recipe | resource | item-fuel | generator；role 区分生产/消耗。 */
export interface Suggestion {
  kind: string;
  name: string;
  role?: "producer" | "consumer";
}

export interface MechanicEntry {
  id: MechanicId;
  enabled: boolean;
  mechanic: Mechanic;
}

export interface BeaconConfig {
  modules: [IdWithQuality, number][];
  beacon: IdWithQuality;
  count: number;
  share: number;
}

export interface ModuleConfig {
  modules: IdWithQuality[];
  beacons: BeaconConfig[];
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
  solar_panel?: IdWithQuality;
  accumulator?: IdWithQuality;
  fluid?: string;
  temperature?: number | null;
  fuel?: Fuel | null;
  neighbours?: number;
  weight_mode?: boolean;
  module_config?: ModuleConfig;
  [key: string]: unknown;
}

/** 明确燃料：语义上直接区分物品燃料（带品质）与流体燃料（名称 + 温度）。 */
export type Fuel =
  | { kind: "item"; item: IdWithQuality }
  | { kind: "fluid"; fluid: string; temperature?: number | null };

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
  /** 单台实例成本（机器碰撞箱面积）。 */
  cost: number;
  /** Ruiz 均衡缩放系数；amount/scale 为内部可比量（判断接近 0 用）。 */
  scale: number;
}

export interface FlowBalance {
  flow: DualVar;
  amount: number;
  /** 该物品平衡约束的 Ruiz 缩放系数；amount/scale 为内部可比量。 */
  scale: number;
}

// ── Solar 配平信息（solar_balance 命令）──────────────────────────

export interface SolarBalance {
  /** 满日照峰值功率（J/s，含星球太阳能系数与品质倍率）。 */
  peak_power: number;
  /** 周期平均稳定出力（J/s）。 */
  average_power: number;
  /** 一个昼夜周期的秒数。 */
  cycle_seconds: number;
  /** 一个周期溢出的总电量（J）——蓄电器需要储存的能量。 */
  surplus_per_cycle: number;
  /** 蓄电器容量（J）。 */
  accumulator_capacity: number;
  /** 推荐蓄电器数量（每块面板）。 */
  recommended_accumulators: number;
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
  /** 本地化显示名（`--dump-prototype-locale`；无翻译时为空串）。 */
  localized_name: string;
  group: string;
  subgroup: string;
  icon_type: string;
  module_slots: number | null;
  /** 兼容性类别：machine→crafting_categories、recipe→categories、mining-machine→resource_categories、resource→category。 */
  categories: string[];
  /** 物品燃料类别（非燃料物品为空串）。 */
  fuel_category: string;
  /** 物品/流体燃料热值（焦耳；非燃料为 null）。 */
  fuel_value_j: number | null;
}

export interface CatalogIndex {
  context_id: string;
  /** 可用品质（normal 起）。 */
  qualities: string[];
  entries: IndexEntry[];
}

export interface FlowAmount {
  kind: "item" | "fluid";
  name: string;
  /** 单次期望量（概率已折算）。 */
  amount: number;
  /** 产出概率（0..1；常规产物为 1）。 */
  probability: number;
  /** 有概率时的原始量区间（amount_min/amount_max）。 */
  amount_min: number | null;
  amount_max: number | null;
  /** 每次产能结算的额外产量（仅产物）。 */
  productivity: number;
  /** 流体温度。 */
  temperature: number | null;
  min_temperature: number | null;
  max_temperature: number | null;
  /** 产物品质下限/上限（如 "uncommon"）。 */
  quality_min: string | null;
  quality_max: string | null;
  /** 品质偏移（品质等级偏移量，0 不显示）。 */
  quality_change: number | null;
}

/** 悬停详情（按需拉取 + 前端缓存）。 */
export interface PrototypeDetail {
  name: string;
  /** 本地化显示名（无翻译时为空串）。 */
  localized_name: string;
  kind: string;
  subgroup: string | null;
  order: string;
  hidden: boolean;
  stack_size: number | null;
  /** 燃料能量（焦耳）。 */
  fuel_value_j: number | null;
  /** 燃料类别（如 "chemical"）。 */
  fuel_category: string;
  /** 燃烧产物。 */
  burnt_result: string;
  /** 变质产物。 */
  spoil_result: string;
  /** 变质时间（刻）。 */
  spoil_ticks: number | null;
  /** 种植产物（种子 → 实体）。 */
  plant_result: string;
  /** 是否可火箭发射。 */
  launchable: boolean;
  /** 火箭发射产物物品名列表（如 satellite）。 */
  rocket_launch_products: string[];
  category: string | null;
  categories: string[];
  energy_required: number | null;
  /** 配方最大产能加成（默认 3.0）。 */
  maximum_productivity: number | null;
  /** 配方表面条件（如 "gravity: 1"）。 */
  surface_conditions: string[];
  ingredients: FlowAmount[];
  results: FlowAmount[];
  crafting_speed: number | null;
  module_slots: number | null;
  /** 机器/信标允许的插件类别（空 = 不限制）。 */
  allowed_module_categories: string[];
  /** 焦耳/刻（功率）；前端换算为 W。 */
  energy_usage_j: number | null;
  /** 机器能量源类型（electric/burner/fluid/heat/void）；burner 才显示燃料配置。 */
  machine_energy_source: string | null;
  /** burner 机器可接受的燃料类别（electric/fluid 为空）。 */
  burner_fuel_categories: string[];
  // generator / boiler / reactor
  /** 发电效率。 */
  effectivity: number | null;
  /** 最大出力（焦耳/刻）。 */
  max_power_output_j: number | null;
  /** 最高/目标温度。 */
  maximum_temperature: number | null;
  /** 发电机是否燃烧流体。 */
  burns_fluid: boolean | null;
  /** 发电机流体用量（单位/刻）。 */
  fluid_usage_per_tick: number | null;
  /** 锅炉能耗（焦耳/刻）。 */
  energy_consumption_j: number | null;
  /** 锅炉目标温度。 */
  target_temperature: number | null;
  /** 反应堆相邻加成。 */
  neighbour_bonus: number | null;
  /** 反应堆加热半径。 */
  heating_radius: number | null;
  /** 反应堆热输出（焦耳/刻）。 */
  heat_output_j: number | null;
  /** 流体箱过滤（如 "steam"）。 */
  fluid_filter: string | null;
  beacon_module_slots: number | null;
  default_temperature: number | null;
  // quality（kind = "quality"）
  quality_level: number | null;
  quality_next: string | null;
  quality_next_probability: number | null;
  quality_crafting_speed: number | null;
  quality_module_speed: number | null;
  quality_module_productivity: number | null;
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
  | "solar-panel"
  | "accumulator"
  | "beacon"
  | "resource"
  | "entity"
  | "technology"
  | "planet"
  | "surface"
  | "quality";
