//! schema 类型 → Rust 类型的映射器。
//!
//! Phase 1 支持：标量、ID 别名、array、struct、builtin、自定义映射。
//! Phase 2 待办：union/tuple/dictionary 细化为具体 Rust 类型（当前映射为
//! `serde_json::Value` 保真，反序列化零成本，后续可逐步替换）。

use crate::config::Config;
use crate::schema::{ComplexType, Schema, TypeRef};
use std::collections::{HashSet, VecDeque};

/// 映射结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mapped {
    /// 具体的 Rust 类型。
    Rust(String),
    /// 整数类型（需要宽松反序列化：float 向 0 舍入）。
    /// 携带 Rust 类型名（如 "u16"）。
    LenientInt(String),
    /// 数组（元素映射）：需要宽松 Vec 反序列化（Lua 空 table → 空 Vec）。
    /// 元素可能是 LenientInt（整数数组 → 宏函数）、Rust（内联函数）、
    /// Array（嵌套数组）或 Skipped（整体跳过）。
    Array(Box<Mapped>),
    /// 字段应跳过（类型被忽略 / literal 常量 / 无法映射）。
    Skipped,
}

/// 标量类型名 → Rust 类型。
fn scalar(name: &str) -> Option<&'static str> {
    match name {
        "string" => Some("String"),
        "boolean" => Some("bool"),
        "double" | "float" | "number" => Some("f64"),
        _ => None,
    }
}

/// 整数标量类型名 → Rust 类型（需要宽松反序列化）。
fn int_scalar(name: &str) -> Option<&'static str> {
    match name {
        "uint8" => Some("u8"),
        "uint16" => Some("u16"),
        "uint32" => Some("u32"),
        "uint64" => Some("u64"),
        "int8" => Some("i8"),
        "int16" => Some("i16"),
        "int32" => Some("i32"),
        "int64" => Some("i64"),
        _ => None,
    }
}

/// 把简单类型名映射为 Rust 类型。
fn map_simple(schema: &Schema, config: &Config, name: &str) -> Mapped {
    // 1. 标量（整数需要宽松反序列化：Lua 的 float 可能放进整数字段）
    if let Some(ty) = scalar(name) {
        return Mapped::Rust(ty.to_string());
    }
    if let Some(ty) = int_scalar(name) {
        return Mapped::LenientInt(ty.to_string());
    }
    // 2. 自定义映射（如 Energy → EnergyAmount）
    if let Some(ty) = config.custom_type(name) {
        return Mapped::Rust(ty.to_string());
    }
    // 3. 忽略集
    if config.is_ignored_type(name) {
        return Mapped::Skipped;
    }
    // 4. types 中的类型定义
    if let Some(t) = schema.type_def(name) {
        match &t.type_ {
            TypeRef::Simple(simple) => {
                if simple == "builtin" {
                    // builtin 类型（如 "string"/"number"）——用类型名本身映射标量
                    if let Some(ty) = scalar(&t.base.name) {
                        return Mapped::Rust(ty.to_string());
                    }
                    if let Some(ty) = int_scalar(&t.base.name) {
                        return Mapped::LenientInt(ty.to_string());
                    }
                    // 未知 builtin：保真
                    return Mapped::Rust("serde_json::Value".to_string());
                }
                // 类型是另一类型的别名（如 ActiveTriggerID → string）。
                // 注意：别名链可能成环（A→B→A），递归会栈溢出；
                // 这里只展开一层，环的尽头会落到"未知类型→Value"兜底。
                if simple != name {
                    return map_simple(schema, config, simple);
                }
                return Mapped::Rust("serde_json::Value".to_string());
            }
            TypeRef::Complex(c) => {
                // struct 类型：类型名作为 Rust struct 名（生成器负责生成该类型）
                if c.complex_type == "struct" {
                    return Mapped::Rust(component_name(schema, &t.base.name));
                }
                // array 别名（如 ItemPrototypeFlags）：展开为 Array 变体
                if c.complex_type == "array" {
                    return Mapped::Array(Box::new(array_elem(schema, config, c)));
                }
                // 其他复杂类型（union/tuple/dictionary...）：递归处理
                return map_complex(schema, config, c);
            }
        }
    }
    // 5. 未知类型：保真
    Mapped::Rust("serde_json::Value".to_string())
}

/// Mapped → 类型字符串（Array 递归展开为 Vec<...>）。
pub fn ty_str_of(m: &Mapped) -> String {
    match m {
        Mapped::Rust(t) => t.clone(),
        Mapped::LenientInt(t) => t.clone(),
        Mapped::Array(inner) => format!("Vec<{}>", ty_str_of(inner)),
        Mapped::Skipped => "serde_json::Value".to_string(),
    }
}

/// 数组元素映射：从 array 的 value 取元素类型并映射；
/// 缺失/非类型引用 → Value 保真。
fn array_elem(schema: &Schema, config: &Config, c: &ComplexType) -> Mapped {
    match &c.value {
        Some(crate::schema::ComplexValue::TypeRef(v)) => map(schema, config, v),
        _ => Mapped::Rust("serde_json::Value".to_string()),
    }
}

/// 映射复杂类型。
fn map_complex(schema: &Schema, config: &Config, c: &ComplexType) -> Mapped {
    match c.complex_type.as_str() {
        "array" => Mapped::Array(Box::new(array_elem(schema, config, c))),
        "struct" => Mapped::Rust("serde_json::Value".to_string()), // 内联 struct：保真
        // dictionary：{ key: string/ID → String, value: 递归映射 }
        "dictionary" => {
            let Some(crate::schema::ComplexValue::TypeRef(v)) = &c.value else {
                return Mapped::Rust("serde_json::Value".to_string());
            };
            match map(schema, config, v) {
                Mapped::Skipped => Mapped::Skipped,
                m => Mapped::Rust(format!("BTreeMap<String, {}>", ty_str_of(&m))),
            }
        }
        // tuple：values → (A, B, C)
        "tuple" => {
            let mut parts = Vec::new();
            if let Some(values) = &c.values {
                for v in values {
                    match map(schema, config, v) {
                        Mapped::Skipped => return Mapped::Skipped,
                        m => parts.push(ty_str_of(&m)),
                    }
                }
            }
            if parts.is_empty() {
                Mapped::Rust("serde_json::Value".to_string())
            } else {
                Mapped::Rust(format!("({})", parts.join(", ")))
            }
        }
        // Phase 2 剩余：union 保持 Value 保真（需要语义的类型走 custom_type_map 手写注册）
        "union" | "type" | "literal" => Mapped::Rust("serde_json::Value".to_string()),
        other => {
            eprintln!("metatorio-data-codegen: 未知 complex_type: {other}");
            Mapped::Rust("serde_json::Value".to_string())
        }
    }
}

/// 映射任意类型引用。
pub fn map(schema: &Schema, config: &Config, t: &TypeRef) -> Mapped {
    match t {
        TypeRef::Simple(name) => map_simple(schema, config, name),
        TypeRef::Complex(c) => map_complex(schema, config, c),
    }
}

/// 收集映射中需要生成 struct 定义的类型名（可达类型集）。
///
/// 使用显式工作栈（而非递归），因为类型引用图存在**环**
/// （如 struct A 的属性引用 struct B、B 的属性又引用 A；
/// 递归实现会栈溢出——这是 2026-08-01 修复过的实际 bug）。
pub fn collect_struct_types<'a>(
    schema: &'a Schema,
    config: &Config,
    roots: impl IntoIterator<Item = &'a str>,
) -> HashSet<String> {
    let mut collected = HashSet::new();
    // 显式工作栈：(待处理的 TypeRef, 该 TypeRef 来自哪个类型名/上下文)
    // 使用 VecDeque 按 BFS 顺序处理，避免长链时递归深度问题。
    let mut queue: VecDeque<TypeRef> = roots
        .into_iter()
        .map(|s| TypeRef::Simple(s.to_string()))
        .collect();

    // queued：已入队的类型名（防环导致重复入队）
    let mut queued: HashSet<String> = HashSet::new();
    // processed：已处理的类型名（防环导致重复处理）
    let mut processed: HashSet<String> = HashSet::new();

    while let Some(t) = queue.pop_front() {
        match t {
            TypeRef::Simple(name) => {
                if processed.contains(&name) {
                    continue;
                }
                processed.insert(name.clone());
                let Some(td) = schema.type_def(&name) else {
                    continue;
                };
                let is_struct =
                    matches!(&td.type_, TypeRef::Complex(c) if c.complex_type == "struct");
                if !is_struct {
                    continue;
                }
                collected.insert(name.clone());
                if let Some(props) = &td.properties {
                    for prop in props {
                        enqueue_type(schema, config, &prop.type_, &mut queue, &mut queued);
                    }
                }
            }
            TypeRef::Complex(c) => {
                enqueue_complex(schema, config, &c, &mut queue, &mut queued);
            }
        }
    }
    collected
}

fn enqueue_type(
    schema: &Schema,
    config: &Config,
    t: &TypeRef,
    queue: &mut VecDeque<TypeRef>,
    queued: &mut HashSet<String>,
) {
    match t {
        TypeRef::Simple(name) => {
            if config.is_ignored_type(name) || config.custom_type(name).is_some() {
                return;
            }
            if queued.contains(name) {
                return;
            }
            queued.insert(name.clone());
            queue.push_back(TypeRef::Simple(name.clone()));
        }
        TypeRef::Complex(c) => enqueue_complex(schema, config, c, queue, queued),
    }
}

fn enqueue_complex(
    schema: &Schema,
    config: &Config,
    c: &ComplexType,
    queue: &mut VecDeque<TypeRef>,
    queued: &mut HashSet<String>,
) {
    if let Some(crate::schema::ComplexValue::TypeRef(v)) = &c.value {
        enqueue_type(schema, config, v, queue, queued);
    }
    if let Some(k) = &c.key {
        enqueue_type(schema, config, k, queue, queued);
    }
    if let Some(vs) = &c.values {
        for v in vs {
            enqueue_type(schema, config, v, queue, queued);
        }
    }
    if let Some(os) = &c.options {
        for o in os {
            enqueue_type(schema, config, o, queue, queued);
        }
    }
}

/// 类型名 → Rust 组件/结构体名。
///
/// **原型继承链层**（prototypes 列表中的类型，含抽象层）→ 去 Prototype 后缀 + Component
/// （如 CraftingMachinePrototype → CraftingMachineComponent，是真正的组件，进 COMPONENT_LIST）；
/// **types 里的普通 struct**（如 Effect、Resistance——只是字段的组成部分）→ 原名，不带后缀。
pub fn component_name(schema: &Schema, schema_name: &str) -> String {
    if schema.is_prototype_type(schema_name) {
        let stem = schema_name.strip_suffix("Prototype").unwrap_or(schema_name);
        format!("{stem}Component")
    } else {
        schema_name.to_string()
    }
}
