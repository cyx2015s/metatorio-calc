use metatorio_core::{DualVar, IdWithQuality};
use serde::{Deserialize, Serialize};

use crate::document::{
    AutoBeaconPlan, FlowTarget, MechanicKind, RecipeProductivity, TargetExpression, TargetTerm,
    TechnologyMilestone, TimeScale,
};
use crate::id::{
    ExternalInputId, FactoryId, MechanicId, ProjectId, TargetExpressionId, TargetId, TargetTermId,
};

/// Framework-independent user intent.  Rendering code should emit these
/// values instead of mutating the project document directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "action", rename_all = "kebab-case")]
pub enum AppMessage {
    Application(ApplicationAction),
    Project {
        project: ProjectId,
        action: ProjectAction,
    },
    Factory {
        project: ProjectId,
        factory: FactoryId,
        action: FactoryAction,
    },
    Ui(UiAction),
}

pub type RuntimeMessage = AppMessage;

/// File, data-context, update, and process-level operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApplicationAction {
    NewProject {
        name: String,
    },
    OpenProject {
        path: String,
    },
    SaveProject {
        project: ProjectId,
    },
    SaveProjectAs {
        project: ProjectId,
        path: String,
    },
    CloseProject {
        project: ProjectId,
        decision: CloseDecision,
    },
    DeleteProject {
        project: ProjectId,
        decision: DeleteDecision,
    },
    ReorderProject {
        project: ProjectId,
        position: usize,
    },
    LoadGameContext {
        executable_path: String,
        mod_path: Option<String>,
    },
    LoadCachedContext,
    CheckForUpdate,
    InstallUpdate,
    RestartAfterUpdate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloseDecision {
    Cancel,
    Discard,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeleteDecision {
    Cancel,
    Confirm,
}

/// Persistent project-level changes formerly handled by ProjectContext.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectAction {
    SetName {
        name: String,
    },
    AddFactory {
        name: String,
        template: FactoryTemplate,
    },
    CloneFactory {
        factory: FactoryId,
    },
    RemoveFactory {
        factory: FactoryId,
    },
    ReorderFactory {
        factory: FactoryId,
        position: usize,
    },
    SetTimeScale {
        time_scale: TimeScale,
    },
    SetAllAccessible {
        enabled: bool,
    },
    AddTechnologyMilestone {
        milestone: TechnologyMilestone,
    },
    ReplaceTechnologyMilestone {
        technology: String,
        replacement: String,
    },
    SetTechnologyUnlocked {
        technology: String,
        unlocked: bool,
    },
    RemoveTechnologyMilestone {
        technology: String,
    },
    SetMiningProductivity {
        productivity: f64,
    },
    SetIgnoreProductivity {
        ignore: bool,
    },
    SetRecipeProductivity {
        productivity: RecipeProductivity,
    },
    RemoveRecipeProductivity {
        recipe: String,
    },
    SetQualityLimit {
        quality: Option<String>,
    },
    /// 把项目绑定到某个游戏上下文（缓存 id）；`None` 表示用当前激活上下文。
    SetContext {
        context: Option<String>,
    },
    Planning(PlanningAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryTemplate {
    Empty,
    DefaultMechanics,
}

/// Changes to one factory document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryAction {
    SetName {
        name: String,
    },
    SetStrictSource {
        strict: bool,
    },
    SetStrictSink {
        strict: bool,
    },
    Context(FactoryContextAction),
    Target(TargetAction),
    TargetExpression(TargetExpressionAction),
    ExternalInput(ExternalInputAction),
    MechanicList(MechanicListAction),
    Mechanic {
        mechanic: MechanicId,
        action: MechanicAction,
    },
    Flow(FlowAction),
    Suggestion(SuggestionAction),
    Cleanup(CleanupAction),
    Solve(SolveAction),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryContextAction {
    SetPlanet { planet: Option<String> },
    SetSurface { surface: Option<String> },
    SetMajorQuality { quality: String },
    SetDebug { enabled: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetAction {
    Add { target: FlowTarget },
    Remove { target: TargetId },
    SetFlow { target: TargetId, flow: DualVar },
    SetAmount { target: TargetId, amount: f64 },
    Reorder { target: TargetId, position: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetExpressionAction {
    Add {
        expression: TargetExpression,
    },
    Remove {
        expression: TargetExpressionId,
    },
    SetConstant {
        expression: TargetExpressionId,
        constant: f64,
    },
    AddTerm {
        expression: TargetExpressionId,
        term: TargetTerm,
    },
    RemoveTerm {
        expression: TargetExpressionId,
        term: TargetTermId,
    },
    SetTermFlow {
        expression: TargetExpressionId,
        term: TargetTermId,
        flow: DualVar,
    },
    SetTermCoefficient {
        expression: TargetExpressionId,
        term: TargetTermId,
        coefficient: f64,
    },
    Reorder {
        expression: TargetExpressionId,
        position: usize,
    },
    ReorderTerm {
        expression: TargetExpressionId,
        term: TargetTermId,
        position: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalInputAction {
    Add {
        input: crate::document::ExternalInput,
    },
    Remove {
        input: ExternalInputId,
    },
    SetFlow {
        input: ExternalInputId,
        flow: DualVar,
    },
    SetPenalty {
        input: ExternalInputId,
        penalty: f64,
    },
    Reorder {
        input: ExternalInputId,
        position: usize,
    },
    ReplaceFromLocation {
        location: ExternalLocation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalLocation {
    Planet(String),
    Surface(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MechanicListAction {
    Add {
        kind: MechanicKind,
    },
    Remove {
        mechanic: MechanicId,
    },
    Clone {
        mechanic: MechanicId,
    },
    Reorder {
        mechanic: MechanicId,
        position: usize,
    },
    SetEnabled {
        mechanic: MechanicId,
        enabled: bool,
    },
}

/// Operations on one mechanic, tagged by mechanic kind.
///
/// Each variant carries exactly the operations that kind supports (matching
/// the field set of the corresponding core `Mechanic` struct), so a recipe
/// mechanic cannot receive a mining operation and vice versa — the reducer
/// rejects a kind mismatch without touching the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MechanicAction {
    Recipe(RecipeMechanicAction),
    Mining(MiningMechanicAction),
    Spoil(SpoilMechanicAction),
    Plant(PlantMechanicAction),
    ItemFuel(ItemFuelMechanicAction),
    ItemLaunch(ItemLaunchMechanicAction),
    Generator(GeneratorMechanicAction),
    Boiler(BoilerMechanicAction),
    Reactor(ReactorMechanicAction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecipeMechanicAction {
    SetRecipe { recipe: IdWithQuality },
    SetMachine { machine: IdWithQuality },
    SetFuel { fuel: Option<String> },
    SetFuelTemperature { temperature: Option<i32> },
    Module(ModuleAction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MiningMechanicAction {
    SetResource { resource: String },
    SetMachine { machine: IdWithQuality },
    SetFuel { fuel: Option<String> },
    SetFuelTemperature { temperature: Option<i32> },
    Module(ModuleAction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpoilMechanicAction {
    SetItem { item: IdWithQuality },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlantMechanicAction {
    SetSeed { seed: IdWithQuality },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemFuelMechanicAction {
    SetItem { item: IdWithQuality },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemLaunchMechanicAction {
    SetItem { item: IdWithQuality },
    SetWeightMode { weight_mode: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorMechanicAction {
    SetGenerator { generator: IdWithQuality },
    SetFluid { fluid: String },
    SetTemperature { temperature: Option<i32> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoilerMechanicAction {
    SetBoiler { boiler: IdWithQuality },
    SetFluid { fluid: String },
    SetTemperature { temperature: Option<i32> },
    SetFuel { fuel: Option<String> },
    SetFuelTemperature { temperature: Option<i32> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactorMechanicAction {
    SetReactor { reactor: IdWithQuality },
    SetFuel { fuel: Option<String> },
    SetNeighbours { neighbours: u8 },
}

/// Operations emitted by the old ModuleConfigEditor, expressed in terms of
/// slots and stable list positions rather than mouse buttons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleAction {
    SetModuleSlot {
        slot: usize,
        module: Option<IdWithQuality>,
    },
    /// 钳制模块数量到机器槽位上限（由外层适配层触发）。
    ClampModules {
        max: usize,
    },
    ClearModules,
    /// 添加一个信标配置。必须携带信标本体（不允许空 id），且不能与已有
    /// 信标重复——"信标配置必须绑定一个有效信标"。
    AddBeacon {
        beacon: IdWithQuality,
    },
    RemoveBeacon {
        beacon: usize,
    },
    SetBeacon {
        beacon: usize,
        value: IdWithQuality,
    },
    SetBeaconCount {
        beacon: usize,
        count: usize,
    },
    SetBeaconShare {
        beacon: usize,
        share: f64,
    },
    AddBeaconModule {
        beacon: usize,
        module: IdWithQuality,
    },
    RemoveBeaconModule {
        beacon: usize,
        module: usize,
    },
    SetBeaconModule {
        beacon: usize,
        module: usize,
        value: IdWithQuality,
    },
    SetBeaconModuleCount {
        beacon: usize,
        module: usize,
        count: usize,
    },
}

/// Project-global automatic-planning preferences.  These describe how the
/// planner enumerates alternatives and are intentionally NOT bound to any
/// single mechanic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanningAction {
    SetAlternativeCount {
        count: usize,
    },
    AddMachinePreference {
        machine: IdWithQuality,
    },
    RemoveMachinePreference {
        machine: IdWithQuality,
    },
    ReorderMachinePreference {
        machine: IdWithQuality,
        position: usize,
    },
    AddEnumeratedModule {
        module: IdWithQuality,
    },
    RemoveEnumeratedModule {
        module: IdWithQuality,
    },
    UseBestModules,
    AddEnumeratedBeacon,
    RemoveEnumeratedBeacon {
        beacon: usize,
    },
    SetEnumeratedBeacon {
        beacon: usize,
        plan: AutoBeaconPlan,
    },
    EnumeratedBeaconModule {
        beacon: usize,
        action: ModuleAction,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowAction {
    AddToTarget { flow: DualVar, amount: f64 },
    AddToExternalInput { flow: DualVar, penalty: f64 },
    RequestSuggestions { flow: DualVar, amount: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuggestionAction {
    SelectMechanic { mechanic: MechanicId },
    SetFilter { filter: String },
    Accept { candidate: SuggestionCandidate },
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuggestionCandidate {
    Recipe { recipe: IdWithQuality },
    Resource { resource: String },
    ItemFuel { item: IdWithQuality },
    Generator { generator: IdWithQuality },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CleanupAction {
    RemoveUnused,
    RemoveUnsolvable,
    SortBySolutionRate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SolveAction {
    Recompute,
    AutoPlan,
}

/// Transient operations are separate from document actions and must not enter
/// undo history.  They cover selector state, tabs, logs, and the old window
/// close confirmation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiAction {
    SelectProject {
        project: Option<ProjectId>,
    },
    SelectFactory {
        factory: Option<FactoryId>,
    },
    SelectPage {
        page: ProjectPage,
    },
    OpenSelector {
        target: SelectorTarget,
    },
    CloseSelector,
    SetSelectorQuery {
        query: String,
    },
    SelectSelectorGroup {
        group: usize,
    },
    SelectSelectorSubgroup {
        subgroup: usize,
    },
    CommitSelector {
        target: SelectorTarget,
        value: SelectorValue,
    },
    SelectSuggestionMechanic {
        mechanic: usize,
    },
    OpenLogs,
    SetFontFilter {
        filter: String,
    },
    SelectFont {
        font: String,
    },
    SetLocale {
        locale: String,
    },
    ReloadIcons,
    RequestWindowClose,
    ResolveWindowClose {
        decision: CloseDecision,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectPage {
    Preferences,
    Factory(FactoryId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectorTarget {
    ProjectPlanet,
    ProjectSurface,
    ProjectQuality,
    Technology,
    Target {
        factory: FactoryId,
        target: TargetId,
    },
    TargetTerm {
        factory: FactoryId,
        expression: TargetExpressionId,
        term: TargetTermId,
    },
    ExternalInput {
        factory: FactoryId,
        input: ExternalInputId,
    },
    Mechanic {
        factory: FactoryId,
        mechanic: MechanicId,
        kind: SelectorKind,
    },
    ModuleSlot {
        factory: FactoryId,
        mechanic: MechanicId,
        slot: usize,
    },
    Beacon {
        factory: FactoryId,
        mechanic: MechanicId,
        beacon: usize,
    },
    BeaconModule {
        factory: FactoryId,
        mechanic: MechanicId,
        beacon: usize,
        module: usize,
    },
    EnumeratedModule {
        factory: FactoryId,
        mechanic: MechanicId,
    },
    EnumeratedBeacon {
        factory: FactoryId,
        mechanic: MechanicId,
        beacon: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectorKind {
    Item,
    Fluid,
    Entity,
    Recipe,
    Technology,
    Planet,
    Surface,
    Quality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectorValue {
    Name(String),
    IdWithQuality(IdWithQuality),
}

/// Effects requested by a reducer after applying an AppMessage.  Keeping
/// these explicit makes the future Tauri adapter thin and testable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeCommand {
    Recompute {
        project: ProjectId,
        factory: FactoryId,
    },
    /// 机制主字段（配方/资源）变化后，由外层校验机器兼容性并回退。
    EnsureMachineCompat {
        project: ProjectId,
        factory: FactoryId,
        mechanic: MechanicId,
    },
    /// 文档变更后，由外层校验项目品质上限是否低于文档中出现的品质
    /// （目标/外部输入/机制），低于则自动提升。
    EnsureQualityLimit {
        project: ProjectId,
    },
    /// 机器变化后，由外层按机器槽位上限钳制模块数量。
    ClampModules {
        project: ProjectId,
        factory: FactoryId,
        mechanic: MechanicId,
    },
    AutoPlan {
        project: ProjectId,
        factory: FactoryId,
    },
    Persist {
        project: ProjectId,
        path: Option<String>,
    },
    LoadProject {
        path: String,
    },
    LoadGameContext {
        executable_path: String,
        mod_path: Option<String>,
    },
    LoadCachedContext,
    CloseProject {
        project: ProjectId,
    },
    CheckForUpdate,
    InstallUpdate,
    RestartAfterUpdate,
    ReplaceExternalInputs {
        project: ProjectId,
        factory: FactoryId,
        location: ExternalLocation,
    },
    RequestSuggestions {
        project: ProjectId,
        factory: FactoryId,
        flow: DualVar,
        amount: f64,
    },
    Cleanup {
        project: ProjectId,
        factory: FactoryId,
        action: CleanupAction,
    },
    UseBestModules {
        project: ProjectId,
        factory: FactoryId,
        mechanic: MechanicId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_mechanic_message_roundtrips_as_tagged_json() {
        let message = AppMessage::Factory {
            project: ProjectId(1),
            factory: FactoryId(2),
            action: FactoryAction::Mechanic {
                mechanic: MechanicId(3),
                action: MechanicAction::Recipe(RecipeMechanicAction::Module(
                    ModuleAction::SetModuleSlot {
                        slot: 1,
                        module: Some(IdWithQuality::new("speed-module-3", "rare")),
                    },
                )),
            },
        };
        let encoded = serde_json::to_value(&message).unwrap();
        assert_eq!(encoded["scope"], "factory");
        assert_eq!(
            encoded["action"]["action"]["mechanic"]["action"]["recipe"]["module"]["set-module-slot"]
                ["slot"],
            1
        );
        let decoded: AppMessage = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn old_planner_operations_have_distinct_document_messages() {
        let actions = [
            FactoryAction::Target(TargetAction::SetAmount {
                target: TargetId(1),
                amount: 60.0,
            }),
            FactoryAction::ExternalInput(ExternalInputAction::SetPenalty {
                input: ExternalInputId(2),
                penalty: 1.0,
            }),
            FactoryAction::MechanicList(MechanicListAction::Reorder {
                mechanic: MechanicId(3),
                position: 0,
            }),
            FactoryAction::Solve(SolveAction::AutoPlan),
        ];
        assert_eq!(actions.len(), 4);
    }
}
