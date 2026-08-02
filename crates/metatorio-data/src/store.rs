//! Phase 3：原型仓库（PrototypeStore）。
//!
//! 把游戏 dump 反序列化为**按 (type, name) 唯一**的组件化原型记录：
//! - 每个原型按 `COMPONENT_LIST`（继承链组件 + 组合组件）反序列化，
//!   结果存入 `components: AIndexMap<String, ComponentValue>`
//! - **聚合标签**（参考游戏 `LuaPrototypes`）：含 `EntityComponent` → `Entity` 组、
//!   含 `ItemComponent` → `Item` 组，其余按原始 type_ 记录为 `Other(type_)`
//! - 主键 = `(PrototypeGroup, name)`：同名跨组是不同原型
//!   （如 "assembling-machine-1" 的 item 与 entity 是两条记录；"speed-module" 的
//!   recipe/module/technology 是三条记录）

use crate::generated_components::{
    COMPONENT_LIST, Component, ComponentValue, deserialize_component,
};
use crate::generated_components::prototype_groups::prototype_group_from_type;
use serde_json::Value;
use std::fmt;

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
    pub fn component_required<T: Component>(&self) -> &T {
        match self.components.get(T::TYPENAME) {
            Some(cv) => T::as_ref(cv),
            None => panic!("原型 {} ({}) 缺少组件 {}", self.name, self.type_, T::TYPENAME),
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

/// 原型仓库：按 (PrototypeGroup, name) 索引的全部原型记录。
#[derive(Debug, Clone, Default)]
pub struct PrototypeStore {
    pub records: AIndexMap<(PrototypeGroup, String), PrototypeRecord>,
}

impl PrototypeStore {
    /// 从游戏 dump（data-raw-dump.json 的顶层对象）加载。
    ///
    /// 遍历 `COMPONENT_LIST` 的关注键，每个原型按组件清单反序列化，
    /// 推导聚合标签，按 `(group, name)` 合并（同键重复时组件并集，罕见）。
    /// 任一原型反序列化失败 → 返回 [`LoadError`]（含全部失败明细）。
    pub fn load(dump: &Value) -> Result<Self, LoadError> {
        let mut records: AIndexMap<(PrototypeGroup, String), PrototypeRecord> =
            AIndexMap::with_hasher(ahash::RandomState::default());
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
                let mut components: AIndexMap<&'static str, ComponentValue> =
                    AIndexMap::with_hasher(ahash::RandomState::default());
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
                    group: group.clone(),
                    components,
                };
                match records.entry((group, name.clone())) {
                    indexmap::map::Entry::Vacant(v) => {
                        v.insert(record);
                    }
                    indexmap::map::Entry::Occupied(mut o) => {
                        // 同 (group, name) 重复（罕见）：组件并集（后到覆盖）
                        o.get_mut().components.extend(record.components);
                    }
                }
            }
        }

        if failures.is_empty() {
            Ok(Self { records })
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
        self.records
            .get(&(PrototypeGroup::Entity, name.to_string()))
    }

    /// 按名字查 Item 组记录。
    pub fn item(&self, name: &str) -> Option<&PrototypeRecord> {
        self.records.get(&(PrototypeGroup::Item, name.to_string()))
    }

    /// 按 (type_, name) 查非聚合组记录（关注类型 → 强类型变体，未知 → Unknown）。
    pub fn other(&self, type_: &str, name: &str) -> Option<&PrototypeRecord> {
        let group = prototype_group_from_type(type_);
        self.records.get(&(group, name.to_string()))
    }

    /// 遍历某组的所有记录。
    pub fn group<'a>(
        &'a self,
        group: &'a PrototypeGroup,
    ) -> impl Iterator<Item = &'a PrototypeRecord> {
        self.records
            .iter()
            .filter(move |((g, _), _)| g == group)
            .map(|(_, r)| r)
    }

    /// 记录总数。
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
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
        let mut m = AIndexMap::with_hasher(ahash::RandomState::default());
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
        // 关注清单外 → Unknown 兜底
        assert_eq!(
            derive_group("custom-mod-type", &comp("RecipeComponent")),
            PrototypeGroup::Unknown("custom-mod-type".to_string())
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
