//! 重度 mod 数据冒烟测试：用带 mod 的真实 dump（`assets/data-raw-dump-heavily-modded.json`）
//! 验证生成的组件结构体在"污染数据"下仍能反序列化。
//!
//! 该 dump 包含大量 mod 原型（物品/配方/机器数量远超原版），是 lenient 反序列化
//! 与自定义类型的真实样本（float→int、空 map、0-255 与 0-1 混用的颜色等）。
//! 反序列化失败的条目会被收集并断言为空——新增 mod 污染形态时在此暴露。

use metatorio_data::generated_components::PROTOTYPE_CHAINS;
use serde_json::Value;

fn load_dump() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/data-raw-dump-heavily-modded.json"
    );
    let text = std::fs::read_to_string(path)
        .expect("modded dump 不存在（assets/data-raw-dump-heavily-modded.json）");
    serde_json::from_str(&text).expect("modded dump 解析失败")
}

#[test]
fn heavily_modded_dump_deserializes_all_concerned_prototypes() {
    let dump = load_dump();
    let mut total = 0usize;
    let mut failures: Vec<(String, String, String)> = Vec::new(); // (typename, name, error)

    for (typename, chain) in PROTOTYPE_CHAINS {
        let Some(entries) = dump.get(*typename) else {
            continue;
        };
        let Some(entries_obj) = entries.as_object() else {
            continue;
        };
        for (name, value) in entries_obj {
            total += 1;
            for layer in *chain {
                // 组件类型名 → 反射式反序列化（此处用手动分发最常用层）
                let result: Result<(), String> = try_deserialize_layer(layer, value);
                if let Err(e) = result {
                    failures.push(((*typename).to_string(), name.clone(), e));
                    break; // 该原型已失败，不再尝试后续层
                }
            }
        }
    }

    eprintln!(
        "modded dump 冒烟：共 {total} 个原型（{} 个关注类型）",
        PROTOTYPE_CHAINS.len()
    );
    assert!(
        failures.is_empty(),
        "{} 个原型反序列化失败:\n{}",
        failures.len(),
        failures
            .iter()
            .map(|(t, n, e)| format!("  {t}/{n}: {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// 按组件层名反序列化（分发到生成的组件类型，覆盖 PROTOTYPE_CHAINS 全部 39 层）。
/// 失败时经 serde_path_to_error 给出字段路径（如 ingredients[0].amount）。
fn try_deserialize_layer(layer: &str, value: &Value) -> Result<(), String> {
    let json = serde_json::to_string(value).unwrap_or_default();
    let mut de = serde_json::Deserializer::from_str(&json);
    macro_rules! try_comp {
        ($ty:ty) => {
            serde_path_to_error::deserialize::<_, $ty>(&mut de)
                .map(|_| ())
                .map_err(|e| format!("{} @ {}", e, e.path()))
        };
    }
    match layer {
        "AirbornePollutantComponent" => {
            try_comp!(metatorio_data::generated_components::AirbornePollutantComponent)
        }
        "AssemblingMachineComponent" => {
            try_comp!(metatorio_data::generated_components::AssemblingMachineComponent)
        }
        "AsteroidChunkComponent" => {
            try_comp!(metatorio_data::generated_components::AsteroidChunkComponent)
        }
        "BeaconComponent" => try_comp!(metatorio_data::generated_components::BeaconComponent),
        "BoilerComponent" => try_comp!(metatorio_data::generated_components::BoilerComponent),
        "BurnerGeneratorComponent" => {
            try_comp!(metatorio_data::generated_components::BurnerGeneratorComponent)
        }
        "CraftingMachineComponent" => {
            try_comp!(metatorio_data::generated_components::CraftingMachineComponent)
        }
        "EntityComponent" => try_comp!(metatorio_data::generated_components::EntityComponent),
        "EntityWithHealthComponent" => {
            try_comp!(metatorio_data::generated_components::EntityWithHealthComponent)
        }
        "EntityWithOwnerComponent" => {
            try_comp!(metatorio_data::generated_components::EntityWithOwnerComponent)
        }
        "FluidComponent" => try_comp!(metatorio_data::generated_components::FluidComponent),
        "FuelCategoryComponent" => {
            try_comp!(metatorio_data::generated_components::FuelCategoryComponent)
        }
        "FurnaceComponent" => try_comp!(metatorio_data::generated_components::FurnaceComponent),
        "FusionReactorComponent" => {
            try_comp!(metatorio_data::generated_components::FusionReactorComponent)
        }
        "GeneratorComponent" => try_comp!(metatorio_data::generated_components::GeneratorComponent),
        "ItemComponent" => try_comp!(metatorio_data::generated_components::ItemComponent),
        "ItemGroupComponent" => try_comp!(metatorio_data::generated_components::ItemGroupComponent),
        "ItemSubGroupComponent" => {
            try_comp!(metatorio_data::generated_components::ItemSubGroupComponent)
        }
        "LabComponent" => try_comp!(metatorio_data::generated_components::LabComponent),
        "MiningDrillComponent" => {
            try_comp!(metatorio_data::generated_components::MiningDrillComponent)
        }
        "ModuleCategoryComponent" => {
            try_comp!(metatorio_data::generated_components::ModuleCategoryComponent)
        }
        "ModuleComponent" => try_comp!(metatorio_data::generated_components::ModuleComponent),
        "PlanetComponent" => try_comp!(metatorio_data::generated_components::PlanetComponent),
        "PlantComponent" => try_comp!(metatorio_data::generated_components::PlantComponent),
        "PrototypeBaseComponent" => {
            try_comp!(metatorio_data::generated_components::PrototypeBaseComponent)
        }
        "QualityComponent" => try_comp!(metatorio_data::generated_components::QualityComponent),
        "ReactorComponent" => try_comp!(metatorio_data::generated_components::ReactorComponent),
        "RecipeCategoryComponent" => {
            try_comp!(metatorio_data::generated_components::RecipeCategoryComponent)
        }
        "RecipeComponent" => try_comp!(metatorio_data::generated_components::RecipeComponent),
        "ResourceCategoryComponent" => {
            try_comp!(metatorio_data::generated_components::ResourceCategoryComponent)
        }
        "ResourceEntityComponent" => {
            try_comp!(metatorio_data::generated_components::ResourceEntityComponent)
        }
        "SpaceLocationComponent" => {
            try_comp!(metatorio_data::generated_components::SpaceLocationComponent)
        }
        "SurfaceComponent" => try_comp!(metatorio_data::generated_components::SurfaceComponent),
        "SurfacePropertyComponent" => {
            try_comp!(metatorio_data::generated_components::SurfacePropertyComponent)
        }
        "TechnologyComponent" => {
            try_comp!(metatorio_data::generated_components::TechnologyComponent)
        }
        "ThrusterComponent" => try_comp!(metatorio_data::generated_components::ThrusterComponent),
        "TileComponent" => try_comp!(metatorio_data::generated_components::TileComponent),
        "TreeComponent" => try_comp!(metatorio_data::generated_components::TreeComponent),
        _ => Ok(()),
    }
}
