//! prototype-api.json 的加载与结构定义。
//!
//! 格式依据官方文档：<https://lua-api.factorio.com/latest/auxiliary/json-docs-prototype.html>
//! 顶层：application / application_version / api_version / stage / prototypes / types / defines
//! 约定：null 成员省略；列表按 name 排序；order 表示网站显示顺序。

use serde::{Deserialize, Serialize};

/// 整个 prototype-api.json 文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub application: String,
    pub application_version: String,
    pub api_version: u32,
    pub stage: String,
    pub prototypes: Vec<Prototype>,
    pub types: Vec<Type>,
    pub defines: Vec<Define>,
}

/// 所有成员的公共字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicMember {
    pub name: String,
    pub order: u32,
    #[serde(default)]
    pub description: String,
}

/// 原型（可创建的顶层类型，如 "assembling-machine"）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prototype {
    #[serde(flatten)]
    pub base: BasicMember,
    /// 需要的游戏扩展（如 "space_age"）。缺省表示无限制。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(rename = "abstract")]
    pub abstract_: Option<bool>,
    /// 该原型的 type 名（如 "boiler"）。抽象原型为 null。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typename: Option<String>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub properties: Vec<Property>,
}

/// 类型/概念（复合类型的定义）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Type {
    #[serde(flatten)]
    pub base: BasicMember,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(rename = "abstract")]
    pub abstract_: Option<bool>,
    /// 是否内联在另一个属性的描述中。
    #[serde(default)]
    pub inline: bool,
    /// 该类型本身的类型。为 "builtin" 表示基础类型（string/number/boolean）。
    #[serde(rename = "type")]
    pub type_: TypeRef,
    /// 若该类型包含 struct，则属性列表在此；否则为 null。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<Property>>,
}

/// 原型或类型的属性。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    #[serde(flatten)]
    pub base: BasicMember,
    /// 属性的替代名，二者皆可用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_name: Option<String>,
    /// 是否覆盖父类同名属性。
    #[serde(default)]
    #[serde(rename = "override")]
    pub override_: bool,
    #[serde(rename = "type")]
    pub type_: TypeRef,
    pub optional: bool,
    /// 缺省值：文本描述或字面值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
}

/// 类型引用：简单字符串（类型名）或复杂类型表。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TypeRef {
    /// 简单类型名，如 "double"、"Energy"、"ItemID"、"builtin"。
    Simple(String),
    Complex(ComplexType),
}

/// 复杂类型的 value 字段：可能是类型引用（array/dictionary 等），
/// 也可能是字面值（literal，如 `{"complex_type": "literal", "value": 1}`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComplexValue {
    /// 类型引用（字符串类型名或嵌套复杂类型）。
    TypeRef(Box<TypeRef>),
    /// 任意字面值（literal 的 value，如数字、字符串、数组、对象）。
    Literal(serde_json::Value),
}

/// 复杂类型表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexType {
    pub complex_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ComplexValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<Box<TypeRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<TypeRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<TypeRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_format: Option<bool>,
}

/// 属性的缺省值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DefaultValue {
    /// 文本描述（如 "math.huge"、"`{0, 0}`"）。
    Text(String),
    /// 字面值。
    Literal(LiteralValue),
}

/// 字面值。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiteralValue {
    pub complex_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

/// Define（枚举常量）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Define {
    #[serde(flatten)]
    pub base: BasicMember,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<DefineValue>>,
    /// Define 可以递归（子 Define）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subkeys: Option<Vec<Define>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefineValue {
    pub name: String,
    pub order: u32,
    #[serde(default)]
    pub description: String,
}

impl Schema {
    /// 从 JSON 文本加载 schema。
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// 按名字查原型。
    pub fn prototype(&self, name: &str) -> Option<&Prototype> {
        self.prototypes.iter().find(|p| p.base.name == name)
    }

    /// 按名字查类型。
    pub fn type_def(&self, name: &str) -> Option<&Type> {
        self.types.iter().find(|t| t.base.name == name)
    }

    /// 该名字是否是一个原型类型（prototypes 列表成员，含抽象层）。
    /// 用于区分"原型继承链层"（生成组件，带 Component 后缀）与
    /// "types 里的普通 struct"（生成原名，不带后缀）。
    pub fn is_prototype_type(&self, name: &str) -> bool {
        self.prototype(name).is_some()
    }

    /// 某原型的继承链（自身在前，根在最后），按名字返回。
    pub fn prototype_chain<'a>(&'a self, prototype: &'a Prototype) -> Vec<&'a Prototype> {
        let mut chain = vec![prototype];
        let mut cur = prototype;
        while let Some(parent_name) = &cur.parent {
            let Some(parent) = self.prototype(parent_name) else {
                break;
            };
            chain.push(parent);
            cur = parent;
        }
        chain
    }

    /// 按 typename（dump 顶层键，如 "assembling-machine"）查具体原型。
    pub fn prototype_by_typename(&self, typename: &str) -> Option<&Prototype> {
        self.prototypes
            .iter()
            .find(|p| p.typename.as_deref() == Some(typename))
    }
}

// 类型名索引（加载后构建，避免线性查找）。
// #[derive(Debug, Default)]
// pub struct TypeIndex {
//     prototypes: HashMap<String, usize>,
//     types: HashMap<String, usize>,
// }

// impl TypeIndex {
//     pub fn build(schema: &Schema) -> Self {
//         Self {
//             prototypes: schema
//                 .prototypes
//                 .iter()
//                 .enumerate()
//                 .map(|(i, p)| (p.base.name.clone(), i))
//                 .collect(),
//             types: schema
//                 .types
//                 .iter()
//                 .enumerate()
//                 .map(|(i, t)| (t.base.name.clone(), i))
//                 .collect(),
//         }
//     }
// }
