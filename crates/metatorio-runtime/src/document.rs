use metatorio_core::{Accessible, DualVar, IdWithQuality, Mechanic, ModuleConfig};
use serde::{Deserialize, Serialize};

use crate::id::{
    ExternalInputId, FactoryId, MechanicId, ProjectId, TargetExpressionId, TargetId, TargetTermId,
};

pub const DOCUMENT_SCHEMA_VERSION: u32 = 1;

/// Serializable application document.  Selection, dialogs, solver results,
/// and file paths are intentionally kept outside this structure.
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSettings {
    pub time_scale: TimeScale,
    pub tech_milestones: Vec<TechnologyMilestone>,
    pub recipe_productivity: Vec<RecipeProductivity>,
    pub ignore_productivity: bool,
    pub mining_productivity: f64,
    pub all_accessible: bool,
    /// 用户显式标记为可达的对象（即使无任何来源也可达，并入根种子）。
    pub marked_accessible: Vec<Accessible>,
    /// 用户显式标记为不可达的对象（剪枝：自身不可达，并阻断依赖它的对象）。
    pub marked_inaccessible: Vec<Accessible>,
    pub quality_limit: Option<String>,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            time_scale: TimeScale::Seconds,
            tech_milestones: Vec::new(),
            recipe_productivity: Vec::new(),
            ignore_productivity: false,
            mining_productivity: 0.0,
            all_accessible: false,
            marked_accessible: Vec::new(),
            marked_inaccessible: Vec::new(),
            quality_limit: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeScale {
    #[default]
    Seconds,
    Minutes,
    Hours,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TechnologyMilestone {
    pub technology: String,
    pub unlocked: bool,
}

impl Default for TechnologyMilestone {
    fn default() -> Self {
        Self {
            technology: String::new(),
            unlocked: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FactorySettings {
    pub planet: Option<String>,
    pub surface: Option<String>,
    pub major_quality: String,
    pub debug: bool,
}

impl Default for FactorySettings {
    fn default() -> Self {
        Self {
            planet: None,
            surface: None,
            major_quality: "normal".to_string(),
            debug: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// One ordered user-facing mechanism.  The core enum is still a single
/// mechanism; ordering and stable identity belong to the application layer.
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

/// Project-global options used by automatic planning.  They are not part of
/// core mechanics and not bound to any single mechanic — they describe how
/// the application enumerates alternatives for every mechanic in the project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanningPreferences {
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
            alternative_count: 3,
            machine_preferences: Vec::new(),
            enumerate_modules: Vec::new(),
            enumerate_beacons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoBeaconPlan {
    pub module_config: ModuleConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    Unsupported,
}

impl MechanicKind {
    pub const ALL: [Self; 12] = [
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
            _ => Self::Unsupported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
