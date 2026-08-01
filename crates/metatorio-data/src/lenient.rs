//! 宽松反序列化：容忍 Lua→JSON 导出的不严格类型。
//!
//! 背景（游戏数据的事实）：
//! - Lua 的 `15/4 = 3.75` 会被 mod 直接放进期望整数的字段（类型转换不规范），
//!   导出 JSON 为浮点；游戏引擎的实际行为是**向 0 舍入**（truncation）。
//!   serde 默认拒绝 float→int，必须在这里兜底。
//! - Lua 的空 table `{}` 既是空 map 也是空 list，导出为 `{}`（空 object），
//!   期望 `Vec<T>` 的字段会收到空 map —— 视为空 Vec。
//!
//! 生成器（metatorio-data-codegen）对整数字段使用 `de_<int>` 系列，
//! 对数组字段生成内联函数（内部调用 [`de_vec_lenient`]）。

use serde::de::{Deserializer, Error, IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::fmt;
use std::marker::PhantomData;

/// 生成一个整数类型的宽松反序列化函数族：
/// `de_<t>`（裸）、`de_opt_<t>`（Option）、`de_vec_<t>`（Vec）、`de_opt_vec_<t>`。
macro_rules! lenient_int_family {
    ($name:ident, $ty:ty, $from_f64:expr) => {
        paste::paste! {
            pub fn [<de_ $name>]<'de, D: Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = $ty;
                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("an integer (floats truncated toward zero)")
                    }
                    fn visit_i64<E: Error>(self, v: i64) -> Result<$ty, E> {
                        Ok(v as $ty)
                    }
                    fn visit_u64<E: Error>(self, v: u64) -> Result<$ty, E> {
                        Ok(v as $ty)
                    }
                    fn visit_f64<E: Error>(self, v: f64) -> Result<$ty, E> {
                        // 向 0 舍入（truncation），与游戏引擎一致
                        Ok($from_f64(v))
                    }
                }
                d.deserialize_any(V)
            }

            pub fn [<de_opt_ $name>]<'de, D: Deserializer<'de>>(d: D) -> Result<Option<$ty>, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = Option<$ty>;
                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("an optional integer (floats truncated toward zero)")
                    }
                    fn visit_unit<E: Error>(self) -> Result<Option<$ty>, E> {
                        Ok(None)
                    }
                    fn visit_none<E: Error>(self) -> Result<Option<$ty>, E> {
                        Ok(None)
                    }
                    fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Option<$ty>, D2::Error> {
                        [<de_ $name>](d).map(Some)
                    }
                }
                d.deserialize_option(V)
            }

            pub fn [<de_vec_ $name>]<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<$ty>, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = Vec<$ty>;
                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("a sequence or an empty map (Lua empty table)")
                    }
                    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<$ty>, A::Error> {
                        // 元素同样用 lenient 整数解析（float → 向 0 舍入）
                        struct Elem;
                        impl<'de> serde::de::DeserializeSeed<'de> for Elem {
                            type Value = $ty;
                            fn deserialize<D: Deserializer<'de>>(
                                self,
                                d: D,
                            ) -> Result<$ty, D::Error> {
                                [<de_ $name>](d)
                            }
                        }
                        let mut out = Vec::new();
                        while let Some(v) = seq.next_element_seed(Elem)? {
                            out.push(v);
                        }
                        Ok(out)
                    }
                    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<$ty>, A::Error> {
                        if map.next_key::<IgnoredAny>()?.is_none() {
                            Ok(Vec::new()) // Lua 空 table 导出为 {} → 空 Vec
                        } else {
                            Err(A::Error::custom(
                                "expected an empty map (Lua empty table) or a sequence, got a non-empty map",
                            ))
                        }
                    }
                }
                d.deserialize_any(V)
            }

            pub fn [<de_opt_vec_ $name>]<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<$ty>>, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = Option<Vec<$ty>>;
                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("an optional sequence or an empty map (Lua empty table)")
                    }
                    fn visit_unit<E: Error>(self) -> Result<Option<Vec<$ty>>, E> {
                        Ok(None)
                    }
                    fn visit_none<E: Error>(self) -> Result<Option<Vec<$ty>>, E> {
                        Ok(None)
                    }
                    fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Option<Vec<$ty>>, D2::Error> {
                        [<de_vec_ $name>](d).map(Some)
                    }
                }
                d.deserialize_option(V)
            }
        }
    };
}

// 注：使用 paste crate 生成函数名；如不想引入依赖，可手工展开。
lenient_int_family!(u8, u8, |v: f64| v.trunc() as u8);
lenient_int_family!(u16, u16, |v: f64| v.trunc() as u16);
lenient_int_family!(u32, u32, |v: f64| v.trunc() as u32);
lenient_int_family!(u64, u64, |v: f64| v.trunc() as u64);
lenient_int_family!(i8, i8, |v: f64| v.trunc() as i8);
lenient_int_family!(i16, i16, |v: f64| v.trunc() as i16);
lenient_int_family!(i32, i32, |v: f64| v.trunc() as i32);
lenient_int_family!(i64, i64, |v: f64| v.trunc() as i64);

/// 宽松 Vec：接受序列，也接受空 map（Lua 空 table 导出 `{}`）。
///
/// 生成器为每个数组字段生成内联函数时调用本函数作为核心。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_truncates_toward_zero() {
        assert_eq!(de_u16(serde_json::Value::from(2.75)).unwrap(), 2);
        assert_eq!(de_u16(serde_json::Value::from(-2.75)).unwrap(), 0); // 向 0 舍入
        assert_eq!(de_i16(serde_json::Value::from(-2.75)).unwrap(), -2);
        assert_eq!(de_u16(serde_json::Value::from(7)).unwrap(), 7);
    }

    #[test]
    fn integer_accepts_plain_and_float() {
        assert_eq!(de_u32(serde_json::Value::from(15u32)).unwrap(), 15);
        assert_eq!(de_u32(serde_json::Value::from(15.0)).unwrap(), 15);
        // 15/4 = 3.75（mod 常见的不规范写法）
        assert_eq!(de_u32(serde_json::Value::from(3.75)).unwrap(), 3);
    }

    #[test]
    fn opt_handles_null() {
        assert_eq!(de_opt_u16(serde_json::Value::Null).unwrap(), None);
        assert_eq!(de_opt_u16(serde_json::Value::from(2.75)).unwrap(), Some(2));
    }

    #[test]
    fn empty_map_becomes_empty_vec() {
        assert_eq!(
            de_vec_u16(serde_json::Value::Object(Default::default())).unwrap(),
            Vec::<u16>::new()
        );
        assert_eq!(
            de_opt_vec_u16(serde_json::Value::Object(Default::default())).unwrap(),
            Some(Vec::new())
        );
        assert_eq!(
            de_opt_vec_u16(serde_json::Value::Null).unwrap(),
            None
        );
    }

    #[test]
    fn vec_lenient_parses_sequence() {
        assert_eq!(
            de_vec_u16(serde_json::Value::Array(vec![1.0.into(), 2.75.into()])).unwrap(),
            vec![1, 2]
        );
    }

    #[test]
    fn generic_vec_lenient() {
        let v: Vec<String> = de_vec_lenient(serde_json::Value::Array(
            vec!["a".into(), "b".into()],
        ))
        .unwrap();
        assert_eq!(v, vec!["a".to_string(), "b".to_string()]);

        let v: Vec<String> = de_vec_lenient(serde_json::Value::Object(Default::default())).unwrap();
        assert!(v.is_empty());
    }
}
