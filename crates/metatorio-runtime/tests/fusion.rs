use metatorio_core::*;
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{
    auto_plan::{EnumerateOptions, enumerate_all},
    message::AppMessage,
    solve::{Runtime, SolveStatus, make_game_state},
};
use serde_json::{Value, json};
fn dump() -> Value {
    json!({
     "quality":{"normal":{"level":0,"next":"rare","order":"a"},"rare":{"level":2,"default_multiplier":1.6,"order":"b"}},
     "fluid":{"cold":{"default_temperature":-150,"max_temperature":-150},"hot":{"default_temperature":180},"plasma":{"default_temperature":1000000,"max_temperature":10000000,"heat_capacity":"25J","fuel_value":"100MJ"}},
     "item":{"cell":{"fuel_value":"40GJ","fuel_categories":["fusion"]},"coal":{"fuel_value":"4MJ","fuel_categories":["chemical"]},"junk":{}},
     "fusion-reactor":{"r":{"max_fluid_usage":4.0/60.0,"power_input":"10MW","heating_energy":"1MW","collision_box":[[-3,-3],[3,3]],
        "burner":{"fuel_categories":["fusion"],"effectivity":1},"energy_source":{"type":"electric","usage_priority":"primary-input"},
        "input_fluid_box":{"filter":"cold"},"output_fluid_box":{"filter":"plasma"}}},
     "fusion-generator":{"g":{"max_fluid_usage":2.0/60.0,"heating_energy":"2MW","collision_box":[[-1.5,-2.5],[1.5,2.5]],
        "input_fluid_box":{"filter":"plasma"},"output_fluid_box":{"filter":"hot"},"energy_source":{"type":"electric","usage_priority":"secondary-output","output_flow_limit":"50MW"}}},
     "reactor":{"n":{"consumption":"40MW","neighbour_bonus":1,"energy_source":{"type":"burner","fuel_categories":["fusion"]},"heat_buffer":{"max_transfer":"10GW","specific_heat":"10MJ","max_temperature":1000}}}
    })
}

#[test]
fn legacy_import_keeps_explicit_neighbours_and_inherits_when_missing() {
    let old = json!({"proj":{},"factories":[{"mechanics":[{
        "type":"factorio:reactor","instances":[
            {"reactor":["n",0]},
            {"reactor":["n",0],"neighbours":0},
            {"reactor":["n",0],"neighbours":5}
        ]
    }]}]});
    let migrated =
        metatorio_runtime::migrate::migrate_old_project(&old, None, &["normal".into()], None)
            .unwrap();
    let doc: metatorio_runtime::document::AppDocument = serde_json::from_value(migrated).unwrap();
    let entries = &doc.projects[0].factories[0].mechanics;
    for (entry, expected) in entries.iter().zip([None, Some(0), Some(5)]) {
        let Mechanic::Reactor(reactor) = &entry.mechanic else {
            panic!("expected reactor")
        };
        assert_eq!(reactor.neighbours, expected);
    }
    assert_eq!(entries.len(), 3);
}

fn send(rt: &mut Runtime, v: Value) {
    rt.dispatch(serde_json::from_value::<AppMessage>(v).unwrap())
        .unwrap();
}
fn setup() -> Runtime {
    let mut rt = Runtime::new();
    rt.install_context("test".into(), PrototypeStore::load(&dump()).unwrap());
    rt.set_active_context(Some("test".into()));
    send(
        &mut rt,
        json!({"scope":"application","action":{"new-project":{"name":"fusion"}}}),
    );
    rt.state.document.projects[0].settings.all_accessible = true;
    if metatorio_solver::LpSolver::Soplex.is_available() {
        rt.state.document.projects[0].settings.solver = metatorio_solver::LpSolver::Soplex;
    }
    rt
}
#[test]
fn hidden_neighbour_estimates_validate_persist_undo_and_invalidate_snapshots() {
    let mut rt = setup();
    let p = rt.state.document.projects[0].id;
    let f = rt.state.document.projects[0].factories[0].id;
    let snapshot = rt.solve_snapshot_inputs(p, f).unwrap();
    send(
        &mut rt,
        json!({"scope":"project","action":{"project":p,"action":{"set-reactor-neighbours":{"estimates":{"r":5,"n":1}}}}}),
    );
    assert!(!rt.document_matches(&snapshot));
    let settings = &rt.state.document.projects[0].settings;
    assert_eq!(settings.reactor_neighbours["r"], 5);
    let wire = serde_json::to_string(&rt.state.document).unwrap();
    let restored: metatorio_runtime::document::AppDocument = serde_json::from_str(&wire).unwrap();
    assert_eq!(restored.projects[0].settings.reactor_neighbours["n"], 1);
    send(&mut rt, json!({"scope":"history","action":"undo"}));
    assert!(
        rt.state.document.projects[0]
            .settings
            .reactor_neighbours
            .is_empty()
    );
    send(&mut rt, json!({"scope":"history","action":"redo"}));
    assert_eq!(
        rt.state.document.projects[0].settings.reactor_neighbours["r"],
        5
    );
    let bad=serde_json::from_value(json!({"scope":"project","action":{"project":p,"action":{"set-reactor-neighbours":{"estimates":{"g":7}}}}})).unwrap();
    assert!(rt.dispatch(bad).is_err());
    assert_eq!(
        rt.state.document.projects[0].settings.reactor_neighbours["r"],
        5
    );
}
#[test]
fn enumeration_uses_overridden_temperatures_and_major_quality() {
    let mut rt = setup();
    let p = &mut rt.state.document.projects[0];
    p.settings.reactor_neighbours.insert("r".into(), 5);
    let store = PrototypeStore::load(&dump()).unwrap();
    let game = make_game_state(&store, p);
    let ctx = Context::new(&store, &game);
    let options = EnumerateOptions {
        alternative_count: 1,
        machine_preferences: vec![],
        enumerate_modules: vec![],
        enumerate_beacons: vec![],
        quality_limit: 1,
        major_quality: 1,
        planet: None,
        surface: None,
        accessibility: None,
    };
    let all = enumerate_all(&store, &ctx, &options);
    assert!(all.iter().any(|m|matches!(m,Mechanic::FusionReactor(r) if r.neighbours.is_none()&&r.reactor.quality=="rare")));
    assert!(all.iter().any(|m|matches!(m,Mechanic::FusionGenerator(g) if g.temperature==Some(6000000)&&g.generator.quality=="rare")));
    assert!(
        all.iter()
            .any(|m| matches!(m,Mechanic::Reactor(r) if r.neighbours.is_none()))
    );
}
#[test]
fn automatic_planning_and_writeback_can_supply_net_fusion_electricity() {
    let mut rt = setup();
    let p = rt.state.document.projects[0].id;
    let f = rt.state.document.projects[0].factories[0].id;
    let mut action = |a| {
        send(
            &mut rt,
            json!({"scope":"factory","action":{"project":p,"factory":f,"action":a}}),
        )
    };
    action(json!({"set-strict-sink":{"strict":false}}));
    for flow in [
        json!({"Item":{"id":"cell","quality":"normal"}}),
        json!({"Fluid":{"name":"cold","temperature":[-150,-150]}}),
    ] {
        action(json!({"external-input":{"add":{"input":{"flow":flow,"penalty":1}}}}));
    }
    action(json!({"target":{"add":{"target":{"flow":"Electricity","amount":50000000.0}}}}));
    let solved = rt.auto_plan(p, f).unwrap();
    assert!(
        matches!(solved.status, SolveStatus::Solved { .. }),
        "{solved:?}"
    );
    let mechanics = &rt.state.document.projects[0].factories[0].mechanics;
    assert!(
        mechanics
            .iter()
            .any(|e| matches!(e.mechanic, Mechanic::FusionReactor(_)))
    );
    assert!(
        mechanics
            .iter()
            .any(|e| matches!(e.mechanic, Mechanic::FusionGenerator(_)))
    );
    assert!(matches!(
        rt.solve_factory(p, f).unwrap().status,
        SolveStatus::Solved { .. }
    ));
}
#[test]
fn both_new_mechanisms_are_editable_in_factories_and_universe_locations() {
    let mut rt = setup();
    let p = rt.state.document.projects[0].id;
    let f = rt.state.document.projects[0].factories[0].id;
    for kind in ["fusion-reactor", "fusion-generator"] {
        send(
            &mut rt,
            json!({"scope":"factory","action":{"project":p,"factory":f,"action":{"mechanic-list":{"add":{"kind":kind}}}}}),
        );
    }
    let ids = rt.state.document.projects[0].factories[0]
        .mechanics
        .iter()
        .map(|e| e.id)
        .collect::<Vec<_>>();
    send(
        &mut rt,
        json!({"scope":"factory","action":{"project":p,"factory":f,"action":{"mechanic":{"mechanic":ids[0],"action":{"fusion-reactor":{"set-reactor":{"reactor":{"id":"r","quality":"rare"}}}}}}}}),
    );
    send(
        &mut rt,
        json!({"scope":"factory","action":{"project":p,"factory":f,"action":{"mechanic":{"mechanic":ids[1],"action":{"fusion-generator":{"set-generator":{"generator":{"id":"g","quality":"normal"}}}}}}}}),
    );
    send(
        &mut rt,
        json!({"scope":"project","action":{"project":p,"action":{"add-universe":{"name":"fusion universe"}}}}),
    );
    let u = rt.state.document.projects[0].universes[0].id;
    send(
        &mut rt,
        json!({"scope":"universe","action":{"project":p,"universe":u,"action":{"add-local-mechanic":{"location":{"kind":"space"},"kind":"fusion-reactor"}}}}),
    );
    let id = rt.state.document.projects[0].universes[0].mechanics[0].id;
    send(
        &mut rt,
        json!({"scope":"universe","action":{"project":p,"universe":u,"action":{"mechanic":{"mechanic":id,"action":{"fusion-reactor":{"set-reactor":{"reactor":{"id":"r","quality":"normal"}}}}}}}}),
    );
}

#[test]
fn universe_auto_plan_uses_fusion_and_inherits_project_estimates() {
    let mut rt = setup();
    let p = rt.state.document.projects[0].id;
    send(
        &mut rt,
        json!({"scope":"project","action":{"project":p,"action":{"set-reactor-neighbours":{"estimates":{"r":5}}}}}),
    );
    send(
        &mut rt,
        json!({"scope":"project","action":{"project":p,"action":{"add-universe":{"name":"space fusion"}}}}),
    );
    let u = rt.state.document.projects[0].universes[0].id;
    let mut action = |a| {
        send(
            &mut rt,
            json!({"scope":"universe","action":{"project":p,"universe":u,"action":a}}),
        )
    };
    action(json!({"set-strict-sink":{"strict":false}}));
    for flow in [
        json!({"Item":{"id":"cell","quality":"normal"}}),
        json!({"Fluid":{"name":"cold","temperature":[-150,-150]}}),
    ] {
        action(
            json!({"add-external-input":{"flow":{"location":{"kind":"space"},"flow":flow},"penalty":1}}),
        );
    }
    action(
        json!({"add-target":{"flow":{"location":{"kind":"space"},"flow":"Electricity"},"amount":50000000.0}}),
    );
    let solved = rt.auto_plan_universe(p, u).unwrap();
    assert!(
        matches!(
            solved.status,
            metatorio_runtime::universe::UniverseSolveStatus::Solved { .. }
        ),
        "{solved:?}"
    );
    let mechanics = &rt.state.document.projects[0].universes[0].mechanics;
    assert!(mechanics.iter().any(|e|matches!(&e.mechanic,LocatedMechanic::Local(m) if matches!(m.mechanic,Mechanic::FusionReactor(_)))));
    assert!(mechanics.iter().any(|e|matches!(&e.mechanic,LocatedMechanic::Local(m) if matches!(m.mechanic,Mechanic::FusionGenerator(_)))));
}
