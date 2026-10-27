use metatorio_core::{Accessible, LocatedMechanic};
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{message::AppMessage, solve::Runtime, universe::UniverseSolveStatus};
use serde_json::{Value, json};

fn send(runtime: &mut Runtime, message: Value) {
    runtime
        .dispatch(serde_json::from_value::<AppMessage>(message).unwrap())
        .unwrap();
}

fn setup(
    unlocked: bool,
    all_accessible: bool,
    local_production: bool,
    space_input: bool,
    location: Value,
) -> Runtime {
    let store = PrototypeStore::load(&json!({
        "planet":{"nauvis":{},"fulgora":{}},
        "quality":{"normal":{"level":0}},
        "item":{"plate":{"stack_size":50,"weight":1000},"science":{"stack_size":200,"weight":1000}},
        "technology":{"survey-other-world":{"unit":{"count":1,"time":1,"ingredients":[["science",1]]},"effects":[{"type":"unlock-space-location","space_location":"fulgora"}]}},
        "assembling-machine":{"assembler":{"crafting_categories":["crafting"],"crafting_speed":1,"energy_source":{"type":"void"},"collision_box":[[-0.5,-0.5],[0.5,0.5]]}},
        "recipe":if local_production {json!({"plate":{"enabled":true,"energy_required":1,"ingredients":[],"results":[{"type":"item","name":"plate","amount":1}]}})} else {json!({})}
    })).unwrap();
    let mut runtime = Runtime::new();
    runtime.install_context("test".into(), store);
    runtime.set_active_context(Some("test".into()));
    send(
        &mut runtime,
        json!({"scope":"application","action":{"new-project":{"name":"test"}}}),
    );
    let project = &mut runtime.state.document.projects[0];
    project.settings.all_accessible = all_accessible;
    project.settings.milestones = [
        (Accessible::Tech("survey-other-world".into()), unlocked),
        (Accessible::Item("plate".into()), true),
        (Accessible::Entity("assembler".into()), true),
        (Accessible::Recipe("plate".into()), local_production),
    ]
    .into_iter()
    .map(|(node, unlocked)| metatorio_runtime::document::Milestone { node, unlocked })
    .collect();
    if metatorio_solver::LpSolver::Soplex.is_available() {
        project.settings.solver = metatorio_solver::LpSolver::Soplex;
    }
    let p = project.id;
    send(
        &mut runtime,
        json!({"scope":"project","action":{"project":p,"action":{"add-universe":{"name":"test"}}}}),
    );
    let u = runtime.state.document.projects[0].universes[0].id;
    let flow = json!({"Item":{"id":"plate","quality":"normal"}});
    if space_input {
        send(
            &mut runtime,
            json!({"scope":"universe","action":{"project":p,"universe":u,"action":{"add-external-input":{"flow":{"location":{"kind":"space"},"flow":flow},"penalty":0}}}}),
        );
    }
    send(
        &mut runtime,
        json!({"scope":"universe","action":{"project":p,"universe":u,"action":{"add-target":{"flow":{"location":location,"flow":flow},"amount":1}}}}),
    );
    runtime
}

#[test]
fn locked_planet_gets_neither_local_production_nor_orbital_delivery() {
    for local in [false, true] {
        let mut runtime = setup(
            false,
            false,
            local,
            true,
            json!({"kind":"planet","name":"fulgora"}),
        );
        let project = &runtime.state.document.projects[0];
        let (p, u) = (project.id, project.universes[0].id);
        let error = runtime.auto_plan_universe(p, u).unwrap_err();
        assert!(
            matches!(error,metatorio_runtime::RuntimeError::Diagnostic(ref diagnostic) if diagnostic.code=="auto-plan-not-solved"),
            "expected missing supply on locked planet, got {error:?}"
        );
        assert!(
            runtime.state.document.projects[0].universes[0]
                .mechanics
                .is_empty(),
            "failed plan must not write back"
        );
    }
}

#[test]
fn unlock_effect_and_all_accessible_allow_the_planet() {
    for (unlocked, all_accessible, local) in [
        (true, false, true),
        (true, false, false),
        (false, true, false),
    ] {
        let mut runtime = setup(
            unlocked,
            all_accessible,
            local,
            !local,
            json!({"kind":"planet","name":"fulgora"}),
        );
        let project = &runtime.state.document.projects[0];
        let (p, u) = (project.id, project.universes[0].id);
        let result = runtime.auto_plan_universe(p, u).unwrap();
        assert!(matches!(result.status, UniverseSolveStatus::Solved { .. }));
        let entries = &runtime.state.document.projects[0].universes[0].mechanics;
        assert!(entries.iter().any(|entry| match &entry.mechanic {
            LocatedMechanic::Local(mechanic) if local =>
                mechanic.location == metatorio_core::Location::planet("fulgora"),
            LocatedMechanic::OrbitalDrop(_) => !local,
            _ => false,
        }));
    }
}

#[test]
fn starting_planet_and_space_still_enumerate_without_other_planet_unlocks() {
    for location in [
        json!({"kind":"planet","name":"nauvis"}),
        json!({"kind":"space"}),
    ] {
        let mut runtime = setup(false, false, true, false, location);
        let project = &runtime.state.document.projects[0];
        let (p, u) = (project.id, project.universes[0].id);
        assert!(matches!(
            runtime.auto_plan_universe(p, u).unwrap().status,
            UniverseSolveStatus::Solved { .. }
        ));
    }
}
