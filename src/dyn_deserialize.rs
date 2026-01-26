use std::collections::HashMap;

#[derive(Debug)]
pub struct DynDeserializer<T: ?Sized> {
    deserialize: fn(serde_json::Value) -> Option<Box<T>>,
}

impl<T: ?Sized> DynDeserializer<T> {
    pub fn new(deserialize: fn(serde_json::Value) -> Option<Box<T>>) -> Self {
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
    pub fn deserialize(&self, value: serde_json::Value) -> Option<Box<T>> {
        let type_name = value.get("type")?.as_str()?;
        if let Some(deserializer) = self.deserializers.get(type_name) {
            eprintln!("[DynDeserializeRegistry] Deserializing type: {}", type_name);
            (deserializer.deserialize)(value)
        } else {
            None
        }
    }
    pub fn register(&mut self, type_name: &'static str, deserializer: DynDeserializer<T>) {
        self.deserializers.insert(type_name, deserializer);
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
            pub fn register(
                registry: &mut $crate::dyn_deserialize::DynDeserializeRegistry<$trait>,
            ) {
                registry.register(
                    $tag,
                    crate::dyn_deserialize::DynDeserializer::new(|value| {
                        let this: $ty = serde_json::from_value(value).unwrap();
                        Some(Box::new(this))
                    }),
                );
            }
        }
    };
}

#[test]
fn test_dyn_deserializer() {
    use crate::{
        concept::*,
        factorio::model::{context::*, module::*, recipe::*, source::*},
    };
    let data = include_str!("../assets/data-raw-dump.json");
    let value = serde_json::from_str(&data).unwrap();
    let ctx = FactorioContext::load(&value);
    let mut registry = DynDeserializeRegistry::<
        dyn Mechanic<ItemIdentType = GenericItem, GameContext = FactorioContext>,
    >::default();

    InfiniteSource::register(&mut registry);
    RecipeConfig::register(&mut registry);

    let source = InfiniteSource {
        item: GenericItem::Item {
            name: "iron-ore".to_string(),
            quality: 0,
        },
    };
    let recipe = RecipeConfig {
        recipe: "iron-gear-wheel".into(),
        machine: Some("assembling-machine-2".into()),
        module_config: ModuleConfig::new(),
        instance_fuel: None,
    };
    dbg!(&source);
    dbg!(&recipe);
    let serialized_source = serde_json::to_value(&source).unwrap();
    let serialized_recipe = serde_json::to_value(&recipe).unwrap();
    dbg!(&serialized_source);
    dbg!(&serialized_recipe);

    if let Some(source) = registry.deserialize(serialized_source.clone()) {
        eprintln!("无限源反序列化成功");
        eprintln!("{:?}", source.as_flow(&ctx));
    }
    if let Some(recipe) = registry.deserialize(serialized_recipe.clone()) {
        eprintln!("配方反序列化成功");
        eprintln!("{:?}", recipe.as_flow(&ctx));
    }
}
