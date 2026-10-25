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
    assert_eq!(up[0].1, 2.0);
    assert_eq!(down[0].0.len(), 2);
    assert_eq!(down[0].0[&space], -1.0);
    assert_eq!(down[0].0[&local], 1.0);
    assert_eq!(down[0].1, 1.28);
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
    assert!((cost - 328.0).abs() < 1e-8);
    assert_eq!(mechanics.len(), 2);
    assert_eq!(
        mechanics
            .iter()
            .find(|m| m.mechanic == MechanicId(10))
            .unwrap()
            .cost,
        2.0
    );
    assert_eq!(
        mechanics
            .iter()
            .find(|m| m.mechanic == MechanicId(11))
            .unwrap()
            .cost,
        1.28
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
    assert!((cost - 328.0).abs() < 1e-8);
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

#[test]
fn default_prices_match_50kg_and_50_item_stack_examples() {
    let store =
        PrototypeStore::load(&json!({"item":{"ore":{"stack_size":50,"weight":50000}}})).unwrap();
    let up = expand_located_mechanic(
        &store,
        &Default::default(),
        &LocatedMechanic::RocketExport(cargo()),
    )
    .unwrap();
    let down = expand_located_mechanic(
        &store,
        &Default::default(),
        &LocatedMechanic::OrbitalDrop(cargo()),
    )
    .unwrap();
    assert_eq!(up[0].1, 50.0);
    assert_eq!(down[0].1, 1.28);
}

#[test]
fn project_price_overrides_roundtrip_and_reach_joint_planning() {
    use metatorio_runtime::document::OrbitalTransportCosts;
    let default: OrbitalTransportCosts = serde_json::from_value(json!({})).unwrap();
    assert_eq!(default, OrbitalTransportCosts::default());
    let partial: OrbitalTransportCosts =
        serde_json::from_value(json!({"rocket_export_per_kg":3})).unwrap();
    assert_eq!(partial.orbital_drop_per_stack, 64.0);
    let mut r = runtime();
    let reply = r.dispatch(serde_json::from_value(json!({"scope":"project","action":{"project":1,"action":{"set-orbital-transport-costs":{"costs":{"rocket_export_per_kg":3,"orbital_drop_per_stack":100}}}}})).unwrap()).unwrap();
    assert!(reply.changed);
    assert!(reply.commands.iter().any(|c| matches!(
        c,
        metatorio_runtime::RuntimeCommand::RecomputeUniverse { .. }
    )));
    assert_eq!(
        r.state.document.projects[0]
            .settings
            .orbital_transport_costs
            .orbital_drop_spoil_penalty,
        216000.0
    );
    let document: AppDocument =
        serde_json::from_value(serde_json::to_value(&r.state.document).unwrap()).unwrap();
    assert_eq!(document, r.state.document);
    let result = r.auto_plan_universe(ProjectId(1), UniverseId(3)).unwrap();
    let UniverseSolveStatus::Solved { cost, .. } = result.status else {
        panic!("{result:?}")
    };
    assert!((cost - 800.0).abs() < 1e-8);
    let document = r.state.document.clone();
    assert!(r.dispatch(serde_json::from_value(json!({"scope":"project","action":{"project":1,"action":{"set-orbital-transport-costs":{"costs":{"orbital_drop_spoil_penalty":-1}}}}})).unwrap()).is_err());
    assert_eq!(r.state.document, document);
    // Undo planning, then the price edit; coefficients use the normal history path.
    for _ in 0..2 {
        r.dispatch(serde_json::from_value(json!({"scope":"history","action":"undo"})).unwrap())
            .unwrap();
    }
    assert_eq!(
        r.state.document.projects[0]
            .settings
            .orbital_transport_costs,
        default
    );
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        for costs in [
            OrbitalTransportCosts {
                rocket_export_per_kg: bad,
                ..default
            },
            OrbitalTransportCosts {
                orbital_drop_per_stack: bad,
                ..default
            },
            OrbitalTransportCosts {
                orbital_drop_spoil_penalty: bad,
                ..default
            },
        ] {
            assert!(
                metatorio_runtime::universe::expand_located_mechanic_with_costs(
                    &store(),
                    &Default::default(),
                    &LocatedMechanic::RocketExport(cargo()),
                    &costs
                )
                .is_err()
            );
        }
    }
}

#[test]
fn positive_export_price_avoids_surplus_without_strict_space_balance() {
    let mut r = runtime();
    let universe = &mut r.state.document.projects[0].universes[0];
    universe.targets[0].flow.location = Location::planet("a");
    universe.strict_sink = false;
    universe
        .mechanics
        .push(metatorio_runtime::document::UniverseMechanicEntry {
            id: MechanicId(10),
            enabled: true,
            mechanic: LocatedMechanic::RocketExport(cargo()),
        });
    let result = r.solve_universe(ProjectId(1), UniverseId(3)).unwrap();
    let UniverseSolveStatus::Solved {
        cost, mechanics, ..
    } = result.status
    else {
        panic!("{result:?}")
    };
    assert_eq!(cost, 0.0);
    assert!(mechanics.iter().all(|m| m.amount == 0.0));
    assert!(!r.state.document.projects[0].universes[0].strict_sink);
}

#[test]
fn drop_spoil_penalty_uses_ticks_and_quality_but_exports_are_unaffected() {
    use metatorio_runtime::document::OrbitalTransportCosts;
    use metatorio_runtime::universe::expand_located_mechanic_with_costs;
    let store = PrototypeStore::load(&json!({
        "item":{"food":{"stack_size":50,"weight":50000,"spoil_ticks":3600,"spoil_result":"waste"},"waste":{"stack_size":50}},
        "quality":{"normal":{"level":0,"next":"rare"},"rare":{"level":2,"spoil_ticks_multiplier":2}}
    })).unwrap();
    for (quality, expected) in [("normal", 61.28), ("rare", 31.28)] {
        let cargo = OrbitalCargoMechanic {
            location: Location::planet("a"),
            item: IdWithQuality::new("food", quality),
        };
        let down = expand_located_mechanic(
            &store,
            &Default::default(),
            &LocatedMechanic::OrbitalDrop(cargo.clone()),
        )
        .unwrap();
        assert!((down[0].1 - expected).abs() < 1e-10);
        let up = expand_located_mechanic(
            &store,
            &Default::default(),
            &LocatedMechanic::RocketExport(cargo.clone()),
        )
        .unwrap();
        assert_eq!(up[0].1, 50.0);
        let costs = OrbitalTransportCosts {
            orbital_drop_spoil_penalty: 0.0,
            ..Default::default()
        };
        let down = expand_located_mechanic_with_costs(
            &store,
            &Default::default(),
            &LocatedMechanic::OrbitalDrop(cargo),
            &costs,
        )
        .unwrap();
        assert_eq!(down[0].1, 1.28);
    }
}

#[test]
fn spoil_penalty_reaches_joint_planning_and_written_mechanic_cost() {
    let mut r = runtime();
    r.install_context("cargo".into(),PrototypeStore::load(&json!({
        "item":{"ore":{"stack_size":50,"weight":2000,"spoil_ticks":3600,"spoil_result":"waste"},"waste":{"stack_size":50}},
        "planet":{"a":{},"b":{}},
        "quality":{"normal":{"level":0,"next":"rare"},"rare":{"level":2,"spoil_ticks_multiplier":2}}
    })).unwrap());
    let result = r.auto_plan_universe(ProjectId(1), UniverseId(3)).unwrap();
    let UniverseSolveStatus::Solved {
        cost, mechanics, ..
    } = result.status
    else {
        panic!("{result:?}")
    };
    assert!((cost - 3328.0).abs() < 1e-8);
    let entries = &r.state.document.projects[0].universes[0].mechanics;
    let drop = entries
        .iter()
        .find(|e| matches!(e.mechanic, LocatedMechanic::OrbitalDrop(_)))
        .unwrap();
    assert!(
        (mechanics
            .iter()
            .find(|m| m.mechanic == drop.id)
            .unwrap()
            .cost
            - 31.28)
            .abs()
            < 1e-10
    );
}
