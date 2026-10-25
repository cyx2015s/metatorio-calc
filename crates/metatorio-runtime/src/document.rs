use metatorio_core::{
    Accessible, DualVar, IdWithQuality, LocatedDualVar, LocatedMechanic, Mechanic, ModuleConfig,
};
use metatorio_solver::LpSolver;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::id::{
    ExternalInputId, FactoryId, MechanicId, ProjectId, TargetExpressionId, TargetId, TargetTermId,
    UniverseId,
};

pub const DOCUMENT_SCHEMA_VERSION: u32 = 2;

/// 可序列化的应用文档。选中项、对话框、求解结果与文件路径**故意**不放进这个结构里。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppDocument {
    pub schema_version: u32,
    pub projects: Vec<ProjectDocument>,
}

impl Default for AppDocument {
    fn default() -> Self {
        Self {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            projects: Vec::new(),
        }
    }
}

impl AppDocument {
    /// 旧工程把主品质放在工厂设置里；新版以第一个工厂为准迁移到项目。
    /// 新工程不会写入 `legacy_major_quality`，所以这里只会影响旧文档。
    pub fn migrate_legacy_major_quality(&mut self) {
        for project in &mut self.projects {
            let legacy = project
                .factories
                .first()
                .map(|factory| factory.settings.legacy_major_quality.clone())
                .unwrap_or_default();
            if !legacy.is_empty() {
                project.settings.major_quality = legacy;
            }
            for factory in &mut project.factories {
                factory.settings.legacy_major_quality.clear();
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectDocument {
    pub id: ProjectId,
    pub name: String,
    pub settings: ProjectSettings,
    pub planning: PlanningPreferences,
    /// 该项目使用的游戏上下文（缓存 id）。`None` = 使用应用当前激活的上下文。
    pub context_id: Option<String>,
    pub factories: Vec<FactoryDocument>,
    /// 宇宙工厂与普通工厂并列；两者共享项目级品质/里程碑/求解器。
    pub universes: Vec<UniverseDocument>,
}

impl Default for ProjectDocument {
    fn default() -> Self {
        Self {
            id: ProjectId::default(),
            name: "Unnamed project".to_string(),
            settings: ProjectSettings::default(),
            planning: PlanningPreferences::default(),
            context_id: None,
            factories: Vec::new(),
            universes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSettings {
    /// Estimated orbital logistics prices, independent of physical flow coefficients.
    pub orbital_transport_costs: OrbitalTransportCosts,
    pub time_scale: TimeScale,
    /// 里程碑（可达性节点级，默认是各科技瓶物品；`unlocked` 是**强制覆盖**——
    /// `unlocked=false` 强制不可达并阻断依赖它的对象，`unlocked=true` 强制可达，
    /// 自动解析（依赖传播）不能覆盖此状态）。
    pub milestones: Vec<Milestone>,
    /// 用户**手动固定**的配方产能（2.a）：指定某配方的产能为指定值，
    /// **替换**该配方的自动推算值。
    pub recipe_productivity: Vec<RecipeProductivity>,
    /// 用户对**无限科技**的研究次数覆盖（2.b）：据此推断该科技的产能贡献，
    /// 替换默认（按可达性推断的等级 1）。
    pub infinite_levels: Vec<InfiniteTechLevel>,
    /// 忽略产能加成（2.c）：丢弃按可达性自动推算的配方/采矿产能，
    /// 但保留用户设定值（recipe_productivity / infinite_levels / mining_productivity）。
    pub ignore_productivity: bool,
    /// 用户**手动固定**的采矿产出加成：作为替换值（设置后替换自动推算值）。
    pub mining_productivity: f64,
    pub all_accessible: bool,
    pub quality_limit: Option<String>,
    /// 项目级主品质：所有工厂/宇宙工厂共享同一品质口径。
    #[serde(default = "default_major_quality")]
    pub major_quality: String,
    /// 项目级 LP 后端。旧文档没有该字段时沿用 Rust based（序列化 id 为 rust-certified）。
    #[serde(default = "legacy_project_solver")]
    pub solver: LpSolver,
}

fn legacy_project_solver() -> LpSolver {
    LpSolver::RustBased
}

fn default_major_quality() -> String {
    "normal".to_string()
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            orbital_transport_costs: OrbitalTransportCosts::default(),
            time_scale: TimeScale::Seconds,
            milestones: Vec::new(),
            recipe_productivity: Vec::new(),
            infinite_levels: Vec::new(),
            ignore_productivity: false,
            mining_productivity: 0.0,
            all_accessible: false,
            quality_limit: None,
            major_quality: default_major_quality(),
            solver: LpSolver::default(),
        }
    }
}

/// Project-level estimates; future editors can expose these without changing
/// transport mechanics or the LP model. Weights in prototype data are grams.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct OrbitalTransportCosts {
    pub rocket_export_per_kg: f64,
    pub orbital_drop_per_stack: f64,
    /// Extra drop cost is this coefficient divided by effective spoil time in ticks.
    pub orbital_drop_spoil_penalty: f64,
}

impl Default for OrbitalTransportCosts {
    fn default() -> Self {
        Self {
            rocket_export_per_kg: 1.0,
            orbital_drop_per_stack: 64.0,
            orbital_drop_spoil_penalty: 60.0 * 60.0 * 60.0,
        }
    }
}

impl OrbitalTransportCosts {
    pub fn validate(&self) -> Result<(), String> {
        if [
            self.rocket_export_per_kg,
            self.orbital_drop_per_stack,
            self.orbital_drop_spoil_penalty,
        ]
        .into_iter()
        .all(|cost| cost.is_finite() && cost >= 0.0)
        {
            Ok(())
        } else {
            Err("轨道货运成本系数必须是有限的非负数".into())
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeScale {
    #[default]
    Seconds,
    Minutes,
    Hours,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Milestone {
    /// 里程碑节点（默认是科技瓶物品；也支持科技/配方等可达性对象）。
    pub node: Accessible,
    /// true = 已解锁；false = 未解锁（剪枝自身并阻断依赖它的对象）。
    pub unlocked: bool,
}

impl Default for Milestone {
    fn default() -> Self {
        Self {
            node: Accessible::Item(String::new()),
            unlocked: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RecipeProductivity {
    pub recipe: String,
    pub productivity: f64,
}

impl Default for RecipeProductivity {
    fn default() -> Self {
        Self {
            recipe: String::new(),
            productivity: 0.0,
        }
    }
}

/// 无限科技的研究次数覆盖（2.b）。
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct InfiniteTechLevel {
    pub tech: String,
    pub level: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FactoryDocument {
    pub id: FactoryId,
    pub name: String,
    pub settings: FactorySettings,
    pub targets: Vec<FlowTarget>,
    pub target_expressions: Vec<TargetExpression>,
    pub external_inputs: Vec<ExternalInput>,
    pub mechanics: Vec<MechanicEntry>,
    pub strict_source: bool,
    pub strict_sink: bool,
}

impl Default for FactoryDocument {
    fn default() -> Self {
        Self {
            id: FactoryId::default(),
            name: "Unnamed factory".to_string(),
            settings: FactorySettings::default(),
            targets: Vec::new(),
            target_expressions: Vec::new(),
            external_inputs: Vec::new(),
            mechanics: Vec::new(),
            strict_source: false,
            strict_sink: false,
        }
    }
}

/// 稳定的宇宙工厂机制条目（顺序与 ID 属于应用层）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UniverseMechanicEntry {
    pub id: MechanicId,
    pub enabled: bool,
    pub mechanic: LocatedMechanic,
}

impl Default for UniverseMechanicEntry {
    fn default() -> Self {
        Self {
            id: MechanicId::default(),
            enabled: true,
            mechanic: LocatedMechanic::default(),
        }
    }
}

/// 宇宙工厂的一个目标：目标流本身带地点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocatedTarget {
    pub id: TargetId,
    pub flow: LocatedDualVar,
    pub amount: f64,
}

impl Default for LocatedTarget {
    fn default() -> Self {
        Self {
            id: TargetId::default(),
            flow: LocatedDualVar::default(),
            amount: 0.0,
        }
    }
}

/// 宇宙目标表达式：每一项各自带地点，因此一个表达式可以横跨任意地点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LocatedTargetExpression {
    pub id: TargetExpressionId,
    pub constant: f64,
    pub terms: Vec<LocatedTargetTerm>,
}

impl Default for LocatedTargetExpression {
    fn default() -> Self {
        Self {
            id: TargetExpressionId::default(),
            constant: 1.0,
            terms: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LocatedTargetTerm {
    pub id: TargetTermId,
    pub flow: LocatedDualVar,
    pub coefficient: f64,
}

impl Default for LocatedTargetTerm {
    fn default() -> Self {
        Self {
            id: TargetTermId::default(),
            flow: LocatedDualVar::default(),
            coefficient: 1.0,
        }
    }
}

/// 宇宙工厂的手动外部输入；成本覆盖同地点的隐式来源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LocatedExternalInput {
    pub id: ExternalInputId,
    pub flow: LocatedDualVar,
    pub penalty: f64,
}

/// 跨地点联合规划的宇宙工厂。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UniverseDocument {
    pub id: UniverseId,
    pub name: String,
    pub mechanics: Vec<UniverseMechanicEntry>,
    pub targets: Vec<LocatedTarget>,
    pub target_expressions: Vec<LocatedTargetExpression>,
    pub external_inputs: Vec<LocatedExternalInput>,
    pub strict_source: bool,
    pub strict_sink: bool,
}

impl Default for UniverseDocument {
    fn default() -> Self {
        Self {
            id: UniverseId::default(),
            name: "Unnamed universe".to_string(),
            mechanics: Vec::new(),
            targets: Vec::new(),
            target_expressions: Vec::new(),
            external_inputs: Vec::new(),
            strict_source: false,
            strict_sink: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FactorySettings {
    pub planet: Option<String>,
    pub surface: Option<String>,
    /// 旧版工厂级主品质，只用于加载旧工程时迁移到项目设置；
    /// 新工程不再序列化该字段。
    #[serde(rename = "major_quality", default, skip_serializing)]
    pub legacy_major_quality: String,
    /// **遗留字段**：仅在旧版 metatorio-egui 工程里被写入，迁移时原样搬运，
    /// 当前**没有任何读取方**（求解、可达性、UI 都不用）。保留是为了工程文件
    /// 与旧格式迁移的兼容；`FactoryContextAction::SetDebug` 是它唯一的写入口，
    /// 前端故意不镜像该动作（见 types.ts 的 FactoryContextAction 注释）。
    pub debug: bool,
}

impl Default for FactorySettings {
    fn default() -> Self {
        Self {
            planet: None,
            surface: None,
            legacy_major_quality: String::new(),
            debug: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct FlowTarget {
    pub id: TargetId,
    pub flow: DualVar,
    pub amount: f64,
}

impl Default for FlowTarget {
    fn default() -> Self {
        Self {
            id: TargetId::default(),
            flow: DualVar::Unknown,
            amount: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TargetExpression {
    pub id: TargetExpressionId,
    pub constant: f64,
    pub terms: Vec<TargetTerm>,
}

impl Default for TargetExpression {
    fn default() -> Self {
        Self {
            id: TargetExpressionId::default(),
            constant: 1.0,
            terms: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TargetTerm {
    pub id: TargetTermId,
    pub flow: DualVar,
    pub coefficient: f64,
}

impl Default for TargetTerm {
    fn default() -> Self {
        Self {
            id: TargetTermId::default(),
            flow: DualVar::Unknown,
            coefficient: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ExternalInput {
    pub id: ExternalInputId,
    pub flow: DualVar,
    pub penalty: f64,
}

impl Default for ExternalInput {
    fn default() -> Self {
        Self {
            id: ExternalInputId::default(),
            flow: DualVar::Unknown,
            penalty: 1.0,
        }
    }
}

/// 一条有序的、面向用户的机制。core 里的枚举仍然只是「一个机制」；顺序与稳定身份
/// 属于应用层。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MechanicEntry {
    pub id: MechanicId,
    pub enabled: bool,
    pub mechanic: Mechanic,
}

impl MechanicEntry {
    pub fn new(id: MechanicId, kind: MechanicKind) -> Option<Self> {
        Some(Self {
            id,
            enabled: true,
            mechanic: kind.default_mechanic()?,
        })
    }

    pub fn kind(&self) -> MechanicKind {
        MechanicKind::of(&self.mechanic)
    }
}

impl Default for MechanicEntry {
    fn default() -> Self {
        Self::new(MechanicId::default(), MechanicKind::Recipe).expect("recipe is supported")
    }
}

/// 自动规划使用的项目级选项。它们不属于 core 机制、也不绑定任何单个机制——只描述
/// 应用如何为项目里的每个机制枚举备选方案。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanningPreferences {
    /// Include enabled manual mechanisms as candidates, and append results
    /// without replacing existing entries. Manual surface exceptions are kept.
    pub preserve_existing_mechanics: bool,
    pub alternative_count: usize,
    pub machine_preferences: Vec<IdWithQuality>,
    pub enumerate_modules: Vec<IdWithQuality>,
    pub enumerate_beacons: Vec<AutoBeaconPlan>,
}

impl Default for PlanningPreferences {
    fn default() -> Self {
        Self {
            // 每种配方默认枚举 3 台候选机器：不同机器的运行条件不同
            // （如 cryogenic-plant 需要氟利昂冷却，雷星无法自产），只枚举
            // 速度最快的 1 台会漏掉真正可行的替代机器，导致链路断裂。
            preserve_existing_mechanics: false,
            alternative_count: 3,
            machine_preferences: Vec::new(),
            enumerate_modules: Vec::new(),
            enumerate_beacons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AutoBeaconPlan {
    pub module_config: ModuleConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MechanicKind {
    #[default]
    Recipe,
    Mining,
    Spoil,
    Plant,
    ItemFuel,
    ItemLaunch,
    Generator,
    Boiler,
    Reactor,
    Solar,
    FluidFuel,
    FluidHeat,
    TileExtract,
    TileDispose,
    VirtualScience,
    Unsupported,
}

impl MechanicKind {
    pub const ALL: [Self; 15] = [
        Self::Recipe,
        Self::Mining,
        Self::Spoil,
        Self::Plant,
        Self::ItemFuel,
        Self::ItemLaunch,
        Self::Generator,
        Self::Boiler,
        Self::Reactor,
        Self::Solar,
        Self::FluidFuel,
        Self::FluidHeat,
        Self::TileExtract,
        Self::TileDispose,
        Self::VirtualScience,
    ];

    pub fn default_mechanic(self) -> Option<Mechanic> {
        Some(match self {
            Self::Recipe => Mechanic::Recipe(Default::default()),
            Self::Mining => Mechanic::Mining(Default::default()),
            Self::Spoil => Mechanic::Spoil(Default::default()),
            Self::Plant => Mechanic::Plant(Default::default()),
            Self::ItemFuel => Mechanic::ItemFuel(Default::default()),
            Self::ItemLaunch => Mechanic::ItemLaunch(Default::default()),
            Self::Generator => Mechanic::Generator(Default::default()),
            Self::Boiler => Mechanic::Boiler(Default::default()),
            Self::Reactor => Mechanic::Reactor(Default::default()),
            Self::Solar => Mechanic::Solar(Default::default()),
            Self::FluidFuel => Mechanic::FluidFuel(Default::default()),
            Self::FluidHeat => Mechanic::FluidHeat(Default::default()),
            Self::TileExtract => Mechanic::TileExtract(Default::default()),
            Self::TileDispose => Mechanic::TileDispose(Default::default()),
            Self::VirtualScience => Mechanic::VirtualScience(Default::default()),
            Self::Unsupported => return None,
        })
    }

    pub fn of(mechanic: &Mechanic) -> Self {
        match mechanic {
            Mechanic::Recipe(_) => Self::Recipe,
            Mechanic::Mining(_) => Self::Mining,
            Mechanic::Spoil(_) => Self::Spoil,
            Mechanic::Plant(_) => Self::Plant,
            Mechanic::ItemFuel(_) => Self::ItemFuel,
            Mechanic::ItemLaunch(_) => Self::ItemLaunch,
            Mechanic::Generator(_) => Self::Generator,
            Mechanic::Boiler(_) => Self::Boiler,
            Mechanic::Reactor(_) => Self::Reactor,
            Mechanic::Solar(_) => Self::Solar,
            Mechanic::FluidFuel(_) => Self::FluidFuel,
            Mechanic::FluidHeat(_) => Self::FluidHeat,
            Mechanic::TileExtract(_) => Self::TileExtract,
            Mechanic::TileDispose(_) => Self::TileDispose,
            Mechanic::VirtualScience(_) => Self::VirtualScience,
            _ => Self::Unsupported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_project_settings_stay_on_rust_and_new_projects_pick_a_compiled_solver() {
        let legacy: ProjectSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.solver, LpSolver::RustBased);
        let fresh = ProjectSettings::default();
        assert!(fresh.solver.is_available());
        assert_eq!(fresh.solver, LpSolver::default());
        let encoded = serde_json::to_value(&fresh).unwrap();
        assert_eq!(encoded["solver"], fresh.solver.as_str());
    }

    #[test]
    fn legacy_factory_major_quality_migrates_to_the_project() {
        let json = serde_json::json!({
            "projects": [{
                "id": 1,
                "name": "legacy",
                "factories": [{
                    "id": 2,
                    "name": "old",
                    "settings": { "major_quality": "epic" }
                }]
            }]
        });
        let mut document: AppDocument = serde_json::from_value(json).unwrap();
        document.migrate_legacy_major_quality();
        assert_eq!(document.projects[0].settings.major_quality, "epic");
        let encoded = serde_json::to_value(&document).unwrap();
        assert!(
            encoded["projects"][0]["factories"][0]["settings"]
                .get("major_quality")
                .is_none()
        );
    }

    #[test]
    fn all_current_core_mechanics_have_runtime_defaults() {
        for kind in MechanicKind::ALL {
            assert!(kind.default_mechanic().is_some());
        }
    }

    #[test]
    fn document_roundtrip_keeps_ordered_mechanic_entries() {
        let project = ProjectDocument {
            id: ProjectId(7),
            ..ProjectDocument::default()
        };
        let project = ProjectDocument {
            factories: vec![FactoryDocument {
                id: FactoryId(9),
                mechanics: vec![MechanicEntry::new(MechanicId(12), MechanicKind::Mining).unwrap()],
                ..FactoryDocument::default()
            }],
            ..project
        };
        let document = AppDocument {
            projects: vec![project],
            ..AppDocument::default()
        };
        let encoded = serde_json::to_string(&document).unwrap();
        let decoded: AppDocument = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, document);
    }
}
