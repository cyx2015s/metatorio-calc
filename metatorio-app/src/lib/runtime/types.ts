// Protocol types are generated from Rust. This file contains only UI projections and helpers.
import type * as Wire from "./bindings";
// Drop optional-never exclusivity markers so `in` narrows action variants.
type Variant<T> = T extends readonly unknown[] ? { [K in keyof T]: Variant<T[K]> } : T extends object ? { [K in keyof T as undefined extends T[K] ? Exclude<T[K], undefined> extends never ? never : K : K]: Variant<T[K]> } : T;
type UnionKeys<T> = T extends unknown ? keyof T : never;
type UnionValue<T, K extends PropertyKey> = T extends unknown ? K extends keyof T ? T[K] : never : never;
export type ProjectId = Wire.ProjectId;
export type FactoryId = Wire.FactoryId;
export type UniverseId = Wire.UniverseId;
export type MechanicId = Wire.MechanicId;
export type TargetId = Wire.TargetId;
export type ExternalInputId = Wire.ExternalInputId;
export type TargetExpressionId = Wire.TargetExpressionId;
export type TargetTermId = Wire.TargetTermId;
export type IdWithQuality = Wire.IdWithQuality;
export type DualVar = Wire.DualVar_Serialize;
export type ApplicationAction = Variant<Wire.ApplicationAction_Deserialize>;
export type FactoryTemplate = Wire.FactoryTemplate;
export type TimeScale = Wire.TimeScale;
export type Accessible = Wire.Accessible;
export type ProjectAction = Variant<Wire.ProjectAction_Deserialize>;
export type MechanicKind = Wire.MechanicKind;
export type MechanicListAction = Wire.MechanicListAction;
export type RecipeMechanicAction = Variant<Wire.RecipeMechanicAction_Deserialize>;
export type MiningMechanicAction = Variant<Wire.MiningMechanicAction_Deserialize>;
export type SpoilMechanicAction = Wire.SpoilMechanicAction;
export type PlantMechanicAction = Wire.PlantMechanicAction;
export type ItemFuelMechanicAction = Wire.ItemFuelMechanicAction;
export type ItemLaunchMechanicAction = Wire.ItemLaunchMechanicAction;
export type GeneratorMechanicAction = Variant<Wire.GeneratorMechanicAction_Deserialize>;
export type BoilerMechanicAction = Variant<Wire.BoilerMechanicAction_Deserialize>;
export type ReactorMechanicAction = Variant<Wire.ReactorMechanicAction_Deserialize>;
export type SolarMechanicAction = Wire.SolarMechanicAction;
export type FluidFuelMechanicAction = Wire.FluidFuelMechanicAction;
export type FluidHeatMechanicAction = Variant<Wire.FluidHeatMechanicAction_Deserialize>;
export type TileExtractMechanicAction = Wire.TileExtractMechanicAction;
export type TileDisposeMechanicAction = Wire.TileDisposeMechanicAction;
export type MechanicAction = Variant<Wire.MechanicAction_Deserialize>;
export type ModuleAction = Variant<Wire.ModuleAction_Deserialize>;
export type PlanningAction = Variant<Wire.PlanningAction_Deserialize>;
export type TargetAction = Variant<Wire.TargetAction_Deserialize>;
export type TargetExpressionAction = Variant<Wire.TargetExpressionAction_Deserialize>;
export type ExternalInputAction = Variant<Wire.ExternalInputAction_Deserialize>;
export type FlowAction = Variant<Wire.FlowAction_Deserialize>;
export type SolveAction = Wire.SolveAction;
export type CleanupAction = Wire.CleanupAction;
export type FactoryContextAction = Variant<Wire.FactoryContextAction_Deserialize>;
export type FactoryAction = Variant<Wire.FactoryAction_Deserialize>;
export type UniverseAction = Variant<Wire.UniverseAction_Deserialize>;
export type AppMessage = Variant<Wire.AppMessage_Deserialize>;
export type PlanningPreferences = Wire.PlanningPreferences_Serialize;
export type AutoBeaconPlan = Wire.AutoBeaconPlan_Serialize;
export type Milestone = Wire.Milestone_Serialize;
export type RecipeProductivity = Wire.RecipeProductivity_Serialize;
export type InfiniteTechLevel = Wire.InfiniteTechLevel_Serialize;
export type ProductivityView = Wire.ProductivityView_Serialize;
export type RecipeProductivityView = Wire.RecipeProductivityView;
export type OrbitalTransportCosts = Wire.OrbitalTransportCosts_Serialize;
export type ProjectSettings = Wire.ProjectSettings_Serialize;
export type LpSolver = Wire.LpSolver_Serialize;
export type Location = Wire.Location;
export type LocatedDualVar = Wire.LocatedDualVar_Serialize;
export type LocatedMechanic = Wire.LocatedMechanic_Serialize;
export type UniverseMechanicEntry = Wire.UniverseMechanicEntry_Serialize;
export type LocatedTarget = Wire.LocatedTarget_Serialize;
export type LocatedTargetTerm = Wire.LocatedTargetTerm_Serialize;
export type LocatedTargetExpression = Wire.LocatedTargetExpression_Serialize;
export type UniverseDocument = Wire.UniverseDocument_Serialize;
export type LocatedMechanicSolution = Wire.LocatedMechanicSolution_Serialize;
export type LocatedFlow = Wire.LocatedFlow_Serialize;
export type UniverseSolveResult = Wire.UniverseSolveResult_Serialize;
export type FactorySettings = Wire.FactorySettings_Serialize;
export type Suggestion = Wire.Suggestion;
export type BeaconConfig = Wire.BeaconConfig_Serialize;
export type ModuleConfig = Wire.ModuleConfig_Serialize;
export type Fuel = Wire.Fuel_Deserialize;
export type DispatchResult = Wire.DispatchResult_Serialize;
export type HistoryFocus = Wire.HistoryFocus_Serialize;
export type HistoryStatus = Wire.HistoryStatus_Serialize;
export type SolveResult = Variant<Wire.SolveResult_Serialize>;
export type RuntimeDiagnostic = Wire.Diagnostic_Serialize;
export type SolveDiagnostics = Wire.SolveDiagnostics_Serialize;
export type OptimizationReport = Wire.OptimizationReport_Serialize;
export type PivotCostChange = Wire.PivotCostChange;
export type SolveStatus = Wire.SolveStatus_Serialize;
export type MechanicSolution = Wire.MechanicSolution;
export type FlowBalance = Wire.FlowBalance_Serialize;
export type SolarBalance = Wire.SolarBalance;
export type ContextInfo = Wire.ContextInfo_Serialize;
export type ContextList = Wire.ContextList_Serialize;
export type TileCapabilities = Wire.TileCapabilities_Serialize;
export type CalculationStat = Wire.CalculationStat;
export type CalculationDetail = Wire.CalculationDetail_Serialize;
export type IndexEntry = Wire.IndexEntry_Serialize;
export type CatalogIndex = Wire.CatalogIndex_Serialize;
export type FlowAmount = Wire.FlowAmount_Serialize;
export type PrototypeDetail = Wire.PrototypeDetail_Serialize;
export type AutoPlanProgress = Wire.AutoPlanProgressEvent_Serialize;
export type Mechanic = { type: Wire.Mechanic_Serialize["type"] } & {
  [K in Exclude<UnionKeys<Wire.Mechanic_Serialize>, "type">]?: UnionValue<Wire.Mechanic_Serialize, K>
};
export type MechanicEntry = Omit<Wire.MechanicEntry_Serialize, "mechanic"> & { mechanic: Mechanic } & { location?: Location; transport?: "rocket-export" | "orbital-drop" };
export type FlowTarget = Wire.FlowTarget_Serialize & { location?: Location };
export type TargetTerm = Wire.TargetTerm_Serialize & { location?: Location };
export type ExternalInput = Wire.ExternalInput_Serialize & { location?: Location };
export type TargetExpression = Omit<Wire.TargetExpression_Serialize, "terms"> & { terms: TargetTerm[] };
export type FactoryDocument = Omit<Wire.FactoryDocument_Serialize, "mechanics" | "targets" | "target_expressions" | "external_inputs"> & { mechanics: MechanicEntry[]; targets: FlowTarget[]; target_expressions: TargetExpression[]; external_inputs: ExternalInput[] };
export type ProjectDocument = Omit<Wire.ProjectDocument_Serialize, "factories"> & { factories: FactoryDocument[] };
export type AppDocument = Omit<Wire.AppDocument_Serialize, "projects"> & { projects: ProjectDocument[] };
export function itemOf(flow: DualVar): IdWithQuality | undefined {
  if (flow !== null && typeof flow === "object" && "Item" in flow) {
    return flow.Item as IdWithQuality;
  }
  return undefined;
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


export type CatalogKind =
  | "item"
  | "fluid"
  | "recipe"
  | "module"
  | "machine"
  | "mining-machine"
  | "offshore-pump"
  | "generator"
  | "boiler"
  | "reactor"
  | "fusion-reactor"
  | "fusion-generator"
  | "solar-panel"
  | "accumulator"
  | "beacon"
  | "resource"
  | "entity"
  | "technology"
  | "planet"
  | "surface"
  | "quality"
  | "tile";

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
    case "offshore-pump":
    case "mining-machine":
    case "generator":
    case "boiler":
    case "reactor":
    case "fusion-reactor":
    case "fusion-generator":
    case "solar-panel":
    case "accumulator":
    case "beacon":
    case "resource":
      return "entity";
    default:
      return null;
  }
}
