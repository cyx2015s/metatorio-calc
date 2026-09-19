//! 宽松反序列化：容忍 Lua→JSON 导出的不严格类型。
//!
//! 背景（游戏数据的事实）：
//! - Lua 的 `15/4 = 3.75` 会被 mod 直接放进期望整数的字段（类型转换不规范），
//!   导出 JSON 为浮点；游戏引擎的实际行为是**向 0 舍入**（truncation）。
//!   serde 默认拒绝 float→int，必须在这里兜底。
//! - Lua 的空 table `{}` 既是空 map 也是空 list，导出为 `{}`（空 object），
//!   期望 `Vec<T>` 的字段会收到空 map —— 视为空 Vec。
//!
//! # 用法（serde `deserialize_with` 按字面量替换，可填泛型函数路径）
//!
//! 生成器在字段上输出（无需任何辅助函数）：
//! ```ignore
//! #[serde(deserialize_with = "crate::lenient::de_int::<u16, _>")]
//! pub count: u16,
//! #[serde(deserialize_with = "crate::lenient::de_vec_lenient::<String, _>")]
//! pub flags: Vec<String>,
//! ```

use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{Deserializer, Error, IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::fmt;
use std::marker::PhantomData;

// ── 整数（float → 向 0 舍入）──────────────────────────────────────

/// 宽松整数类型：从整数/浮点转换（浮点向 0 舍入），与游戏引擎一致。
pub trait LenientInt: Sized {
    fn from_i64(v: i64) -> Self;
    fn from_u64(v: u64) -> Self;
    fn from_f64(v: f64) -> Self;
}

macro_rules! impl_lenient_int {
    ($($t:ty),*) => {
        $(impl LenientInt for $t {
            fn from_i64(v: i64) -> Self { v as $t }
            fn from_u64(v: u64) -> Self { v as $t }
            fn from_f64(v: f64) -> Self { v.trunc() as $t }
        })*
    };
}
impl_lenient_int!(u8, u16, u32, u64, i8, i16, i32, i64);

/// 宽松整数：接受整数与浮点（浮点向 0 舍入）。
///
/// `#[serde(deserialize_with = "crate::lenient::de_int::<u16, _>")]`
pub fn de_int<'de, T: LenientInt, D: Deserializer<'de>>(d: D) -> Result<T, D::Error> {
    struct V<T>(PhantomData<T>);
    impl<'de, T: LenientInt> Visitor<'de> for V<T> {
        type Value = T;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("一个整数（浮点会被截断到零）")
        }
        fn visit_i64<E: Error>(self, v: i64) -> Result<T, E> {
            Ok(T::from_i64(v))
        }
        fn visit_u64<E: Error>(self, v: u64) -> Result<T, E> {
            Ok(T::from_u64(v))
        }
        fn visit_f64<E: Error>(self, v: f64) -> Result<T, E> {
            Ok(T::from_f64(v))
        }
    }
    d.deserialize_any(V(PhantomData))
}

/// 可选的宽松整数。
///
/// `#[serde(deserialize_with = "crate::lenient::de_opt_int::<u16, _>")]`
pub fn de_opt_int<'de, T: LenientInt, D: Deserializer<'de>>(d: D) -> Result<Option<T>, D::Error> {
    struct V<T>(PhantomData<T>);
    impl<'de, T: LenientInt> Visitor<'de> for V<T> {
        type Value = Option<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("一个可选整数（浮点会被截断到零）")
        }
        fn visit_unit<E: Error>(self) -> Result<Option<T>, E> {
            Ok(None)
        }
        fn visit_none<E: Error>(self) -> Result<Option<T>, E> {
            Ok(None)
        }
        fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Option<T>, D2::Error> {
            de_int(d).map(Some)
        }
    }
    d.deserialize_option(V(PhantomData))
}

// ── Vec（空 map → 空 Vec）────────────────────────────────────────

/// 宽松 Vec：接受序列，也接受空 map（Lua 空 table 导出 `{}`）。
/// 元素用标准反序列化（适用于 String/struct/Value 等元素）。
///
/// `#[serde(deserialize_with = "crate::lenient::de_vec_lenient::<String, _>")]`
pub fn de_vec_lenient<'de, T: serde::Deserialize<'de>, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    struct V<T>(PhantomData<T>);
    impl<'de, T: serde::Deserialize<'de>> Visitor<'de> for V<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("一个数组，或空 map（Lua 空表）")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut out = Vec::new();
            while let Some(v) = seq.next_element::<T>()? {
                out.push(v);
            }
            Ok(out)
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<T>, A::Error> {
            if map.next_key::<IgnoredAny>()?.is_none() {
                Ok(Vec::new())
            } else {
                Err(A::Error::custom(
                    "期望空 map（Lua 空表）或数组，实际拿到非空 map",
                ))
            }
        }
    }
    d.deserialize_any(V(PhantomData))
}

/// 可选的宽松 Vec。
///
/// `#[serde(deserialize_with = "crate::lenient::de_opt_vec_lenient::<String, _>")]`
pub fn de_opt_vec_lenient<'de, T: serde::Deserialize<'de>, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<Vec<T>>, D::Error> {
    struct V<T>(PhantomData<T>);
    impl<'de, T: serde::Deserialize<'de>> Visitor<'de> for V<T> {
        type Value = Option<Vec<T>>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("一个可选数组，或空 map（Lua 空表）")
        }
        fn visit_unit<E: Error>(self) -> Result<Option<Vec<T>>, E> {
            Ok(None)
        }
        fn visit_none<E: Error>(self) -> Result<Option<Vec<T>>, E> {
            Ok(None)
        }
        fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Option<Vec<T>>, D2::Error> {
            de_vec_lenient(d).map(Some)
        }
    }
    d.deserialize_option(V(PhantomData))
}

/// 宽松整数 Vec：空 map → 空 Vec；元素用宽松整数（float → 向 0 舍入）。
///
/// `#[serde(deserialize_with = "crate::lenient::de_vec_int::<u16, _>")]`
pub fn de_vec_int<'de, T: LenientInt, D: Deserializer<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    struct V<T>(PhantomData<T>);
    impl<'de, T: LenientInt> Visitor<'de> for V<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("一个整数数组，或空 map（Lua 空表）")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            struct Elem<T>(PhantomData<T>);
            impl<'de, T: LenientInt> serde::de::DeserializeSeed<'de> for Elem<T> {
                type Value = T;
                fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
                    de_int(d)
                }
            }
            let mut out = Vec::new();
            while let Some(v) = seq.next_element_seed(Elem(PhantomData))? {
                out.push(v);
            }
            Ok(out)
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<T>, A::Error> {
            if map.next_key::<IgnoredAny>()?.is_none() {
                Ok(Vec::new())
            } else {
                Err(A::Error::custom(
                    "期望空 map（Lua 空表）或数组，实际拿到非空 map",
                ))
            }
        }
    }
    d.deserialize_any(V(PhantomData))
}

/// 可选的宽松整数 Vec。
///
/// `#[serde(deserialize_with = "crate::lenient::de_opt_vec_int::<u16, _>")]`
pub fn de_opt_vec_int<'de, T: LenientInt, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<Vec<T>>, D::Error> {
    struct V<T>(PhantomData<T>);
    impl<'de, T: LenientInt> Visitor<'de> for V<T> {
        type Value = Option<Vec<T>>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("一个可选的整数数组，或空 map")
        }
        fn visit_unit<E: Error>(self) -> Result<Option<Vec<T>>, E> {
            Ok(None)
        }
        fn visit_none<E: Error>(self) -> Result<Option<Vec<T>>, E> {
            Ok(None)
        }
        fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Option<Vec<T>>, D2::Error> {
            de_vec_int(d).map(Some)
        }
    }
    d.deserialize_option(V(PhantomData))
}

// ── 可选表（struct）─────────────────────────────────────────────

/// 宽松「可选表」：`null` / `false` / `0` 一律当作**未设置**（`None`）。
///
/// 背景（游戏数据的事实）：Lua 里把可选表字段「关掉」的常见写法是直接赋 `0` 或 `false`。
/// 实测样本（casting_ladle 0.2.1 的 `data-updates.lua` 第 6 行，注释写的是「关闭熔炉自带产能」）：
/// ```lua
/// data.raw["assembling-machine"]["foundry"].effect_receiver = 0
/// ```
/// 游戏本体照常加载（2.1.17 实测：该 mod 正常载入、日志无告警），并把这种**非表值**
/// 当作「该字段没设置」——熔炉于是拿到 `effect_receiver` 的默认值（没有自带产能），
/// 与该 mod 注释的意图一致。
///
/// serde 默认会在 `Option<EffectReceiver>` 上尝试把 `0` 解析成结构体，报
/// `invalid type: integer 0, expected struct EffectReceiver`；而我们的加载器
/// **任一字段失败即整体失败**，于是一个字段的写法差异拖垮全部 5024 个原型。
/// 这里按游戏语义归一，只针对可选表字段。
///
/// 放宽的只有「零值」三兄弟（`null` / `false` / `0`）：`true`、非零数字、字符串、
/// 数组仍然走严格解析并报出准确错误——不把真正的数据问题吞掉。
///
/// `#[serde(deserialize_with = "crate::lenient::de_opt_struct_lenient")]`
pub fn de_opt_struct_lenient<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    struct V<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
        type Value = Option<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("一个可选的表（null / false / 0 视为未设置）")
        }
        fn visit_unit<E: Error>(self) -> Result<Option<T>, E> {
            Ok(None)
        }
        fn visit_none<E: Error>(self) -> Result<Option<T>, E> {
            Ok(None)
        }
        fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Option<T>, D2::Error> {
            d.deserialize_any(V(PhantomData))
        }
        fn visit_bool<E: Error>(self, v: bool) -> Result<Option<T>, E> {
            if v {
                Err(E::custom(
                    "期望一个表，实际是 true；只有 false / 0 / null 表示「未设置」",
                ))
            } else {
                Ok(None)
            }
        }
        fn visit_i64<E: Error>(self, v: i64) -> Result<Option<T>, E> {
            if v == 0 {
                Ok(None)
            } else {
                Err(E::custom(format!(
                    "期望一个表，实际是整数 {v}；只有 0 表示「未设置」"
                )))
            }
        }
        fn visit_u64<E: Error>(self, v: u64) -> Result<Option<T>, E> {
            if v == 0 {
                Ok(None)
            } else {
                Err(E::custom(format!(
                    "期望一个表，实际是整数 {v}；只有 0 表示「未设置」"
                )))
            }
        }
        fn visit_f64<E: Error>(self, v: f64) -> Result<Option<T>, E> {
            if v == 0.0 {
                Ok(None)
            } else {
                Err(E::custom(format!(
                    "期望一个表，实际是数字 {v}；只有 0 表示「未设置」"
                )))
            }
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Option<T>, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map)).map(Some)
        }
    }
    d.deserialize_option(V(PhantomData))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_truncates_toward_zero() {
        assert_eq!(de_int::<u16, _>(serde_json::Value::from(2.75)).unwrap(), 2);
        assert_eq!(de_int::<u16, _>(serde_json::Value::from(-2.75)).unwrap(), 0);
        assert_eq!(
            de_int::<i16, _>(serde_json::Value::from(-2.75)).unwrap(),
            -2
        );
        assert_eq!(de_int::<u16, _>(serde_json::Value::from(7)).unwrap(), 7);
    }

    #[test]
    fn integer_accepts_plain_and_float() {
        assert_eq!(
            de_int::<u32, _>(serde_json::Value::from(15u32)).unwrap(),
            15
        );
        assert_eq!(de_int::<u32, _>(serde_json::Value::from(15.0)).unwrap(), 15);
        // 15/4 = 3.75（mod 常见的不规范写法）
        assert_eq!(de_int::<u32, _>(serde_json::Value::from(3.75)).unwrap(), 3);
    }

    #[test]
    fn opt_handles_null() {
        assert_eq!(de_opt_int::<u16, _>(serde_json::Value::Null).unwrap(), None);
        assert_eq!(
            de_opt_int::<u16, _>(serde_json::Value::from(2.75)).unwrap(),
            Some(2)
        );
    }

    #[test]
    fn empty_map_becomes_empty_vec() {
        assert_eq!(
            de_vec_lenient::<u16, _>(serde_json::Value::Object(Default::default())).unwrap(),
            Vec::<u16>::new()
        );
        assert_eq!(
            de_opt_vec_lenient::<u16, _>(serde_json::Value::Object(Default::default())).unwrap(),
            Some(Vec::new())
        );
        assert_eq!(
            de_opt_vec_lenient::<u16, _>(serde_json::Value::Null).unwrap(),
            None
        );
    }

    #[test]
    fn vec_lenient_parses_sequence() {
        assert_eq!(
            de_vec_lenient::<String, _>(serde_json::Value::Array(vec!["a".into(), "b".into()]))
                .unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn vec_int_truncates_elements() {
        assert_eq!(
            de_vec_int::<u16, _>(serde_json::Value::Array(vec![1.0.into(), 2.75.into()])).unwrap(),
            vec![1, 2]
        );
        // 空 map → 空 Vec（整数元素同样兼容 Lua 空表）
        assert_eq!(
            de_vec_int::<u16, _>(serde_json::Value::Object(Default::default())).unwrap(),
            Vec::<u16>::new()
        );
    }

    /// 验证 serde `deserialize_with` 字面量可填裸泛型函数路径
    /// （泛型参数由字段类型统一化推断，无需 turbofish）。
    #[test]
    fn serde_deserialize_with_generic_path() {
        #[derive(serde::Deserialize, Debug)]
        struct Sample {
            #[serde(deserialize_with = "crate::lenient::de_int")]
            count: u16,
            #[serde(deserialize_with = "crate::lenient::de_opt_vec_lenient")]
            names: Option<Vec<String>>,
            #[serde(deserialize_with = "crate::lenient::de_vec_int")]
            amounts: Vec<i32>,
            #[serde(deserialize_with = "crate::lenient::de_opt_struct_lenient")]
            limits: Option<Limits>,
        }
        let s: Sample = serde_json::from_str(
            r#"{"count": 3.75, "names": {}, "amounts": [1.9, -2.9], "limits": 0}"#,
        )
        .unwrap();
        assert_eq!(s.count, 3);
        assert_eq!(s.names, Some(vec![]));
        assert_eq!(s.amounts, vec![1, -2]);
        assert!(s.limits.is_none());
    }

    #[derive(serde::Deserialize, Debug, Default)]
    #[serde(default)]
    struct Limits {
        low: f64,
        high: f64,
    }

    fn parse_limits(json: &str) -> Result<Option<Limits>, serde_json::Error> {
        #[derive(serde::Deserialize, Debug, Default)]
        #[serde(default)]
        struct Holder {
            #[serde(deserialize_with = "crate::lenient::de_opt_struct_lenient")]
            limits: Option<Limits>,
        }
        serde_json::from_str::<Holder>(json).map(|h| h.limits)
    }

    /// 真实样本：mod 写 `effect_receiver = 0` 表示「关掉这个可选表」。
    #[test]
    fn zero_false_and_null_all_mean_unset() {
        assert!(parse_limits(r#"{"limits": 0}"#).unwrap().is_none());
        assert!(parse_limits(r#"{"limits": 0.0}"#).unwrap().is_none());
        assert!(parse_limits(r#"{"limits": false}"#).unwrap().is_none());
        assert!(parse_limits(r#"{"limits": null}"#).unwrap().is_none());
        assert!(parse_limits(r#"{}"#).unwrap().is_none());
        // 空表是「设置了一个空表」，不是「未设置」
        let empty = parse_limits(r#"{"limits": {}}"#).unwrap().unwrap();
        assert_eq!((empty.low, empty.high), (0.0, 0.0));
    }

    #[test]
    fn real_table_is_still_parsed() {
        let limits = parse_limits(r#"{"limits": {"low": -0.8, "high": 1000}}"#)
            .unwrap()
            .expect("表应被解析为 Some");
        assert_eq!((limits.low, limits.high), (-0.8, 1000.0));
    }

    /// 非零值、`true`、字符串、数组仍必须报错：这些不是「未设置」的写法。
    #[test]
    fn non_zero_values_still_fail_loudly() {
        for json in [
            r#"{"limits": 5}"#,
            r#"{"limits": true}"#,
            r#"{"limits": "x"}"#,
            r#"{"limits": [1, 2]}"#,
        ] {
            let err = parse_limits(json).expect_err(json);
            let text = err.to_string();
            assert!(
                text.contains("表") || text.contains("expected"),
                "{json} 的错误信息应说明期望一个表：{text}"
            );
        }
    }
}
