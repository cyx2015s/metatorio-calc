//! Factorio 2.1.23 item-weight algorithm. Raw ItemComponent::weight is preserved;
//! resolved weights (grams, independent of quality) live in the store's cache.
use crate::store::{AIndexMap, PrototypeGroup, PrototypeRecord, PrototypeStore};
use crate::types::{Ingredient, Product};
use crate::{ItemComponent, PrototypeBaseComponent, RecipeComponent};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct WeightConstants {
    pub default_item_weight: f64,
    pub default_rocket_lift_weight: f64,
}
impl Default for WeightConstants {
    fn default() -> Self {
        // Base-game utility constants; also support small dumps without utilities.
        Self {
            default_item_weight: 100.0,
            default_rocket_lift_weight: 1_000_000.0,
        }
    }
}
impl WeightConstants {
    pub(crate) fn from_dump(dump: &serde_json::Value) -> Self {
        let defaults = Self::default();
        let constants = dump
            .get("utility-constants")
            .and_then(|v| v.as_object())
            .and_then(|v| v.values().next());
        let read = |key, fallback| {
            constants
                .and_then(|v| v.get(key))
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(fallback)
        };
        Self {
            default_item_weight: read("default_item_weight", defaults.default_item_weight),
            default_rocket_lift_weight: read(
                "default_rocket_lift_weight",
                defaults.default_rocket_lift_weight,
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeightSource {
    Explicit,
    Cursor,
    Recipe,
    Default,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemWeight {
    pub weight: f64,
    pub source: WeightSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<&'static str>,
}

impl PrototypeStore {
    pub fn weight_constants(&self) -> &WeightConstants {
        &self.weight_constants
    }

    pub fn item_weight(&self, name: &str) -> Option<&ItemWeight> {
        self.item_weights.get_or_init(|| calculate(self)).get(name)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct RecipeOrderKey {
    different_name: bool,
    catalyst: bool,
    not_intermediate: bool,
    categories: Vec<String>,
    subgroup: (String, String),
    order: String,
    name: String,
}

fn recipe_key(store: &PrototypeStore, record: &PrototypeRecord, item: &str) -> RecipeOrderKey {
    let recipe = record.component_required::<RecipeComponent>();
    let base = record.component_required::<PrototypeBaseComponent>();
    let (derived_subgroup, derived_order) = store.recipe_derived_order(record);
    let subgroup = base
        .subgroup
        .clone()
        .or(derived_subgroup)
        .unwrap_or_default();
    let subgroup_order = store
        .get(PrototypeGroup::ItemSubgroup, &subgroup)
        .and_then(|r| r.component::<PrototypeBaseComponent>())
        .map(|b| b.order.clone())
        .unwrap_or_default();
    RecipeOrderKey {
        different_name: record.name != item,
        catalyst: recipe
            .ingredients
            .iter()
            .any(|i| matches!(i, Ingredient::Item(i) if i.name == item)),
        not_intermediate: !recipe.allow_as_intermediate,
        categories: recipe.categories().to_vec(),
        subgroup: (subgroup_order, subgroup),
        order: if base.order.is_empty() {
            derived_order
        } else {
            base.order.clone()
        },
        name: record.name.clone(), // deterministic tie-break, independent of dump iteration
    }
}

fn fixed_weight(value: f64) -> f64 {
    (value * 65536.0).floor() / 65536.0
}

fn calculate(store: &PrototypeStore) -> AIndexMap<String, ItemWeight> {
    let mut selected: AIndexMap<String, &PrototypeRecord> = AIndexMap::default();
    for record in store.group(PrototypeGroup::Recipe) {
        let recipe = record.component_required::<RecipeComponent>();
        if recipe.hidden || !recipe.allow_decomposition {
            continue;
        }
        for product in &recipe.results {
            if let Product::Item(product) = product {
                let name = &product.name;
                if !selected.contains_key(name)
                    || recipe_key(store, record, name) < recipe_key(store, selected[name], name)
                {
                    selected.insert(name.clone(), record);
                }
            }
        }
    }
    let mut values = AIndexMap::default();
    let fallback = |reason, recipe: Option<&PrototypeRecord>| ItemWeight {
        weight: store.weight_constants.default_item_weight,
        source: WeightSource::Default,
        recipe: recipe.map(|r| r.name.clone()),
        fallback: Some(reason),
    };
    let mut names: Vec<_> = store
        .group(PrototypeGroup::Item)
        .map(|r| r.name.clone())
        .collect();
    names.sort();
    // Explicit stack avoids overflowing the Rust stack on long modded chains.
    for root in names {
        let mut stack = vec![(root, false)];
        let mut path: Vec<String> = Vec::new();
        while let Some((name, exit)) = stack.pop() {
            if exit {
                path.pop();
                let record = selected[&name];
                let recipe = record.component_required::<RecipeComponent>();
                let item = store
                    .item(&name)
                    .unwrap()
                    .component_required::<ItemComponent>();
                let recipe_weight: f64 = recipe
                    .ingredients
                    .iter()
                    .map(|i| match i {
                        Ingredient::Item(i) => {
                            values
                                .get(&i.name)
                                .map(|w: &ItemWeight| w.weight)
                                .unwrap_or(store.weight_constants.default_item_weight)
                                * f64::from(i.amount)
                        }
                        Ingredient::Fluid(i) => i.amount * 100.0,
                    })
                    .sum();
                let product_count: f64 = recipe
                    .results
                    .iter()
                    .map(|p| match p {
                        Product::Item(p) => p.normalized_output().base,
                        Product::Fluid(_) => 0.0,
                    })
                    .sum();
                let result = if recipe_weight <= 0.0 {
                    fallback("zero-recipe-weight", Some(record))
                } else if product_count <= 0.0 {
                    fallback("zero-product-count", Some(record))
                } else {
                    // The engine's intermediate is an integer number of grams,
                    // unlike explicitly specified Weight and the stack division.
                    let intermediate = (recipe_weight / product_count
                        * item.ingredient_to_weight_coefficient)
                        .floor();
                    let simple = store.weight_constants.default_rocket_lift_weight
                        / f64::from(item.stack_size.max(1));
                    let weight = if !recipe.allow_productivity && simple >= intermediate {
                        simple
                    } else {
                        let stacks = simple / intermediate;
                        if intermediate == 0.0 || stacks <= 1.0 {
                            intermediate
                        } else {
                            simple / stacks.floor()
                        }
                    };
                    if weight.is_finite() && weight >= 0.0 {
                        ItemWeight {
                            weight: fixed_weight(weight),
                            source: WeightSource::Recipe,
                            recipe: Some(record.name.clone()),
                            fallback: None,
                        }
                    } else {
                        fallback("invalid-recipe-weight", Some(record))
                    }
                };
                values.insert(name, result);
                continue;
            }
            if values.contains_key(&name) {
                continue;
            }
            if path.iter().any(|p| p == &name) {
                values.insert(
                    name.clone(),
                    fallback("recipe-cycle", selected.get(&name).copied()),
                );
                continue;
            }
            let Some(item) = store
                .item(&name)
                .and_then(|r| r.component::<ItemComponent>())
            else {
                continue;
            };
            if let Some(weight) = item.weight {
                values.insert(
                    name,
                    ItemWeight {
                        weight: fixed_weight(weight),
                        source: WeightSource::Explicit,
                        recipe: None,
                        fallback: None,
                    },
                );
            } else if item.flags.iter().any(|f| f == "only-in-cursor")
                && item.flags.iter().any(|f| f == "spawnable")
            {
                values.insert(
                    name,
                    ItemWeight {
                        weight: 0.0,
                        source: WeightSource::Cursor,
                        recipe: None,
                        fallback: None,
                    },
                );
            } else if let Some(record) = selected.get(&name) {
                path.push(name.clone());
                stack.push((name, true));
                for ingredient in record
                    .component_required::<RecipeComponent>()
                    .ingredients
                    .iter()
                    .rev()
                {
                    if let Ingredient::Item(i) = ingredient {
                        stack.push((i.name.clone(), false));
                    }
                }
            } else {
                values.insert(name, fallback("no-recipe", None));
            }
        }
    }
    values
}
