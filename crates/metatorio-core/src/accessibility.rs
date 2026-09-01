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
//! 2. **用户层**：[`AccessibilityOptions::forced_accessible`]（显式可达，并入根种子）
//!    / [`AccessibilityOptions::forced_inaccessible`]（显式剪枝，永不可达）
//!    / [`AccessibilityOptions::all_accessible`]（无视一切，全可达）。

use std::collections::{HashMap, VecDeque};

use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use metatorio_data::types::{Ingredient, Modifier, Product, TriggerEffect};
use metatorio_data::{
    AsteroidChunkComponent, EnemySpawnerComponent, EntityComponent, EntityWithHealthComponent,
    ItemComponent, PlantComponent, PrototypeBaseComponent, RecipeComponent,
    SpaceConnectionComponent, SpaceLocationComponent, TechnologyComponent,
};

use crate::dual_var::DualVar;
use crate::id::NORMAL_QUALITY;

/// 保序哈希集合（与 core 其余部分一致）。
pub type AIndexSet<T> = indexmap::IndexSet<T, ahash::RandomState>;

/// 可达性对象。原型级：物品/实体不携带品质、流体不携带温度——
/// 可达性是原型属性（品质解锁由 `UnlockQuality` 科技经 [`Accessible::Quality`] 表达）。
///
/// serde 采用外部标签（默认）：`{"Tech":"automation-science-pack"}`、
/// 单元变体 `"Electricity"`。runtime 文档持久化与前端消息共用此格式。
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Accessible {
    Tech(String),
    Recipe(String),
    Quality(String),
    Space(String),
    Item(String),
    Fluid(String),
    Entity(String),
    /// 星球（普通节点）：nauvis 恒解锁（根）；其他星球 = 需要
    /// `planet-discovery-<星球>` 科技。星球解锁后其资源可自由移动。
    Planet(String),
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
            Accessible::Planet(name) => ("planet", name),
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
    /// **强制可达**的对象（里程碑 `unlocked = true`）：并入根种子传播，
    /// 且**最终结果保证可达**——自动解析（依赖传播）不能覆盖这个状态。
    pub forced_accessible: AIndexSet<Accessible>,
    /// **强制不可达**的对象（里程碑 `unlocked = false`）：剪枝（阻断依赖它的
    /// 对象），且**最终结果保证不可达**——自动解析不能覆盖。
    pub forced_inaccessible: AIndexSet<Accessible>,
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
        self.accessible
            .contains(&Accessible::Item(name.to_string()))
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
#[derive(Debug)]
pub struct GraphData {
    /// 物品/流体名 → 产出它的配方名。
    recipes_by_product: HashMap<String, Vec<String>>,
    /// 物品名 → 可采出它的矿藏实体名（minable 产出）。
    resources_by_product: HashMap<String, Vec<String>>,
    /// 变质产物名 → 可变质成它的物品名（`item.spoil_result`，隐含
    /// "spoil.<item>" 配方，对应 yafc 的 getSpoilRecipe）。
    spoil_sources: HashMap<String, Vec<String>>,
    /// 实体名 → 放置后成为该实体的物品名（`item.place_result`）。
    items_by_place_result: HashMap<String, Vec<String>>,
    /// 科技名 → 科技依赖的物品。
    tech_units: HashMap<String, Vec<String>>,
    /// 配方/品质/空间名 → 解锁它的科技名（UnlockRecipe/UnlockQuality/UnlockSpaceLocation）。
    techs_by_unlock: HashMap<String, Vec<String>>,
    /// 资源实体/星球流体名 → 生成它的星球列表（planet_autoplaced_flows +
    /// seed_available_on_planet）。星球解锁（planet-discovery-<p> 科技可达，
    /// nauvis 恒解锁）后该资源可自由移动到任何星球 → 根。
    resource_planets: HashMap<String, Vec<String>>,
    /// 实体名 → 会产生它的触发实体（反查生成的实体）。如星岩碎片 ← 小星岩。
    generated_by: HashMap<String, Vec<String>>,
    /// 星岩/小行星实体名 → 生成它的空间地点（SpaceLocation 自带星岩：
    /// 停靠该地点即可得，如深空地带 solar-system-edge / shattered-planet）。
    asteroid_locations: HashMap<String, Vec<Accessible>>,
    /// 星岩/小行星实体名 → 生成它的空间连接 (from, to)。SpaceConnection 星岩
    /// 只在飞船**飞行**时沿途生成，需两端地点都可达才飞得过去。
    asteroid_connections: HashMap<String, Vec<(String, String)>>,
    /// 物品名 → 发射出它的物品（某物品可通过把另一个物品发射上太空得到）。
    /// 逆向表：产物 → 发射它的基物品（`item.rocket_launch_products`）。
    launch_sources: HashMap<String, Vec<String>>,
}

/// 资源实体/星球流体名 → 生成它的星球列表。
/// 收集**所有**在该星球自动放置的可采集实体（不限 resource 类）：
/// - 星球流体（带流体 tile）；
/// - 可挖掘实体（矿藏/岩石/卵筏等）→ 实体本身 + minable 产物；
/// - 击杀掉落（loot）实体 → 实体本身 + loot 产物；
/// - 虫巢（unit-spawner）→ 捕获产物（captured_spawner_entity 产卵器）；
/// - 植物（seed_available_on_planet）。
/// 星球解锁（[`Accessible::Planet`] 节点可达）后资源可自由移动到任何星球。
fn build_resource_planets(store: &PrototypeStore) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for planet_record in store.group(PrototypeGroup::Planet) {
        let planet = &planet_record.name;
        let map_gen = crate::planet::planet_map_gen(store, planet);
        // 星球流体（planet_autoplaced_flows 的流体部分；矿藏实体由下方统一处理）
        for (flow, _) in crate::planet::planet_autoplaced_flows(store, planet) {
            if let Some(Accessible::Fluid(name)) = Accessible::from_flow(&flow) {
                map.entry(name).or_default().push(planet.clone());
            }
        }
        // 所有自动放置实体（autoplace 命中该星球）的可采集内容
        if let Some(map_gen) = &map_gen {
            let entity_settings: std::collections::HashSet<String> = map_gen
                .autoplace_settings
                .get("entity")
                .map(|settings| settings.settings.keys().cloned().collect())
                .unwrap_or_default();
            for record in store.group(PrototypeGroup::Entity) {
                let Some(entity) = record.component::<EntityComponent>() else {
                    continue;
                };
                let autoplaced = entity.autoplace.as_ref().is_some_and(|autoplace| {
                    autoplace
                        .control
                        .as_ref()
                        .is_some_and(|control| map_gen.autoplace_controls.contains_key(control))
                        || entity_settings.contains(&record.name)
                });
                if !autoplaced {
                    continue;
                }
                // 可采集内容：可挖掘 / 击杀掉落 / 可捕获虫巢
                let minable = entity.minable();
                let loot_items: Vec<String> = record
                    .component::<EntityWithHealthComponent>()
                    .map(|health| health.loot.iter().map(|loot| loot.name.clone()).collect())
                    .unwrap_or_default();
                let is_spawner = record.component::<EnemySpawnerComponent>().is_some();
                if minable.is_some() || !loot_items.is_empty() || is_spawner {
                    map.entry(record.name.clone())
                        .or_default()
                        .push(planet.clone());
                }
                if let Some(minable) = minable {
                    if let Some(result) = &minable.result {
                        map.entry(result.clone()).or_default().push(planet.clone());
                    }
                    for product in &minable.results {
                        match product {
                            Product::Item(item) => {
                                map.entry(item.name.clone())
                                    .or_default()
                                    .push(planet.clone());
                            }
                            Product::Fluid(fluid) => {
                                map.entry(fluid.name.clone())
                                    .or_default()
                                    .push(planet.clone());
                            }
                        }
                    }
                }
                for loot in loot_items {
                    map.entry(loot).or_default().push(planet.clone());
                }
                // 捕获虫巢 → 产卵器实体（biter-egg 获取途径）
                if let Some(spawner) = record.component::<EnemySpawnerComponent>()
                    && let Some(captured) = &spawner.captured_spawner_entity
                {
                    map.entry(captured.clone())
                        .or_default()
                        .push(planet.clone());
                }
            }
        }
        // 植物实体：存在种子且该星球可种（seed_available_on_planet）
        for record in store.group(PrototypeGroup::Entity) {
            if record.component::<PlantComponent>().is_none() {
                continue;
            }
            let plant_entity = &record.name;
            let seed_available = store.group(PrototypeGroup::Item).any(|item| {
                item.component::<ItemComponent>().is_some_and(|item| {
                    item.plant_result.as_deref() == Some(plant_entity.as_str())
                        && crate::planet::seed_available_on_planet(
                            store,
                            item,
                            plant_entity,
                            Some(planet),
                        )
                })
            });
            if seed_available {
                map.entry(plant_entity.clone())
                    .or_default()
                    .push(planet.clone());
            }
        }
    }
    map
}

/// 构建可达性依赖图（一次遍历，供 `compute_accessibility` / `milestone_order`
/// 复用；`Runtime` 会按上下文缓存，避免每次交互重建）。
pub fn build_graph(store: &PrototypeStore) -> GraphData {
    let mut recipes_by_product: HashMap<String, Vec<String>> = HashMap::new();
    let mut resources_by_product: HashMap<String, Vec<String>> = HashMap::new();
    let mut spoil_sources: HashMap<String, Vec<String>> = HashMap::new();
    let mut items_by_place_result: HashMap<String, Vec<String>> = HashMap::new();
    let mut techs_by_unlock: HashMap<String, Vec<String>> = HashMap::new();
    let mut tech_units: HashMap<String, Vec<String>> = HashMap::new();
    let mut launch_sources: HashMap<String, Vec<String>> = HashMap::new();

    for record in store.group(PrototypeGroup::Item) {
        if let Some(item) = record.component::<ItemComponent>() {
            if !item.place_result.is_empty() {
                items_by_place_result
                    .entry(item.place_result.clone())
                    .or_default()
                    .push(record.name.clone());
            }
            if let Some(spoil) = &item.spoil_result
                && !spoil.is_empty()
            {
                spoil_sources
                    .entry(spoil.clone())
                    .or_default()
                    .push(record.name.clone());
            }
            // 发射产物：把该物品发射上太空（ItemLaunch 机制）可得到的物品。
            for product in &item.rocket_launch_products {
                launch_sources
                    .entry(product.name.clone())
                    .or_default()
                    .push(record.name.clone());
            }
        }
    }
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
    // 太空小行星（asteroid-chunk，AsteroidChunk 组）：星岩收集器在空间平台
    // 采集 chunk 实体 → 同名物品（minable.result/results）。收集该来源，
    // 使星岩物品（如 promethium-asteroid-chunk）可达 ⟸ 星岩实体可达。
    for record in store.group(PrototypeGroup::AsteroidChunk) {
        let Some(chunk) = record.component::<AsteroidChunkComponent>() else {
            continue;
        };
        let Some(minable) = &chunk.minable else {
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
        if let Some(unit) = &tech.unit {
            for ingredient in &unit.ingredients {
                if let Some(item) = store.item(&ingredient.0)
                    && let Some(base) = item.component::<PrototypeBaseComponent>()
                    && !base.hidden
                {
                    tech_units
                        .entry(record.name.clone())
                        .or_default()
                        .push(item.name.clone());
                }
            }
        }
    }
    // 死亡触发效果：`dying_trigger_effect` 中 create-asteroid-chunk /
    // create-entity 生成的实体。小星岩死亡 → 小一号星岩碎片（碎片链）。
    // 只保留反查表 generated_by（实体 → 生成它的触发实体）。
    let mut generated_by: HashMap<String, Vec<String>> = HashMap::new();
    for record in store.group(PrototypeGroup::Entity) {
        let Some(health) = record.component::<EntityWithHealthComponent>() else {
            continue;
        };
        let Some(effects) = &health.dying_trigger_effect else {
            continue;
        };
        for effect in &effects.0 {
            let generated = match effect {
                TriggerEffect::CreateAsteroidChunk { asteroid_name } => asteroid_name.clone(),
                TriggerEffect::CreateEntity { entity_name } => entity_name.clone(),
                _ => None,
            };
            if let Some(generated) = generated {
                generated_by
                    .entry(generated)
                    .or_default()
                    .push(record.name.clone());
            }
        }
    }
    // 星岩/小行星的空间地点生成：SpaceLocation / Planet（星球 orbit）的
    // asteroid_spawn_definitions。深空地点（solar-system-edge / shattered-planet）
    // 生成对应星岩；星球（nauvis 等，Planet 可达）的 orbit 也生成基础星岩。
    // 星岩在该地点可达后（Accessible::Space / Accessible::Planet）才可得。
    let mut asteroid_locations: HashMap<String, Vec<Accessible>> = HashMap::new();
    for record in store.group(PrototypeGroup::SpaceLocation) {
        let Some(location) = record.component::<SpaceLocationComponent>() else {
            continue;
        };
        for definition in &location.asteroid_spawn_definitions {
            let Some(asteroid) = &definition.asteroid else {
                continue;
            };
            asteroid_locations
                .entry(asteroid.clone())
                .or_default()
                .push(Accessible::Space(record.name.clone()));
        }
    }
    // 星球 orbit：planet 继承 space-location，其 asteroid_spawn_definitions
    // 是**星球轨道**的星岩（如 nauvis orbit 初始就有基础星岩）。按星球地点
    // 推断（Planet 可达，nauvis 恒解锁）。
    for record in store.group(PrototypeGroup::Planet) {
        let Some(location) = record.component::<SpaceLocationComponent>() else {
            continue;
        };
        for definition in &location.asteroid_spawn_definitions {
            let Some(asteroid) = &definition.asteroid else {
                continue;
            };
            asteroid_locations
                .entry(asteroid.clone())
                .or_default()
                .push(Accessible::Planet(record.name.clone()));
        }
    }
    // 星岩的空间连接生成：SpaceConnection.asteroid_spawn_definitions。
    // 该星岩只在飞船飞行沿途生成，需两端地点都可达（导航通过）。
    let mut asteroid_connections: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for record in store.group(PrototypeGroup::SpaceConnection) {
        let Some(connection) = record.component::<SpaceConnectionComponent>() else {
            continue;
        };
        for definition in &connection.asteroid_spawn_definitions {
            let Some(asteroid) = &definition.asteroid else {
                continue;
            };
            asteroid_connections
                .entry(asteroid.clone())
                .or_default()
                .push((connection.from.clone(), connection.to.clone()));
        }
    }

    GraphData {
        recipes_by_product,
        resources_by_product,
        spoil_sources,
        items_by_place_result,
        tech_units,
        techs_by_unlock,
        resource_planets: build_resource_planets(store),
        generated_by,
        asteroid_locations,
        asteroid_connections,
        launch_sources,
    }
}

/// 空间地点名 → 可达性节点：星球（planet，planet 是 space-location 子类）
/// 归 Planet 组（planet-discovery 科技）；深空地点 → Space（unlock 科技）。
fn space_node(store: &PrototypeStore, name: &str) -> Accessible {
    if store.get(PrototypeGroup::Planet, name).is_some() {
        Accessible::Planet(name.to_string())
    } else {
        Accessible::Space(name.to_string())
    }
}

/// 里程碑节点按依赖关系**拓扑排序**（依赖在前）。
///
/// 不特设任何具体类型/链路——按 `Requirements` **递归展开**实际依赖图：对每个
/// 里程碑节点，BFS 展开其依赖叶（`item → recipe → 解锁科技 → tech_units 物品 →
/// …`，visited 去重防环），收集它**传递依赖**的其它里程碑，再 Kahn 排序。
/// 依赖环内节点按原序附加（UI 不崩）。
pub fn milestone_order(store: &PrototypeStore, milestones: &[Accessible]) -> Vec<Accessible> {
    let graph = build_graph(store);
    milestone_order_with_graph(store, &graph, milestones)
}

/// 用已构建的依赖图做里程碑依赖排序（供 `Runtime` 复用缓存的图）。
pub fn milestone_order_with_graph(
    store: &PrototypeStore,
    graph: &GraphData,
    milestones: &[Accessible],
) -> Vec<Accessible> {
    // 依赖图本身存在环：`Any` 节点只需满足其中一支，但闭包会把两支都当作
    // 依赖，于是同一对里程碑可能互相"依赖"。不过真实的解锁链是**更短**的那
    // 条（例如 logistic→automation 直接可达，而 automation→logistic 要绕行整
    // 个星球科技树，路径长得多）。因此对每对里程碑，以最短路径（BFS 跳数）判
    // 定方向：较短的即为真实依赖，据此有向即可打破环。
    let dist = milestone_pair_distances(store, graph, milestones);

    // 有向边：前置 → 后置；indeg = 该节点"必须先满足"的里程碑数（去重）。
    let mut adj: HashMap<Accessible, Vec<Accessible>> = HashMap::new();
    let mut indeg: HashMap<Accessible, usize> = HashMap::new();
    for m in milestones {
        indeg.entry(m.clone()).or_insert(0);
    }
    for a in milestones {
        for b in milestones {
            if a == b {
                continue;
            }
            // a 依赖 b（b 更短即真实前置）→ b 排在 a 前。
            let d_ab = dist.get(a).and_then(|m| m.get(b)).copied().unwrap_or(usize::MAX);
            let d_ba = dist.get(b).and_then(|m| m.get(a)).copied().unwrap_or(usize::MAX);
            match d_ab.cmp(&d_ba) {
                // a→b 更短：a 依赖 b。
                std::cmp::Ordering::Less => {
                    adj.entry(b.clone()).or_default().push(a.clone());
                    *indeg.entry(a.clone()).or_insert(0) += 1;
                }
                // b→a 更短：b 依赖 a。
                std::cmp::Ordering::Greater => {
                    adj.entry(a.clone()).or_default().push(b.clone());
                    *indeg.entry(b.clone()).or_insert(0) += 1;
                }
                // 等距（真 Any 互相或无关）：不建边，维持原序。
                std::cmp::Ordering::Equal => {}
            }
        }
    }

    // Kahn：无里程碑依赖（入度 0）的先出队；按里程碑原序稳定排序保证确定性。
    let mut queue: VecDeque<Accessible> = milestones
        .iter()
        .filter(|node| indeg.get(*node).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();
    let mut out: Vec<Accessible> = Vec::new();
    while let Some(node) = queue.pop_front() {
        out.push(node.clone());
        if let Some(dependents) = adj.get(&node) {
            for dependent in dependents {
                if let Some(deg) = indeg.get_mut(dependent) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }
    }
    // 残余环（等距互依）按原序补在后面。
    for node in milestones {
        if !out.contains(node) {
            out.push(node.clone());
        }
    }
    out
}

/// 每对里程碑之间的最短依赖跳数（BFS 于需求闭包上、全节点类型展开）。
/// `dist[&a][&b]` = 从 a 出发沿需求最少几步可达 b（b 是 a 的（传递）前提）。
/// BFS 首次到达即最短；不可达则无对应项。
fn milestone_pair_distances(
    store: &PrototypeStore,
    graph: &GraphData,
    milestones: &[Accessible],
) -> HashMap<Accessible, HashMap<Accessible, usize>> {
    let mut result = HashMap::new();
    for m in milestones {
        let mut order_index: HashMap<Accessible, usize> = HashMap::new();
        let mut dist: HashMap<Accessible, usize> = HashMap::new();
        // BFS 层级：dist[node] = 里程碑 → node 的最小跳数。
        dist.insert(m.clone(), 0);
        let mut queue: VecDeque<Accessible> = VecDeque::new();
        queue.push_back(m.clone());
        while let Some(node) = queue.pop_front() {
            let cur = dist[&node];
            let mut leaves = Vec::new();
            requirements(store, graph, &node).leaves(&mut leaves);
            for leaf in leaves {
                // 只记录首次（最短）到达。
                if dist.contains_key(&leaf) {
                    continue;
                }
                dist.insert(leaf.clone(), cur + 1);
                queue.push_back(leaf.clone());
            }
        }
        // 抽取里程碑节点的最短距离。
        for milestone in milestones {
            if milestone == m {
                continue;
            }
            if let Some(&d) = dist.get(milestone) {
                order_index.entry(milestone.clone()).or_insert(d);
            }
        }
        result.insert(m.clone(), order_index);
    }
    result
}

/// 对象 → 依赖声明（按类型 match 分发）。
fn requirements(store: &PrototypeStore, graph: &GraphData, node: &Accessible) -> Requirement {
    match node {
        Accessible::Tech(name) => {
            let tech = store
                .get(PrototypeGroup::Technology, name)
                .and_then(|record| record.component::<TechnologyComponent>());

            match tech {
                // 科技可达 = 所有前置科技 + 所有科技原料物品（unit.ingredients 中
                // 非 hidden 的物品）同时可达。原版 schema 中 enabled 表示"科技是否
                // 出现在科技树"（默认 true），并非"已解锁"——科技仍需研究，故不豁免
                // enabled：禁用科技瓶（物品里程碑）即可阻断依赖它的科技。
                Some(tech) => {
                    let mut all: Vec<Requirement> = Vec::new();
                    for prereq in &tech.prerequisites {
                        all.push(Requirement::Node(Accessible::Tech(prereq.clone())));
                    }
                    if let Some(items) = graph.tech_units.get(name) {
                        for item in items {
                            all.push(Requirement::Node(Accessible::Item(item.clone())));
                        }
                    }
                    // 无任何依赖（无前置且无科技原料物品）→ All() 空 → 根。
                    Requirement::All(all)
                }
                // 缺失科技（不会作为节点，也不会被 seed）。
                None => Requirement::Any(Vec::new()),
            }
        }
        Accessible::Recipe(name) => {
            let record = store.get(PrototypeGroup::Recipe, name);
            let recipe = record.and_then(|r| r.component::<RecipeComponent>());
            let hidden = record
                .and_then(|r| r.component::<PrototypeBaseComponent>())
                .map(|base| base.hidden)
                .unwrap_or(false);
            match recipe {
                // hidden 配方：mod 保守替换产物（即便 enabled=true）→ 无法制作，
                // 其产物不能视为可达（不作根种子，也不因依赖满足而可达）。
                Some(_) if hidden => Requirement::Any(Vec::new()),
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
                    // 机器依赖：配方出现即视为有对应组装机（合理 mod 设计）——
                    // 不再要求机器可达。自动规划在枚举时会处理"无解锁机器则选
                    // 评分最低的 1 台"。
                    // 至少一个解锁科技可达
                    let unlocks = graph.techs_by_unlock.get(name).cloned().unwrap_or_default();
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
                any.extend(
                    recipes
                        .iter()
                        .map(|recipe| Requirement::Node(Accessible::Recipe(recipe.clone()))),
                );
            }
            if let Some(resources) = graph.resources_by_product.get(name) {
                any.extend(
                    resources
                        .iter()
                        .map(|entity| Requirement::Node(Accessible::Entity(entity.clone()))),
                );
            }
            if let Some(spoils) = graph.spoil_sources.get(name) {
                // 变质来源：可变质成该物品的任一物品（隐含 spoil.<item> 配方）
                any.extend(
                    spoils
                        .iter()
                        .map(|source| Requirement::Node(Accessible::Item(source.clone()))),
                );
            }
            // 星球资源（可挖掘/loot/捕获产物）：星球解锁即可获取
            if let Some(planets) = graph.resource_planets.get(name) {
                any.extend(
                    planets
                        .iter()
                        .map(|planet| Requirement::Node(Accessible::Planet(planet.clone()))),
                );
            }
            // 发射产物：把其它物品发射上太空可得到该物品（ItemLaunch 机制）。
            // 物品可通过发射另一物品得到——发射的基物品可达即可。
            if let Some(bases) = graph.launch_sources.get(name) {
                any.extend(
                    bases
                        .iter()
                        .map(|base| Requirement::Node(Accessible::Item(base.clone()))),
                );
            }
            Requirement::Any(any)
        }
        Accessible::Fluid(name) => {
            let mut any: Vec<Requirement> = Vec::new();
            if let Some(recipes) = graph.recipes_by_product.get(name) {
                any.extend(
                    recipes
                        .iter()
                        .map(|recipe| Requirement::Node(Accessible::Recipe(recipe.clone()))),
                );
            }
            if let Some(resources) = graph.resources_by_product.get(name) {
                any.extend(
                    resources
                        .iter()
                        .map(|entity| Requirement::Node(Accessible::Entity(entity.clone()))),
                );
            }
            // 星球流体（如某星球生成的水/岩浆）：星球解锁即可获取
            if let Some(planets) = graph.resource_planets.get(name) {
                any.extend(
                    planets
                        .iter()
                        .map(|planet| Requirement::Node(Accessible::Planet(planet.clone()))),
                );
            }
            Requirement::Any(any)
        }
        Accessible::Entity(name) => {
            // 星岩/小行星（含星岩碎片 chunk）：来源 = 生成它的空间地点
            // （SpaceLocation 自带 / SpaceConnection 飞行沿途）+ 死亡 trigger
            // 产物（大星岩 dying_trigger_effect → create-asteroid-chunk）。
            // 不从名称或"碎片"特判为根——都按星球地点 / 死亡链推断。
            let mut sources: Vec<Requirement> = Vec::new();
            if let Some(locations) = graph.asteroid_locations.get(name) {
                sources.extend(locations.iter().map(|loc| Requirement::Node(loc.clone())));
            }
            if let Some(connections) = graph.asteroid_connections.get(name) {
                for (from, to) in connections {
                    // space-connection 星岩：飞船飞行沿途生成，需两端地点都可达。
                    sources.push(Requirement::All(vec![
                        Requirement::Node(space_node(store, from)),
                        Requirement::Node(space_node(store, to)),
                    ]));
                }
            }
            if let Some(parents) = graph.generated_by.get(name) {
                sources.extend(
                    parents
                        .iter()
                        .map(|parent| Requirement::Node(Accessible::Entity(parent.clone()))),
                );
            }
            if !sources.is_empty() {
                return Requirement::Any(sources);
            }
            // 无地点/连接/死亡来源：星岩严格按数据来源判定，无来源 → 不可达
            // （不引入"初始轨道恒有"等额外假设）。以下是普通实体的原逻辑。
            let record = store.get(PrototypeGroup::Entity, name);
            if graph.resource_planets.contains_key(name) {
                // 星球刷新的可采集实体（矿藏/植物/可挖掘/loot/产卵器）：
                // 任一生成星球解锁即可采集（星球解锁后资源可自由移动）。
                let planets = graph
                    .resource_planets
                    .get(name)
                    .cloned()
                    .unwrap_or_default();
                Requirement::Any(
                    planets
                        .into_iter()
                        .map(|planet| Requirement::Node(Accessible::Planet(planet)))
                        .collect(),
                )
            } else {
                // 实体解锁条件 = Any([
                //   a) place_result 是该实体的物品（放置后成为该实体），
                //   b) entity.placeable_by 中的物品（机器人/蓝图放置物），
                // ])
                let mut any: Vec<Requirement> = Vec::new();
                if let Some(items) = graph.items_by_place_result.get(name) {
                    any.extend(
                        items
                            .iter()
                            .map(|item| Requirement::Node(Accessible::Item(item.clone()))),
                    );
                }
                if let Some(placeable) = record
                    .and_then(|r| r.component::<EntityComponent>())
                    .and_then(|e| e.placeable_by.as_ref())
                {
                    any.extend(
                        placeable
                            .items
                            .iter()
                            .map(|item| Requirement::Node(Accessible::Item(item.clone()))),
                    );
                }
                Requirement::Any(any)
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
        Accessible::Planet(name) => {
            // nauvis 初始解锁（根）；其他星球需要 planet-discovery-<星球> 科技。
            if name == "nauvis" {
                Requirement::All(Vec::new())
            } else {
                Requirement::All(vec![Requirement::Node(Accessible::Tech(format!(
                    "planet-discovery-{name}"
                )))])
            }
        }
        Accessible::Electricity | Accessible::Heat => Requirement::All(Vec::new()),
    }
}

/// 收集全部可达性节点（科技/配方/物品/流体/实体/品质/空间/星球）。
/// 物品/流体以**配方引用**为准（原料+产物）——某些物品类型不在
/// Item 组（如 rail-planner 的 rail），但它们可经配方获得，必须入图。
fn collect_nodes(store: &PrototypeStore) -> Vec<Accessible> {
    let mut out = Vec::new();
    for group in [
        PrototypeGroup::Technology,
        PrototypeGroup::Recipe,
        PrototypeGroup::Entity,
        PrototypeGroup::AsteroidChunk,
        PrototypeGroup::Quality,
        PrototypeGroup::SpaceLocation,
        PrototypeGroup::Planet,
    ] {
        for record in store.group(group) {
            let node = match group {
                PrototypeGroup::Technology => Accessible::Tech(record.name.clone()),
                PrototypeGroup::Recipe => Accessible::Recipe(record.name.clone()),
                PrototypeGroup::Entity | PrototypeGroup::AsteroidChunk => {
                    Accessible::Entity(record.name.clone())
                }
                PrototypeGroup::Quality => Accessible::Quality(record.name.clone()),
                PrototypeGroup::SpaceLocation => Accessible::Space(record.name.clone()),
                PrototypeGroup::Planet => Accessible::Planet(record.name.clone()),
                _ => continue,
            };
            out.push(node);
        }
    }
    // 物品/流体：Item/Fluid 组 ∪ 全部配方的原料与产物。
    let mut items: AIndexSet<String> = store
        .group(PrototypeGroup::Item)
        .map(|record| record.name.clone())
        .collect();
    let mut fluids: AIndexSet<String> = store
        .group(PrototypeGroup::Fluid)
        .map(|record| record.name.clone())
        .collect();
    for record in store.group(PrototypeGroup::Recipe) {
        let Some(recipe) = record.component::<RecipeComponent>() else {
            continue;
        };
        for ingredient in &recipe.ingredients {
            match ingredient {
                Ingredient::Item(item) => {
                    items.insert(item.name.clone());
                }
                Ingredient::Fluid(fluid) => {
                    fluids.insert(fluid.name.clone());
                }
            }
        }
        for result in &recipe.results {
            match result {
                Product::Item(product) => {
                    items.insert(product.name.clone());
                }
                Product::Fluid(product) => {
                    fluids.insert(product.name.clone());
                }
            }
        }
    }
    out.extend(items.into_iter().map(Accessible::Item));
    out.extend(fluids.into_iter().map(Accessible::Fluid));
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
    compute_accessibility_with_graph(store, options, &graph)
}

/// 用已构建的依赖图计算可达性（供 `Runtime` 复用缓存的图，避免每交互重建）。
pub fn compute_accessibility_with_graph(
    store: &PrototypeStore,
    options: &AccessibilityOptions,
    graph: &GraphData,
) -> Accessibility {
    if options.all_accessible {
        let accessible = collect_nodes(store).into_iter().collect();
        return Accessibility { accessible };
    }

    let nodes = collect_nodes(store);

    // 反向依赖表：依赖某个对象的对象集合（用于传播时触发重新评估）。
    let mut reverse: HashMap<Accessible, Vec<Accessible>> = HashMap::new();
    for node in &nodes {
        let mut leaves = Vec::new();
        requirements(store, graph, node).leaves(&mut leaves);
        for leaf in leaves {
            reverse.entry(leaf).or_default().push(node.clone());
        }
    }

    let mut accessible: AIndexSet<Accessible> = AIndexSet::default();
    let mut queue: VecDeque<Accessible> = VecDeque::new();
    let mut seed = |node: Accessible| {
        if options.forced_inaccessible.contains(&node) {
            return;
        }
        if accessible.insert(node.clone()) {
            queue.push_back(node);
        }
    };

    // 强制可达（里程碑 unlocked=true）：并入根种子。
    for node in &options.forced_accessible {
        seed(node.clone());
    }
    // 恒真根：电/热
    seed(Accessible::Electricity);
    seed(Accessible::Heat);
    // 无依赖对象（enabled 配方/科技、矿藏实体、normal 品质……）作为根种子。
    for node in &nodes {
        if matches!(requirements(store, graph, node), Requirement::All(ref list) if list.is_empty())
        {
            seed(node.clone());
        }
    }

    // 正向传播。
    while let Some(node) = queue.pop_front() {
        let dependents = reverse.get(&node).cloned().unwrap_or_default();
        for dependent in dependents {
            if options.forced_inaccessible.contains(&dependent) {
                continue;
            }
            if accessible.contains(&dependent) {
                continue;
            }
            if requirements(store, graph, &dependent).is_satisfied(&accessible) {
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

    /// 仓库内真实 dump（相对路径，测试可移植；不依赖机器上的 %APPDATA%）。
    const REAL_DUMP: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/data-raw-dump.json"
    );

    fn load_real_dump() -> Option<PrototypeStore> {
        if !std::path::Path::new(REAL_DUMP).exists() {
            eprintln!("[skip] 无真实 dump（{REAL_DUMP}），跳过");
            return None;
        }
        let raw = std::fs::read(REAL_DUMP).expect("读 dump");
        let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump");
        match PrototypeStore::load(&dump) {
            Ok(store) => Some(store),
            Err(error) => {
                for failure in &error.failures {
                    eprintln!("[load 失败] {:?}", failure);
                }
                panic!("dump 加载失败: {:?}", error);
            }
        }
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

    fn recipe(
        name: &str,
        ingredients: Vec<&str>,
        results: Vec<&str>,
        enabled: bool,
        unlock_techs: Vec<&str>,
    ) -> serde_json::Value {
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
                "rail": { "type": "item", "name": "rail" },
                "fish-food": { "type": "item", "name": "fish-food", "spoil_result": "spoilage-fish" },
                "spoilage-fish": { "type": "item", "name": "spoilage-fish" },
                "assembling-machine-1": { "type": "item", "name": "assembling-machine-1", "place_result": "assembling-machine-1" }
            },
            "fluid": {},
            "technology": {},
            "recipe": {
                "iron-ore": recipe("iron-ore", vec![], vec!["iron-ore"], true, vec![]),
                "iron-plate": recipe("iron-plate", vec!["iron-ore"], vec!["iron-plate"], true, vec![]),
                "steel-plate": recipe("steel-plate", vec!["iron-plate"], vec!["steel-plate"], false, vec!["tech-steel"]),
                "engine-unit": recipe("engine-unit", vec!["iron-plate", "steel-plate"], vec!["engine-unit"], false, vec!["tech-engine"]),
                "rail": recipe("rail", vec!["iron-plate"], vec!["rail"], true, vec![]),
                "fish-food": recipe("fish-food", vec!["iron-plate"], vec!["fish-food"], true, vec![]),
                "assembling-machine-1": recipe("assembling-machine-1", vec!["iron-plate"], vec!["assembling-machine-1"], true, vec![])
            },
            "assembling-machine": {
                "assembling-machine-1": {
                    "type": "assembling-machine", "name": "assembling-machine-1",
                    "crafting_categories": ["crafting"], "crafting_speed": 1,
                    "module_slots": 0, "energy_usage": "90kW",
                    "energy_source": { "type": "electric", "drain": "0J" }
                },
                "test-entity-b": {
                    "type": "assembling-machine", "name": "test-entity-b",
                    "crafting_categories": ["crafting"], "crafting_speed": 1,
                    "module_slots": 0, "energy_usage": "90kW",
                    "energy_source": { "type": "electric", "drain": "0J" },
                    "placeable_by": { "item": "rail", "count": 1 }
                },
                "test-entity-dead": {
                    "type": "assembling-machine", "name": "test-entity-dead",
                    "crafting_categories": ["crafting"], "crafting_speed": 1,
                    "module_slots": 0, "energy_usage": "90kW",
                    "energy_source": { "type": "electric", "drain": "0J" },
                    "placeable_by": { "item": "magic-item", "count": 1 }
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
        dump["technology"]["tech-steel"]["effects"] =
            json!([{ "type": "unlock-recipe", "recipe": "steel-plate" }]);
        dump["technology"]["tech-engine"]["effects"] =
            json!([{ "type": "unlock-recipe", "recipe": "engine-unit" }]);
        dump
    }

    #[test]
    fn forward_propagation_from_enabled_roots() {
        let store = load(chain_dump());
        let result = compute_accessibility(&store, &AccessibilityOptions::default());
        // enabled 配方产物：iron-ore、iron-plate、assembling-machine-1 可达
        assert!(
            result.is_item_accessible("iron-plate"),
            "enabled 配方产物应可达"
        );
        assert!(
            result.is_item_accessible("assembling-machine-1"),
            "机器（enabled 配方产出）应可达"
        );
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
        // 实体解锁：place_result 反查（机器由同名 item 放置）→ 可达
        assert!(
            result.is_accessible(&Accessible::Entity("assembling-machine-1".to_string())),
            "机器实体应通过 place_result 物品可达"
        );
        // 实体解锁：placeable_by（b 分支）——rail 可达 → 实体可达；
        // magic-item 不可达 → 实体不可达
        assert!(
            result.is_accessible(&Accessible::Entity("test-entity-b".to_string())),
            "placeable_by 指向可达物品时应可达"
        );
        assert!(
            !result.is_accessible(&Accessible::Entity("test-entity-dead".to_string())),
            "placeable_by 指向不可达物品时应不可达"
        );
        // 变质来源：fish-food（enabled 配方产物）变质为 spoilage-fish → 后者可达
        assert!(
            result.is_item_accessible("spoilage-fish"),
            "仅靠变质来源获得的物品应可达（spoil_result 链）"
        );
    }

    #[test]
    fn user_forced_inaccessible_prunes_descendants() {
        let store = load(chain_dump());
        let options = AccessibilityOptions {
            forced_inaccessible: [Accessible::Item("iron-plate".to_string())]
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
    fn user_forced_accessible_overrides_missing_sources() {
        let store = load(chain_dump());
        let options = AccessibilityOptions {
            forced_accessible: [Accessible::Item("magic-item".to_string())]
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
        let Some(store) = load_real_dump() else {
            return;
        };
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
        for item in [
            "coal",
            "crude-oil",
            "petroleum-gas",
            "chemical-plant",
            "oil-refinery",
            "sulfur",
        ] {
            eprintln!("塑料链 [{item}] = {}", result.is_item_accessible(item));
        }
        for tech in [
            "automation",
            "oil-processing",
            "plastics",
            "advanced-oil-processing",
            "sulfur-processing",
        ] {
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
        assert!(
            result.is_item_accessible("iron-ore"),
            "铁矿应可达（enabled 采矿配方）"
        );
        assert!(result.is_item_accessible("iron-plate"));
        assert!(
            result.is_item_accessible("steel-plate"),
            "钢（科技链）应可达"
        );
        assert!(result.is_item_accessible("copper-cable"));
        // 石油链（原油 minable 或泵送 + 科技解锁）
        assert!(
            result.is_item_accessible("plastic-bar"),
            "塑料（石油链）应可达"
        );
        // 核：离心机链（科技解锁）
        assert!(
            result.is_item_accessible("uranium-235"),
            "铀-235（科技链）应可达"
        );
        // 高级链
        assert!(
            result.is_item_accessible("electromagnetic-science-pack"),
            "电磁科学包应可达"
        );
        assert!(
            result.is_item_accessible("space-science-pack"),
            "空间科学包应可达"
        );
        // 变质机制：spoilage（仅靠可变质物品变质而来）应可达
        assert!(
            result.is_item_accessible("spoilage"),
            "spoilage（变质来源）应可达"
        );
    }

    /// 真实 dump：不显式禁用科技时，原版所有科技瓶都应可达（可解锁）。
    /// 科技瓶链覆盖基础/石油/军事/生产/物流/空间/冶金/电磁/农业/低温/普罗米修斯，
    /// 是可达性传播完备性的关键验证。
    #[test]
    fn real_dump_all_vanilla_science_packs_reachable() {
        let Some(store) = load_real_dump() else {
            return;
        };
        let result = compute_accessibility(&store, &AccessibilityOptions::default());

        let packs = [
            "automation-science-pack",
            "logistic-science-pack",
            "military-science-pack",
            "chemical-science-pack",
            "production-science-pack",
            "utility-science-pack",
            "space-science-pack",
            "metallurgic-science-pack",
            "electromagnetic-science-pack",
            "agricultural-science-pack",
            "cryogenic-science-pack",
            "promethium-science-pack",
        ];
        let mut all_ok = true;
        for pack in packs {
            let ok = result.is_item_accessible(pack);
            eprintln!("科技瓶 [{pack}] = {ok}");
            all_ok &= ok;
        }
        assert!(all_ok, "不显式禁用科技时原版所有科技瓶都应可达");
    }

    /// 诊断：rail 在 store 中的实际归属（验证"rail 是否在 Item 组"）。
    #[test]
    fn real_dump_rail_in_store() {
        let Some(store) = load_real_dump() else {
            return;
        };
        eprintln!(
            "Item 组含 rail = {}",
            store.get(PrototypeGroup::Item, "rail").is_some()
        );
        eprintln!(
            "Entity 组含 straight-rail = {}",
            store.get(PrototypeGroup::Entity, "straight-rail").is_some()
        );
        eprintln!(
            "Recipe 组含 rail = {}",
            store.get(PrototypeGroup::Recipe, "rail").is_some()
        );
        let rail_items: Vec<String> = store
            .group(PrototypeGroup::Item)
            .filter(|record| record.name.contains("rail"))
            .map(|record| record.name.clone())
            .collect();
        eprintln!("Item 组含 rail 的名字: {rail_items:?}");
        // rail 配方的 results 是否含 Product::Item("rail")
        if let Some(record) = store.get(PrototypeGroup::Recipe, "rail") {
            if let Some(recipe) = record.component::<RecipeComponent>() {
                for result in &recipe.results {
                    if let Product::Item(product) = result {
                        eprintln!("rail 配方产物: item [{}]", product.name);
                    }
                }
            }
        }
    }

    /// enabled 但 hidden 的配方：mod 保守替换产物，不可制作 → 产物不可达
    /// （不放进可达性根种子；即便有解锁科技或空原料也因 hidden 不可制作）。
    #[test]
    fn hidden_enabled_recipe_is_not_root() {
        let dump = json!({
            "item": {
                "visible-item": { "type": "item", "name": "visible-item" },
                "secret-item": { "type": "item", "name": "secret-item" }
            },
            "fluid": {},
            "technology": {
                "tech-secret": {
                    "type": "technology", "name": "tech-secret",
                    "prerequisites": [], "enabled": true,
                    "effects": [{ "type": "unlock-recipe", "recipe": "secret" }],
                    "unit": { "count": 10, "time": 10, "ingredients": [] }
                }
            },
            "recipe": {
                "visible": {
                    "type": "recipe", "name": "visible",
                    "energy_required": 1, "ingredients": [],
                    "results": [{ "type": "item", "name": "visible-item", "amount": 1 }],
                    "categories": ["crafting"], "enabled": true, "hidden": false
                },
                "secret": {
                    "type": "recipe", "name": "secret",
                    "energy_required": 1, "ingredients": [],
                    "results": [{ "type": "item", "name": "secret-item", "amount": 1 }],
                    "categories": ["crafting"], "enabled": true, "hidden": true
                },
                "secret-unlocked": {
                    "type": "recipe", "name": "secret-unlocked",
                    "energy_required": 1, "ingredients": [],
                    "results": [{ "type": "item", "name": "secret-item", "amount": 1 }],
                    "categories": ["crafting"], "enabled": false, "hidden": true
                }
            }
        });
        let store = load(dump);
        let result = compute_accessibility(&store, &AccessibilityOptions::default());
        assert!(
            result.is_item_accessible("visible-item"),
            "未 hidden 的 enabled 配方产物应可达（根）"
        );
        assert!(
            !result.is_item_accessible("secret-item"),
            "enabled 但 hidden 的配方产物不可达（不作根）"
        );
    }

    /// 星岩按空间地点判定：基础三色星岩（初始轨道）可达；promethium 星岩
    /// 经 shattered-planet（深空地点，promethium-science-pack 科技可达）与
    /// 死亡生成链可递推到星岩碎片。
    #[test]
    fn real_dump_asteroid_locations() {
        let Some(store) = load_real_dump() else {
            return;
        };
        let result = compute_accessibility(&store, &AccessibilityOptions::default());
        for e in [
            "metallic-asteroid-chunk",
            "carbonic-asteroid-chunk",
            "oxide-asteroid-chunk",
        ] {
            assert!(
                result.is_accessible(&Accessible::Entity(e.to_string())),
                "基础星岩（初始轨道）应可达: {e}"
            );
        }
        // promethium 星岩：huge（深空地点生成）→ small → chunk，且碎片可采集为物品。
        assert!(
            result.is_accessible(&Accessible::Entity("huge-promethium-asteroid".to_string())),
            "promethium 大星岩应可达（shattered-planet 地点）"
        );
        assert!(
            result.is_accessible(&Accessible::Entity("promethium-asteroid-chunk".to_string())),
            "promethium 星岩碎片应可达（死亡链）"
        );
        assert!(
            result.is_item_accessible("promethium-asteroid-chunk"),
            "promethium 星岩碎片物品应可达（星岩收集器采集）"
        );
        // promethium **大星岩**仅由 shattered-planet 深空地点生成——锁定该地点后
        // 大星岩不可达（地点判定生效）。星岩碎片（AsteroidChunk 组）初始可采集，
        // 不受地点锁定影响。
        let mut locked = AccessibilityOptions::default();
        locked.forced_inaccessible = [Accessible::Space("shattered-planet".to_string())]
            .into_iter()
            .collect();
        let locked_result = compute_accessibility(&store, &locked);
        assert!(
            !locked_result.is_accessible(&Accessible::Entity("huge-promethium-asteroid".to_string())),
            "锁定 shattered-planet 后 promethium 大星岩应不可达（地点判定生效）"
        );
        assert!(
            locked_result.is_item_accessible("metallic-asteroid-chunk"),
            "锁定深空地点不应影响基础星岩"
        );
    }

    /// 新里程碑（强制覆盖）行为：禁用 automation-science-pack 物品后，大量依赖
    /// 科技瓶的科技不可达——至少一半（科技依赖 = 前置科技 + 科技原料物品）。
    #[test]
    fn real_dump_disabling_automation_pack_blocks_half_the_techs() {
        let Some(store) = load_real_dump() else {
            return;
        };
        let mut options = AccessibilityOptions::default();
        options.forced_inaccessible =
            [Accessible::Item("automation-science-pack".to_string())].into_iter().collect();
        let result = compute_accessibility(&store, &options);

        let total = store.group(PrototypeGroup::Technology).count();
        let accessible = store
            .group(PrototypeGroup::Technology)
            .filter(|record| result.is_accessible(&Accessible::Tech(record.name.clone())))
            .count();
        let inaccessible = total - accessible;
        eprintln!("禁用 automation-science-pack：科技 total={total} 可达={accessible} 不可达={inaccessible}");
        assert!(
            inaccessible >= total / 2,
            "禁用 automation-science-pack 后至少一半科技不可达（实际 {inaccessible}/{total}）"
        );
    }

    /// 里程碑拓扑排序：科技瓶按依赖序（非字母序）——automation-science-pack
    /// 依赖最少（无其它科技瓶依赖它），logistics/military 等传递依赖它，应靠后。
    #[test]
    fn milestone_order_follows_transitive_dependencies() {
        let Some(store) = load_real_dump() else {
            return;
        };
        let nodes = [
            Accessible::Item("logistic-science-pack".to_string()),
            Accessible::Item("automation-science-pack".to_string()),
            Accessible::Item("military-science-pack".to_string()),
        ];
        let order = milestone_order(&store, &nodes);
        let pos = |name: &str| {
            order
                .iter()
                .position(|n| n == &Accessible::Item(name.to_string()))
                .expect("节点应在排序结果中")
        };
        assert!(
            pos("automation-science-pack") < pos("logistic-science-pack"),
            "automation-science-pack 应排在 logistic-science-pack 前（logistics 依赖它）"
        );
        assert!(
            pos("automation-science-pack") < pos("military-science-pack"),
            "automation-science-pack 应排在 military-science-pack 前"
        );
    }

    /// 发射产物可达：某物品可经把另一物品发射上太空得到，基物品可达即可。
    /// 回归：此前可达性未考虑 rocket_launch_products，发射专属物品（无配方/矿藏）
    /// 被误判为不可达。
    #[test]
    fn launch_products_reachable_from_base_item() {
        let store = load(json!({
            "item": {
                "satellite": {
                    "type": "item", "name": "satellite", "stack_size": 1,
                    "rocket_launch_products": [
                        { "type": "item", "name": "space-item", "amount": 100 }
                    ]
                },
                "space-item": { "type": "item", "name": "space-item", "stack_size": 1 }
            },
            "recipe": {
                "satellite": {
                    "type": "recipe", "name": "satellite",
                    "energy_required": 1, "enabled": true,
                    "ingredients": [],
                    "results": [{ "type": "item", "name": "satellite", "amount": 1 }]
                }
            }
        }));
        let result = compute_accessibility(&store, &AccessibilityOptions::default());
        assert!(
            result.is_item_accessible("satellite"),
            "卫星应可达（enabled 配方）"
        );
        assert!(
            result.is_item_accessible("space-item"),
            "space-item 应经发射卫星（rocket_launch_products）可达"
        );
    }
}
