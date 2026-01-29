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
                    $crate::dyn_deserialize::DynDeserializer::new(|value| {
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
    use crate::{concept::*, factorio::*};
    let ctx = FactorioContext::default();
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

    if let Some(recipe) = registry.deserialize(serialized_recipe.clone()) {
        eprintln!("配方反序列化成功");
        eprintln!("{:?}", recipe.as_flow(&ctx));
    }
    if let Some(mining) = registry.deserialize(serialized_mining.clone()) {
        eprintln!("采矿反序列化成功");
        eprintln!("{:?}", mining.as_flow(&ctx));
    }
}
