use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{
    document::AppDocument,
    id::{FactoryId, ProjectId, UniverseId},
    message::AppMessage,
    solve::Runtime,
};
use serde_json::{Value, json};

fn setup(factory: Value, universe: Value) -> Runtime {
    let doc:AppDocument=serde_json::from_value(json!({"projects":[{"id":1,"settings":{"all_accessible":true,"solver":"microlp"},"planning":{"preserve_existing_mechanics":true},"factories":[factory],"universes":[universe]}]})).unwrap();
    let mut r = Runtime::from_document(doc);
    r.install_context("audit".into(),PrototypeStore::load(&json!({
  "item":{"ore":{"stack_size":100},"plate":{"stack_size":100}},
  "fluid":{"steam":{"default_temperature":15,"max_temperature":500,"heat_capacity":"200J"}},
  "planet":{"nauvis":{}},"quality":{"normal":{"level":0,"next":"rare"},"rare":{"level":2}},
  "recipe":{"smelt":{"category":"crafting","energy_required":1,"ingredients":[{"type":"item","name":"ore","amount":1}],"results":[{"type":"item","name":"plate","amount":1}]}},
  "assembling-machine":{"assembler":{"crafting_categories":["crafting"],"crafting_speed":1,"module_slots":2,"energy_usage":"100W","energy_source":{"type":"electric"},"collision_box":[[0,0],[1,1]]}}
 })).unwrap());
    r.set_active_context(Some("audit".into()));
    r
}
fn local() -> Value {
    json!({"type":"recipe","recipe":{"id":"smelt","quality":"normal"},"machine":{"id":"assembler","quality":"normal"}})
}
fn location() -> Value {
    json!({"kind":"planet","name":"nauvis"})
}
#[test]
fn expression_conversion_parity() {
    let narrow = json!({"Fluid":{"name":"steam","temperature":[165,165]}});
    let broad = json!({"Fluid":{"name":"steam","temperature":[15,500]}});
    let factory = json!({"id":2,"strict_source":true,"external_inputs":[{"id":8,"flow":narrow,"penalty":0}],"target_expressions":[{"id":9,"constant":1,"terms":[{"id":10,"flow":broad,"coefficient":1}]}]});
    let universe = json!({"id":3,"strict_source":true,"external_inputs":[{"id":18,"flow":{"location":location(),"flow":narrow},"penalty":0}],"target_expressions":[{"id":19,"constant":1,"terms":[{"id":20,"flow":{"location":location(),"flow":broad},"coefficient":1}]}]});
    let mut r = setup(factory, universe);
    let a = r.solve_factory(ProjectId(1), FactoryId(2)).unwrap();
    let b = r.solve_universe(ProjectId(1), UniverseId(3)).unwrap();
    println!("EXPRESSION ordinary={:?} universe={:?}", a.status, b.status);
    assert!(matches!(
        a.status,
        metatorio_runtime::SolveStatus::Solved { .. }
    ));
    assert!(matches!(
        b.status,
        metatorio_runtime::universe::UniverseSolveStatus::Solved { .. }
    ));
    assert!(matches!(
        r.auto_plan(ProjectId(1), FactoryId(2)).unwrap().status,
        metatorio_runtime::SolveStatus::Solved { .. }
    ));
    assert!(matches!(
        r.auto_plan_universe(ProjectId(1), UniverseId(3))
            .unwrap()
            .status,
        metatorio_runtime::universe::UniverseSolveStatus::Solved { .. }
    ));
}

#[test]
fn equivalent_replacement_plan_preserves_original_ids_without_history_noise() {
    let item = json!({"Item":{"id":"plate","quality":"normal"}});
    let mut runtime = setup(
        json!({"id":2}),
        json!({"id":3,"strict_source":true,"mechanics":[{"id":20,"mechanic":{"type":"local","location":location(),"mechanic":local()}}],"targets":[{"id":21,"flow":{"location":location(),"flow":item},"amount":1}],"external_inputs":[{"id":22,"flow":{"location":location(),"flow":{"Item":{"id":"ore","quality":"normal"}}},"penalty":0},{"id":23,"flow":{"location":location(),"flow":"Electricity"},"penalty":0}]}),
    );
    runtime.state.document.projects[0]
        .planning
        .preserve_existing_mechanics = false;
    let before = runtime.state.document.clone();
    runtime
        .auto_plan_universe(ProjectId(1), UniverseId(3))
        .unwrap();
    assert_eq!(runtime.state.document, before);
    assert_eq!(runtime.state.revision, 0);
    assert!(!runtime.state.history_status().can_undo);
}
#[test]
fn edit_side_effects_parity() {
    let mut r = setup(
        json!({"id":2,"mechanics":[{"id":10,"mechanic":local()}]}),
        json!({"id":3,"mechanics":[{"id":20,"mechanic":{"type":"local","location":location(),"mechanic":local()}}]}),
    );
    // Blank machine in both documents, then set it through the normal command.
    if let metatorio_core::Mechanic::Recipe(m) =
        &mut r.state.document.projects[0].factories[0].mechanics[0].mechanic
    {
        m.machine.id.clear();
    }
    if let metatorio_core::LocatedMechanic::Local(m) =
        &mut r.state.document.projects[0].universes[0].mechanics[0].mechanic
    {
        if let metatorio_core::Mechanic::Recipe(m) = &mut m.mechanic {
            m.machine.id.clear();
        }
    }
    let action =
        json!({"recipe":{"set-machine":{"machine":{"id":"assembler","quality":"normal"}}}});
    let a=r.dispatch(serde_json::from_value::<AppMessage>(json!({"scope":"factory","action":{"project":1,"factory":2,"action":{"mechanic":{"mechanic":10,"action":action}}}})).unwrap()).unwrap();
    let b=r.dispatch(serde_json::from_value::<AppMessage>(json!({"scope":"universe","action":{"project":1,"universe":3,"action":{"mechanic":{"mechanic":20,"action":action}}}})).unwrap()).unwrap();
    println!("EDIT ordinary={:?} universe={:?}", a.commands, b.commands);
    assert!(format!("{:?}", a.commands).contains("ClampModules"));
    assert!(format!("{:?}", b.commands).contains("RepairUniverseMechanic"));
    assert!(format!("{:?}", b.commands).contains("RecomputeUniverse"));
}
#[test]
fn auto_plan_commit_parity() {
    let item = json!({"Item":{"id":"plate","quality":"normal"}});
    let universe = json!({"id":3,"strict_source":false,"mechanics":[{"id":20,"enabled":false,"mechanic":{"type":"local","location":location(),"mechanic":local()}}],"targets":[{"id":30,"flow":{"location":location(),"flow":item},"amount":1}],"external_inputs":[{"id":31,"flow":{"location":location(),"flow":{"Item":{"id":"ore","quality":"normal"}}},"penalty":0},{"id":32,"flow":{"location":location(),"flow":"Electricity"},"penalty":0}]});
    let mut r = setup(json!({"id":2}), universe);
    let rev = r.state.revision;
    let before = r.state.history_status();
    let result = r.auto_plan_universe(ProjectId(1), UniverseId(3)).unwrap();
    let u = &r.state.document.projects[0].universes[0];
    println!(
        "AUTO revision {} -> {}; dirty={:?}; undo {} -> {}; strict={}; original_disabled_preserved={}; report_candidates={}; status={:?}",
        rev,
        r.state.revision,
        r.state.dirty_projects,
        before.can_undo,
        r.state.history_status().can_undo,
        u.strict_source,
        u.mechanics.iter().any(|e| e.id.0 == 20 && !e.enabled),
        result.report.candidates_total,
        result.status
    );
    assert!(!u.mechanics.is_empty());
    assert!(r.state.revision > rev);
    assert!(r.state.dirty_projects.contains(&ProjectId(1)));
    assert!(r.state.history_status().can_undo);
    assert!(u.strict_source);
    assert!(u.mechanics.iter().any(|e| e.id.0 == 20 && !e.enabled));
    let after = r.state.document.clone();
    let rev = r.state.revision;
    r.auto_plan_universe(ProjectId(1), UniverseId(3)).unwrap();
    assert_eq!(r.state.revision, rev);
    assert_eq!(r.state.document, after);
    let undone = r
        .dispatch(serde_json::from_value(json!({"scope":"history","action":"undo"})).unwrap())
        .unwrap();
    assert_eq!(undone.history.focus.unwrap().universe, Some(UniverseId(3)));
    assert_eq!(r.state.document.projects[0].universes[0].mechanics.len(), 1);
    assert!(!r.state.document.projects[0].universes[0].strict_source);
}
#[test]
fn invalid_location_parity() {
    let mut r = setup(
        json!({"id":2}),
        json!({"id":3,"mechanics":[{"id":20,"mechanic":{"type":"local","location":location(),"mechanic":local()}}]}),
    );
    let a=r.dispatch(serde_json::from_value::<AppMessage>(json!({"scope":"factory","action":{"project":1,"factory":2,"action":{"context":{"set-planet":{"planet":"not-a-planet"}}}}})).unwrap());
    let b=r.dispatch(serde_json::from_value::<AppMessage>(json!({"scope":"universe","action":{"project":1,"universe":3,"action":{"set-mechanic-location":{"mechanic":20,"location":{"kind":"planet","name":"not-a-planet"}}}}})).unwrap());
    println!(
        "VALIDATION ordinary_rejected={} universe_accepted={}",
        a.is_err(),
        b.is_ok()
    );
    assert!(a.is_err());
    assert!(b.is_err());
}

#[test]
fn rare_transfer_candidate_parity() {
    let plate = json!({"Item":{"id":"plate","quality":"rare"}});
    let universe = json!({"id":3,"strict_source":true,"mechanics":[{"id":20,"mechanic":{"type":"transfer","location":location(),"item":{"id":"plate","quality":"rare"}}}],"targets":[{"id":30,"flow":{"location":{"kind":"space"},"flow":plate},"amount":1}],"external_inputs":[{"id":31,"flow":{"location":location(),"flow":plate},"penalty":0},{"id":32,"flow":{"location":location(),"flow":"RocketWeightCapacity"},"penalty":0}]});
    let mut r = setup(json!({"id":2}), universe);
    let before = r.solve_universe(ProjectId(1), UniverseId(3)).unwrap();
    r.state.document.projects[0]
        .planning
        .preserve_existing_mechanics = false;
    r.state.document.projects[0].universes[0].mechanics.clear();
    let after = r.auto_plan_universe(ProjectId(1), UniverseId(3));
    println!(
        "RARE_TRANSFER manual={:?} automatic={:?}",
        before.status, after
    );
    assert!(matches!(
        before.status,
        metatorio_runtime::universe::UniverseSolveStatus::Solved { .. }
    ));
    assert!(matches!(
        after.unwrap().status,
        metatorio_runtime::universe::UniverseSolveStatus::Solved { .. }
    ));
}

#[test]
fn unfinished_card_tolerance_parity() {
    let factory = json!({"id":2,"strict_source":true,"mechanics":[{"id":10,"mechanic":{"type":"recipe"}}],"external_inputs":[{"id":11,"flow":"Electricity","penalty":0}],"targets":[{"id":12,"flow":"Electricity","amount":1}]});
    let universe = json!({"id":3,"strict_source":true,"mechanics":[{"id":20,"mechanic":{"type":"local","location":location(),"mechanic":{"type":"recipe"}}}],"external_inputs":[{"id":21,"flow":{"location":location(),"flow":"Electricity"},"penalty":0}],"targets":[{"id":22,"flow":{"location":location(),"flow":"Electricity"},"amount":1}]});
    let mut r = setup(factory, universe);
    let a = r.solve_factory(ProjectId(1), FactoryId(2)).unwrap();
    let b = r.solve_universe(ProjectId(1), UniverseId(3));
    println!("UNFINISHED ordinary={:?} universe={:?}", a.status, b);
    assert!(matches!(
        a.status,
        metatorio_runtime::SolveStatus::Solved { .. }
    ));
    assert!(matches!(
        b.unwrap().status,
        metatorio_runtime::universe::UniverseSolveStatus::Solved { .. }
    ));
}

#[test]
#[cfg(feature = "rust-lp")]
fn tiny_required_machine_writeback_parity() {
    let plate = json!({"Item":{"id":"plate","quality":"normal"}});
    let mut r = setup(
        json!({"id":2,"targets":[{"id":10,"flow":plate,"amount":1}],"external_inputs":[{"id":11,"flow":"Electricity","penalty":0}]}),
        json!({"id":3,"targets":[{"id":20,"flow":{"location":location(),"flow":plate},"amount":1}],"external_inputs":[{"id":21,"flow":{"location":location(),"flow":"Electricity"},"penalty":0}]}),
    );
    r.install_context("audit".into(),PrototypeStore::load(&json!({"item":{"plate":{"stack_size":100}},"planet":{"nauvis":{}},"quality":{"normal":{"level":0,"next":"rare"},"rare":{"level":2}},"recipe":{"fast":{"category":"crafting","energy_required":1,"ingredients":[],"results":[{"type":"item","name":"plate","amount":1}]}},"assembling-machine":{"assembler":{"crafting_categories":["crafting"],"crafting_speed":1000000000000.0,"module_slots":0,"energy_usage":"100W","energy_source":{"type":"electric"},"collision_box":[[0,0],[1,1]]}}})).unwrap());
    r.state.document.projects[0].settings.solver = metatorio_solver::LpSolver::RustBased;
    let a = r.auto_plan(ProjectId(1), FactoryId(2));
    let b = r.auto_plan_universe(ProjectId(1), UniverseId(3));
    let b = b.unwrap();
    assert!(b.report.relative_primal_verified);
    assert!(b.report.exact_primal_verified);
    assert!(b.report.planning_optimization.is_some());
    assert_eq!(b.report.candidates_used, 1);
    assert!(matches!(
        a.unwrap().status,
        metatorio_runtime::SolveStatus::Solved { .. }
    ));
    assert!(matches!(
        b.status,
        metatorio_runtime::universe::UniverseSolveStatus::Solved { .. }
    ));
}

#[test]
fn stale_universe_snapshot_rejects_writeback_but_ignores_unrelated_factory_changes() {
    let mut runtime = setup(json!({"id":2}), json!({"id":3}));
    let snapshot = runtime
        .universe_snapshot_inputs(ProjectId(1), UniverseId(3))
        .unwrap();
    runtime.dispatch(serde_json::from_value(json!({"scope":"factory","action":{"project":1,"factory":2,"action":{"set-name":{"name":"other"}}}})).unwrap()).unwrap();
    assert!(runtime.universe_document_matches(&snapshot));
    runtime.dispatch(serde_json::from_value(json!({"scope":"universe","action":{"project":1,"universe":3,"action":{"set-strict-source":{"strict":true}}}})).unwrap()).unwrap();
    assert!(!runtime.universe_document_matches(&snapshot));
}

#[test]
fn universe_cleanup_is_one_undoable_commit_and_keeps_tiny_support() {
    let mut runtime = setup(
        json!({"id":2}),
        json!({"id":3,"mechanics":[{"id":20,"mechanic":{"type":"local","location":location(),"mechanic":local()}},{"id":21,"mechanic":{"type":"local","location":location(),"mechanic":local()}}]}),
    );
    let before = runtime.state.document.clone();
    let revision = runtime.state.revision;
    runtime
        .state
        .apply_universe_cleanup(
            ProjectId(1),
            UniverseId(3),
            metatorio_runtime::message::CleanupAction::RemoveUnused,
            &[(metatorio_runtime::MechanicId(20), 1e-12)]
                .into_iter()
                .collect(),
        )
        .unwrap();
    assert_eq!(runtime.state.revision, revision + 1);
    assert_eq!(
        runtime.state.document.projects[0].universes[0]
            .mechanics
            .len(),
        1
    );
    runtime
        .dispatch(serde_json::from_value(json!({"scope":"history","action":"undo"})).unwrap())
        .unwrap();
    assert_eq!(runtime.state.document, before);
}

#[test]
fn cancelled_universe_planning_never_mutates_document() {
    let runtime = setup(json!({"id":2}), json!({"id":3}));
    let snapshot = runtime
        .universe_snapshot_inputs(ProjectId(1), UniverseId(3))
        .unwrap();
    let cancellation = metatorio_solver::control::CancellationToken::default();
    cancellation.cancel();
    let result = cancellation.scope(|| {
        metatorio_runtime::solve::plan_universe_with_progress(
            &snapshot,
            &snapshot.resolve_accessibility(),
            |_| panic!("cancelled work must not start"),
        )
    });
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("取消"));
    assert!(!runtime.state.history_status().can_undo);
}

#[test]
fn headless_cleanup_executes_the_universe_command() {
    let mut runtime = setup(
        json!({"id":2}),
        json!({"id":3,"strict_source":true,"mechanics":[{"id":20,"mechanic":{"type":"local","location":location(),"mechanic":local()}},{"id":21,"mechanic":{"type":"local","location":location(),"mechanic":local()}}],"targets":[{"id":22,"flow":{"location":location(),"flow":{"Item":{"id":"plate","quality":"normal"}}},"amount":1}],"external_inputs":[{"id":23,"flow":{"location":location(),"flow":{"Item":{"id":"ore","quality":"normal"}}},"penalty":0},{"id":24,"flow":{"location":location(),"flow":"Electricity"},"penalty":0}]}),
    );
    let before = runtime.state.document.clone();
    let effect = runtime
        .run_command(&metatorio_runtime::RuntimeCommand::CleanupUniverse {
            project: ProjectId(1),
            universe: UniverseId(3),
            action: metatorio_runtime::message::CleanupAction::RemoveUnused,
        })
        .unwrap();
    assert!(matches!(
        effect,
        metatorio_runtime::solve::CommandEffect::Mutated
    ));
    assert_eq!(
        runtime.state.document.projects[0].universes[0]
            .mechanics
            .len(),
        1
    );
    runtime
        .dispatch(serde_json::from_value(json!({"scope":"history","action":"undo"})).unwrap())
        .unwrap();
    assert_eq!(runtime.state.document, before);
}

#[test]
#[cfg(not(feature = "clarabel"))]
fn unsupported_backend_is_not_relabelled_as_infeasible() {
    let mut runtime = setup(
        json!({"id":2}),
        json!({"id":3,"targets":[{"id":20,"flow":{"location":location(),"flow":{"Item":{"id":"plate","quality":"normal"}}},"amount":1}]}),
    );
    runtime.state.document.projects[0].settings.solver = metatorio_solver::LpSolver::Clarabel;
    let error = runtime
        .auto_plan_universe(ProjectId(1), UniverseId(3))
        .unwrap_err();
    let metatorio_runtime::RuntimeError::Diagnostic(diagnostic) = error else {
        panic!("expected structured diagnostic")
    };
    assert!(diagnostic.message.contains("unavailable") || diagnostic.message.contains("不支持"));
    assert!(!diagnostic.message.contains("候选集合无可行解"));
}
