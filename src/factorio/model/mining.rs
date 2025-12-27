use std::collections::HashMap;

use serde::Deserialize;

use crate::{
    concept::AsFlow, factorio::{
        common::{
            Effect, EffectReceiver, EffectTypeLimitation, EnergyAmount, EnergySource, HasPrototypeBase, PrototypeBase, option_as_vec_or_empty, update_map
        },
        model::{context::{Context, GenericItem}, energy::energy_source_as_flow, entity::EntityPrototype, recipe::RecipeResult},
    }
};

#[derive(Debug, Clone, Deserialize)]
pub struct ResourcePrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub category: Option<String>,

    #[serde(default)]
    pub infinite: bool,
}

impl HasPrototypeBase for ResourcePrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base.base
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MiningDrillPrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub mining_speed: f64,

    pub resource_categories: Vec<String>,

    pub energy_source: EnergySource,
    #[serde(default)]
    pub energy_usage: Option<EnergyAmount>,
    #[serde(default)]
    pub effect_receiver: Option<EffectReceiver>,
    #[serde(default)]
    pub module_slots: f64,
    #[serde(default)]
    pub quality_affects_module_slots: bool,

    pub allowed_affects: Option<EffectTypeLimitation>,

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    #[serde(default)]
    pub allowed_module_categories: Option<Vec<String>>,

    #[serde(default)]
    pub uses_force_mining_productivity_bonus: bool,

    pub resource_drain_rate_percent: Option<f64>,
}

impl HasPrototypeBase for MiningDrillPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base.base
    }
}

#[derive(Debug, Clone)]
pub struct MiningConfig {
    pub resource: String,
    pub quality: u8,
    pub machine: Option<String>,
    pub modules: Vec<(String, u8)>,
    pub extra_effects: Effect,
    pub instance_fuel: Option<(String, i32)>,
}

impl AsFlow for MiningConfig {
    type ItemIdentType = GenericItem;
    type ContextType = Context;

    fn as_flow(&self, ctx: &Self::ContextType) -> Vec<HashMap<Self::ItemIdentType, f64>> {
        let mut map = HashMap::new();

        let mut module_effects = Effect::default();

        let mut base_speed = 1.0;
        let resource_ore = match ctx.resources.get(&self.resource) {
            Some(r) => r,
            None => return vec![map],
        };

        assert!(resource_ore.base.minable.is_some());

        let mining_property = resource_ore.base.minable.as_ref().unwrap();
        let miner = match &self.machine {
            Some(machine_name) => ctx.miners.get(machine_name),
            None => None,
        };

        base_speed = base_speed / mining_property.mining_time;

        let mut resource_drain_rate = 1.0;

        for module in self.modules.iter() {
            let module_prototype = ctx
                .modules
                .get(&module.0) // 暂时忽略品质
                .expect("MiningConfig 中的插件在上下文中不存在");
            module_effects = module_effects + module_prototype.effect.clone();
        }

        module_effects = module_effects + self.extra_effects.clone();
        module_effects = module_effects.clamped();

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
            let energy_related_flow = energy_source_as_flow(
                ctx,
                &miner.energy_source,
                miner.energy_usage.as_ref().expect("MiningDrillPrototype 中的机器没有能量消耗"),
                &module_effects,
                &self.instance_fuel,
                &mut base_speed,
            );
            for (key, value) in energy_related_flow.into_iter() {
                update_map(&mut map, key, value);
            }
        }

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
        vec![map]
    }
}

#[test]
fn test_mining_normalized() {
    let ctx = Context::load(
        &serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap(),
    );
    let mining_config = MiningConfig {
        resource: "uranium-ore".to_string(),
        quality: 0,
        machine: Some("big-mining-drill".to_string()),
        modules: vec![],
        extra_effects: Effect {
            productivity: 1.0,
            ..Default::default()
        },
        instance_fuel: None,
    };

    let result = mining_config.as_flow(&ctx);
    println!("Mining Result: {:?}", result);
    let result_with_location =
        crate::factorio::model::context::make_located_generic_recipe(result[0].clone(), 42);
    println!("Mining Result with Location: {:?}", result_with_location);
}
