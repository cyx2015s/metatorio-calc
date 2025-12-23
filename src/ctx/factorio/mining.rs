use std::collections::HashMap;

use serde::Deserialize;

use crate::ctx::{
    RecipeLike,
    factorio::{
        common::{
            Effect, EffectReceiver, EffectTypeLimitation, EnergySource, HasPrototypeBase,
            PrototypeBase, option_as_vec_or_empty, update_map,
        },
        context::{FactorioContext, GenericItem, make_located_generic_recipe},
        entity::EntityPrototype,
        recipe::{ItemResult, RecipeResult},
    },
};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ResourcePrototype {
    #[serde(flatten)]
    pub(crate) base: EntityPrototype,

    pub(crate) category: Option<String>,

    #[serde(default)]
    pub(crate) infinite: bool,
}

impl HasPrototypeBase for ResourcePrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base.base
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MiningDrillPrototype {
    #[serde(flatten)]
    pub(crate) base: EntityPrototype,

    pub(crate) mining_speed: f64,

    pub(crate) resource_categories: Vec<String>,

    pub(crate) energy_source: EnergySource,
    #[serde(default)]
    pub(crate) effect_receiver: Option<EffectReceiver>,
    #[serde(default)]
    pub(crate) module_slots: f64,
    #[serde(default)]
    pub(crate) quality_affects_module_slots: bool,

    pub(crate) allowed_affects: Option<EffectTypeLimitation>,

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    #[serde(default)]
    pub(crate) allowed_module_categories: Option<Vec<String>>,

    #[serde(default)]
    pub(crate) uses_force_mining_productivity_bonus: bool,

    pub(crate) resource_drain_rate_percent: Option<f64>,
}

impl HasPrototypeBase for MiningDrillPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base.base
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MiningConfig {
    pub(crate) resource: String,
    pub(crate) quality: u8,
    pub(crate) machine: Option<String>,
    pub(crate) modules: Vec<String>,
    pub(crate) extra_effects: Effect,
}

impl RecipeLike for MiningConfig {
    type KeyType = GenericItem;
    type ContextType = FactorioContext;

    fn as_hash_map(&self, ctx: &Self::ContextType) -> HashMap<Self::KeyType, f64> {
        let mut map = HashMap::new();

        let mut module_effects = Effect::default();

        let mut base_speed = 1.0;
        let resource_ore = match ctx.resources.get(&self.resource) {
            Some(r) => r,
            None => return map,
        };

        assert!(resource_ore.base.minable.is_some());

        let mining_property = resource_ore.base.minable.as_ref().unwrap();
        let miner = match &self.machine {
            Some(machine_name) => ctx.miners.get(machine_name),
            None => None,
        };

        base_speed = 1.0 / mining_property.mining_time;

        let mut resource_drain_rate = 1.0;
        if let Some(miner) = miner {
            module_effects = module_effects
                + miner
                    .effect_receiver
                    .clone()
                    .unwrap_or_default()
                    .base_effect
                    .clone();
            resource_drain_rate = miner.resource_drain_rate_percent.unwrap_or(100.0) / 100.0;
            base_speed *= miner.mining_speed;
        }

        module_effects = module_effects.clamped();

        // 计算矿物实体本身的消耗
        update_map(
            &mut map,
            GenericItem::Entity {
                name: resource_ore.base.base.name.clone(),
                quality: self.quality,
            },
            -base_speed * (1.0 + module_effects.speed) * resource_drain_rate,
        );

        // 计算开采液体的消耗
        if let Some(fluid) = resource_ore
            .base
            .minable
            .as_ref()
            .and_then(|m| m.required_fluid.clone())
        {
            let fluid_item = GenericItem::Fluid {
                name: fluid,
                temperature: None,
            };
            // TODO: 流体消耗受 drain_rate 影响吗？
            let amount = base_speed
                * (1.0 + module_effects.speed)
                * mining_property
                    .fluid_amount
                    .expect("必须指定每次开采的流体消耗");
            update_map(&mut map, fluid_item, -amount);
        }

        if let Some(results) = &mining_property.results {
            for result in results.iter() {
                let item = match result {
                    RecipeResult::Item(r) => GenericItem::Entity {
                        name: r.name.clone(),
                        quality: self.quality,
                    },
                    RecipeResult::Fluid(r) => GenericItem::Fluid {
                        name: r.name.clone(),
                        temperature: None,
                    },
                };
                let (base_yield, extra_yield) = match result {
                    RecipeResult::Item(r) => r.normalized_output(),
                    RecipeResult::Fluid(r) => r.normalized_output(),
                };
                update_map(
                    &mut map,
                    item,
                    base_speed
                        * (1.0 + module_effects.speed)
                        * (base_yield + extra_yield * module_effects.productivity),
                );
            }
        } else {
            let result = mining_property
                .result
                .as_ref()
                .expect("results or result must exist");
            let count = mining_property.count.unwrap_or(1.0);
            update_map(
                &mut map,
                GenericItem::Item {
                    name: result.clone(),
                    quality: self.quality,
                },
                base_speed
                    * (1.0 + module_effects.speed)
                    * count
                    * (1.0 + module_effects.productivity),
            );
        }
        map
    }
}

#[test]
fn test_mining_normalized() {
    let ctx = FactorioContext::load(
        &serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap(),
    );
    let mining_config = MiningConfig {
        resource: "uranium-ore".to_string(),
        quality: 0,
        machine: Some("electric-mining-drill".to_string()),
        modules: vec![],
        extra_effects: Effect::default(),
    };

    let result = mining_config.as_hash_map(&ctx);
    println!("Mining Result: {:?}", result);
    let result_with_location = make_located_generic_recipe(result, 42);
    println!("Mining Result with Location: {:?}", result_with_location);
}
