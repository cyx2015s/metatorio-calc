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
            f.write_str("an integer (floats truncated toward zero)")
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
            f.write_str("an optional integer (floats truncated toward zero)")
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
            f.write_str("a sequence or an empty map (Lua empty table)")
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
                    "expected an empty map (Lua empty table) or a sequence, got a non-empty map",
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
            f.write_str("an optional sequence or an empty map (Lua empty table)")
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
            f.write_str("a sequence of integers or an empty map (Lua empty table)")
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
                    "expected an empty map (Lua empty table) or a sequence, got a non-empty map",
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
            f.write_str("an optional sequence of integers or an empty map")
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
        }
        let s: Sample =
            serde_json::from_str(r#"{"count": 3.75, "names": {}, "amounts": [1.9, -2.9]}"#)
                .unwrap();
        assert_eq!(s.count, 3);
        assert_eq!(s.names, Some(vec![]));
        assert_eq!(s.amounts, vec![1, -2]);
    }
}
