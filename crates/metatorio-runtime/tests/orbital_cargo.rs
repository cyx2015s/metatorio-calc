use metatorio_core::{
    DualVar, IdWithQuality, LocatedDualVar, LocatedMechanic, Location, OrbitalCargoMechanic,
};
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{
    MechanicId, ProjectId, Runtime, UniverseId,
    document::AppDocument,
    universe::{UniverseSolveStatus, expand_located_mechanic},
};
use serde_json::json;

fn solver() -> metatorio_solver::LpSolver {
    #[cfg(feature = "soplex")]
    {
        metatorio_solver::LpSolver::Soplex
    }
    #[cfg(not(feature = "soplex"))]
    {
        metatorio_solver::LpSolver::Microlp
    }
}
fn cargo() -> OrbitalCargoMechanic {
    OrbitalCargoMechanic {
        location: Location::planet("a"),
        item: IdWithQuality::new("ore", "rare"),
    }
}
fn store() -> PrototypeStore {
    PrototypeStore::load(&json!({"item":{"ore":{"stack_size":50,"weight":2000}},"planet":{"a":{},"b":{}},"quality":{"normal":{"level":0,"next":"rare"},"rare":{"level":2}}})).unwrap()
}
fn runtime() -> Runtime {
    let mut doc:AppDocument=serde_json::from_value(json!({"projects":[{"id":1,"settings":{"all_accessible":true,"quality_limit":"rare"},"planning":{"preserve_existing_mechanics":false},"universes":[{"id":3,"strict_source":true,"targets":[{"id":4,"flow":{"location":{"kind":"planet","name":"b"},"flow":{"Item":{"id":"ore","quality":"rare"}}},"amount":100}],"external_inputs":[{"id":5,"flow":{"location":{"kind":"planet","name":"a"},"flow":{"Item":{"id":"ore","quality":"rare"}}},"penalty":0},{"id":6,"flow":{"location":{"kind":"planet","name":"a"},"flow":"RocketWeightCapacity"},"penalty":0}]}]}]})).unwrap();
    doc.projects[0].settings.solver = solver();
    let mut r = Runtime::from_document(doc);
    r.install_context("cargo".into(), store());
    r.set_active_context(Some("cargo".into()));
    r
}
#[test]
fn opposite_directions_preserve_quality_and_charge_weight_or_stacks() {
    let store = store();
    let game = Default::default();
    let cargo = cargo();
    let up = expand_located_mechanic(&store, &game, &LocatedMechanic::RocketExport(cargo.clone()))
        .unwrap();
    let down = expand_located_mechanic(&store, &game, &LocatedMechanic::OrbitalDrop(cargo.clone()))
        .unwrap();
    let local = LocatedDualVar::new(cargo.location.clone(), DualVar::Item(cargo.item.clone()));
    let space = LocatedDualVar::new(Location::Space, DualVar::Item(cargo.item.clone()));
    assert_eq!(up[0].0[&local], -1.0);
    assert_eq!(up[0].0[&space], 1.0);
    assert_eq!(
        up[0].0[&LocatedDualVar::new(cargo.location.clone(), DualVar::RocketWeightCapacity)],
        -2000.0
    );
    assert_eq!(up[0].1, 0.0);
    assert_eq!(down[0].0.len(), 2);
    assert_eq!(down[0].0[&space], -1.0);
    assert_eq!(down[0].0[&local], 1.0);
    assert_eq!(down[0].1, 0.02);
    for transport in [LocatedMechanic::RocketExport, LocatedMechanic::OrbitalDrop] {
        assert!(
            expand_located_mechanic(
                &store,
                &game,
                &transport(OrbitalCargoMechanic {
                    location: Location::Space,
                    ..cargo.clone()
                })
            )
            .is_err()
        );
    }
    assert!(
        expand_located_mechanic(
            &store,
            &game,
            &LocatedMechanic::OrbitalDrop(OrbitalCargoMechanic {
                item: IdWithQuality::from("missing"),
                ..cargo
            })
        )
        .is_err()
    );
}
#[test]
fn no_hidden_delivery_and_disabled_drop_cannot_satisfy_target() {
    let mut r = runtime();
    r.state.document.projects[0].universes[0].mechanics =
        vec![metatorio_runtime::document::UniverseMechanicEntry {
            id: MechanicId(10),
            enabled: true,
            mechanic: LocatedMechanic::RocketExport(cargo()),
        }];
    assert!(matches!(
        r.solve_universe(ProjectId(1), UniverseId(3))
            .unwrap()
            .status,
        UniverseSolveStatus::NotSolved { .. }
    ));
    let mut destination = cargo();
    destination.location = Location::planet("b");
    r.state.document.projects[0].universes[0].mechanics.push(
        metatorio_runtime::document::UniverseMechanicEntry {
            id: MechanicId(11),
            enabled: true,
            mechanic: LocatedMechanic::OrbitalDrop(destination),
        },
    );
    let result = r.solve_universe(ProjectId(1), UniverseId(3)).unwrap();
    let UniverseSolveStatus::Solved {
        cost, mechanics, ..
    } = result.status
    else {
        panic!("{result:?}")
    };
    assert!((cost - 2.0).abs() < 1e-8);
    assert_eq!(mechanics.len(), 2);
    assert_eq!(
        mechanics
            .iter()
            .find(|m| m.mechanic == MechanicId(10))
            .unwrap()
            .cost,
        0.0
    );
    assert_eq!(
        mechanics
            .iter()
            .find(|m| m.mechanic == MechanicId(11))
            .unwrap()
            .cost,
        0.02
    );
    assert!(mechanics.iter().all(|m| (m.amount - 100.0).abs() < 1e-8));
    r.state.document.projects[0].universes[0].mechanics[1].enabled = false;
    assert!(matches!(
        r.solve_universe(ProjectId(1), UniverseId(3))
            .unwrap()
            .status,
        UniverseSolveStatus::NotSolved { .. }
    ));
}
#[test]
fn joint_planning_writes_both_directions_with_actual_delivery_cost() {
    let mut r = runtime();
    let result = r.auto_plan_universe(ProjectId(1), UniverseId(3)).unwrap();
    let UniverseSolveStatus::Solved { cost, .. } = result.status else {
        panic!("{result:?}")
    };
    assert!((cost - 2.0).abs() < 1e-8);
    let entries = &r.state.document.projects[0].universes[0].mechanics;
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e|matches!(&e.mechanic,LocatedMechanic::RocketExport(c) if c.location==Location::planet("a")&&c.item.quality=="rare")));
    assert!(entries.iter().any(|e|matches!(&e.mechanic,LocatedMechanic::OrbitalDrop(c) if c.location==Location::planet("b")&&c.item.quality=="rare")));
    assert!(r.state.history_status().can_undo);
}
#[test]
fn transport_commands_are_editable_clonable_and_undoable() {
    let mut r = runtime();
    let before = r.state.document.clone();
    let reply=r.dispatch(serde_json::from_value(json!({"scope":"universe","action":{"project":1,"universe":3,"action":{"add-orbital-drop":{"location":{"kind":"planet","name":"b"},"item":{"id":"ore","quality":"normal"}}}}})).unwrap()).unwrap();
    let id = reply.created.mechanics[0];
    for action in [
        json!({"set-transport-item":{"mechanic":id,"item":{"id":"ore","quality":"rare"}}}),
        json!({"clone-mechanic":{"mechanic":id}}),
    ] {
        r.dispatch(
            serde_json::from_value(
                json!({"scope":"universe","action":{"project":1,"universe":3,"action":action}}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(r.state.document.projects[0].universes[0].mechanics.len(), 2);
    for _ in 0..3 {
        r.dispatch(serde_json::from_value(json!({"scope":"history","action":"undo"})).unwrap())
            .unwrap();
    }
    assert_eq!(r.state.document, before);
    assert!(serde_json::from_value::<LocatedMechanic>(json!({"type":"transfer"})).is_err());
}
