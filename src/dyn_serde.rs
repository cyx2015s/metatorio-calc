use std::collections::HashMap;

use crate::error::AppError;

#[derive(Debug)]
pub struct DynDeserializer<T: ?Sized> {
    deserialize: fn(serde_json::Value) -> Result<Box<T>, AppError>,
}

impl<T: ?Sized> DynDeserializer<T> {
    pub fn new(deserialize: fn(serde_json::Value) -> Result<Box<T>, AppError>) -> Self {
        Self { deserialize }
    }
}

#[derive(Debug)]
pub struct DynDeserializeRegistry<T: ?Sized> {
    deserializers: HashMap<&'static str, DynDeserializer<T>>,
}

impl<T: ?Sized> Default for DynDeserializeRegistry<T> {
    fn default() -> Self {
        Self {
            deserializers: HashMap::new(),
        }
    }
}

impl<T: ?Sized> DynDeserializeRegistry<T> {
    pub fn deserialize(&self, value: serde_json::Value) -> Result<Box<T>, AppError> {
        let type_name = value
            .get("type")
            .ok_or_else(|| AppError::Registry("缺少字段 type".to_string()))?
            .as_str()
            .ok_or_else(|| AppError::Registry("字段 type 不是字符串".to_string()))?;
        if let Some(deserializer) = self.deserializers.get(type_name) {
            (deserializer.deserialize)(value)
        } else {
            Err(AppError::Registry(format!(
                "未知的类型标识符：{}，已注册的类型有：{:?}",
                type_name,
                self.registered_types()
            )))
        }
    }
    pub fn register(&mut self, type_name: &'static str, deserializer: DynDeserializer<T>) {
        self.deserializers.insert(type_name, deserializer);
    }
    pub fn registered_types(&self) -> Vec<&'static str> {
        self.deserializers.keys().cloned().collect()
    }
}

#[macro_export]
macro_rules! impl_register_deserializer {
    (
        for $ty:ty
        as $tag:expr
        => $trait:ty
    ) => {
        impl $ty {
            pub fn register(registry: &mut $crate::dyn_serde::DynDeserializeRegistry<$trait>) {
                registry.register(
                    $tag,
                    $crate::dyn_serde::DynDeserializer::new(|value| {
                        let this = serde_json::from_value::<$ty>(value).map_err(|e| {
                            $crate::error::AppError::Registry(format!(
                                "反序列化类型 {} 失败: {}",
                                $tag, e
                            ))
                        })?;
                        Ok(Box::new(this))
                    }),
                );
            }
        }
    };
}

#[test]
fn test_dyn_deserializer() {
    use crate::{concept::*, factorio::*};
    let ctx = FactorioContext::test_load();
    let mut registry = DynDeserializeRegistry::<
        dyn Mechanic<ItemIdentType = GenericItem, GameContext = FactorioContext>,
    >::default();

    RecipeConfig::register(&mut registry);
    MiningConfig::register(&mut registry);
    let recipe = RecipeConfig {
        recipe: "iron-gear-wheel".into(),
        machine: "assembling-machine-2".into(),
        module_config: ModuleConfig::new(),
        instance_fuel: None,
    };
    let mining = MiningConfig {
        resource: "iron-ore".into(),
        machine: "electric-mining-drill".into(),
        module_config: ModuleConfig::new(),
        instance_fuel: None,
    };
    dbg!(&recipe);
    dbg!(&mining);
    let serialized_recipe = serde_json::to_value(&recipe).unwrap();
    let serialized_mining = serde_json::to_value(&mining).unwrap();
    dbg!(&serialized_recipe);
    dbg!(&serialized_mining);

    if let Ok(recipe) = registry.deserialize(serialized_recipe.clone()) {
        eprintln!("配方反序列化成功");
        eprintln!("{:?}", recipe.as_flow(&ctx));
    }
    if let Ok(mining) = registry.deserialize(serialized_mining.clone()) {
        eprintln!("采矿反序列化成功");
        eprintln!("{:?}", mining.as_flow(&ctx));
    }
}

pub fn save_to_file<T: serde::Serialize>(
    value: &T,
    path: &std::path::Path,
) -> Result<(), AppError> {
    let serialized = serde_json::to_string_pretty(value).map_err(|e| {
        AppError::IO(format!(
            "序列化数据到 JSON 失败（准备写入 {}）：{}",
            path.display(),
            e
        ))
    })?;
    std::fs::write(path, serialized)
        .map_err(|e| AppError::IO(format!("写入文件 {} 失败：{}", path.display(), e)))?;
    Ok(())
}
