use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{message::AppMessage, solve::Runtime};
use serde_json::{Value, json};

fn dispatch(runtime: &mut Runtime, value: Value) {
    runtime
        .dispatch(serde_json::from_value::<AppMessage>(value).unwrap())
        .unwrap();
}

#[test]
fn universe_external_inputs_override_only_the_same_location_and_survive_history_and_import() {
    let mut runtime = Runtime::new();
    runtime.install_context(
        "test".into(),
        PrototypeStore::load(&json!({"planet":{"nauvis":{},"fulgora":{}}})).unwrap(),
    );
    runtime.set_active_context(Some("test".into()));
    dispatch(
        &mut runtime,
        json!({"scope":"application","action":{"new-project":{"name":"test"}}}),
    );
    let p = runtime.state.document.projects[0].id;
    dispatch(
        &mut runtime,
        json!({"scope":"project","action":{"project":p,"action":{"add-universe":{"name":"宇宙"}}}}),
    );
    let u = runtime.state.document.projects[0].universes[0].id;
    let send = |runtime: &mut Runtime, action: Value| {
        dispatch(
            runtime,
            json!({"scope":"universe","action":{"project":p,"universe":u,"action":action}}),
        )
    };
    for (planet, penalty) in [("nauvis", 2.0), ("fulgora", 5.0)] {
        let flow = json!({"location":{"kind":"planet","name":planet},"flow":"Electricity"});
        send(
            &mut runtime,
            json!({"add-external-input":{"flow":flow,"penalty":penalty}}),
        );
        send(
            &mut runtime,
            json!({"add-target":{"flow":flow,"amount":3.0}}),
        );
    }
    send(&mut runtime, json!({"set-strict-source":{"strict":true}}));
    let result = runtime.solve_universe(p, u).unwrap();
    let metatorio_runtime::universe::UniverseSolveStatus::Solved { cost, .. } = result.status
    else {
        panic!("{result:?}")
    };
    assert!((cost - 21.0).abs() < 1e-6, "{cost}");
    let id = runtime.state.universe(p, u).unwrap().external_inputs[0].id;
    send(
        &mut runtime,
        json!({"set-external-input-penalty":{"input":id,"penalty":0}}),
    );
    dispatch(&mut runtime, json!({"scope":"history","action":"undo"}));
    assert_eq!(
        runtime.state.universe(p, u).unwrap().external_inputs[0].penalty,
        2.0
    );
    let before = runtime.state.document.clone();
    let invalid=serde_json::from_value(json!({"scope":"universe","action":{"project":p,"universe":u,"action":{"set-external-input-penalty":{"input":id,"penalty":-1}}}})).unwrap();
    assert!(runtime.dispatch(invalid).is_err());
    assert_eq!(runtime.state.document, before);
    send(
        &mut runtime,
        json!({"set-external-input-flow":{"input":id,"flow":{"location":{"kind":"space"},"flow":"Heat"}}}),
    );
    assert_eq!(
        runtime.state.universe(p, u).unwrap().external_inputs[0]
            .flow
            .location,
        metatorio_core::Location::Space
    );
    dispatch(
        &mut runtime,
        json!({"scope":"project","action":{"project":p,"action":{"clone-universe":{"universe":u}}}}),
    );
    let cloned = &runtime.state.document.projects[0].universes[1];
    assert_ne!(cloned.external_inputs[0].id, id);
    let mut imported = Runtime::new();
    imported.state.import_projects(&runtime.state.document);
    let imported_id = imported.state.document.projects[0].universes[0].external_inputs[0].id;
    let wire = serde_json::to_value(&imported.state.document).unwrap();
    assert!(wire["projects"][0]["universes"][0]["external_inputs"].is_array());
    send(&mut runtime, json!({"remove-external-input":{"input":id}}));
    assert_eq!(
        runtime.state.universe(p, u).unwrap().external_inputs.len(),
        1
    );
    assert_eq!(imported_id, id);
}

#[test]
fn universe_manual_space_input_overrides_free_asteroids_and_keeps_free_delivery() {
    use metatorio_core::{DualVar, IdWithQuality, LocatedDualVar, Location};
    use metatorio_runtime::{
        document::{LocatedExternalInput, LocatedTarget, ProjectDocument, UniverseDocument},
        id::{ExternalInputId, TargetId},
        universe::{UniverseSolveStatus, solve_universe},
    };
    let store=PrototypeStore::load(&json!({"item":{"chunk":{"stack_size":50}},"asteroid-chunk":{"direct":{"minable":{"mining_time":1,"result":"chunk"}}}})).unwrap();
    let flow = DualVar::Item(IdWithQuality {
        id: "chunk".into(),
        quality: "normal".into(),
    });
    // Old files have no external_inputs field and still load normally.
    let mut universe: UniverseDocument = serde_json::from_value(json!({"targets":[]})).unwrap();
    universe.strict_source = true;
    universe.targets.push(LocatedTarget {
        id: TargetId(1),
        flow: LocatedDualVar::new(Location::planet("nauvis"), flow.clone()),
        amount: 2.0,
    });
    let project = ProjectDocument::default();
    let solve_cost =
        |u: &UniverseDocument| match solve_universe(&store, &project, u, &Default::default())
            .unwrap()
            .status
        {
            UniverseSolveStatus::Solved { cost, .. } => cost,
            status => panic!("{status:?}"),
        };
    assert_eq!(solve_cost(&universe), 0.0);
    universe.external_inputs.push(LocatedExternalInput {
        id: ExternalInputId(2),
        flow: LocatedDualVar::new(Location::Space, flow),
        penalty: 7.0,
    });
    assert!((solve_cost(&universe) - 14.0).abs() < 1e-6);
    universe.external_inputs.clear();
    assert_eq!(solve_cost(&universe), 0.0);
}
#[test]
fn universe_editor_commands_clone_and_transfer_edits_are_undoable_and_scoped() {
    let mut runtime = Runtime::new();
    runtime.install_context("test".into(),PrototypeStore::load(&json!({"item":{"iron":{"stack_size":100,"weight":1000},"copper":{"stack_size":100,"weight":2000}},"planet":{"nauvis":{}}})).unwrap());
    runtime.set_active_context(Some("test".into()));
    dispatch(
        &mut runtime,
        json!({"scope":"application","action":{"new-project":{"name":"test"}}}),
    );
    let p = runtime.state.document.projects[0].id;
    let factory = runtime.state.document.projects[0].factories[0].clone();
    dispatch(
        &mut runtime,
        json!({"scope":"project","action":{"project":p,"action":{"add-universe":{"name":"宇宙"}}}}),
    );
    let u = runtime.state.document.projects[0].universes[0].id;
    let send = |runtime: &mut Runtime, action: Value| {
        dispatch(
            runtime,
            json!({"scope":"universe","action":{"project":p,"universe":u,"action":action}}),
        )
    };
    send(
        &mut runtime,
        json!({"add-transfer":{"location":{"kind":"planet","name":"nauvis"},"item":{"id":"iron","quality":"normal"}}}),
    );
    let m = runtime.state.universe(p, u).unwrap().mechanics[0].id;
    send(
        &mut runtime,
        json!({"set-transfer-item":{"mechanic":m,"item":{"id":"copper","quality":"normal"}}}),
    );
    send(&mut runtime, json!({"clone-mechanic":{"mechanic":m}}));
    let entries = &runtime.state.universe(p, u).unwrap().mechanics;
    assert_eq!(entries.len(), 2);
    assert_ne!(entries[0].id, entries[1].id);
    assert_eq!(entries[0].mechanic, entries[1].mechanic);
    dispatch(&mut runtime, json!({"scope":"history","action":"undo"}));
    assert_eq!(runtime.state.universe(p, u).unwrap().mechanics.len(), 1);
    assert_eq!(runtime.state.document.projects[0].factories[0], factory);
    let before = runtime.state.document.clone();
    let invalid:AppMessage=serde_json::from_value(json!({"scope":"universe","action":{"project":p,"universe":u,"action":{"set-transfer-item":{"mechanic":m,"item":{"id":"missing","quality":"normal"}}}}})).unwrap();
    assert!(runtime.dispatch(invalid).is_err());
    assert_eq!(runtime.state.document, before);
}

#[test]
fn universe_expression_reordering_uses_the_same_wire_contract_as_shared_panels() {
    let mut runtime = Runtime::new();
    dispatch(
        &mut runtime,
        json!({"scope":"application","action":{"new-project":{"name":"test"}}}),
    );
    let p = runtime.state.document.projects[0].id;
    dispatch(
        &mut runtime,
        json!({"scope":"project","action":{"project":p,"action":{"add-universe":{"name":"宇宙"}}}}),
    );
    let u = runtime.state.document.projects[0].universes[0].id;
    let send = |runtime: &mut Runtime, action: Value| {
        dispatch(
            runtime,
            json!({"scope":"universe","action":{"project":p,"universe":u,"action":action}}),
        )
    };
    send(&mut runtime, json!("add-target-expression"));
    send(&mut runtime, json!("add-target-expression"));
    let e = runtime.state.universe(p, u).unwrap().target_expressions[0].id;
    send(
        &mut runtime,
        json!({"reorder-target-expression":{"expression":e,"position":1}}),
    );
    assert_eq!(
        runtime.state.universe(p, u).unwrap().target_expressions[1].id,
        e
    );
    for name in ["nauvis", "fulgora"] {
        send(
            &mut runtime,
            json!({"add-target-expression-term":{"expression":e,"flow":{"location":{"kind":"planet","name":name},"flow":"Electricity"}}}),
        );
    }
    let term = runtime.state.universe(p, u).unwrap().target_expressions[1].terms[0].id;
    send(
        &mut runtime,
        json!({"reorder-target-expression-term":{"expression":e,"term":term,"position":1}}),
    );
    assert_eq!(
        runtime.state.universe(p, u).unwrap().target_expressions[1].terms[1].id,
        term
    );
}

#[test]
fn universe_preview_preserves_locations_disabled_entries_and_individual_errors() {
    use metatorio_core::{
        IdWithQuality, ItemFuelMechanic, LocalLocatedMechanic, LocatedMechanic, Location, Mechanic,
        RecipeMechanic, TransferLocatedMechanic,
    };
    use metatorio_runtime::document::{ProjectDocument, UniverseDocument, UniverseMechanicEntry};
    use metatorio_runtime::id::MechanicId;
    let store=PrototypeStore::load(&json!({"item":{"coal":{"stack_size":50,"weight":1000,"fuel_value":"1MJ","fuel_categories":["chemical"]}}})).unwrap();
    let item = IdWithQuality::new("coal", "normal");
    let universe = UniverseDocument {
        mechanics: vec![
            UniverseMechanicEntry {
                id: MechanicId(1),
                enabled: false,
                mechanic: LocatedMechanic::Local(LocalLocatedMechanic {
                    location: Location::planet("nauvis"),
                    mechanic: Mechanic::ItemFuel(ItemFuelMechanic { item: item.clone() }),
                }),
            },
            UniverseMechanicEntry {
                id: MechanicId(2),
                enabled: true,
                mechanic: LocatedMechanic::Transfer(TransferLocatedMechanic {
                    location: Location::planet("nauvis"),
                    item,
                }),
            },
            UniverseMechanicEntry {
                id: MechanicId(3),
                enabled: true,
                mechanic: LocatedMechanic::Local(LocalLocatedMechanic {
                    location: Location::Space,
                    mechanic: Mechanic::Recipe(RecipeMechanic::default()),
                }),
            },
        ],
        ..Default::default()
    };
    let preview = metatorio_runtime::universe::preview_universe(
        &store,
        &ProjectDocument::default(),
        &universe,
        &Default::default(),
    );
    assert_eq!(preview.mechanics.len(), 3);
    assert!(!preview.mechanics[0].flows.is_empty());
    assert!(
        preview.mechanics[0]
            .flows
            .iter()
            .all(|f| f.flow.location == Location::planet("nauvis"))
    );
    let transfer = &preview.mechanics[1].flows;
    assert_eq!(transfer.len(), 3);
    assert!(
        transfer
            .iter()
            .any(|f| f.flow.location == Location::Space && f.amount == 1.0)
    );
    assert!(transfer.iter().any(|f| f.amount == -1000.0));
    assert!(preview.mechanics[2].error.is_some());
    assert!(preview.mechanics[2].flows.is_empty());
}

#[test]
fn universe_solar_preview_matches_the_same_location_expansion() {
    use metatorio_core::{
        DualVar, IdWithQuality, LocalLocatedMechanic, LocatedMechanic, Location, Mechanic,
        SolarMechanic,
    };
    use metatorio_runtime::document::{ProjectDocument, UniverseDocument, UniverseMechanicEntry};
    use metatorio_runtime::id::MechanicId;
    let store = PrototypeStore::load(&json!({
        "planet":{"home":{"surface_properties":{"solar-power":100,"day-night-cycle":25200}}},
        "solar-panel":{"panel":{"production":"60kW"}},
        "accumulator":{"storage":{"energy_source":{"type":"electric","buffer_capacity":"5MJ"}}}
    }))
    .unwrap();
    let mechanics = [Location::planet("home"), Location::Space]
        .into_iter()
        .enumerate()
        .map(|(i, location)| UniverseMechanicEntry {
            id: MechanicId(i as u64 + 1),
            enabled: true,
            mechanic: LocatedMechanic::Local(LocalLocatedMechanic {
                location,
                mechanic: Mechanic::Solar(SolarMechanic {
                    solar_panel: IdWithQuality::from("panel"),
                    accumulator: IdWithQuality::from("storage"),
                }),
            }),
        })
        .collect();
    let preview = metatorio_runtime::universe::preview_universe(
        &store,
        &ProjectDocument::default(),
        &UniverseDocument {
            mechanics,
            ..Default::default()
        },
        &Default::default(),
    );
    for (i, entry) in preview.mechanics.iter().enumerate() {
        let balance = entry.solar_balance.as_ref().unwrap();
        assert_eq!(balance.continuous_daylight, i == 1);
        let power = entry
            .flows
            .iter()
            .find(|f| f.flow.flow == DualVar::Electricity)
            .unwrap()
            .amount;
        assert!((balance.average_power - power).abs() < 1e-6);
    }
}
