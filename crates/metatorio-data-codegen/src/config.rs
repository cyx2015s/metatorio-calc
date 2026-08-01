//! 生成器配置：关注的原型类型清单、忽略的类型/字段、自定义类型映射。
//!
//! 这是"自定义余地"的入口：
//! - `concerned_typenames`：dump 中关注哪些顶层类型（其余不生成组件）
//! - `ignored_types`：不关心的复合类型（Sound/Animation/Sprite 等视觉类型），
//!   引用它们的字段直接跳过（不生成、不反序列化）
//! - `custom_type_map`：schema 类型名 → 自定义 Rust 类型（如 Energy → metatorio 的能量解析器）

/// 默认忽略的视觉/音频/杂项复合类型。
/// 这些类型的字段对计算无意义，直接跳过可大幅缩小生成面。
pub const DEFAULT_IGNORED_TYPES: &[&str] = &[
    // 声音
    "Sound",
    "SoundType",
    "InterruptibleSound",
    "SoundDefinition",
    "SoundTypeDefinition",
    // 精灵/动画/图形
    "Sprite",
    "Sprite4Way",
    "SpriteVariations",
    "SpritePriority",
    "SpriteSizeType",
    "SpriteUsageHint",
    "Animation",
    "Animation4Way",
    "AnimationVariations",
    "AnimationVariations8Way",
    "AnimationVariationsOnOff",
    "RotatedAnimation",
    "RotatedAnimationVariations",
    "RotatedSprite",
    "RotatedSpriteVariations",
    "LightDefinition",
    "LightDefinitionArray",
    "RenderLayer",
    "BlendMode",
    "IconData",
    "IconDataArray",
    "IconDataUnion",
    "ProcessionGraphic",
    "ProcessionGraphicCatalogue",
    "ProcessionAudioCatalogue",
    "CraftingMachineGraphicsSet",
    "BoilerPictureSet",
    "BeaconGraphicsSet",
    "AsteroidGraphicsSet",
    "AsteroidCollectorGraphicsSet",
    "CargoBayConnectableGraphicsSet",
    "GeneratorGraphicsSet",
    "MiningDrillGraphicsSet",
    "ReactorGraphicsSet",
    "PlantGraphicsSet",
    "TurretGraphicsSet",
    "CraftingMachineGraphicsSet",
    "RollingStockRotatedSlopedGraphics",
    "CharacterArmorAnimation",
    "FootstepTriggerEffectList",
    // 触发/特效（战斗相关，计算不需要）
    "Trigger",
    "TriggerEffect",
    "TriggerEffectItem",
    "TriggerDelivery",
    "AttackParameters",
    "DamageType",
    "Resistance",
    "DamageTypeID",
    "AmmoType",
    "CapsuleAction",
    "RoboportEffectDelivery",
    // 图形 UI
    "LocalisedString",
    "LocalisedStringArray",
];

/// 默认关注的原型类型（dump 顶层键）。
/// 基于 metatorio_egui 的 DataContext 字段对应的计算相关类型。
pub const DEFAULT_CONCERNED_TYPENAMES: &[&str] = &[
    // 物品与原料
    "item",
    "fluid",
    "recipe",
    "recipe-category",
    "module",
    "module-category",
    "fuel-category",
    "airborne-pollutant",
    // 制造与建筑
    "assembling-machine",
    "furnace",
    "mining-drill",
    "boiler",
    "generator",
    "burner-generator",
    "reactor",
    "fusion-reactor",
    "plant",
    "beacon",
    "lab",
    "resource",
    "resource-category",
    // 科技与行星
    "technology",
    "planet",
    "space-location",
    "surface",
    "surface-property",
    "quality",
    "tile",
    "item-group",
    "item-subgroup",
    // 空间时代相关计算项
    "asteroid-chunk",
    "cargo-landing-pad",
    "thruster",
];

/// 生成配置。
#[derive(Debug, Clone)]
pub struct Config {
    /// 关注的原型类型（dump 顶层键名）。
    pub concerned_typenames: Vec<String>,
    /// 忽略的复合类型名。
    pub ignored_types: Vec<String>,
    /// schema 类型名 → 自定义 Rust 类型名（生成时直接使用该类型）。
    /// 例如 "Energy" → "metatorio_data::energy::EnergyAmount"
    pub custom_type_map: Vec<(String, String)>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            concerned_typenames: DEFAULT_CONCERNED_TYPENAMES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ignored_types: DEFAULT_IGNORED_TYPES.iter().map(|s| s.to_string()).collect(),
            custom_type_map: vec![
                // 能量字符串（"5MJ"、"300kW"）→ 自定义解析类型
                ("Energy".to_string(), "crate::EnergyAmount".to_string()),
                ("EnergyAmount".to_string(), "crate::EnergyAmount".to_string()),
                // ID 类直接映射为 String（schema 中本来就是 string 别名，此处留作扩展）
            ],
        }
    }
}

impl Config {
    /// 该类型是否被忽略。
    pub fn is_ignored_type(&self, type_name: &str) -> bool {
        self.ignored_types.iter().any(|t| t == type_name)
    }

    /// 查自定义类型映射。
    pub fn custom_type(&self, type_name: &str) -> Option<&str> {
        self.custom_type_map
            .iter()
            .find(|(k, _)| k == type_name)
            .map(|(_, v)| v.as_str())
    }
}
