//! Phase 3：原型仓库（PrototypeStore）。
//!
//! 把游戏 dump 反序列化为**按 (type, name) 唯一**的组件化原型记录：
//! - 每个原型按 `COMPONENT_LIST`（继承链组件 + 组合组件）反序列化，
//!   结果存入 `components: AIndexMap<String, ComponentValue>`
//! - **聚合标签**（参考游戏 `LuaPrototypes`）：含 `EntityComponent` → `Entity` 组、
//!   含 `ItemComponent` → `Item` 组，其余按原始 type_ 记录为 `Other(type_)`
//! - 主键语义 = `(PrototypeGroup, name)`：同名跨组是不同原型
//!   （如 "assembling-machine-1" 的 item 与 entity 是两条记录；"speed-module" 的
//!   recipe/module/technology 是三条记录）；存储上**先按组分类**（外层 map 的 key
//!   是 PrototypeGroup，内层是 name → 记录），组内查询与排序（如按 order 排序）更直接
//! - 惰性派生（加载后不可变，`OnceLock` 惰性构建）：组内 order 排序、科技反向依赖

use crate::generated_components::prototype_groups::prototype_group_from_type;
use crate::generated_components::{
    BoilerComponent, COMPONENT_LIST, Component, ComponentValue, CraftingMachineComponent,
    GeneratorComponent, ItemSubGroupComponent, PrototypeBaseComponent, QualityComponent,
    RecipeComponent, TechnologyComponent, deserialize_component,
};
use serde_json::Value;
use std::{fmt, sync::OnceLock};

/// 带 ahash 的索引 Map（与 metatorio_egui 的 AIndexMap 同构）。
pub type AIndexMap<K, V> = indexmap::IndexMap<K, V, ahash::RandomState>;

/// 聚合组（生成器生成：Entity/Item + 每个关注类型一个变体 + Unknown 兜底）。
pub use crate::generated_components::prototype_groups::PrototypeGroup;

/// 单个原型记录：组件集合 + 聚合标签。
#[derive(Debug, Clone)]
pub struct PrototypeRecord {
    /// 原型名（dump 内唯一）。
    pub name: String,
    /// 原始 dump 键名（如 "assembling-machine"、"recipe"）。
    pub type_: String,
    /// 聚合标签（组件推导）。
    pub group: PrototypeGroup,
    /// 组件集合：组件名（COMPONENT_LIST 条目）→ 反序列化后的组件。
    pub components: AIndexMap<&'static str, ComponentValue>,
}

impl PrototypeRecord {
    /// 是否含某组件（按组件名，如 "CraftingMachineComponent"）。
    pub fn has(&self, component: &str) -> bool {
        self.components.contains_key(component)
    }

    /// 取组件（按组件名）。
    pub fn get(&self, component: &str) -> Option<&ComponentValue> {
        self.components.get(component)
    }

    /// 类型安全地取组件（`component::<CraftingMachineComponent>()`）。
    /// 组件缺失 → None；类型不匹配 → panic（插入时已保证变体正确）。
    pub fn component<T: Component>(&self) -> Option<&T> {
        self.components.get(T::TYPENAME).and_then(T::as_ref_opt)
    }

    /// 类型安全地取组件，缺失时 panic（带记录名与组件名）。
    #[track_caller]
    pub fn component_required<T: Component>(&self) -> &T {
        match self.components.get(T::TYPENAME) {
            Some(cv) => T::as_ref(cv),
            None => panic!(
                "原型 {} ({}) 缺少组件 {}",
                self.name,
                self.type_,
                T::TYPENAME
            ),
        }
    }
}

/// 加载失败汇总（不 panic：收集全部失败，由调用方决定处理）。
#[derive(Debug, Clone)]
pub struct LoadError {
    /// 失败的 (type_, name, 组件名 + 错误信息)。
    pub failures: Vec<(String, String, String)>,
    /// 尝试反序列化的原型总数。
    pub total: usize,
    /// 成功数。
    pub succeeded: usize,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "原型加载失败 {}/{}：\n{}",
            self.failures.len(),
            self.total,
            self.failures
                .iter()
                .map(|(t, n, e)| format!("  {t}/{n}: {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

impl std::error::Error for LoadError {}

/// 排序信息：大组 → 小组 → 组内条目名（大组/小组/条目均按 order 排序）。
///
/// 用于选择器渲染：大组分 tab，每个小组渲染完换行（参考原始 get_order_info）。
pub type OrderInfo = AIndexMap<String, AIndexMap<String, Vec<String>>>;

/// 反查索引：条目名 → (大组索引, 小组索引, 条目索引)。
///
/// 用于按顺序渲染配方预览中的原料和产物（参考原始 get_reverse_order_info）。
pub type ReverseOrderInfo = AIndexMap<String, (usize, usize, usize)>;

/// 原型仓库：按 (PrototypeGroup, name) 索引的全部原型记录。
#[derive(Debug, Clone, Default)]
pub struct PrototypeStore {
    /// 按聚合组分类：组 → (name → 记录)。主键语义 (PrototypeGroup, name) 不变。
    pub groups: AIndexMap<PrototypeGroup, AIndexMap<String, PrototypeRecord>>,
    /// 惰性派生：每组的排序信息（大组 → 小组 → 条目，加载后不可变）。
    order_info: OnceLock<AIndexMap<PrototypeGroup, OrderInfo>>,
    /// 惰性派生：每组的反查索引（条目名 → 三层索引）。
    reverse_order_info: OnceLock<AIndexMap<PrototypeGroup, ReverseOrderInfo>>,
    /// 惰性派生：按 order 排序的品质名列表（0 = normal；品质等级 ↔ 名字映射）。
    quality_order: OnceLock<Vec<String>>,
    /// 惰性派生：流体可用温度表（流体名 → 排序去重的温度集合，i32 精度妥协）。
    fluid_temps: OnceLock<AIndexMap<String, Vec<i32>>>,
    /// 惰性派生：科技反向依赖（科技原型只声明 prerequisites，这里预处理"谁依赖我"）。
    technology_dependents: OnceLock<AIndexMap<String, Vec<String>>>,
}

impl PrototypeStore {
    /// 从游戏 dump（data-raw-dump.json 的顶层对象）加载。
    ///
    /// 遍历 `COMPONENT_LIST` 的关注键，每个原型按组件清单反序列化，
    /// 推导聚合标签，按 `(group, name)` 合并（同键重复时组件并集，罕见）。
    /// 任一原型反序列化失败 → 返回 [`LoadError`]（含全部失败明细）。
    pub fn load(dump: &Value) -> Result<Self, LoadError> {
        let mut records: AIndexMap<PrototypeGroup, AIndexMap<String, PrototypeRecord>> =
            AIndexMap::default();
        let mut failures: Vec<(String, String, String)> = Vec::new();
        let mut total = 0usize;

        for (typename, component_list) in COMPONENT_LIST {
            let Some(entries) = dump.get(*typename) else {
                continue;
            };
            let Some(entries_obj) = entries.as_object() else {
                continue;
            };
            for (name, value) in entries_obj {
                total += 1;
                let mut components: AIndexMap<&'static str, ComponentValue> = AIndexMap::default();
                let mut ok = true;
                for comp in *component_list {
                    match deserialize_component(comp, value) {
                        Ok(cv) => {
                            components.insert(comp, cv);
                        }
                        Err(e) => {
                            failures.push((
                                (*typename).to_string(),
                                name.clone(),
                                format!("{comp}: {e}"),
                            ));
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                let group = derive_group(typename, &components);
                let record = PrototypeRecord {
                    name: name.clone(),
                    type_: (*typename).to_string(),
                    group,
                    components,
                };
                match records.entry(group) {
                    indexmap::map::Entry::Vacant(v) => {
                        let mut inner = AIndexMap::default();
                        inner.insert(name.clone(), record);
                        v.insert(inner);
                    }
                    indexmap::map::Entry::Occupied(mut o) => {
                        match o.get_mut().entry(name.clone()) {
                            indexmap::map::Entry::Vacant(v) => {
                                v.insert(record);
                            }
                            indexmap::map::Entry::Occupied(mut e) => {
                                // 同 (group, name) 重复（罕见）：组件并集（后到覆盖）
                                e.get_mut().components.extend(record.components);
                            }
                        }
                    }
                }
            }
        }

        if failures.is_empty() {
            Ok(Self {
                groups: records,
                order_info: OnceLock::new(),
                reverse_order_info: OnceLock::new(),
                quality_order: OnceLock::new(),
                fluid_temps: OnceLock::new(),
                technology_dependents: OnceLock::new(),
            })
        } else {
            Err(LoadError {
                succeeded: total - failures.len(),
                failures,
                total,
            })
        }
    }

    /// 按名字查 Entity 组记录。
    pub fn entity(&self, name: &str) -> Option<&PrototypeRecord> {
        self.groups.get(&PrototypeGroup::Entity)?.get(name)
    }

    /// 按名字查 Item 组记录。
    pub fn item(&self, name: &str) -> Option<&PrototypeRecord> {
        self.groups.get(&PrototypeGroup::Item)?.get(name)
    }

    /// 按 (组变体, name) 查记录（统一入口：`get(&PrototypeGroup::Recipe, "iron-plate")`）。
    pub fn get(&self, group: PrototypeGroup, name: &str) -> Option<&PrototypeRecord> {
        self.groups.get(&group)?.get(name)
    }

    /// 取某组的 name → 记录 map（按组分类的直接入口）。
    pub fn group_map(&self, group: PrototypeGroup) -> Option<&AIndexMap<String, PrototypeRecord>> {
        self.groups.get(&group)
    }

    /// 遍历某组的所有记录。
    pub fn group(&self, group: PrototypeGroup) -> impl Iterator<Item = &PrototypeRecord> {
        self.groups.get(&group).into_iter().flat_map(|m| m.values())
    }

    /// 惰性派生：每组的排序信息（大组 → 小组 → 条目，均按 order 排序）。
    pub fn order_info(&self) -> &AIndexMap<PrototypeGroup, OrderInfo> {
        self.order_info.get_or_init(|| {
            let mut out: AIndexMap<PrototypeGroup, OrderInfo> = AIndexMap::default();
            for (group, _) in &self.groups {
                out.insert(*group, self.build_order_info(*group));
            }
            out
        })
    }

    /// 惰性派生：每组的反查索引（条目名 → (大组, 小组, 条目) 三层索引）。
    pub fn reverse_order_info(&self) -> &AIndexMap<PrototypeGroup, ReverseOrderInfo> {
        self.reverse_order_info.get_or_init(|| {
            let mut out: AIndexMap<PrototypeGroup, ReverseOrderInfo> = AIndexMap::default();
            for (group, order) in self.order_info() {
                let mut rev: ReverseOrderInfo = AIndexMap::default();
                for (gi, (_, subgroups)) in order.iter().enumerate() {
                    for (si, (_, items)) in subgroups.iter().enumerate() {
                        for (ii, name) in items.iter().enumerate() {
                            rev.insert(name.clone(), (gi, si, ii));
                        }
                    }
                }
                out.insert(*group, rev);
            }
            out
        })
    }

    /// 构建某组的排序信息（参考原始 get_order_info 的三层结构）。
    fn build_order_info(&self, group: PrototypeGroup) -> OrderInfo {
        // 小组 → 大组 映射、小组 order、大组 order（原型数据）
        let mut subgroup_group: AIndexMap<String, String> = AIndexMap::default();
        let mut subgroup_order: AIndexMap<String, String> = AIndexMap::default();
        if let Some(sgs) = self.groups.get(&PrototypeGroup::ItemSubgroup) {
            for (name, r) in sgs {
                let group_name = r
                    .component::<ItemSubGroupComponent>()
                    .map(|c| c.group.clone())
                    .unwrap_or_default();
                let order = r
                    .component::<PrototypeBaseComponent>()
                    .map(|b| b.order.clone())
                    .unwrap_or_default();
                subgroup_group.insert(name.clone(), group_name);
                subgroup_order.insert(name.clone(), order);
            }
        }
        let mut group_order: AIndexMap<String, String> = AIndexMap::default();
        if let Some(gs) = self.groups.get(&PrototypeGroup::ItemGroup) {
            for (name, r) in gs {
                let order = r
                    .component::<PrototypeBaseComponent>()
                    .map(|b| b.order.clone())
                    .unwrap_or_default();
                group_order.insert(name.clone(), order);
            }
        }

        // 条目分组：大组 → 小组 → Vec<(order, name)>
        let other = "other".to_string();
        let mut grouped: AIndexMap<String, AIndexMap<String, Vec<(String, String)>>> =
            AIndexMap::default();
        if let Some(records) = self.groups.get(&group) {
            for (name, r) in records {
                let subgroup = r
                    .component::<PrototypeBaseComponent>()
                    .and_then(|b| b.subgroup.clone());
                let (g, sg) = match &subgroup {
                    Some(sg_name) => (
                        subgroup_group
                            .get(sg_name)
                            .cloned()
                            .unwrap_or_else(|| other.clone()),
                        sg_name.clone(),
                    ),
                    None => (other.clone(), String::new()),
                };
                let order = r
                    .component::<PrototypeBaseComponent>()
                    .map(|b| b.order.clone())
                    .unwrap_or_default();
                grouped
                    .entry(g)
                    .or_default()
                    .entry(sg)
                    .or_default()
                    .push((order, name.clone()));
            }
        }

        // 排序输出：大组按 order、小组按 order、条目按 (order, name)
        // （无 order 记录的组排最前，与原始 Option 比较一致）
        let mut out: OrderInfo = AIndexMap::default();
        let mut group_keys: Vec<String> = grouped.keys().cloned().collect();
        group_keys.sort_by(|a, b| group_order.get(a).cmp(&group_order.get(b)));
        for gk in group_keys {
            let subgroups = &grouped[&gk];
            let mut sg_keys: Vec<String> = subgroups.keys().cloned().collect();
            sg_keys.sort_by(|a, b| subgroup_order.get(a).cmp(&subgroup_order.get(b)));
            let mut sg_map: AIndexMap<String, Vec<String>> = AIndexMap::default();
            for sk in sg_keys {
                let mut items = subgroups[&sk].clone();
                items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
                sg_map.insert(sk, items.into_iter().map(|(_, n)| n).collect());
            }
            out.insert(gk, sg_map);
        }
        out
    }

    /// 按 order 排序的品质名列表（0 = normal）。
    ///
    /// 品质等级 ↔ 名字的稳定映射（品质原型加载后不可变），供
    /// 品质分布计算与品质倍率查询复用——避免每次调用重复排序。
    pub fn quality_order(&self) -> &[String] {
        self.quality_order.get_or_init(|| {
            let qualities = self.groups.get(&PrototypeGroup::Quality);
            let Some(qualities) = qualities else {
                return Vec::new();
            };
            // name → next 链映射（品质等级顺序由 next 链定义，非 order）
            let next_of: AIndexMap<String, String> = qualities
                .iter()
                .filter_map(|(name, record)| {
                    record
                        .component::<QualityComponent>()
                        .and_then(|q| q.next.clone())
                        .map(|next| (name.clone(), next))
                })
                .collect();
            // 链头：内置 "normal"；否则第一个不是任何 next 目标的品质
            let head = qualities
                .keys()
                .find(|n| *n == "normal")
                .cloned()
                .or_else(|| {
                    qualities
                        .keys()
                        .find(|n| !next_of.values().any(|v| v == *n))
                        .cloned()
                });
            let Some(mut current) = head else {
                return Vec::new();
            };
            let mut order = Vec::new();
            let mut visited: std::collections::HashSet<String> = Default::default();
            // 沿 next 链遍历（visited 防 mod 数据成环）
            while visited.insert(current.clone()) {
                order.push(current.clone());
                match next_of.get(&current) {
                    Some(next) => current = next.clone(),
                    None => break,
                }
            }
            // 链外品质（无 next 关系的独立品质不用管）
            // let mut extra: Vec<(String, String)> = qualities
            //     .keys()
            //     .filter(|n| !visited.contains(*n))
            //     .map(|name| {
            //         let order = qualities
            //             .get(name)
            //             .and_then(|r| r.component::<PrototypeBaseComponent>())
            //             .map(|b| b.order.clone())
            //             .unwrap_or_default();
            //         (order, name.clone())
            //     })
            //     .collect();
            // extra.sort_by(|a, b| a.0.cmp(&b.0));
            // order.extend(extra.into_iter().map(|(_, n)| n));
            order
        })
    }

    /// 流体的可用温度表：流体名 → 排序去重的温度集合。
    ///
    /// 收集点（报告制）：① 配方产物流体的温度（temperature/min/max）；
    /// ② 机器流体盒的温度筛选（Boiler/Generator 的 fluid_box、CraftingMachine 的
    /// fluid_boxes 的 minimum/maximum + Boiler.target_temperature 输出温度）。
    /// 配方/机器按此表生成单态温度决策（一分为 N）。
    pub fn fluid_temperatures(&self) -> &AIndexMap<String, Vec<i32>> {
        self.fluid_temps.get_or_init(|| {
            let mut out: AIndexMap<String, Vec<i32>> = AIndexMap::default();
            let add = |out: &mut AIndexMap<String, Vec<i32>>, name: &str, temp: f64| {
                let t = temp as i32;
                if let Some(list) = out.get_mut(name) {
                    if !list.contains(&t) {
                        list.push(t);
                    }
                } else {
                    out.insert(name.to_string(), vec![t]);
                }
            };
            let add_box =
                |out: &mut AIndexMap<String, Vec<i32>>,
                 box_: &crate::generated_components::FluidBox| {
                    let Some(filter) = &box_.filter else {
                        return;
                    };
                    if let Some(t) = box_.minimum_temperature {
                        add(out, filter, t);
                    }
                    if let Some(t) = box_.maximum_temperature {
                        add(out, filter, t);
                    }
                };
            // 收集点 1：配方产物流体温度
            for record in self
                .groups
                .get(&PrototypeGroup::Recipe)
                .into_iter()
                .flat_map(|m| m.values())
            {
                if let Some(recipe) = record.component::<RecipeComponent>() {
                    for result in &recipe.results {
                        if let crate::types::Product::Fluid(f) = result
                            && let Some(t) = f.temperature {
                                add(&mut out, &f.name, t);
                            }
                    }
                }
            }
            // 收集点 2：机器流体盒温度筛选
            for record in self
                .groups
                .get(&PrototypeGroup::Entity)
                .into_iter()
                .flat_map(|m| m.values())
            {
                if let Some(b) = record.component::<BoilerComponent>() {
                    add_box(&mut out, &b.fluid_box);
                    add_box(&mut out, &b.output_fluid_box);
                    if let Some(t) = b.target_temperature
                        && let Some(filter) = &b.output_fluid_box.filter
                    {
                        add(&mut out, filter, t);
                    }
                }
                if let Some(g) = record.component::<GeneratorComponent>() {
                    add_box(&mut out, &g.fluid_box);
                }
                if let Some(c) = record.component::<CraftingMachineComponent>() {
                    for box_ in &c.fluid_boxes {
                        add_box(&mut out, box_);
                    }
                }
            }
            for list in out.values_mut() {
                list.sort_unstable();
                list.dedup();
            }
            out
        })
    }
    /// 惰性派生：科技反向依赖（科技只声明 `prerequisites`，这里预处理"谁依赖我"）。
    pub fn technology_dependents(&self) -> &AIndexMap<String, Vec<String>> {
        self.technology_dependents.get_or_init(|| {
            let mut out: AIndexMap<String, Vec<String>> = AIndexMap::default();
            if let Some(techs) = self.groups.get(&PrototypeGroup::Technology) {
                for (tech_name, record) in techs {
                    if let Some(tech) = record.component::<TechnologyComponent>() {
                        for prereq in &tech.prerequisites {
                            out.entry(prereq.clone())
                                .or_default()
                                .push(tech_name.clone());
                        }
                    }
                }
            }
            out
        })
    }

    /// 记录总数。
    pub fn len(&self) -> usize {
        self.groups.values().map(|m| m.len()).sum()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

/// 聚合标签推导：组件集合 → 组。
///
/// 含 EntityComponent → Entity；含 ItemComponent → Item；
/// 否则 → Other(原始 type_)。与游戏 `LuaPrototypes` 的聚合一致。
pub fn derive_group(
    type_: &str,
    components: &AIndexMap<&'static str, ComponentValue>,
) -> PrototypeGroup {
    if components.contains_key("EntityComponent") {
        PrototypeGroup::Entity
    } else if components.contains_key("ItemComponent") {
        PrototypeGroup::Item
    } else {
        prototype_group_from_type(type_)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp(name: &'static str) -> AIndexMap<&'static str, ComponentValue> {
        let mut m = AIndexMap::default();
        m.insert(
            name,
            deserialize_component(name, &serde_json::json!({})).unwrap(),
        );
        m
    }

    #[test]
    fn derive_group_by_components() {
        // EntityComponent → Entity（组装机等实体子类型）
        assert_eq!(
            derive_group("assembling-machine", &comp("EntityComponent")),
            PrototypeGroup::Entity
        );
        // ItemComponent → Item
        assert_eq!(
            derive_group("item", &comp("ItemComponent")),
            PrototypeGroup::Item
        );
        // 无 Entity/Item 组件 → 强类型变体（关注类型）
        assert_eq!(
            derive_group("recipe", &comp("RecipeComponent")),
            PrototypeGroup::Recipe
        );

        // 同时含 Entity 与 Item（物品实体）→ Entity 优先（LuaPrototypes 语义）
        let mut m = comp("EntityComponent");
        m.insert(
            "ItemComponent",
            deserialize_component("ItemComponent", &serde_json::json!({})).unwrap(),
        );
        assert_eq!(derive_group("item", &m), PrototypeGroup::Entity);
    }
}
