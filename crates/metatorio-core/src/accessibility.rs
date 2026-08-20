//! 可达性（accessibility）：科技/配方/物品/流体/实体/品质/空间位置等
//! 原型从游戏起点是否可达。
//!
//! 模型为纯 ADT（不用继承体系）：
//! - [`Accessible`]：可达性对象（原型级——无品质/温度，可达性是原型属性）
//! - [`Requirement`]：依赖声明（`All` = 全部满足 / `Any` = 任一满足 / `Node` = 叶子）
//!
//! 规则按对象类型 `match` 分发（编译器保证穷尽），正向 BFS 从根种子传播：
//! child 可达 ⟺ 其依赖声明全部满足（对应 yafc 的 `WalkAccessibilityGraph`）。
//!
//! 两层语义：
//! 1. **自动层**：`enabled` 配方/科技（游戏开始可用）与无依赖对象作为根种子，
//!    递归传播到"前置依赖全部可达"的对象（用户要求的"从没有前置依赖的
//!    科技开始递归传播可达性，直到遇到不可达科技"）。
//! 2. **用户层**：[`AccessibilityOptions::marked_accessible`]（显式可达，并入根种子）
//!    / [`AccessibilityOptions::marked_inaccessible`]（显式剪枝，永不可达）
//!    / [`AccessibilityOptions::all_accessible`]（无视一切，全可达）。

use std::collections::{HashMap, VecDeque};

use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use metatorio_data::types::{Ingredient, Modifier, Product};
use metatorio_data::{
    CraftingMachineComponent, EntityComponent, RecipeComponent, ResourceEntityComponent,
    TechnologyComponent,
};

use crate::dual_var::DualVar;
use crate::id::NORMAL_QUALITY;

/// 保序哈希集合（与 core 其余部分一致）。
pub type AIndexSet<T> = indexmap::IndexSet<T, ahash::RandomState>;

/// 可达性对象。原型级：物品/实体不携带品质、流体不携带温度——
/// 可达性是原型属性（品质解锁由 `UnlockQuality` 科技经 [`Accessible::Quality`] 表达）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Accessible {
    Tech(String),
    Recipe(String),
    Quality(String),
    Space(String),
    Item(String),
    Fluid(String),
    Entity(String),
    Electricity,
    Heat,
}

impl Accessible {
    /// 从流标识提取可达性对象（虚拟流如燃料/污染/火箭容量不参与可达性）。
    pub fn from_flow(flow: &DualVar) -> Option<Accessible> {
        match flow {
            DualVar::Item(id) => Some(Accessible::Item(id.id.clone())),
            DualVar::Fluid { name, .. } => Some(Accessible::Fluid(name.clone())),
            DualVar::Entity(id) => Some(Accessible::Entity(id.id.clone())),
            DualVar::Electricity => Some(Accessible::Electricity),
            DualVar::Heat => Some(Accessible::Heat),
            _ => None,
        }
    }

    /// 选择器分类与名称（与 `accessible_prototypes` 的 category → name 结构一致）。
    pub fn category_name(&self) -> (&'static str, &str) {
        match self {
            Accessible::Tech(name) => ("technology", name),
            Accessible::Recipe(name) => ("recipe", name),
            Accessible::Quality(name) => ("quality", name),
            Accessible::Space(name) => ("space-location", name),
            Accessible::Item(name) => ("item", name),
            Accessible::Fluid(name) => ("fluid", name),
            Accessible::Entity(name) => ("entity", name),
            Accessible::Electricity => ("electricity", ""),
            Accessible::Heat => ("heat", ""),
        }
    }
}

/// 依赖声明（ADT）：`All` = 全部子依赖满足；`Any` = 任一子依赖满足；
/// `Node` = 叶子对象。空 `All` 恒真（根），空 `Any` 恒假（无来源）。
#[derive(Debug, Clone)]
pub enum Requirement {
    All(Vec<Requirement>),
    Any(Vec<Requirement>),
    Node(Accessible),
}

impl Requirement {
    fn is_satisfied(&self, accessible: &AIndexSet<Accessible>) -> bool {
        match self {
            Requirement::All(list) => list.iter().all(|r| r.is_satisfied(accessible)),
            Requirement::Any(list) => list.iter().any(|r| r.is_satisfied(accessible)),
            Requirement::Node(node) => accessible.contains(node),
        }
    }

    fn leaves(&self, out: &mut Vec<Accessible>) {
        match self {
            Requirement::All(list) | Requirement::Any(list) => {
                for r in list {
                    r.leaves(out);
                }
            }
            Requirement::Node(node) => out.push(node.clone()),
        }
    }
}

/// 用户层可达性选项。
#[derive(Debug, Clone, Default)]
pub struct AccessibilityOptions {
    /// 显式标记可达的对象（并入根种子；即使无任何来源也可达）。
    pub marked_accessible: AIndexSet<Accessible>,
    /// 显式标记不可达的对象（剪枝：自身不可达，且阻断依赖它的对象）。
    pub marked_inaccessible: AIndexSet<Accessible>,
    /// 无视一切可达性限制（全可达）。
    pub all_accessible: bool,
}

/// 可达性计算结果。
#[derive(Debug, Clone, Default)]
pub struct Accessibility {
    accessible: AIndexSet<Accessible>,
}

impl Accessibility {
    pub fn is_accessible(&self, node: &Accessible) -> bool {
        self.accessible.contains(node)
    }

    pub fn accessible(&self) -> &AIndexSet<Accessible> {
        &self.accessible
    }

    /// 便捷查询：物品名。
    pub fn is_item_accessible(&self, name: &str) -> bool {
        self.accessible.contains(&Accessible::Item(name.to_string()))
    }

    /// 便捷查询：流是否可达（虚拟流默认视为可达——燃料/污染等不参与科技链）。
    pub fn is_flow_accessible(&self, flow: &DualVar) -> bool {
        match Accessible::from_flow(flow) {
            Some(node) => self.is_accessible(&node),
            None => true,
        }
    }
}

/// 依赖图构建的辅助反向表（一次遍历缓存，避免每次查询全量扫描）。
struct GraphData {
    /// 物品/流体名 → 产出它的配方名。
    recipes_by_product: HashMap<String, Vec<String>>,
    /// 物品名 → 可采出它的矿藏实体名（minable 产出）。
    resources_by_product: HashMap<String, Vec<String>>,
    /// 配方/品质/空间名 → 解锁它的科技名（UnlockRecipe/UnlockQuality/UnlockSpaceLocation）。
    techs_by_unlock: HashMap<String, Vec<String>>,
    /// 配方类别 → 能做的机器实体名。
    machines_by_category: HashMap<String, Vec<String>>,
}

fn build_graph(store: &PrototypeStore) -> GraphData {
    let mut recipes_by_product: HashMap<String, Vec<String>> = HashMap::new();
    let mut resources_by_product: HashMap<String, Vec<String>> = HashMap::new();
    let mut techs_by_unlock: HashMap<String, Vec<String>> = HashMap::new();
    let mut machines_by_category: HashMap<String, Vec<String>> = HashMap::new();

    for record in store.group(PrototypeGroup::Recipe) {
        let Some(recipe) = record.component::<RecipeComponent>() else {
            continue;
        };
        for result in &recipe.results {
            match result {
                Product::Item(product) => recipes_by_product
                    .entry(product.name.clone())
                    .or_default()
                    .push(record.name.clone()),
                Product::Fluid(product) => recipes_by_product
                    .entry(product.name.clone())
                    .or_default()
                    .push(record.name.clone()),
            }
        }
    }
    for record in store.group(PrototypeGroup::Entity) {
        let Some(entity) = record.component::<EntityComponent>() else {
            continue;
        };
        let Some(minable) = &entity.minable else {
            continue;
        };
        let mut products: Vec<String> = Vec::new();
        if let Some(result) = &minable.result {
            products.push(result.clone());
        }
        for product in &minable.results {
            match product {
                Product::Item(item) => products.push(item.name.clone()),
                Product::Fluid(fluid) => products.push(fluid.name.clone()),
            }
        }
        for name in products {
            resources_by_product
                .entry(name)
                .or_default()
                .push(record.name.clone());
        }
    }
    for record in store.group(PrototypeGroup::Technology) {
        let Some(tech) = record.component::<TechnologyComponent>() else {
            continue;
        };
        for effect in &tech.effects {
            match effect {
                Modifier::UnlockRecipe(unlock) => techs_by_unlock
                    .entry(unlock.recipe.clone())
                    .or_default()
                    .push(record.name.clone()),
                Modifier::UnlockQuality(unlock) => techs_by_unlock
                    .entry(unlock.quality.clone())
                    .or_default()
                    .push(record.name.clone()),
                Modifier::UnlockSpaceLocation(unlock) => techs_by_unlock
                    .entry(unlock.space_location.clone())
                    .or_default()
                    .push(record.name.clone()),
                _ => {}
            }
        }
    }
    for record in store.group(PrototypeGroup::Entity) {
        let Some(machine) = record.component::<CraftingMachineComponent>() else {
            continue;
        };
        for category in &machine.crafting_categories {
            machines_by_category
                .entry(category.clone())
                .or_default()
                .push(record.name.clone());
        }
    }

    GraphData {
        recipes_by_product,
        resources_by_product,
        techs_by_unlock,
        machines_by_category,
    }
}

/// 配方实际生效的类别（空 → `["crafting"]`，与自动规划一致）。
fn recipe_categories(recipe: &RecipeComponent) -> Vec<String> {
    let categories = recipe.categories.clone().unwrap_or_default();
    if categories.is_empty() {
        vec!["crafting".to_string()]
    } else {
        categories
    }
}

/// 对象 → 依赖声明（按类型 match 分发）。
fn requirements(store: &PrototypeStore, graph: &GraphData, node: &Accessible) -> Requirement {
    match node {
        Accessible::Tech(name) => {
            let tech = store
                .get(PrototypeGroup::Technology, name)
                .and_then(|record| record.component::<TechnologyComponent>());
            match tech {
                Some(tech) if !tech.enabled && !tech.prerequisites.is_empty() => {
                    Requirement::All(
                        tech.prerequisites
                            .iter()
                            .map(|prereq| Requirement::Node(Accessible::Tech(prereq.clone())))
                            .collect(),
                    )
                }
                // enabled 科技（游戏开始可用）或无条件科技：根
                _ => Requirement::All(Vec::new()),
            }
        }
        Accessible::Recipe(name) => {
            let recipe = store
                .get(PrototypeGroup::Recipe, name)
                .and_then(|record| record.component::<RecipeComponent>());
            match recipe {
                Some(recipe) if !recipe.enabled => {
                    let mut all: Vec<Requirement> = Vec::new();
                    // 原料（全部满足）
                    for ingredient in &recipe.ingredients {
                        match ingredient {
                            Ingredient::Item(item) => {
                                all.push(Requirement::Node(Accessible::Item(item.name.clone())));
                            }
                            Ingredient::Fluid(fluid) => {
                                all.push(Requirement::Node(Accessible::Fluid(fluid.name.clone())));
                            }
                        }
                    }
                    // 至少一台能做的机器可达（Entity 可达 = 同名可放置 Item 可达）。
                    // crafting 类配方玩家可手工制造（character 手工 = 根可达），
                    // 打破"机器需要机器"的死锁（如 stone-furnace 需 assembling-machine）。
                    let mut machines: Vec<Requirement> = recipe_categories(recipe)
                        .iter()
                        .flat_map(|category| graph.machines_by_category.get(category))
                        .flatten()
                        .map(|machine| Requirement::Node(Accessible::Entity(machine.clone())))
                        .collect();
                    if recipe_categories(recipe)
                        .iter()
                        .any(|category| category == "crafting")
                    {
                        machines.push(Requirement::Node(Accessible::Entity(
                            "character".to_string(),
                        )));
                    }
                    all.push(Requirement::Any(machines));
                    // 至少一个解锁科技可达
                    let unlocks = graph
                        .techs_by_unlock
                        .get(name)
                        .cloned()
                        .unwrap_or_default();
                    all.push(Requirement::Any(
                        unlocks
                            .into_iter()
                            .map(|tech| Requirement::Node(Accessible::Tech(tech)))
                            .collect(),
                    ));
                    Requirement::All(all)
                }
                // enabled 配方（游戏开始可用）：根
                _ => Requirement::All(Vec::new()),
            }
        }
        Accessible::Item(name) => {
            let mut any: Vec<Requirement> = Vec::new();
            if let Some(recipes) = graph.recipes_by_product.get(name) {
                any.extend(recipes.iter().map(|recipe| {
                    Requirement::Node(Accessible::Recipe(recipe.clone()))
                }));
            }
            if let Some(resources) = graph.resources_by_product.get(name) {
                any.extend(resources.iter().map(|entity| {
                    Requirement::Node(Accessible::Entity(entity.clone()))
                }));
            }
            Requirement::Any(any)
        }
        Accessible::Fluid(name) => {
            let mut any: Vec<Requirement> = Vec::new();
            if let Some(recipes) = graph.recipes_by_product.get(name) {
                any.extend(recipes.iter().map(|recipe| {
                    Requirement::Node(Accessible::Recipe(recipe.clone()))
                }));
            }
            if let Some(resources) = graph.resources_by_product.get(name) {
                any.extend(resources.iter().map(|entity| {
                    Requirement::Node(Accessible::Entity(entity.clone()))
                }));
            }
            Requirement::Any(any)
        }
        Accessible::Entity(name) => {
            let record = store.get(PrototypeGroup::Entity, name);
            if record
                .and_then(|r| r.component::<ResourceEntityComponent>())
                .is_some()
            {
                // 矿藏实体：星球生成，不参与科技链（根）
                Requirement::All(Vec::new())
            } else {
                // 可放置实体：由同名物品放置
                Requirement::Node(Accessible::Item(name.clone()))
            }
        }
        Accessible::Quality(name) => {
            if name == NORMAL_QUALITY {
                Requirement::All(Vec::new())
            } else {
                let any: Vec<Requirement> = graph
                    .techs_by_unlock
                    .get(name)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|tech| Requirement::Node(Accessible::Tech(tech)))
                    .collect();
                Requirement::Any(any)
            }
        }
        Accessible::Space(name) => {
            let any: Vec<Requirement> = graph
                .techs_by_unlock
                .get(name)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|tech| Requirement::Node(Accessible::Tech(tech)))
                .collect();
            Requirement::Any(any)
        }
        Accessible::Electricity | Accessible::Heat => Requirement::All(Vec::new()),
    }
}

/// 收集全部可达性节点（科技/配方/物品/流体/实体/品质/空间位置）。
fn collect_nodes(store: &PrototypeStore) -> Vec<Accessible> {
    let mut out = Vec::new();
    for group in [
        PrototypeGroup::Technology,
        PrototypeGroup::Recipe,
        PrototypeGroup::Item,
        PrototypeGroup::Fluid,
        PrototypeGroup::Entity,
        PrototypeGroup::Quality,
        PrototypeGroup::SpaceLocation,
    ] {
        for record in store.group(group) {
            let node = match group {
                PrototypeGroup::Technology => Accessible::Tech(record.name.clone()),
                PrototypeGroup::Recipe => Accessible::Recipe(record.name.clone()),
                PrototypeGroup::Item => Accessible::Item(record.name.clone()),
                PrototypeGroup::Fluid => Accessible::Fluid(record.name.clone()),
                PrototypeGroup::Entity => Accessible::Entity(record.name.clone()),
                PrototypeGroup::Quality => Accessible::Quality(record.name.clone()),
                PrototypeGroup::SpaceLocation => Accessible::Space(record.name.clone()),
                _ => continue,
            };
            out.push(node);
        }
    }
    out
}

/// 计算可达性：正向 BFS 从根种子传播，child 可达 ⟺ 依赖声明全部满足。
pub fn compute_accessibility(
    store: &PrototypeStore,
    options: &AccessibilityOptions,
) -> Accessibility {
    if options.all_accessible {
        let accessible = collect_nodes(store).into_iter().collect();
        return Accessibility { accessible };
    }

    let graph = build_graph(store);
    let nodes = collect_nodes(store);

    // 反向依赖表：依赖某个对象的对象集合（用于传播时触发重新评估）。
    let mut reverse: HashMap<Accessible, Vec<Accessible>> = HashMap::new();
    for node in &nodes {
        let mut leaves = Vec::new();
        requirements(store, &graph, node).leaves(&mut leaves);
        for leaf in leaves {
            reverse.entry(leaf).or_default().push(node.clone());
        }
    }

    let mut accessible: AIndexSet<Accessible> = AIndexSet::default();
    let mut queue: VecDeque<Accessible> = VecDeque::new();
    let mut seed = |node: Accessible| {
        if options.marked_inaccessible.contains(&node) {
            return;
        }
        if accessible.insert(node.clone()) {
            queue.push_back(node);
        }
    };

    // 用户显式可达（并入根种子）。
    for node in &options.marked_accessible {
        seed(node.clone());
    }
    // 恒真根：电/热/角色（角色手工制造打破机器死锁）。
    seed(Accessible::Electricity);
    seed(Accessible::Heat);
    seed(Accessible::Entity("character".to_string()));
    // 无依赖对象（enabled 配方/科技、矿藏实体、normal 品质……）作为根种子。
    for node in &nodes {
        if matches!(requirements(store, &graph, node), Requirement::All(ref list) if list.is_empty()) {
            seed(node.clone());
        }
    }

    // 正向传播。
    while let Some(node) = queue.pop_front() {
        let dependents = reverse.get(&node).cloned().unwrap_or_default();
        for dependent in dependents {
            if options.marked_inaccessible.contains(&dependent) {
                continue;
            }
            if accessible.contains(&dependent) {
                continue;
            }
            if requirements(store, &graph, &dependent).is_satisfied(&accessible) {
                accessible.insert(dependent.clone());
                queue.push_back(dependent);
            }
        }
    }

    Accessibility { accessible }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn load(dump: serde_json::Value) -> PrototypeStore {
        PrototypeStore::load(&dump).expect("dump 加载失败")
    }

    fn tech(name: &str, prerequisites: Vec<&str>, enabled: bool) -> serde_json::Value {
        json!({
            "type": "technology", "name": name,
            "prerequisites": prerequisites,
            "enabled": enabled,
            "effects": [],
            "unit": { "count": 10, "time": 10, "ingredients": [] }
        })
    }

    fn recipe(name: &str, ingredients: Vec<&str>, results: Vec<&str>, enabled: bool, unlock_techs: Vec<&str>) -> serde_json::Value {
        let mut recipe = json!({
            "type": "recipe", "name": name,
            "energy_required": 1,
            "ingredients": ingredients.iter().map(|ing| json!({ "type": "item", "name": ing, "amount": 1 })).collect::<Vec<_>>(),
            "results": results.iter().map(|res| json!({ "type": "item", "name": res, "amount": 1 })).collect::<Vec<_>>(),
            "categories": ["crafting"],
            "enabled": enabled,
        });
        if !unlock_techs.is_empty() {
            recipe["unlock_techs"] = serde_json::Value::Null; // 占位，未用
        }
        recipe
    }

    /// 完整合成 dump：科技链 + 配方链 + 机器。
    fn chain_dump() -> serde_json::Value {
        let mut dump = json!({
            "item": {
                "iron-ore": { "type": "item", "name": "iron-ore" },
                "iron-plate": { "type": "item", "name": "iron-plate" },
                "steel-plate": { "type": "item", "name": "steel-plate" },
                "engine-unit": { "type": "item", "name": "engine-unit" },
                "magic-item": { "type": "item", "name": "magic-item" },
                "assembling-machine-1": { "type": "item", "name": "assembling-machine-1" }
            },
            "fluid": {},
            "technology": {},
            "recipe": {
                "iron-ore": recipe("iron-ore", vec![], vec!["iron-ore"], true, vec![]),
                "iron-plate": recipe("iron-plate", vec!["iron-ore"], vec!["iron-plate"], true, vec![]),
                "steel-plate": recipe("steel-plate", vec!["iron-plate"], vec!["steel-plate"], false, vec!["tech-steel"]),
                "engine-unit": recipe("engine-unit", vec!["iron-plate", "steel-plate"], vec!["engine-unit"], false, vec!["tech-engine"]),
                "assembling-machine-1": recipe("assembling-machine-1", vec!["iron-plate"], vec!["assembling-machine-1"], true, vec![])
            },
            "assembling-machine": {
                "assembling-machine-1": {
                    "type": "assembling-machine", "name": "assembling-machine-1",
                    "crafting_categories": ["crafting"], "crafting_speed": 1,
                    "module_slots": 0, "energy_usage": "90kW",
                    "energy_source": { "type": "electric", "drain": "0J" }
                }
            }
        });
        // 科技：根（enabled）+ 链 + 死胡同（前置不存在）
        let techs = json!({
            "tech-base": tech("tech-base", vec![], true),
            "tech-iron": tech("tech-iron", vec!["tech-base"], false),
            "tech-steel": tech("tech-steel", vec!["tech-iron"], false),
            "tech-engine": tech("tech-engine", vec!["tech-steel"], false),
            "tech-void": tech("tech-void", vec!["missing-tech"], false),
        });
        dump["technology"] = techs;
        // 配方解锁：steel-plate ← tech-steel；engine-unit ← tech-engine
        dump["technology"]["tech-steel"]["effects"] = json!([{ "type": "unlock-recipe", "recipe": "steel-plate" }]);
        dump["technology"]["tech-engine"]["effects"] = json!([{ "type": "unlock-recipe", "recipe": "engine-unit" }]);
        dump
    }

    #[test]
    fn forward_propagation_from_enabled_roots() {
        let store = load(chain_dump());
        let result = compute_accessibility(&store, &AccessibilityOptions::default());
        // enabled 配方产物：iron-ore、iron-plate、assembling-machine-1 可达
        assert!(result.is_item_accessible("iron-plate"), "enabled 配方产物应可达");
        assert!(result.is_item_accessible("assembling-machine-1"), "机器（enabled 配方产出）应可达");
        // 科技链：tech-base(enabled) → tech-iron → tech-steel → tech-engine 全部可达
        assert!(result.is_accessible(&Accessible::Tech("tech-base".into())));
        assert!(result.is_accessible(&Accessible::Tech("tech-iron".into())));
        assert!(result.is_accessible(&Accessible::Tech("tech-steel".into())));
        assert!(result.is_accessible(&Accessible::Tech("tech-engine".into())));
        // 死胡同科技（前置不存在）不可达
        assert!(!result.is_accessible(&Accessible::Tech("tech-void".into())));
        // steel-plate：原料 iron-plate + 机器 + 解锁科技 tech-steel 全可达 → 可达
        assert!(result.is_item_accessible("steel-plate"));
        // engine-unit：需要 tech-engine（可达）→ 可达
        assert!(result.is_item_accessible("engine-unit"));
        // 无产出配方的物品不可达
        assert!(!result.is_item_accessible("magic-item"));
    }

    #[test]
    fn user_marked_inaccessible_prunes_descendants() {
        let store = load(chain_dump());
        let options = AccessibilityOptions {
            marked_inaccessible: [Accessible::Item("iron-plate".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let result = compute_accessibility(&store, &options);
        assert!(!result.is_item_accessible("iron-plate"), "显式剪枝应生效");
        assert!(
            !result.is_item_accessible("steel-plate"),
            "剪掉原料后下游配方产物应不可达"
        );
        assert!(!result.is_item_accessible("engine-unit"));
    }

    #[test]
    fn user_marked_accessible_overrides_missing_sources() {
        let store = load(chain_dump());
        let options = AccessibilityOptions {
            marked_accessible: [Accessible::Item("magic-item".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let result = compute_accessibility(&store, &options);
        assert!(
            result.is_item_accessible("magic-item"),
            "显式可达应覆盖无产出配方的物品"
        );
    }

    #[test]
    fn all_accessible_ignores_everything() {
        let store = load(chain_dump());
        let options = AccessibilityOptions {
            all_accessible: true,
            ..Default::default()
        };
        let result = compute_accessibility(&store, &options);
        assert!(result.is_item_accessible("magic-item"));
        assert!(result.is_accessible(&Accessible::Tech("tech-void".into())));
    }

    /// 真实 dump：基础科技链/物品链应可达，矿藏（minable）应作为来源。
    /// 依赖本机导出 dump，存在则验证；不存在则跳过。
    #[test]
    fn real_dump_basic_accessibility_chains() {
        let path = "C:\\Users\\mirac\\AppData\\Roaming\\Factorio\\script-output\\data-raw-dump.json";
        if !std::path::Path::new(path).exists() {
            eprintln!("[skip] 无真实 dump，跳过");
            return;
        }
        let raw = std::fs::read(path).expect("读 dump");
        let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump");
        let store = load(dump);
        let result = compute_accessibility(&store, &AccessibilityOptions::default());

        // 诊断：打印关键物品的可达性（便于调整断言）。
        for item in [
            "iron-ore",
            "iron-plate",
            "steel-plate",
            "copper-cable",
            "plastic-bar",
            "uranium-235",
            "electromagnetic-science-pack",
            "space-science-pack",
            "fusion-reactor",
        ] {
            eprintln!("可达性 [{item}] = {}", result.is_item_accessible(item));
        }
        // 塑料链断点诊断
        for item in ["coal", "crude-oil", "petroleum-gas", "chemical-plant", "oil-refinery", "sulfur"] {
            eprintln!("塑料链 [{item}] = {}", result.is_item_accessible(item));
        }
        for tech in ["automation", "oil-processing", "plastics", "advanced-oil-processing", "sulfur-processing"] {
            eprintln!(
                "科技 [{tech}] = {}",
                result.is_accessible(&Accessible::Tech(tech.to_string()))
            );
        }
        eprintln!(
            "配方 [plastic-bar] = {}",
            result.is_accessible(&Accessible::Recipe("plastic-bar".to_string()))
        );

        // 基础链：enabled 配方产物
        assert!(result.is_item_accessible("iron-ore"), "铁矿应可达（enabled 采矿配方）");
        assert!(result.is_item_accessible("iron-plate"));
        assert!(result.is_item_accessible("steel-plate"), "钢（科技链）应可达");
        assert!(result.is_item_accessible("copper-cable"));
        // 石油链（原油 minable 或泵送 + 科技解锁）
        assert!(result.is_item_accessible("plastic-bar"), "塑料（石油链）应可达");
        // 核：离心机链（科技解锁）
        assert!(result.is_item_accessible("uranium-235"), "铀-235（科技链）应可达");
        // 高级链
        assert!(result.is_item_accessible("electromagnetic-science-pack"), "电磁科学包应可达");
        assert!(result.is_item_accessible("space-science-pack"), "空间科学包应可达");
    }
}
