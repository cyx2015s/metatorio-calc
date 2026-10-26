//! Calculation facts, in SI units, shared by hover views and cached per prototype.
//! The wire format carries values and stable keys, never HTML or presentation layout.
use metatorio_core::{
    IdWithQuality,
    mechanic::module_effect_at_quality,
    planet::{TileCapabilities, tile_capabilities},
    science::science_value,
};
use metatorio_data::store::{PrototypeGroup, PrototypeRecord, PrototypeStore};
use metatorio_data::{
    AccumulatorComponent, BeaconComponent, CraftingMachineComponent, EntityComponent,
    FluidComponent, MiningDrillComponent, ModuleComponent, OffshorePumpComponent, PlanetComponent,
    QualityComponent, RecipeComponent, SolarPanelComponent, SurfaceComponent, TechnologyComponent,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatUnit {
    Number,
    Seconds,
    Watts,
    Joules,
    JoulesPerDegree,
    Percent,
    Multiplier,
    PerSecond,
    Grams,
}
#[derive(Debug, Clone, Serialize)]
pub struct CalculationStat {
    pub key: &'static str,
    pub value: f64,
    pub unit: StatUnit,
}
#[derive(Debug, Clone, Serialize)]
pub struct QualityCalculation {
    pub quality: String,
    pub stats: Vec<CalculationStat>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct CalculationDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_weight: Option<metatorio_data::item_weight::ItemWeight>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub surface_properties: std::collections::BTreeMap<String, f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub research: Option<ResearchCalculation>,
    pub stats: Vec<CalculationStat>,
    pub qualities: Vec<QualityCalculation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tile: Option<TileCapabilities>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResearchCalculation {
    pub units: Option<u64>,
    pub unit_seconds: f64,
    pub ingredients: Vec<ResearchIngredient>,
    pub prerequisites: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResearchIngredient {
    pub name: String,
    pub per_unit: u16,
    pub total: Option<f64>,
}
fn stat(out: &mut Vec<CalculationStat>, key: &'static str, value: f64, unit: StatUnit) {
    if value.is_finite() {
        out.push(CalculationStat { key, value, unit });
    }
}

pub fn calculation_detail(store: &PrototypeStore, record: &PrototypeRecord) -> CalculationDetail {
    use StatUnit::*;
    let mut detail = CalculationDetail {
        tile: (record.group == PrototypeGroup::Tile)
            .then(|| tile_capabilities(store, &record.name))
            .flatten(),
        ..Default::default()
    };
    if record.group == PrototypeGroup::Item
        && let Some(weight) = store.item_weight(&record.name)
    {
        stat(&mut detail.stats, "item-weight", weight.weight, Grams);
        if let Some(item) = record.component::<metatorio_data::ItemComponent>() {
            stat(
                &mut detail.stats,
                "stack-weight",
                weight.weight * f64::from(item.stack_size),
                Grams,
            );
        }
        if weight.weight > 0.0 {
            stat(
                &mut detail.stats,
                "rocket-item-capacity",
                (store.weight_constants().default_rocket_lift_weight / weight.weight).floor(),
                Number,
            );
        }
        detail.item_weight = Some(weight.clone());
    }
    if let Some(planet) = record.component::<PlanetComponent>() {
        detail.surface_properties = planet.surface_properties.clone();
    }
    if let Some(surface) = record.component::<SurfaceComponent>() {
        detail.surface_properties = surface.surface_properties.clone();
    }
    if let Some(technology) = record.component::<TechnologyComponent>()
        && let Some(unit) = &technology.unit
    {
        detail.research = Some(ResearchCalculation {
            units: unit.count,
            unit_seconds: unit.time,
            ingredients: unit
                .ingredients
                .iter()
                .map(|(name, amount)| ResearchIngredient {
                    name: name.clone(),
                    per_unit: *amount,
                    total: unit.count.map(|count| count as f64 * f64::from(*amount)),
                })
                .collect(),
            prerequisites: technology.prerequisites.clone(),
        });
    }
    if let Some(recipe) = record.component::<RecipeComponent>() {
        stat(
            &mut detail.stats,
            "craft-time",
            recipe.energy_required,
            Seconds,
        );
        stat(
            &mut detail.stats,
            "productivity-cap",
            recipe.maximum_productivity,
            Percent,
        );
    }
    if record.has("ResourceEntityComponent")
        && let Some(entity) = record.component::<EntityComponent>()
        && let Some(minable) = entity.minable()
    {
        stat(
            &mut detail.stats,
            "mining-time",
            minable.mining_time,
            Seconds,
        );
    }
    if let Some(drill) = record.component::<MiningDrillComponent>() {
        stat(
            &mut detail.stats,
            "mining-speed",
            drill.mining_speed,
            Number,
        );
        stat(
            &mut detail.stats,
            "power",
            drill.energy_usage.amount * 60.0,
            Watts,
        );
    }
    if let Some(pump) = record.component::<OffshorePumpComponent>() {
        stat(
            &mut detail.stats,
            "pumping-rate",
            pump.pumping_speed * 60.0,
            PerSecond,
        );
    }
    if let Some(fluid) = record.component::<FluidComponent>() {
        stat(
            &mut detail.stats,
            "heat-capacity",
            fluid.heat_capacity().amount,
            JoulesPerDegree,
        );
        if fluid.fuel_value().amount > 0.0 {
            stat(
                &mut detail.stats,
                "fuel-value",
                fluid.fuel_value().amount,
                Joules,
            );
        }
    }
    if let Some(beacon) = record.component::<BeaconComponent>() {
        for (key, count) in [
            ("beacon-profile-1", 1),
            ("beacon-profile-4", 4),
            ("beacon-profile-8", 8),
        ] {
            stat(
                &mut detail.stats,
                key,
                beacon.get_profile(count),
                Multiplier,
            );
        }
    }
    if let Some(quality) = record.component::<QualityComponent>() {
        for (key, value) in [
            (
                "crafting-multiplier",
                quality.crafting_machine_speed_multiplier(),
            ),
            ("module-speed-multiplier", quality.module_speed_multiplier()),
            (
                "module-productivity-multiplier",
                quality.module_productivity_multiplier(),
            ),
            ("science-multiplier", quality.science_capacity_multiplier()),
            (
                "accumulator-multiplier",
                quality.accumulator_capacity_multiplier(),
            ),
        ] {
            stat(&mut detail.stats, key, value, Multiplier);
        }
    }
    let fallback = vec!["normal".to_owned()];
    let qualities = if store.quality_order().is_empty() {
        &fallback
    } else {
        store.quality_order()
    };
    for name in qualities {
        let quality = store
            .get(PrototypeGroup::Quality, name)
            .and_then(|r| r.component::<QualityComponent>());
        let mut stats = Vec::new();
        if let Some(machine) = record.component::<CraftingMachineComponent>() {
            let speed = machine
                .crafting_speed_quality_multiplier
                .get(name)
                .copied()
                .or_else(|| quality.map(QualityComponent::crafting_machine_speed_multiplier))
                .unwrap_or(1.0);
            stat(
                &mut stats,
                "crafting-speed",
                machine.crafting_speed * speed,
                Number,
            );
            stat(
                &mut stats,
                "module-slots",
                metatorio_runtime::prototype::effective_module_slots(store, &record.name, name)
                    as f64,
                Number,
            );
            let energy = if machine.quality_affects_energy_usage {
                quality.map_or(1.0, |q| q.crafting_machine_energy_usage_multiplier)
            } else {
                1.0
            };
            stat(
                &mut stats,
                "power",
                machine.energy_usage.amount * 60.0 * energy,
                Watts,
            );
        }
        if let Some(drill) = record.component::<MiningDrillComponent>() {
            stat(
                &mut stats,
                "resource-drain",
                f64::from(drill.resource_drain_rate_percent.unwrap_or(100)) / 100.0
                    * quality.map_or(1.0, |q| q.mining_drill_resource_drain_multiplier),
                Percent,
            );
            stat(
                &mut stats,
                "module-slots",
                metatorio_runtime::prototype::effective_module_slots(store, &record.name, name)
                    as f64,
                Number,
            );
        }
        if let Some(module) = record.component::<ModuleComponent>() {
            let effect = quality.map_or(module.effect, |q| module_effect_at_quality(module, q));
            for (key, value) in [
                ("effect-speed", effect.speed),
                ("effect-productivity", effect.productivity),
                ("effect-quality", effect.quality),
                ("effect-consumption", effect.consumption),
                ("effect-pollution", effect.pollution),
            ] {
                if value != 0.0 {
                    stat(&mut stats, key, value, Percent);
                }
            }
        }
        if let Some(beacon) = record.component::<BeaconComponent>() {
            let level = quality.map_or(0.0, |q| f64::from(q.level));
            stat(
                &mut stats,
                "beacon-effectivity",
                beacon.distribution_effectivity
                    + beacon
                        .distribution_effectivity_bonus_per_quality_level
                        .unwrap_or(0.0)
                        * level,
                Percent,
            );
            let slots = usize::from(beacon.module_slots)
                + if beacon.quality_affects_module_slots {
                    quality.map_or(0, |q| usize::from(q.beacon_module_slots_bonus()))
                } else {
                    0
                };
            stat(&mut stats, "module-slots", slots as f64, Number);
            stat(
                &mut stats,
                "power",
                beacon.energy_usage.amount
                    * 60.0
                    * quality.map_or(1.0, |q| q.beacon_power_usage_multiplier),
                Watts,
            );
        }
        if let Some(panel) = record.component::<SolarPanelComponent>() {
            stat(
                &mut stats,
                "solar-peak",
                panel.production.amount
                    * 60.0
                    * quality.map_or(1.0, QualityComponent::default_multiplier),
                Watts,
            );
        }
        if let Some(accumulator) = record.component::<AccumulatorComponent>()
            && let Some(capacity) = accumulator.energy_source.buffer_capacity
        {
            stat(
                &mut stats,
                "buffer-capacity",
                capacity.amount
                    * quality.map_or(1.0, QualityComponent::accumulator_capacity_multiplier),
                Joules,
            );
        }
        if record.group == PrototypeGroup::Item
            && store.science_pack_names().contains(&record.name)
            && let Some(value) = science_value(store, &IdWithQuality::new(&record.name, name))
        {
            stat(&mut stats, "science-value", value, Number);
        }
        let multiplier = quality.map_or(1.0, QualityComponent::default_multiplier);
        if let Some(reactor) = record.component::<metatorio_data::FusionReactorComponent>() {
            stat(
                &mut stats,
                "fluid-usage",
                reactor.max_fluid_usage * 60.0 * multiplier,
                PerSecond,
            );
            stat(
                &mut stats,
                "power",
                reactor.power_input.amount * 60.0,
                Watts,
            );
            if let Some(fluid) = reactor
                .output_fluid_box
                .filter
                .as_deref()
                .and_then(|n| store.get(PrototypeGroup::Fluid, n))
                .and_then(|r| r.component::<FluidComponent>())
            {
                let temperature = reactor
                    .target_temperature
                    .unwrap_or(fluid.default_temperature);
                stat(
                    &mut stats,
                    "fusion-fuel-power",
                    reactor.max_fluid_usage
                        * 60.0
                        * fluid.heat_capacity().amount
                        * temperature
                        * multiplier
                        / reactor.burner.effectivity,
                    Watts,
                );
            }
        }
        if let Some(generator) = record.component::<metatorio_data::FusionGeneratorComponent>() {
            stat(
                &mut stats,
                "fluid-usage",
                generator.max_fluid_usage * 60.0 * multiplier,
                PerSecond,
            );
            if let Some(limit) = generator.energy_source.output_flow_limit {
                stat(
                    &mut stats,
                    "max-output",
                    limit.amount * 60.0 * multiplier,
                    Watts,
                );
            }
        }
        if !stats.is_empty() {
            detail.qualities.push(QualityCalculation {
                quality: name.clone(),
                stats,
            });
        }
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn item_weight_hover_uses_the_same_cached_value_as_expansion() {
        let s = PrototypeStore::load(&json!({
            "item":{"raw":{"weight":1000},"out":{"stack_size":100}},
            "recipe":{"out":{"allow_productivity":true,"ingredients":[{"type":"item","name":"raw","amount":1}],"results":[{"type":"item","name":"out","amount":1}]}}
        })).unwrap();
        let detail = calculation_detail(&s, s.item("out").unwrap());
        let find = |key| {
            detail
                .stats
                .iter()
                .find(|stat| stat.key == key)
                .unwrap()
                .value
        };
        assert_eq!(find("item-weight"), 500.0);
        assert_eq!(find("stack-weight"), 50_000.0);
        assert_eq!(find("rocket-item-capacity"), 2000.0);
        assert_eq!(detail.item_weight.unwrap().recipe.as_deref(), Some("out"));
    }
    fn store() -> PrototypeStore {
        PrototypeStore::load(&json!({
            "quality": {"normal":{"level":0,"order":"a","next":"rare"},"rare":{"level":2,"order":"b","module_speed_multiplier":4,"science_capacity_multiplier":5,"tool_durability_multiplier":8}},
            "item": {"red":{"science_capacity":2},"ore":{}},
            "tool": {"green":{"durability":20}},
            "lab": {"lab":{"inputs":["red","green"]}},
            "technology": {"test-tech":{"unit":{"count":10,"time":5,"ingredients":[["red",2]]}}},
            "planet": {"test-planet":{"surface_properties":{"gravity":10}}},
            "fluid": {"water":{"default_temperature":15}},
            "tile": {"water":{"fluid":"water"},"land":{},"lava":{"destroys_dropped_items":true},"bad":{"fluid":"missing"}},
            "offshore-pump": {"pump":{"pumping_speed":0.5}},
            "resource": {"ore-node":{"minable":{"mining_time":2,"result":"ore","count":2,"required_fluid":"water","fluid_amount":10}}},
            "beacon": {"tower":{"module_slots":2,"quality_affects_module_slots":true,"energy_usage":"480kW","distribution_effectivity":0.5}},
            "assembling-machine": {"assembler":{"crafting_speed":2,"crafting_speed_quality_multiplier":{"rare":9},"module_slots":3,"module_slots_quality_bonus":{"rare":2},"energy_usage":"60kW"}},
            "module": {"speed":{"effect":{"speed":0.5}}},
            "recipe": {"ore":{"energy_required":2,"ingredients":[{"type":"item","name":"ore","amount":3}],"results":[{"type":"item","name":"red","amount":4,"probability":0.5}]}}
        })).unwrap()
    }
    fn value(detail: &CalculationDetail, quality: &str, key: &str) -> f64 {
        detail
            .qualities
            .iter()
            .find(|q| q.quality == quality)
            .unwrap()
            .stats
            .iter()
            .find(|s| s.key == key)
            .unwrap()
            .value
    }
    #[test]
    fn quality_facts_respect_overrides_and_existing_helpers() {
        let store = store();
        let detail = calculation_detail(&store, store.entity("assembler").unwrap());
        assert_eq!(value(&detail, "rare", "crafting-speed"), 18.0);
        assert_eq!(value(&detail, "rare", "module-slots"), 5.0);
        let module = calculation_detail(&store, store.item("speed").unwrap());
        assert_eq!(value(&module, "rare", "effect-speed"), 2.0);
        let tower = calculation_detail(&store, store.entity("tower").unwrap());
        assert_eq!(value(&tower, "rare", "module-slots"), 4.0);
        assert_eq!(
            value(
                &calculation_detail(&store, store.item("red").unwrap()),
                "rare",
                "science-value"
            ),
            10.0
        );
        assert_eq!(
            value(
                &calculation_detail(&store, store.item("green").unwrap()),
                "rare",
                "science-value"
            ),
            160.0
        );
        assert!(
            calculation_detail(&store, store.item("ore").unwrap())
                .qualities
                .is_empty()
        );
    }
    #[test]
    fn tile_metadata_and_detail_lookup_share_the_same_capabilities() {
        let store = store();
        let index = crate::app::catalog_index_from_store(&store, &std::collections::HashMap::new());
        let tiles: Vec<_> = index.iter().filter(|entry| entry.kind == "tile").collect();
        assert_eq!(
            tiles
                .iter()
                .filter(|entry| entry.tile.as_ref().unwrap().fluid.is_some())
                .count(),
            1
        );
        assert_eq!(
            tiles
                .iter()
                .filter(|entry| entry.tile.as_ref().unwrap().destroys_dropped_items)
                .count(),
            1
        );
        let state = crate::app::AppState::default();
        state
            .runtime
            .lock()
            .unwrap()
            .install_context("test".into(), store);
        for (name, fluid, destroy) in [
            ("water", Some("water"), false),
            ("land", None, false),
            ("lava", None, true),
            ("bad", None, false),
        ] {
            let detail =
                crate::app::prototype_detail(&state, "test".into(), "tile".into(), name.into())
                    .unwrap()
                    .unwrap();
            let tile = detail.calculation.tile.unwrap();
            assert_eq!(tile.fluid.as_deref(), fluid);
            assert_eq!(tile.destroys_dropped_items, destroy);
        }
        let fluid =
            crate::app::prototype_detail(&state, "test".into(), "fluid".into(), "water".into())
                .unwrap()
                .unwrap();
        assert_eq!(
            fluid
                .calculation
                .stats
                .iter()
                .find(|s| s.key == "heat-capacity")
                .unwrap()
                .value,
            1000.0
        );
        let pump = crate::app::prototype_detail(
            &state,
            "test".into(),
            "offshore-pump".into(),
            "pump".into(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            pump.calculation
                .stats
                .iter()
                .find(|s| s.key == "pumping-rate")
                .unwrap()
                .value,
            30.0
        );
    }
    #[test]
    fn recipe_rows_distinguish_expected_quantities_from_baseline_rates() {
        let state = crate::app::AppState::default();
        state
            .runtime
            .lock()
            .unwrap()
            .install_context("test".into(), store());
        let detail =
            crate::app::prototype_detail(&state, "test".into(), "recipe".into(), "ore".into())
                .unwrap()
                .unwrap();
        assert_eq!(detail.ingredients[0].base_rate, Some(1.5));
        assert_eq!(detail.results[0].amount, 2.0);
        assert_eq!(detail.results[0].base_rate, Some(1.0));
        assert_eq!(detail.results[0].probability, 0.5);
        let resource = crate::app::prototype_detail(
            &state,
            "test".into(),
            "resource".into(),
            "ore-node".into(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(resource.ingredients[0].name, "water");
        assert_eq!(resource.ingredients[0].amount, 1.0);
        assert_eq!(resource.ingredients[0].base_rate, Some(0.5));
        assert_eq!(resource.results[0].base_rate, Some(1.0));
        let tech = crate::app::prototype_detail(
            &state,
            "test".into(),
            "technology".into(),
            "test-tech".into(),
        )
        .unwrap()
        .unwrap()
        .calculation
        .research
        .unwrap();
        assert_eq!(tech.units, Some(10));
        assert_eq!(tech.ingredients[0].total, Some(20.0));
        let planet = crate::app::prototype_detail(
            &state,
            "test".into(),
            "planet".into(),
            "test-planet".into(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(planet.calculation.surface_properties["gravity"], 10.0);
    }
    #[test]
    fn vanilla_calculation_facts_serialize_as_finite_si_values() {
        let dump =
            serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap();
        let store = PrototypeStore::load(&dump).unwrap();
        for group in [
            PrototypeGroup::Item,
            PrototypeGroup::Entity,
            PrototypeGroup::Fluid,
            PrototypeGroup::Tile,
            PrototypeGroup::Quality,
            PrototypeGroup::Planet,
            PrototypeGroup::Surface,
            PrototypeGroup::Technology,
        ] {
            for record in store.group(group) {
                let detail = calculation_detail(&store, record);
                assert!(
                    detail
                        .stats
                        .iter()
                        .chain(detail.qualities.iter().flat_map(|q| &q.stats))
                        .all(|s| s.value.is_finite())
                );
                serde_json::to_value(detail).unwrap();
            }
        }
    }
}
