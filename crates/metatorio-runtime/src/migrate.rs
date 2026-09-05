//! 把旧版 metatorio-egui 工程文件转换为新版 AppDocument JSON。
//!
//! 旧版持久化是 dyn Trait + typetag 的多态对象（每个机制一个
//! `{"type":"factorio:X", "instances":[...]}`），且机制内部是 **SoA**：
//! 同质变体存进一个 `instances` 数组。新版的 `Mechanic` 是枚举变体（**AoS**），
//! 每个 `MechanicEntry` 是一个具体机制。因此转换时把旧版每种机制的
//! `instances` 展开成多条新 `MechanicEntry`（每条一个变体）。
//!
//! 核心差异：
//! - 旧 `IdWithQuality` = `(name, quality_level:u8)`（JSON 数组 `["name",level]`），
//!   新版 `{id, quality}` 且 quality 是**字符串名** → 需按当前上下文品质顺序
//!   把 level 映射为名称。
//! - `FactoryContext.major_quality`（u8 索引）→ `FactorySettings.major_quality`（名称）。
//! - 旧 `DualVar` 的 `FluidHeat/FluidFuel.filter` 是 `Option<String>`、`ItemFuel.category`
//!   是单个字符串 → 新版对应 `filter: String`（空串 = 任意）、`category: Vec<String>`。
//!
//! 直接操作 `serde_json::Value`（不引入强类型变换）。转换按"当前游戏上下文"
//! 的品质顺序进行：level → 品质名。项目绑定到当前激活上下文 id。

use serde_json::{Value, json};

use metatorio_data::store::{PrototypeGroup, PrototypeStore};

/// 是否是旧版工程文件（有 `proj` + `factories`，无新版 `schema_version`）。
pub fn is_old_project_format(value: &Value) -> bool {
    value.get("proj").is_some()
        && value.get("factories").is_some()
        && value.get("schema_version").is_none()
        && value.get("projects").is_none()
}

/// 旧版工程文件 → 新版 `AppDocument` JSON。
///
/// `context_id`：要绑定到该项目的上下文缓存 id（当前激活上下文）。
/// `quality_order`：当前上下文的品质名顺序（index → 名称）；缺失时回退默认。
/// `store`：当前上下文仓库，用于把旧版**扁平的技术里程碑**（混杂物品/科技/
/// 配方/品质/星球名）分类为正确的可达性节点类型（Item/Tech/Recipe/…）。
pub fn migrate_old_project(
    value: &Value,
    context_id: Option<&str>,
    quality_order: &[String],
    store: Option<&PrototypeStore>,
) -> Result<Value, String> {
    let proj = value.get("proj").ok_or("旧版工程缺少 proj")?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("导入的项目")
        .to_string();
    let factories = value
        .get("factories")
        .and_then(Value::as_array)
        .ok_or("旧版工程缺少 factories")?;

    let settings = settings_of(proj, store);
    let mut planning = default_planning();
    // 把旧版"每机制"的自动规划偏好（recipe/mining）合并到项目级（新版的
    // PlanningPreferences 是项目级）。取第一个含偏好的机制即可。
    for fact in factories {
        if let Some(mechs) = fact.get("mechanics").and_then(Value::as_array) {
            for m in mechs {
                if merge_planning(&mut planning, m, quality_order) {
                    break;
                }
            }
        }
    }

    let new_factories = factories
        .iter()
        .enumerate()
        .map(|(idx, fact)| factory_of(fact, idx, quality_order, store))
        .collect::<Vec<_>>();

    Ok(json!({
        "schema_version": 1,
        "projects": [{
            "id": 1,
            "name": name,
            "settings": settings,
            "planning": planning,
            "context_id": context_id.map(|s| json!(s)).unwrap_or(Value::Null),
            "factories": new_factories,
        }],
    }))
}

// ── 项目设置 ──────────────────────────────────────────────────────

fn settings_of(proj: &Value, store: Option<&PrototypeStore>) -> Value {
    let milestones = proj
        .get("tech_milestones")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|pair| {
                    let tech = pair
                        .get(0)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let unlocked = pair.get(1).and_then(Value::as_bool).unwrap_or(true);
                    json!({ "node": classify_node(&tech, store), "unlocked": unlocked })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let recipe_productivity = proj
        .get("recipe_productivity")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(recipe, value)| json!({ "recipe": recipe, "productivity": value }))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "time_scale": proj.get("time_scale").cloned().unwrap_or(json!("seconds")),
        "milestones": milestones,
        "recipe_productivity": recipe_productivity,
        "infinite_levels": [],
        "ignore_productivity": proj.get("ignore_productivity").cloned().unwrap_or(json!(false)),
        "mining_productivity": proj.get("mining_productivity").cloned().unwrap_or(json!(0.0)),
        "all_accessible": proj.get("all_accessible").cloned().unwrap_or(json!(false)),
        "quality_limit": Value::Null,
    })
}

fn default_planning() -> Value {
    json!({
        "alternative_count": 3,
        "machine_preferences": Value::Array(Vec::new()),
        "enumerate_modules": Value::Array(Vec::new()),
        "enumerate_beacons": Value::Array(Vec::new()),
    })
}

/// 尝试从机制对象提取自动规划偏好并入 `planning`。成功则返回 true。
fn merge_planning(planning: &mut Value, mechanic: &Value, q: &[String]) -> bool {
    let is_planning_type = matches!(
        mechanic.get("type").and_then(Value::as_str),
        Some("factorio:recipe") | Some("factorio:mining")
    );
    if !is_planning_type {
        return false;
    }
    if let Some(ac) = mechanic.get("alternative_count") {
        planning["alternative_count"] = ac.clone();
    }
    if let Some(arr) = mechanic
        .get("machine_preferences")
        .and_then(Value::as_array)
    {
        planning["machine_preferences"] = Value::Array(arr.iter().map(|v| id_of(v, q)).collect());
    }
    if let Some(arr) = mechanic.get("enumerate_modules").and_then(Value::as_array) {
        planning["enumerate_modules"] = Value::Array(arr.iter().map(|v| id_of(v, q)).collect());
    }
    if let Some(arr) = mechanic.get("enumerate_beacons").and_then(Value::as_array) {
        planning["enumerate_beacons"] =
            Value::Array(arr.iter().map(|v| auto_beacon_of(v, q)).collect());
    }
    true
}

// ── 工厂 ──────────────────────────────────────────────────────────

fn factory_of(f: &Value, idx: usize, q: &[String], store: Option<&PrototypeStore>) -> Value {
    let factory = f.get("factory").cloned().unwrap_or(json!({}));
    let planet = factory.get("planet").cloned().unwrap_or(Value::Null);
    let surface = factory.get("surface").cloned().unwrap_or(Value::Null);
    let major_quality_level = factory
        .get("major_quality")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let major_quality = quality_name(q, major_quality_level);
    let debug = factory.get("debug").cloned().unwrap_or(json!(false));

    let targets = f
        .get("target")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(ti, t)| {
                    json!({
                        "id": ti,
                        "flow": dualvar_of(t.get(0).unwrap_or(&Value::Null), q),
                        "amount": t.get(1).cloned().unwrap_or(json!(1.0)),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let target_expressions = f
        .get("target_group")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(ei, e)| {
                    let terms = e
                        .get("coefficients")
                        .and_then(Value::as_array)
                        .map(|terms| {
                            terms
                                .iter()
                                .enumerate()
                                .map(|(ti, t)| {
                                    json!({
                                        "id": ti,
                                        "flow": dualvar_of(t.get(0).unwrap_or(&Value::Null), q),
                                        "coefficient": t.get(1).cloned().unwrap_or(json!(1.0)),
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    json!({
                        "id": ei,
                        "constant": e.get("constant").cloned().unwrap_or(json!(1.0)),
                        "terms": terms,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let external_inputs = f
        .get("external")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(ei, t)| {
                    json!({
                        "id": ei,
                        "flow": dualvar_of(t.get(0).unwrap_or(&Value::Null), q),
                        "penalty": t.get(1).cloned().unwrap_or(json!(1.0)),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut mechanics = Vec::new();
    if let Some(arr) = f.get("mechanics").and_then(Value::as_array) {
        for m in arr {
            mechanic_entries(m, q, store, &mut mechanics);
        }
    }
    let mechanics = mechanics
        .into_iter()
        .enumerate()
        .map(|(mi, mech)| json!({ "id": mi + 1, "enabled": true, "mechanic": mech }))
        .collect::<Vec<_>>();

    json!({
        "id": idx + 1,
        "name": f.get("name").cloned().unwrap_or(json!(format!("工厂 {}", idx + 1))),
        "settings": {
            "planet": planet,
            "surface": surface,
            "major_quality": major_quality,
            "debug": debug,
        },
        "targets": targets,
        "target_expressions": target_expressions,
        "external_inputs": external_inputs,
        "mechanics": mechanics,
        "strict_source": f.get("strict_source").cloned().unwrap_or(json!(false)),
        "strict_sink": f.get("strict_sink").cloned().unwrap_or(json!(false)),
    })
}

// ── 机制（SoA → AoS）─────────────────────────────────────────────

/// 把旧版一个机制对象（内部多条 instances）展开成多条新版机制 JSON。
fn mechanic_entries(
    mechanic: &Value,
    q: &[String],
    store: Option<&PrototypeStore>,
    out: &mut Vec<Value>,
) {
    let Some(typ) = mechanic.get("type").and_then(Value::as_str) else {
        return;
    };
    let Some(new_type) = map_mechanic_type(typ) else {
        return;
    };
    let instances = mechanic
        .get("instances")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if instances.is_empty() {
        // 空模板（未配置具体变体）：新版没有对应实体，跳过。
        return;
    }
    for inst in &instances {
        out.push(mechanic_of(new_type, inst, q, store));
    }
}

fn map_mechanic_type(old: &str) -> Option<&'static str> {
    Some(match old {
        "factorio:recipe" => "recipe",
        "factorio:mining" => "mining",
        "factorio:item-fuel" => "item-fuel",
        "factorio:item-launch" => "item-launch",
        "factorio:generator" => "generator",
        "factorio:boiler" => "boiler",
        "factorio:reactor" => "reactor",
        "factorio:plant" => "plant",
        "factorio:spoil" => "spoil",
        "factorio:fluid-fuel" => "fluid-fuel",
        "factorio:fluid-heat" => "fluid-heat",
        _ => return None,
    })
}

fn mechanic_of(
    new_type: &str,
    inst: &Value,
    q: &[String],
    store: Option<&PrototypeStore>,
) -> Value {
    match new_type {
        "recipe" => json!({
            "type": "recipe",
            "recipe": id_of(inst_get(inst, "recipe"), q),
            "machine": id_of(inst_get(inst, "machine"), q),
            "module_config": module_config_of(inst_get(inst, "module_config")),
            "fuel": fuel_value(inst_get(inst, "fuel"), q, store),
        }),
        "mining" => json!({
            "type": "mining",
            "resource": inst.get("resource").cloned().unwrap_or(json!("")),
            "machine": id_of(inst_get(inst, "machine"), q),
            "module_config": module_config_of(inst_get(inst, "module_config")),
            "fuel": fuel_value(inst_get(inst, "fuel"), q, store),
        }),
        "item-fuel" => json!({ "type": "item-fuel", "item": id_of(inst_get(inst, "item"), q) }),
        "item-launch" => json!({
            "type": "item-launch",
            "item": id_of(inst_get(inst, "item"), q),
            "weight_mode": inst.get("weight_mode").cloned().unwrap_or(json!(false)),
        }),
        "plant" => json!({ "type": "plant", "seed": id_of(inst_get(inst, "seed"), q) }),
        "spoil" => json!({ "type": "spoil", "item": id_of(inst_get(inst, "item"), q) }),
        "generator" => json!({
            "type": "generator",
            "generator": id_of(inst_get(inst, "generator"), q),
            "fluid": inst.get("fluid").cloned().unwrap_or(json!("")),
            "temperature": inst.get("temperature").cloned().unwrap_or(json!(0)),
        }),
        "boiler" => json!({
            "type": "boiler",
            "boiler": id_of(inst_get(inst, "boiler"), q),
            "fluid": inst.get("fluid").cloned().unwrap_or(json!("")),
            "temperature": inst.get("temperature").cloned().unwrap_or(json!(0)),
            "fuel": fuel_value(inst_get(inst, "fuel"), q, store),
        }),
        "reactor" => json!({
            "type": "reactor",
            "reactor": id_of(inst_get(inst, "reactor"), q),
            "neighbours": inst.get("neighbours").cloned().unwrap_or(json!(0)),
            "fuel": fuel_value(inst_get(inst, "fuel"), q, store),
        }),
        "fluid-fuel" => json!({
            "type": "fluid-fuel",
            "fluid": inst.get("fluid").cloned().unwrap_or(json!("")),
            "temperature": inst.get("temperature").cloned().unwrap_or(json!(0)),
        }),
        "fluid-heat" => json!({
            "type": "fluid-heat",
            "fluid": inst.get("fluid").cloned().unwrap_or(json!("")),
            "temperature": inst.get("temperature").cloned().unwrap_or(json!(0)),
        }),
        _ => Value::Null,
    }
}

fn inst_get<'a>(inst: &'a Value, key: &str) -> &'a Value {
    inst.get(key).unwrap_or(&Value::Null)
}

/// 旧版燃料（array `[name, second]` 或 null）→ 新版 `Fuel` 枚举 JSON。
/// `second` 对流体燃料是温度、对物品燃料是品质索引；按上下文分类成
/// `Fuel::Fluid`（名称+温度）或 `Fuel::Item`（带品质）。null → null。
fn fuel_value(fuel: &Value, q: &[String], store: Option<&PrototypeStore>) -> Value {
    let Some(arr) = fuel.as_array() else {
        return Value::Null;
    };
    let name = arr.first().and_then(Value::as_str).unwrap_or("");
    let second = arr.get(1);
    let is_fluid = store.is_some_and(|s| s.get(PrototypeGroup::Fluid, name).is_some());
    if is_fluid {
        json!({
            "kind": "fluid",
            "fluid": name,
            "temperature": second.cloned().unwrap_or(Value::Null),
        })
    } else {
        let level = second.and_then(Value::as_i64).unwrap_or(0);
        json!({
            "kind": "item",
            "item": { "id": name, "quality": quality_name(q, level) },
        })
    }
}

// ── 插件配置 / 信标 ───────────────────────────────────────────────

fn module_config_of(v: &Value) -> Value {
    let Some(map) = v.as_object() else {
        return json!({ "modules": [], "beacons": [] });
    };
    let modules = map
        .get("modules")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(id_of_plain).collect::<Vec<_>>())
        .unwrap_or_default();
    let beacons = map
        .get("beacons")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(beacon_of).collect::<Vec<_>>())
        .unwrap_or_default();
    json!({ "modules": modules, "beacons": beacons })
}

fn beacon_of(v: &Value) -> Value {
    let Some(map) = v.as_object() else {
        return json!({ "modules": [], "beacon": id_empty(), "count": 1, "share": 1.0 });
    };
    let modules = map
        .get("modules")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|m| {
                    let idv = m
                        .get(0)
                        .map(id_of_plain)
                        .unwrap_or_else(id_empty);
                    let count = m.get(1).cloned().unwrap_or(json!(1));
                    json!([idv, count])
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let beacon = map
        .get("beacon")
        .map(id_of_plain)
        .unwrap_or_else(id_empty);
    json!({
        "modules": modules,
        "beacon": beacon,
        "count": map.get("count").cloned().unwrap_or(json!(1)),
        "share": map.get("share").cloned().unwrap_or(json!(1.0)),
    })
}

/// 旧自动规划信标 `{module_config:{...}}` → 新版 `AutoBeaconPlan {module_config}`。
fn auto_beacon_of(v: &Value, _q: &[String]) -> Value {
    json!({ "module_config": module_config_of(inst_get(v, "module_config")) })
}

// ── 通用转换 ──────────────────────────────────────────────────────

/// 旧版"技术里程碑"的节点名可能是物品/科技/配方/实体/品质/星球名（旧版是
/// 扁平列表）。按当前上下文仓库分类为正确的可达性节点（Item/Tech/Recipe/
/// Entity/Quality/Planet）；无法判定时回退 Tech（旧语义）。
fn classify_node(name: &str, store: Option<&PrototypeStore>) -> Value {
    let in_group = |group: PrototypeGroup| store.is_some_and(|s| s.get(group, name).is_some());
    if in_group(PrototypeGroup::Technology) {
        json!({ "Tech": name })
    } else if in_group(PrototypeGroup::Item) {
        json!({ "Item": name })
    } else if in_group(PrototypeGroup::Recipe) {
        json!({ "Recipe": name })
    } else if in_group(PrototypeGroup::Entity) {
        json!({ "Entity": name })
    } else if in_group(PrototypeGroup::Quality) {
        json!({ "Quality": name })
    } else if in_group(PrototypeGroup::Planet) {
        json!({ "Planet": name })
    } else {
        json!({ "Tech": name })
    }
}

/// 品质等级（u8 索引）→ 名称。0 = normal；越界回退最后一个；空则 normal。
fn quality_name(q: &[String], level: i64) -> String {
    if level <= 0 {
        return "normal".to_string();
    }
    q.get(level as usize)
        .cloned()
        .or_else(|| q.last().cloned())
        .unwrap_or_else(|| "normal".to_string())
}

/// 旧 `IdWithQuality`（`["name", level]`）→ 新版 `{id, quality}`。已有的对象原样返回。
fn id_of(v: &Value, q: &[String]) -> Value {
    match v {
        Value::Array(arr) if arr.len() >= 2 => {
            let name = arr[0].as_str().unwrap_or("");
            let level = arr[1].as_i64().unwrap_or(0);
            json!({ "id": name, "quality": quality_name(q, level) })
        }
        _ => {
            // 已是对象 / 缺省：保留原样（对象）；null → 空 id。
            if v.is_object() {
                v.clone()
            } else {
                id_empty_with_q(q, 0)
            }
        }
    }
}

/// 无品质上下文时（模块/信标 id 在旧格式里不带 level 语义之外的差异），
/// 仅转换数组形态，level 用默认 normal。
fn id_of_plain(v: &Value) -> Value {
    match v {
        Value::Array(arr) if arr.len() >= 2 => {
            let name = arr[0].as_str().unwrap_or("");
            let level = arr[1].as_i64().unwrap_or(0);
            json!({ "id": name, "quality": quality_name(&[], level) })
        }
        Value::Object(_) => v.clone(),
        _ => id_empty(),
    }
}

fn id_empty() -> Value {
    json!({ "id": "", "quality": "normal" })
}

fn id_empty_with_q(q: &[String], level: i64) -> Value {
    json!({ "id": "", "quality": quality_name(q, level) })
}

/// 旧 `DualVar` → 新版 `DualVar`（只改各变体的 id/品质形态与 filter/category 形态）。
fn dualvar_of(v: &Value, q: &[String]) -> Value {
    match v {
        Value::String(s) => json!(s),
        Value::Object(map) => {
            if let Some(inner) = map.get("Item") {
                return json!({ "Item": id_of(inner, q) });
            }
            if let Some(inner) = map.get("Entity") {
                return json!({ "Entity": id_of(inner, q) });
            }
            if let Some(inner) = map.get("Fluid") {
                return json!({
                    "Fluid": {
                        "name": inner.get("name").cloned().unwrap_or(json!("")),
                        "temperature": inner.get("temperature").cloned()
                            .unwrap_or_else(|| json!([i32::MIN, i32::MAX])),
                    }
                });
            }
            if let Some(inner) = map.get("FluidHeat") {
                let filter = inner.get("filter").and_then(Value::as_str).unwrap_or("");
                return json!({ "FluidHeat": { "filter": filter } });
            }
            if let Some(inner) = map.get("FluidFuel") {
                let filter = inner.get("filter").and_then(Value::as_str).unwrap_or("");
                return json!({ "FluidFuel": { "filter": filter } });
            }
            if let Some(inner) = map.get("ItemFuel") {
                let category = inner.get("category").and_then(Value::as_str).unwrap_or("");
                return json!({ "ItemFuel": { "category": vec![category], "has_burnt_result": false } });
            }
            if let Some(inner) = map.get("Pollution") {
                return json!({ "Pollution": { "name": inner.get("name").cloned().unwrap_or(json!("")) } });
            }
            if let Some(inner) = map.get("Custom") {
                return json!({ "Custom": { "name": inner.get("name").cloned().unwrap_or(json!("")) } });
            }
            v.clone()
        }
        _ => v.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::AppDocument;

    /// 原版品质顺序（vanilla）：0 normal, 1 uncommon, 2 rare, 3 epic, 4 legendary。
    const VANILLA_Q: &[&str] = &["normal", "uncommon", "rare", "epic", "legendary"];

    fn vanilla_q() -> Vec<String> {
        VANILLA_Q.iter().map(|s| s.to_string()).collect()
    }

    const EXAMPLE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../metatorio-egui/migrate-example.fpp"
    );

    #[test]
    fn old_project_file_is_detected() {
        if !std::path::Path::new(EXAMPLE).exists() {
            eprintln!("[skip] 无旧版示例（{EXAMPLE}），跳过");
            return;
        }
        let raw = std::fs::read_to_string(EXAMPLE).unwrap();
        let value: Value = serde_json::from_str(&raw).unwrap();
        assert!(is_old_project_format(&value), "示例文件应判定为旧版格式");
    }

    #[test]
    fn migrate_example_produces_valid_app_document() {
        if !std::path::Path::new(EXAMPLE).exists() {
            eprintln!("[skip] 无旧版示例（{EXAMPLE}），跳过");
            return;
        }
        let raw = std::fs::read_to_string(EXAMPLE).unwrap();
        let value: Value = serde_json::from_str(&raw).unwrap();
        let q = vanilla_q();
        let migrated = migrate_old_project(&value, Some("ctx-1"), &q, None).expect("迁移应成功");

        // 必须能反序列化为新版 AppDocument（字段/结构校验）。
        let doc: AppDocument = serde_json::from_value(migrated).expect("迁移结果应可反序列化");
        let proj = &doc.projects[0];
        assert_eq!(proj.context_id.as_deref(), Some("ctx-1"));
        assert_eq!(
            proj.settings.time_scale,
            crate::document::TimeScale::Seconds
        );

        // 品质 level 4 → legendary（工厂 major_quality 与目标/机制 id 都映射）。
        assert_eq!(proj.factories[0].settings.major_quality, "legendary");
        // 目标流：`{"Item":["electromagnetic-science-pack",4]}` → `{id, quality: legendary}`。
        let first_target = &proj.factories[0].targets[0];
        if let metatorio_core::DualVar::Item(idq) = &first_target.flow {
            assert_eq!(idq.id, "electromagnetic-science-pack");
            assert_eq!(idq.quality, "legendary");
        } else {
            panic!("目标应为物品流");
        }

        // 末个工厂的 recipe 机制（SoA 多条实例）应展开成多条 MechanicEntry（AoS）。
        // 示例里有空模板工厂（instances 空 → 不产出条目），故扫描所有工厂找
        // 含多条 recipe 条目的那个。
        use crate::document::MechanicKind;
        let with_recipe = proj.factories.iter().find(|f| {
            f.mechanics
                .iter()
                .filter(|e| MechanicKind::of(&e.mechanic) == MechanicKind::Recipe)
                .count()
                >= 2
        });
        let recipe_count = with_recipe
            .map(|f| {
                f.mechanics
                    .iter()
                    .filter(|e| MechanicKind::of(&e.mechanic) == MechanicKind::Recipe)
                    .count()
            })
            .unwrap_or(0);
        assert!(recipe_count >= 2, "recipe 实例应展开为多条机制条目");

        // 里程碑 + 配方产能 + 采矿产能从旧 proj 迁移。
        assert!(
            !proj.settings.milestones.is_empty(),
            "tech_milestones 应迁移为里程碑"
        );
        assert_eq!(
            proj.settings.mining_productivity, 0.3,
            "mining_productivity 应迁移"
        );
        assert!(
            proj.settings
                .recipe_productivity
                .iter()
                .any(|r| r.recipe == "steel-plate")
        );
    }

    #[test]
    fn milestone_node_classification_uses_context() {
        // 最小仓库：一个科技、一个物品、一个品质；验证里程碑节点按上下文分类。
        let dump = serde_json::json!({
            "technology": {
                "my-tech": {
                    "type": "technology", "name": "my-tech", "enabled": true,
                    "prerequisites": [], "effects": [],
                    "unit": { "count": 1, "time": 1, "ingredients": [] }
                }
            },
            "item": { "my-item": { "type": "item", "name": "my-item" } },
            "quality": { "ordinary": { "type": "quality", "name": "ordinary", "level": 1 } },
            "planet": { "my-planet": { "type": "planet", "name": "my-planet" } }
        });
        let store = PrototypeStore::load(&dump).expect("最小仓库应可加载");
        assert_eq!(
            classify_node("my-tech", Some(&store)),
            json!({ "Tech": "my-tech" })
        );
        assert_eq!(
            classify_node("my-item", Some(&store)),
            json!({ "Item": "my-item" })
        );
        assert_eq!(
            classify_node("ordinary", Some(&store)),
            json!({ "Quality": "ordinary" })
        );
        assert_eq!(
            classify_node("my-planet", Some(&store)),
            json!({ "Planet": "my-planet" })
        );
        // 未知名字回退 Tech（旧语义），无仓库时也回退。
        assert_eq!(
            classify_node("nope", Some(&store)),
            json!({ "Tech": "nope" })
        );
        assert_eq!(classify_node("my-item", None), json!({ "Tech": "my-item" }));
    }

    #[test]
    fn fuel_handles_fluid_vs_burner() {
        // 仓库：steam 是流体，coal 是物品。
        let dump = serde_json::json!({
            "fluid": { "steam": { "type": "fluid", "name": "steam" } },
            "item": { "coal": { "type": "item", "name": "coal" } }
        });
        let store = PrototypeStore::load(&dump).expect("最小仓库应可加载");
        let q = vanilla_q();
        // 流体燃料：`["steam", 165]` → `Fuel::Fluid`（temperature=165）。
        assert_eq!(
            fuel_value(&json!(["steam", 165]), &q, Some(&store)),
            json!({ "kind": "fluid", "fluid": "steam", "temperature": 165 })
        );
        // burner 物品燃料：`["coal", 3]` → `Fuel::Item`（品质 3 → epic）。
        assert_eq!(
            fuel_value(&json!(["coal", 3]), &q, Some(&store)),
            json!({ "kind": "item", "item": { "id": "coal", "quality": "epic" } })
        );
        // 无燃料。
        assert_eq!(fuel_value(&Value::Null, &q, Some(&store)), Value::Null);
    }
}
