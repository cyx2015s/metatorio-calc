use std::{collections::HashSet, fmt::Debug};

use crate::factorio::{
    Dict, FactorioContext, GenericItem, HasPrototypeBase, IdWithQuality, PrototypeBase,
};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlanetPrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    #[serde(default)]
    pub entities_require_heating: bool,

    #[serde(default)]
    pub pollutant_type: Option<String>,
    #[serde(default)]
    pub map_gen_settings: PlanetPrototypeMapGenSettings,
}

impl HasPrototypeBase for PlanetPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base
    }
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct PlanetPrototypeMapGenSettings {
    #[serde(default)]
    pub autoplace_controls: Dict<FrequencySizeRichness>,
    #[serde(default)]
    pub autoplace_settings: TypedAutoplaceSettings,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct TypedAutoplaceSettings {
    pub entity: AutoplaceSettings,
    pub tile: AutoplaceSettings,
    // 无意义，不需要实现
    // pub decorative: AutoplaceSettings,
}

#[derive(Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct FrequencySizeRichness {
    pub frequency: Option<f64>,
    pub size: Option<f64>,
    pub richness: Option<f64>,
}

impl FrequencySizeRichness {
    pub fn product(&self) -> f64 {
        self.frequency.unwrap_or(1.0) * self.size.unwrap_or(1.0) * self.richness.unwrap_or(1.0)
    }
}

impl Debug for FrequencySizeRichness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FrequencySizeRichness {{ frequency: {:?}, size: {:?}, richness: {:?} }}",
            self.frequency, self.size, self.richness
        )
    }
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct AutoplaceSettings {
    #[serde(default)]
    pub treat_missing_as_default: bool,
    pub settings: Dict<Option<FrequencySizeRichness>>,
}

impl PlanetPrototype {
    pub fn collect_autoplaced(&self, ctx: &FactorioContext) -> HashSet<GenericItem> {
        let mut items = HashSet::new();
        for entity in ctx.entities.values() {
            if entity.base.r#type != "resource" && entity.base.r#type != "asteroid-chunk" {
                continue;
            }
            if let Some(autoplace) = &entity.autoplace {
                // 有自动放置
                // 1. 先看 control 有没有出现在星球的 autoplace_controls 里
                if self
                    .map_gen_settings
                    .autoplace_controls
                    .contains_key(&autoplace.control)
                {
                    // 别判断密度了，认为启用就行了
                    items.insert(GenericItem::Entity(IdWithQuality(
                        entity.base.name.clone(),
                        0,
                    )));
                } else {
                    // TODO
                    // 如果 default_enabled 为 true，则认为启用
                    if self
                        .map_gen_settings
                        .autoplace_settings
                        .entity
                        .settings
                        .contains_key(entity.base.name.as_str())
                    {
                        items.insert(GenericItem::Entity(IdWithQuality(
                            entity.base.name.clone(),
                            0,
                        )));
                    }
                }
            }
        }
        for tile in ctx.tiles.values() {
            if let Some(autoplace) = &tile.autoplace
                && let Some(fluid) = &tile.fluid
            {
                // 有自动放置
                // 1. 先看 control 有没有出现在星球的 autoplace_controls 里
                if self
                    .map_gen_settings
                    .autoplace_controls
                    .contains_key(&autoplace.control)
                {
                    // 别判断密度了，认为启用就行了
                    items.insert(GenericItem::Fluid {
                        name: fluid.clone(),
                        temperature: None,
                    });
                } else {
                    // TODO
                    // 如果 default_enabled 为 true，则认为启用
                    if self
                        .map_gen_settings
                        .autoplace_settings
                        .tile
                        .settings
                        .contains_key(tile.base.name.as_str())
                    {
                        items.insert(GenericItem::Fluid {
                            name: fluid.clone(),
                            temperature: None,
                        });
                    }
                }
            }
        }

        items
    }
}
