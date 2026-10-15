use metatorio_core::{DualVar, IdWithQuality, Mechanic};
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::*;
use serde_json::json;

fn setup() -> (Runtime, ProjectId, FactoryId) {
    let store = PrototypeStore::load(&json!({
        "item": {"chunk-item": {}, "plate": {}, "red": {}, "not-science": {}},
        "assembling-machine": {"machine": {"crafting_speed": 1, "crafting_categories": ["crafting"], "energy_usage": "0J", "energy_source": {"type": "void"}}},
        "recipe": {"plate": {"energy_required": 1, "enabled": true, "ingredients": [{"type": "item", "name": "chunk-item", "amount": 1}], "results": [{"type": "item", "name": "plate", "amount": 1}]}},
        "lab": {"lab": {"inputs": ["red"]}},
        "quality": {"normal": {"level": 0, "order": "a", "next": "legendary"}, "legendary": {"level": 5, "order": "b"}},
        "planet": {"nauvis": {"asteroid_spawn_definitions": [{"asteroid": "chunk-entity", "probability": 1}]}},
        "surface": {"space-platform": {"surface_properties": {"gravity": 0}}},
        "asteroid-chunk": {"chunk-entity": {"minable": {"mining_time": 1, "result": "chunk-item"}}}
    })).unwrap();
    let mut runtime = Runtime::new();
    runtime.install_context("test".into(), store);
    runtime.set_active_context(Some("test".into()));
    runtime
        .dispatch(AppMessage::Application(ApplicationAction::NewProject {
            name: "test".into(),
        }))
        .unwrap();
    let project = runtime.state.document.projects[0].id;
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::SetAllAccessible { enabled: true },
        })
        .unwrap();
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::AddFactory {
                name: "test".into(),
                template: FactoryTemplate::Empty,
            },
        })
        .unwrap();
    let factory = runtime.state.project(project).unwrap().factories[0].id;
    for action in [
        FactoryAction::SetStrictSource { strict: true },
        FactoryAction::Context(FactoryContextAction::SetSurface {
            surface: Some("space-platform".into()),
        }),
    ] {
        send(&mut runtime, project, factory, action);
    }
    (runtime, project, factory)
}

fn send(runtime: &mut Runtime, project: ProjectId, factory: FactoryId, action: FactoryAction) {
    runtime
        .dispatch(AppMessage::Factory {
            project,
            factory,
            action,
        })
        .unwrap();
}

fn cost(result: SolveResult) -> f64 {
    let SolveStatus::Solved { cost, .. } = result.status else {
        panic!("{:?}", result.status)
    };
    cost
}

#[test]
fn implicit_source_promotion_overrides_zero_cost_and_supports_history_and_reload() {
    let (mut runtime, project, factory) = setup();
    let flow = DualVar::Item(IdWithQuality::new("chunk-item", "normal"));
    send(
        &mut runtime,
        project,
        factory,
        FactoryAction::Flow(FlowAction::AddToTarget {
            flow: DualVar::Item(IdWithQuality::new("plate", "normal")),
            amount: 2.0,
        }),
    );
    let baseline = cost(runtime.auto_plan(project, factory).unwrap());
    assert_eq!(
        cost(runtime.solve_factory(project, factory).unwrap()),
        baseline
    );
    send(
        &mut runtime,
        project,
        factory,
        FactoryAction::ExternalInput(ExternalInputAction::Add {
            input: ExternalInput {
                id: ExternalInputId(0),
                flow: flow.clone(),
                penalty: 3.0,
            },
        }),
    );
    assert_eq!(
        cost(runtime.solve_factory(project, factory).unwrap()),
        baseline + 6.0
    );
    assert_eq!(
        runtime
            .state
            .factory(project, factory)
            .unwrap()
            .external_inputs
            .len(),
        1
    );
    let document = runtime.document_for_save(project).unwrap();
    let serialized = serde_json::to_string(&document).unwrap();
    let reloaded: AppDocument = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        reloaded.projects[0].factories[0].external_inputs[0].penalty,
        3.0
    );
    runtime
        .dispatch(AppMessage::History(HistoryAction::Undo))
        .unwrap();
    assert!(
        runtime
            .state
            .factory(project, factory)
            .unwrap()
            .external_inputs
            .is_empty()
    );
    assert_eq!(
        cost(runtime.solve_factory(project, factory).unwrap()),
        baseline
    );
    runtime
        .dispatch(AppMessage::History(HistoryAction::Redo))
        .unwrap();
    assert_eq!(
        cost(runtime.auto_plan(project, factory).unwrap()),
        baseline + 6.0
    );
    send(
        &mut runtime,
        project,
        factory,
        FactoryAction::Context(FactoryContextAction::SetSurface { surface: None }),
    );
    let input = runtime
        .state
        .factory(project, factory)
        .unwrap()
        .external_inputs[0]
        .id;
    send(
        &mut runtime,
        project,
        factory,
        FactoryAction::ExternalInput(ExternalInputAction::Remove { input }),
    );
    assert!(matches!(
        runtime.solve_factory(project, factory).unwrap().status,
        SolveStatus::NotSolved { .. }
    ));
}

#[test]
fn auto_plan_converts_legendary_science_and_writes_back_zero_cost_mechanism() {
    let (mut runtime, project, factory) = setup();
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::SetQualityLimit {
                quality: Some("legendary".into()),
            },
        })
        .unwrap();
    send(
        &mut runtime,
        project,
        factory,
        FactoryAction::Flow(FlowAction::AddToTarget {
            flow: DualVar::VirtualScience("red".into()),
            amount: 12.0,
        }),
    );
    send(
        &mut runtime,
        project,
        factory,
        FactoryAction::ExternalInput(ExternalInputAction::Add {
            input: ExternalInput {
                id: ExternalInputId(0),
                flow: DualVar::Item(IdWithQuality::new("red", "legendary")),
                penalty: 3.0,
            },
        }),
    );
    let result = runtime.auto_plan(project, factory).unwrap();
    assert_eq!(cost(result), 6.0);
    let doc = runtime.state.factory(project, factory).unwrap();
    assert_eq!(doc.mechanics.len(), 1);
    assert!(
        matches!(&doc.mechanics[0].mechanic, Mechanic::VirtualScience(m) if m.item.quality == "legendary")
    );
    assert_eq!(cost(runtime.solve_factory(project, factory).unwrap()), 6.0);
    for name in ["not-science", "missing"] {
        let error = runtime.dispatch(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Flow(FlowAction::AddToTarget {
                flow: DualVar::VirtualScience(name.into()),
                amount: 1.0,
            }),
        });
        assert!(error.is_err());
    }
}
