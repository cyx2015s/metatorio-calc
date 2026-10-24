use metatorio_data::{
    ItemComponent,
    item_weight::WeightSource,
    store::{PrototypeGroup, PrototypeStore},
};
use serde_json::{Value, json};

fn store(items: Value, recipes: Value) -> PrototypeStore {
    PrototypeStore::load(&json!({"item": items, "recipe": recipes})).unwrap()
}
fn recipe(ingredients: Value, results: Value) -> Value {
    json!({"allow_productivity":true, "ingredients":ingredients, "results":results})
}
fn w(store: &PrototypeStore, name: &str) -> f64 {
    store.item_weight(name).unwrap().weight
}

#[test]
fn official_circuit_example_and_raw_weight_preservation() {
    let s = store(
        json!({"iron-plate":{"stack_size":100,"weight":1000},"copper-cable":{"stack_size":200,"weight":250},"electronic-circuit":{"stack_size":200,"ingredient_to_weight_coefficient":0.28}}),
        json!({"electronic-circuit":recipe(json!([{"type":"item","name":"iron-plate","amount":1},{"type":"item","name":"copper-cable","amount":3}]),json!([{"type":"item","name":"electronic-circuit","amount":1}]))}),
    );
    assert_eq!(w(&s, "electronic-circuit"), 500.0);
    assert_eq!(
        s.item_weight("electronic-circuit")
            .unwrap()
            .recipe
            .as_deref(),
        Some("electronic-circuit")
    );
    assert_eq!(
        s.item("electronic-circuit")
            .unwrap()
            .component_required::<ItemComponent>()
            .weight,
        None
    );
    assert!(std::ptr::eq(
        s.item_weight("electronic-circuit").unwrap(),
        s.item_weight("electronic-circuit").unwrap()
    ));
    assert!(s.item_weight("missing").is_none());
}

#[test]
fn explicit_flags_and_default_weights() {
    let s = store(
        json!({"explicit":{"weight":42},"zero":{"weight":0},"cursor":{"flags":["only-in-cursor","spawnable"]},"one-flag":{"flags":["spawnable"]},"explicit-cursor":{"weight":17,"flags":["only-in-cursor","spawnable"]}}),
        json!({}),
    );
    assert_eq!(w(&s, "explicit"), 42.0);
    assert_eq!(w(&s, "zero"), 0.0);
    assert_eq!(w(&s, "cursor"), 0.0);
    assert_eq!(w(&s, "one-flag"), 100.0);
    assert_eq!(w(&s, "explicit-cursor"), 17.0);
    assert!(matches!(
        s.item_weight("cursor").unwrap().source,
        WeightSource::Cursor
    ));
}

#[test]
fn all_item_products_are_counted_after_probabilities_and_fluids_are_not() {
    let s = store(
        json!({"a":{"stack_size":1},"b":{"stack_size":1}}),
        json!({"a":recipe(json!([{"type":"fluid","name":"water","amount":100}]),json!([{"type":"item","name":"a","amount_min":1,"amount_max":3,"probability":0.5},{"type":"item","name":"b","amount":2,"probability":0.5},{"type":"fluid","name":"water","amount":999999}]))}),
    );
    // 100*100 / (2*0.5 + 2*0.5) * 0.5 = 2500; integer-stack rounding is exact.
    assert_eq!(w(&s, "a"), 2500.0);
    assert_eq!(w(&s, "b"), 2500.0);
}

#[test]
fn non_productive_simple_weight_and_overweight_branch() {
    let mut r = recipe(
        json!([{"type":"item","name":"raw","amount":1}]),
        json!([{"type":"item","name":"out","amount":1}]),
    );
    r["allow_productivity"] = json!(false);
    let s = store(
        json!({"raw":{"weight":1000},"out":{"stack_size":100}}),
        json!({"out":r.clone()}),
    );
    assert_eq!(w(&s, "out"), 10000.0);
    r["ingredients"][0]["amount"] = json!(30);
    let s = store(
        json!({"raw":{"weight":1000},"out":{"stack_size":100}}),
        json!({"out":r}),
    );
    assert_eq!(w(&s, "out"), 15000.0);
}

#[test]
fn selected_recipe_not_cheapest_or_only_unlocked_recipe() {
    let mut preferred = recipe(
        json!([{"type":"item","name":"raw","amount":20}]),
        json!([{"type":"item","name":"out","amount":1}]),
    );
    preferred["enabled"] = json!(false);
    let cheap = recipe(
        json!([{"type":"item","name":"raw","amount":1}]),
        json!([{"type":"item","name":"out","amount":1}]),
    );
    let s = store(
        json!({"raw":{"weight":1000},"out":{"stack_size":1}}),
        json!({"out":preferred,"aaa":cheap}),
    );
    assert_eq!(w(&s, "out"), 10000.0);
}

#[test]
fn hidden_and_non_decomposable_recipes_are_excluded() {
    for field in ["hidden", "allow_decomposition"] {
        let mut excluded = recipe(
            json!([{"type":"fluid","name":"water","amount":999}]),
            json!([{"type":"item","name":"out","amount":1}]),
        );
        excluded[field] = json!(field == "hidden");
        let s = store(
            json!({"out":{"stack_size":1}}),
            json!({"out":excluded,"fallback":recipe(json!([{"type":"fluid","name":"water","amount":20}]),json!([{"type":"item","name":"out","amount":1}]))}),
        );
        assert_eq!(w(&s, "out"), 1000.0);
        assert_eq!(
            s.item_weight("out").unwrap().recipe.as_deref(),
            Some("fallback")
        );
    }
}

#[test]
fn catalyst_and_handcraft_intermediate_priority() {
    let mut catalyst = recipe(
        json!([{"type":"item","name":"out","amount":1}]),
        json!([{"type":"item","name":"out","amount":2}]),
    );
    catalyst["order"] = json!("a");
    let mut preferred = recipe(
        json!([{"type":"fluid","name":"water","amount":20}]),
        json!([{"type":"item","name":"out","amount":1}]),
    );
    preferred["order"] = json!("z");
    let mut not_intermediate = preferred.clone();
    not_intermediate["allow_as_intermediate"] = json!(false);
    not_intermediate["order"] = json!("a");
    let s = store(
        json!({"out":{"stack_size":1}}),
        json!({"a":catalyst,"b":not_intermediate,"c":preferred}),
    );
    assert_eq!(s.item_weight("out").unwrap().recipe.as_deref(), Some("c"));
}

#[test]
fn zero_inputs_and_zero_expected_products_fall_back() {
    for (ingredients, results, reason) in [
        (
            json!([]),
            json!([{"type":"item","name":"out","amount":1}]),
            "zero-recipe-weight",
        ),
        (
            json!([{"type":"fluid","name":"water","amount":10}]),
            json!([{"type":"item","name":"out","amount":1,"probability":0}]),
            "zero-product-count",
        ),
    ] {
        let s = store(
            json!({"out":{"stack_size":1}}),
            json!({"out":recipe(ingredients,results)}),
        );
        assert_eq!(w(&s, "out"), 100.0);
        assert_eq!(s.item_weight("out").unwrap().fallback, Some(reason));
    }
}

#[test]
fn cycles_use_a_temporary_default_then_finish_the_recipe_calculation() {
    let s = store(
        json!({"a":{"stack_size":1},"b":{"stack_size":1},"c":{"stack_size":1}}),
        json!({
        "a":recipe(json!([{"type":"item","name":"b","amount":4}]),json!([{"type":"item","name":"a","amount":1}])),
        "b":recipe(json!([{"type":"item","name":"a","amount":4}]),json!([{"type":"item","name":"b","amount":1}])),
        "c":recipe(json!([{"type":"item","name":"a","amount":4}]),json!([{"type":"item","name":"c","amount":1}]))}),
    );
    assert_eq!(w(&s, "a"), 400.0);
    assert_eq!(w(&s, "b"), 200.0);
    assert_eq!(w(&s, "c"), 800.0);
}

#[test]
fn modded_utility_constants_drive_defaults_and_stack_rounding() {
    let s=PrototypeStore::load(&json!({"utility-constants":{"default":{"default_item_weight":70,"default_rocket_lift_weight":1200}},"item":{"raw":{},"out":{"stack_size":10}},"recipe":{"out":recipe(json!([{"type":"item","name":"raw","amount":1}]),json!([{"type":"item","name":"out","amount":1}]))}})).unwrap();
    assert_eq!(w(&s, "raw"), 70.0);
    assert_eq!(w(&s, "out"), 40.0);
}

#[test]
fn fractional_weights_use_game_fixed_point_precision() {
    let s = store(json!({"fraction":{"weight":1.23456789}}), json!({}));
    assert_eq!(w(&s, "fraction"), 1.23455810546875);
}

#[test]
fn integer_intermediate_and_fractional_simple_result_match_game() {
    let mut r = recipe(
        json!([{"type":"item","name":"raw","amount":1}]),
        json!([{"type":"item","name":"out","amount":1}]),
    );
    let s = store(
        json!({"raw":{"weight":247.8},"out":{"stack_size":10000}}),
        json!({"out":r.clone()}),
    );
    assert_eq!(w(&s, "out"), 123.0);
    r["allow_productivity"] = json!(false);
    let s = store(
        json!({"raw":{"weight":1000},"out":{"stack_size":3}}),
        json!({"out":r}),
    );
    assert_eq!(w(&s, "out"), 333333.33332824707);
}

#[test]
fn very_small_derived_weights_do_not_produce_infinite_or_wrapped_weights() {
    let s = store(
        json!({"raw":{"weight":0.75},"out":{"stack_size":100}}),
        json!({"out":recipe(json!([{"type":"item","name":"raw","amount":2}]),json!([{"type":"item","name":"out","amount":1}]))}),
    );
    assert_eq!(w(&s, "out"), 0.0);
}

#[test]
fn thousands_of_dependencies_use_an_explicit_stack() {
    let mut items = serde_json::Map::new();
    let mut recipes = serde_json::Map::new();
    items.insert("i3000".into(), json!({"weight":1000}));
    for i in 0..3000 {
        let name = format!("i{i:04}");
        let next = if i == 2999 {
            "i3000".into()
        } else {
            format!("i{:04}", i + 1)
        };
        items.insert(name.clone(), json!({"stack_size":100}));
        recipes.insert(
            name.clone(),
            recipe(
                json!([{"type":"item","name":next,"amount":2}]),
                json!([{"type":"item","name":name,"amount":1}]),
            ),
        );
    }
    let s = store(Value::Object(items), Value::Object(recipes));
    assert_eq!(w(&s, "i0000"), 1000.0);
}

#[test]
fn category_name_then_subgroup_order_then_recipe_order_break_ties() {
    let base = recipe(
        json!([{"type":"fluid","name":"water","amount":20}]),
        json!([{"type":"item","name":"out","amount":1}]),
    );
    for (field, expected) in [("categories", "a"), ("subgroup", "b"), ("order", "b")] {
        let mut a = base.clone();
        let mut b = base.clone();
        if field == "categories" {
            a[field] = json!(["aa"]);
            b[field] = json!(["zz"]);
        } else if field == "subgroup" {
            a[field] = json!("aa");
            b[field] = json!("zz");
        } else {
            a[field] = json!("z");
            b[field] = json!("a");
        }
        let s=PrototypeStore::load(&json!({"item":{"out":{"stack_size":1}},"recipe":{"a":a,"b":b},"recipe-category":{"aa":{"order":"z"},"zz":{"order":"a"}},"item-subgroup":{"aa":{"order":"z"},"zz":{"order":"a"}}})).unwrap();
        assert_eq!(
            s.item_weight("out").unwrap().recipe.as_deref(),
            Some(expected),
            "{field}"
        );
    }
}

#[test]
fn full_assets_have_finite_cached_weights_for_every_item() {
    for file in ["data-raw-dump.json", "data-raw-dump-heavily-modded.json"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(file);
        let dump: Value =
            serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
                .unwrap();
        let s = PrototypeStore::load(&dump).unwrap();
        let mut sources = std::collections::BTreeMap::new();
        for record in s.group(PrototypeGroup::Item) {
            let result = s.item_weight(&record.name).unwrap();
            assert!(
                result.weight.is_finite() && result.weight >= 0.0,
                "{}: {result:?}",
                record.name
            );
            *sources
                .entry(format!("{:?}", result.source))
                .or_insert(0usize) += 1;
            if let Some(explicit) = record.component_required::<ItemComponent>().weight {
                assert_eq!(result.weight, (explicit * 65536.0).floor() / 65536.0);
            }
        }
        eprintln!("{file}: {sources:?}");
    }
}

/// Optional game oracle; keep the game installation and generated dumps out of CI.
#[test]
#[ignore = "requires an isolated Factorio raw dump and runtime weight export"]
fn game_export_matches_all_item_weights() {
    let dump_path = std::env::var("METATORIO_WEIGHT_REFERENCE_DUMP").unwrap();
    let weights_path = std::env::var("METATORIO_WEIGHT_REFERENCE_WEIGHTS").unwrap();
    let dump: Value = serde_json::from_reader(std::io::BufReader::new(
        std::fs::File::open(dump_path).unwrap(),
    ))
    .unwrap();
    let expected: std::collections::BTreeMap<String, f64> = serde_json::from_reader(
        std::io::BufReader::new(std::fs::File::open(weights_path).unwrap()),
    )
    .unwrap();
    let s = PrototypeStore::load(&dump).unwrap();
    let mut mismatches = Vec::new();
    for (name, expected) in &expected {
        let actual = s
            .item_weight(name)
            .unwrap_or_else(|| panic!("missing reference item {name}"));
        if (actual.weight - expected).abs() > 1.0 / 65536.0 {
            mismatches.push(format!("{name}: expected {expected}, actual {actual:?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    eprintln!("Compared {} game item weights", expected.len());
}
