//! Wire-format contract tests: these dispatch the EXACT JSON shapes the
//! Svelte frontend sends (see metatorio-app/src/lib/runtime/types.ts) so a
//! serde mismatch between frontend and backend fails here, not in the GUI.

use metatorio_runtime::message::{AppMessage, RuntimeCommand};
use metatorio_runtime::solve::{Runtime, SolveStatus};
use serde_json::json;

const DEMO_DUMP: &str =
    include_str!("../../../metatorio-app/src-tauri/dumps/demo_dump.json");

fn load_demo_runtime() -> Runtime {
    let dump: serde_json::Value = serde_json::from_str(DEMO_DUMP).unwrap();
    let prototype = metatorio_data::store::PrototypeStore::load(&dump).unwrap();
    let mut runtime = Runtime::new();
    runtime.install_context("demo-context".to_string(), prototype);
    runtime.set_active_context(Some("demo-context".to_string()));
    runtime
}

#[test]
fn frontend_json_new_project_selects_the_project() {
    let mut runtime = load_demo_runtime();

    // Exactly what client.ts sends for runtime.newProject("Demo project").
    let message: AppMessage = serde_json::from_value(json!({
        "scope": "application",
        "action": { "new-project": { "name": "Demo project" } }
    }))
    .unwrap();

    let result = runtime.dispatch(message).unwrap();
    assert!(result.changed);
    assert_eq!(result.revision, 1);
    let project = runtime
        .state
        .ui
        .selected_project
        .expect("new-project must select the created project");
    assert_eq!(runtime.state.project(project).unwrap().name, "Demo project");
}

fn dispatch(
    runtime: &mut Runtime,
    json: serde_json::Value,
) -> metatorio_runtime::state::DispatchResult {
    let message: AppMessage = serde_json::from_value(json).unwrap();
    runtime.dispatch(message).unwrap()
}

#[test]
fn frontend_json_one_click_demo_runs_end_to_end() {
    let mut runtime = load_demo_runtime();

    // 1. New project (exact TS shape).
    let r = dispatch(&mut runtime, json!({
        "scope": "application",
        "action": { "new-project": { "name": "Demo project" } }
    }));
    let _ = r;
    let project = runtime.state.ui.selected_project.unwrap();

    // 2. Add factory (struct variant: fields are wrapped under "action").
    let r = dispatch(&mut runtime, json!({
        "scope": "project",
        "action": {
            "project": project,
            "action": { "add-factory": { "name": "Demo factory", "template": "empty" } }
        }
    }));
    let factory = runtime.state.ui.selected_factory.unwrap();
    assert!(r.commands.contains(&RuntimeCommand::Recompute { project, factory }));

    // 3. Add recipe mechanic.
    dispatch(&mut runtime, json!({
        "scope": "factory",
        "action": {
            "project": project,
            "factory": factory,
            "action": { "mechanic-list": { "add": { "kind": "recipe" } } }
        }
    }));
    let mechanic = runtime.state.factory(project, factory).unwrap().mechanics[0].id;

    // 4. Set recipe + machine (exact TS shape; actions are kind-tagged).
    dispatch(&mut runtime, json!({
        "scope": "factory",
        "action": {
            "project": project,
            "factory": factory,
            "action": {
                "mechanic": {
                    "mechanic": mechanic,
                    "action": { "recipe": { "set-recipe": { "recipe": { "id": "iron-gear-wheel", "quality": "normal" } } } }
                }
            }
        }
    }));
    dispatch(&mut runtime, json!({
        "scope": "factory",
        "action": {
            "project": project,
            "factory": factory,
            "action": {
                "mechanic": {
                    "mechanic": mechanic,
                    "action": { "recipe": { "set-machine": { "machine": { "id": "assembling-machine-1", "quality": "normal" } } } }
                }
            }
        }
    }));

    // 5. Add target.
    dispatch(&mut runtime, json!({
        "scope": "factory",
        "action": {
            "project": project,
            "factory": factory,
            "action": {
                "flow": {
                    "add-to-target": {
                        "flow": { "Item": { "id": "iron-gear-wheel", "quality": "normal" } },
                        "amount": 1
                    }
                }
            }
        }
    }));

    // 6. Explicit recompute → must solve with the demo dump.
    let r = dispatch(&mut runtime, json!({
        "scope": "factory",
        "action": {
            "project": project,
            "factory": factory,
            "action": { "solve": "recompute" }
        }
    }));
    assert!(r.commands.contains(&RuntimeCommand::Recompute { project, factory }));

    let solve = runtime.solve_factory(project, factory).unwrap();
    let SolveStatus::Solved { mechanics, flows, .. } = solve.status else {
        panic!("expected the demo factory to solve, got: {solve:?}");
    };
    assert!(
        mechanics.iter().any(|item| item.mechanic == mechanic && item.amount > 0.0),
        "recipe mechanic must produce: {mechanics:?}"
    );
    assert!(
        mechanics
            .iter()
            .any(|item| item.mechanic == mechanic && item.cost > 0.0),
        "每台实例必须有正成本: {mechanics:?}"
    );
    assert!(flows.iter().any(|item| item.amount > 0.0), "flows: {flows:?}");
}

#[test]
fn frontend_json_supports_all_dual_var_flow_kinds() {
    let mut runtime = load_demo_runtime();

    let dispatch = |runtime: &mut Runtime, json: serde_json::Value| {
        let message: AppMessage = serde_json::from_value(json).unwrap();
        runtime.dispatch(message).unwrap()
    };

    dispatch(&mut runtime, json!({
        "scope": "application",
        "action": { "new-project": { "name": "flows" } }
    }));
    let project = runtime.state.ui.selected_project.unwrap();
    dispatch(&mut runtime, json!({
        "scope": "project",
        "action": { "project": project, "action": { "add-factory": { "name": "f", "template": "empty" } } }
    }));
    let factory = runtime.state.ui.selected_factory.unwrap();

    // 流体目标（单点温度）
    dispatch(&mut runtime, json!({
        "scope": "factory",
        "action": {
            "project": project,
            "factory": factory,
            "action": {
                "flow": {
                    "add-to-target": {
                        "flow": { "Fluid": { "name": "water", "temperature": [15, 15] } },
                        "amount": 100
                    }
                }
            }
        }
    }));
    // 电外部输入（unit 变体 = 裸字符串）
    dispatch(&mut runtime, json!({
        "scope": "factory",
        "action": {
            "project": project,
            "factory": factory,
            "action": {
                "external-input": {
                    "add": { "input": { "id": 0, "flow": "Electricity", "penalty": 1 } }
                }
            }
        }
    }));
    // 火箭运力目标
    dispatch(&mut runtime, json!({
        "scope": "factory",
        "action": {
            "project": project,
            "factory": factory,
            "action": {
                "flow": {
                    "add-to-target": {
                        "flow": "RocketWeightCapacity",
                        "amount": 10
                    }
                }
            }
        }
    }));

    let factory = runtime.state.factory(project, factory).unwrap();
    assert_eq!(
        factory.targets[0].flow,
        metatorio_core::DualVar::Fluid {
            name: "water".to_string(),
            temperature: [15, 15]
        }
    );
    assert_eq!(factory.targets[1].flow, metatorio_core::DualVar::RocketWeightCapacity);
    assert_eq!(
        factory.external_inputs[0].flow,
        metatorio_core::DualVar::Electricity
    );
}
