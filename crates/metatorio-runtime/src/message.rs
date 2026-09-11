use metatorio_core::{Accessible, DualVar, Fuel, IdWithQuality};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::document::{
    AutoBeaconPlan, FlowTarget, InfiniteTechLevel, MechanicKind, RecipeProductivity,
    TargetExpression, TargetTerm, TimeScale,
};
use crate::id::{
    ExternalInputId, FactoryId, MechanicId, ProjectId, TargetExpressionId, TargetId, TargetTermId,
};

/// Framework-independent user intent.  Rendering code should emit these
/// values instead of mutating the project document directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
}

pub type RuntimeMessage = AppMessage;

/// File, data-context, update, and process-level operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    /// 切换当前激活的上下文（`None` = 不激活任何上下文）。
    ///
    /// 上下文注册表（磁盘缓存清单 + 内存 store）由 app 层持有，reducer 不
    /// 接触它，因此 reducer 只发 [`RuntimeCommand::SetActiveContext`]；app 层
    /// 负责校验 id 是否存在、按需从磁盘载入并广播 `contexts-changed`。
    SetActiveContext {
        context: Option<String>,
    },
    /// 重命名已注册的上下文（只改显示名；id 是内容哈希，不可变）。
    RenameContext {
        id: String,
        name: String,
    },
    /// 删除已注册的上下文：被项目引用时拒绝，否则清缓存并卸载 store。
    DeleteContext {
        id: String,
    },
    /// **未实现**（三条更新动作）：reducer 会发出对应的 `RuntimeCommand`，但
    /// app 层没有实现——GUI 的自动更新走前端 `@tauri-apps/plugin-updater`。
    /// 调用会拿到明确的「未实现」错误，而不是静默成功。
    CheckForUpdate,
    InstallUpdate,
    RestartAfterUpdate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CloseDecision {
    Cancel,
    Discard,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DeleteDecision {
    Cancel,
    Confirm,
}

/// Persistent project-level changes formerly handled by ProjectContext.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// 添加里程碑（默认解锁；`unlocked=false` 的节点剪枝——自身不可达并
    /// 阻断依赖它的对象，模拟科技树不同分支）。
    AddMilestone {
        node: Accessible,
        unlocked: bool,
    },
    SetMilestoneUnlocked {
        node: Accessible,
        unlocked: bool,
    },
    RemoveMilestone {
        node: Accessible,
    },
    /// 把里程碑整体重置为「默认集合」（实验室消耗的科技瓶物品，全部解锁）。
    ///
    /// 默认集合由原型仓库推导，而 reducer 不持有 store，因此本变体在
    /// [`crate::Runtime::dispatch`] 进入 reducer 之前被拦截解析；reducer 层
    /// 的同名分支只兜底报错（见 `RuntimeState::dispatch`）。
    SetDefaultMilestones,
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
    SetInfiniteTechLevel {
        level: InfiniteTechLevel,
    },
    RemoveInfiniteTechLevel {
        tech: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryTemplate {
    Empty,
    DefaultMechanics,
}

/// Changes to one factory document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    Cleanup(CleanupAction),
    Solve(SolveAction),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryContextAction {
    SetPlanet { planet: Option<String> },
    SetSurface { surface: Option<String> },
    SetMajorQuality { quality: String },
    SetDebug { enabled: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TargetAction {
    Add { target: FlowTarget },
    Remove { target: TargetId },
    SetFlow { target: TargetId, flow: DualVar },
    SetAmount { target: TargetId, amount: f64 },
    Reorder { target: TargetId, position: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// **未实现**：reducer 会发出 `ReplaceExternalInputs` 命令，app 层没有实现
    /// （GUI 的「隐式来源」走只读的 `implicit_sources` 命令 + `external-input`
    /// 消息）。调用会拿到明确的「未实现」错误。
    ReplaceFromLocation {
        location: ExternalLocation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalLocation {
    Planet(String),
    Surface(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    Solar(SolarMechanicAction),
    FluidFuel(FluidFuelMechanicAction),
    FluidHeat(FluidHeatMechanicAction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RecipeMechanicAction {
    SetRecipe { recipe: IdWithQuality },
    SetMachine { machine: IdWithQuality },
    SetFuel { fuel: Option<Fuel> },
    SetFuelTemperature { temperature: Option<i32> },
    Module(ModuleAction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MiningMechanicAction {
    SetResource { resource: String },
    SetMachine { machine: IdWithQuality },
    SetFuel { fuel: Option<Fuel> },
    SetFuelTemperature { temperature: Option<i32> },
    Module(ModuleAction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SpoilMechanicAction {
    SetItem { item: IdWithQuality },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PlantMechanicAction {
    SetSeed { seed: IdWithQuality },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ItemFuelMechanicAction {
    SetItem { item: IdWithQuality },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ItemLaunchMechanicAction {
    SetItem { item: IdWithQuality },
    SetWeightMode { weight_mode: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorMechanicAction {
    SetGenerator { generator: IdWithQuality },
    SetFluid { fluid: String },
    SetTemperature { temperature: Option<i32> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BoilerMechanicAction {
    SetBoiler { boiler: IdWithQuality },
    SetFluid { fluid: String },
    SetTemperature { temperature: Option<i32> },
    SetFuel { fuel: Option<Fuel> },
    SetFuelTemperature { temperature: Option<i32> },
    // SetMode（工作模式）已移除：锅炉 mode 只读，运行/展开时从原型读取。
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ReactorMechanicAction {
    SetReactor { reactor: IdWithQuality },
    SetFuel { fuel: Option<Fuel> },
    SetNeighbours { neighbours: u8 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SolarMechanicAction {
    SetSolarPanel { solar_panel: IdWithQuality },
    SetAccumulator { accumulator: IdWithQuality },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FluidFuelMechanicAction {
    SetFluid { fluid: String },
    SetTemperature { temperature: Option<i32> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FluidHeatMechanicAction {
    SetFluid { fluid: String },
    SetTemperature { temperature: Option<i32> },
}

/// Operations emitted by the old ModuleConfigEditor, expressed in terms of
/// slots and stable list positions rather than mouse buttons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleAction {
    SetModuleSlot {
        slot: usize,
        module: Option<IdWithQuality>,
    },
    /// 钳制插件数量到机器槽位上限（由外层适配层触发）。
    ClampModules {
        max: usize,
    },
    ClearModules,
    /// 添加一个插件塔配置。必须携带插件塔本体（不允许空 id），且不能与已有
    /// 插件塔重复——"插件塔配置必须绑定一个有效插件塔"。
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// 用「每个插件类别中 tier 最高的插件」整体替换**项目级**枚举插件列表
    /// （`planning.enumerate_modules`，自动规划参与组合的插件集合）。
    ///
    /// 语义要点：
    /// - 这是**项目级**偏好，所以不接收 factory / mechanic——旧版
    ///   `UseBestModules { factory, mechanic }` 把 factory/mechanic 塞进一个项目级
    ///   设置里，语义错位，且 app 层从未实现；
    /// - **品质必须由调用方显式给出**，运行时不做任何推断。品质是特殊维度：
    ///   「解锁了某个品质」不等于「能大规模量产该品质的插件」（产能、废料、
    ///   配方链都可能卡住），所以不能拿「项目品质上限」之类的东西当默认值。
    ///   GUI 的「使用最佳插件」传当前工厂的主品质（`factory.settings.major_quality`），
    ///   与品质无关的调用方应传 `normal`；
    /// - 候选按**当前可达性**过滤（不可达插件不进枚举列表）；
    /// - 同类别 tier 并列时取名字最小者，保证确定性与幂等。
    ///
    /// 需要原型仓库（reducer 不持有），因此在 [`crate::Runtime::dispatch`] 进入
    /// reducer 之前拦截解析；reducer 分支只兜底报错。
    UseBestModules {
        quality: String,
    },
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FlowAction {
    AddToTarget {
        flow: DualVar,
        amount: f64,
    },
    AddToExternalInput {
        flow: DualVar,
        penalty: f64,
    },
    /// **未实现**：reducer 会发出 `RequestSuggestions` 命令，但 app 层没有实现
    /// （GUI 的「建议」面板走只读的 `suggest` 命令，返回候选后由前端用
    /// `mechanic-list` 消息落地）。调用会拿到明确的「未实现」错误。
    RequestSuggestions {
        flow: DualVar,
        amount: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CleanupAction {
    RemoveUnused,
    RemoveUnsolvable,
    SortBySolutionRate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SolveAction {
    /// 按工厂**当前的** `strict_source` / `strict_sink` 重新求解（不修改文档设置）。
    Recompute,
    /// 自动规划：按项目的枚举偏好生成候选机制 → LP 选优 → **回写机制列表** → 重解。
    ///
    /// **总是按严格供给（strict source）求解**，并在回写时把工厂的 `strict_source`
    /// 置为 true：规划用严格供给算出 A，而普通重解按工厂设置可能给出 B，同一个工厂
    /// 两套结果会让人和 agent 都无法判断哪个算数。因此调用方**不需要**先手动开严格
    /// 供给；如果只想按当前设置重解，用 `recompute`。
    ///
    /// 候选空间由 `planning`（替代数量 / 机器偏好 / 枚举插件 / 枚举插件塔）决定。
    AutoPlan,
}

/// Effects requested by a reducer after applying an AppMessage.  Keeping
/// these explicit makes the future Tauri adapter thin and testable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// 机器变化后，由外层按机器槽位上限钳制插件数量。
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
    /// 显式保存到「已记忆的路径」：与自动落盘的 [`Self::Persist`] 不同，这里
    /// 没有记忆路径时由 app 层**报错**（而不是静默 no-op），否则 agent 会把
    /// 「什么都没做」当成保存成功。
    SaveProject {
        project: ProjectId,
    },
    LoadProject {
        path: String,
    },
    LoadGameContext {
        executable_path: String,
        mod_path: Option<String>,
    },
    LoadCachedContext,
    /// 切换当前激活的上下文（`None` = 不激活；项目需自带 `context_id`）。
    /// 上下文注册表在 app 层，reducer 只发命令。
    SetActiveContext {
        context: Option<String>,
    },
    /// 重命名已注册的上下文（只改显示名；`id` 是内容哈希，不可变）。
    RenameContext {
        id: String,
        name: String,
    },
    /// 删除已注册的上下文；被项目引用时由 app 层拒绝。
    DeleteContext {
        id: String,
    },
    CloseProject {
        project: ProjectId,
    },
    /// 以下三条命令**已在 reducer 中声明但 app 层未实现**（`execute_command`
    /// 返回明确的「未实现」错误）：更新三连由前端 updater 插件承担，
    /// `ReplaceExternalInputs` / `RequestSuggestions` 的等效能力由只读命令
    /// `implicit_sources` / `suggest` 提供。保留变体是为了让协议不自相矛盾地
    /// 假装支持，同时给 headless 模式留接口。
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
