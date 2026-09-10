//! Wire-format contract tests: these dispatch the EXACT JSON shapes the
//! Svelte frontend sends (see metatorio-app/src/lib/runtime/types.ts) so a
//! serde mismatch between frontend and backend fails here, not in the GUI.

use metatorio_runtime::message::{AppMessage, RuntimeCommand};
use metatorio_runtime::solve::{Runtime, SolveStatus};
use serde_json::json;

const DEMO_DUMP: &str = include_str!("../../../metatorio-app/src-tauri/dumps/demo_dump.json");

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
    let project = runtime.state.document.projects[0].id;
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
    let r = dispatch(
        &mut runtime,
        json!({
            "scope": "application",
            "action": { "new-project": { "name": "Demo project" } }
        }),
    );
    let _ = r;
    let project = runtime.state.document.projects[0].id;

    // 2. Add factory (struct variant: fields are wrapped under "action").
    let r = dispatch(
        &mut runtime,
        json!({
            "scope": "project",
            "action": {
                "project": project,
                "action": { "add-factory": { "name": "Demo factory", "template": "empty" } }
            }
        }),
    );
    let factory = runtime.state.project(project).unwrap().factories[0].id;
    assert!(
        r.commands
            .contains(&RuntimeCommand::Recompute { project, factory })
    );

    // 3. Add recipe mechanic.
    dispatch(
        &mut runtime,
        json!({
            "scope": "factory",
            "action": {
                "project": project,
                "factory": factory,
                "action": { "mechanic-list": { "add": { "kind": "recipe" } } }
            }
        }),
    );
    let mechanic = runtime.state.factory(project, factory).unwrap().mechanics[0].id;

    // 4. Set recipe + machine (exact TS shape; actions are kind-tagged).
    dispatch(
        &mut runtime,
        json!({
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
        }),
    );
    dispatch(
        &mut runtime,
        json!({
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
        }),
    );

    // 5. Add target.
    dispatch(
        &mut runtime,
        json!({
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
        }),
    );

    // 6. Explicit recompute → must solve with the demo dump.
    let r = dispatch(
        &mut runtime,
        json!({
            "scope": "factory",
            "action": {
                "project": project,
                "factory": factory,
                "action": { "solve": "recompute" }
            }
        }),
    );
    assert!(
        r.commands
            .contains(&RuntimeCommand::Recompute { project, factory })
    );

    let solve = runtime.solve_factory(project, factory).unwrap();
    let SolveStatus::Solved {
        mechanics, flows, ..
    } = solve.status
    else {
        panic!("expected the demo factory to solve, got: {solve:?}");
    };
    assert!(
        mechanics
            .iter()
            .any(|item| item.mechanic == mechanic && item.amount > 0.0),
        "recipe mechanic must produce: {mechanics:?}"
    );
    assert!(
        mechanics
            .iter()
            .any(|item| item.mechanic == mechanic && item.cost > 0.0),
        "每台实例必须有正成本: {mechanics:?}"
    );
    assert!(
        flows.iter().any(|item| item.amount > 0.0),
        "flows: {flows:?}"
    );
}

/// 上下文动作的线上形状：前端 store 的 setActiveContext / renameContext /
/// deleteContext 现在发送这些 JSON（过去是 Tauri 命令）。形状错了会退化成
/// serde 的 "unknown variant" —— 在 GUI 里表现为「操作没反应」。
///
/// 这些动作改的是 app 层注册表（磁盘缓存清单 + 内存 store），reducer 只发命令，
/// 因此 `changed` 必须为 false：上下文切换不该递增文档 revision。
#[test]
fn frontend_json_context_actions_become_commands() {
    let mut runtime = load_demo_runtime();

    let activated = dispatch(
        &mut runtime,
        json!({
            "scope": "application",
            "action": { "set-active-context": { "context": "demo-context" } }
        }),
    );
    assert_eq!(
        activated.commands,
        vec![RuntimeCommand::SetActiveContext {
            context: Some("demo-context".to_string())
        }]
    );
    assert!(!activated.changed, "切换上下文不改文档，不应递增 revision");

    let cleared = dispatch(
        &mut runtime,
        json!({
            "scope": "application",
            "action": { "set-active-context": { "context": null } }
        }),
    );
    assert_eq!(
        cleared.commands,
        vec![RuntimeCommand::SetActiveContext { context: None }]
    );

    let renamed = dispatch(
        &mut runtime,
        json!({
            "scope": "application",
            "action": { "rename-context": { "id": "demo-context", "name": "Demo" } }
        }),
    );
    assert_eq!(
        renamed.commands,
        vec![RuntimeCommand::RenameContext {
            id: "demo-context".to_string(),
            name: "Demo".to_string()
        }]
    );

    let deleted = dispatch(
        &mut runtime,
        json!({
            "scope": "application",
            "action": { "delete-context": { "id": "demo-context" } }
        }),
    );
    assert_eq!(
        deleted.commands,
        vec![RuntimeCommand::DeleteContext {
            id: "demo-context".to_string()
        }]
    );
}

/// 打开/保存的线上形状：GUI（store.openProject / saveCurrentProject /
/// saveProjectAs）现在只调用文件对话框命令拿路径，真正的读盘/写盘走这些消息。
///
/// 显式保存必须用**专用命令** `SaveProject`（无记忆路径时 app 层报错），
/// 而不是 `Persist{path:None}`——后者是自动落盘，未保存过的新项目应静默跳过。
#[test]
fn frontend_json_persistence_actions_become_commands() {
    let mut runtime = load_demo_runtime();
    dispatch(
        &mut runtime,
        json!({ "scope": "application", "action": { "new-project": { "name": "save me" } } }),
    );
    let project = runtime.state.document.projects[0].id;

    let opened = dispatch(
        &mut runtime,
        json!({ "scope": "application", "action": { "open-project": { "path": "C:/tmp/p.json" } } }),
    );
    assert_eq!(
        opened.commands,
        vec![RuntimeCommand::LoadProject {
            path: "C:/tmp/p.json".to_string()
        }],
        "open-project 只发 LoadProject 命令（导入在命令阶段完成）"
    );

    let saved = dispatch(
        &mut runtime,
        json!({
            "scope": "application",
            "action": { "save-project": { "project": project } }
        }),
    );
    assert_eq!(
        saved.commands,
        vec![RuntimeCommand::SaveProject { project }]
    );

    let saved_as = dispatch(
        &mut runtime,
        json!({
            "scope": "application",
            "action": { "save-project-as": { "project": project, "path": "C:/tmp/out.json" } }
        }),
    );
    assert_eq!(
        saved_as.commands,
        vec![RuntimeCommand::Persist {
            project,
            path: Some("C:/tmp/out.json".to_string())
        }]
    );
}

/// 「已声明但未实现」的变体必须在 JSON Schema 里带上人话警告。
///
/// MCP 的 `dispatch` 工具直接把 `AppMessage` 的 JsonSchema 作为 inputSchema 下发，
/// agent 会**先读 schema 再动手**。如果 schema 里看不出某条变体一定失败，agent
/// 就会浪费一次调用（甚至误以为能力存在）。这里守住「文档注释真的进了 schema」
/// 这一前提——否则下面的注释只是给人看的摆设。
#[test]
fn unimplemented_variants_are_documented_in_the_schema() {
    let schema = schemars::schema_for!(AppMessage);
    let json = serde_json::to_string(&schema).unwrap();
    assert!(
        json.contains("未实现"),
        "未实现变体的说明没有进入 JSON Schema（schemars 不再取文档注释？）"
    );
    for needle in [
        "request-suggestions",
        "replace-from-location",
        "check-for-update",
        "install-update",
    ] {
        assert!(json.contains(needle), "schema 里缺少 {needle} 的定义");
    }
}

/// 「使用最佳插件」的线上形状：前端按钮发的是
/// `{scope:"project", action:{project, action:{planning:{"use-best-modules":{quality}}}}}`。
///
/// demo dump 里没有插件原型，所以最佳集合为空——这里断言的是**形状被接受**
/// （`quality` 可空可省）且不会凭空产生变更；品质语义与替换结果由
/// `solve::tests::use_best_modules_replaces_the_enumeration_at_project_quality` 覆盖。
#[test]
fn frontend_json_use_best_modules_shape() {
    let mut runtime = load_demo_runtime();
    dispatch(
        &mut runtime,
        json!({ "scope": "application", "action": { "new-project": { "name": "best modules" } } }),
    );
    let project = runtime.state.document.projects[0].id;

    let explicit_null = dispatch(
        &mut runtime,
        json!({
            "scope": "project",
            "action": {
                "project": project,
                "action": { "planning": { "use-best-modules": { "quality": null } } }
            }
        }),
    );
    assert!(!explicit_null.changed, "空仓库上不应产生变更");

    // 省略 quality 也要能解析（Option 字段缺省 = None）。
    let omitted = dispatch(
        &mut runtime,
        json!({
            "scope": "project",
            "action": {
                "project": project,
                "action": { "planning": { "use-best-modules": {} } }
            }
        }),
    );
    assert!(!omitted.changed);

    assert!(
        runtime
            .state
            .project(project)
            .unwrap()
            .planning
            .enumerate_modules
            .is_empty()
    );
}

#[test]
fn frontend_json_supports_all_dual_var_flow_kinds() {
    let mut runtime = load_demo_runtime();

    let dispatch = |runtime: &mut Runtime, json: serde_json::Value| {
        let message: AppMessage = serde_json::from_value(json).unwrap();
        runtime.dispatch(message).unwrap()
    };

    dispatch(
        &mut runtime,
        json!({
            "scope": "application",
            "action": { "new-project": { "name": "flows" } }
        }),
    );
    let project = runtime.state.document.projects[0].id;
    dispatch(
        &mut runtime,
        json!({
            "scope": "project",
            "action": { "project": project, "action": { "add-factory": { "name": "f", "template": "empty" } } }
        }),
    );
    let factory = runtime.state.project(project).unwrap().factories[0].id;

    // 流体目标（单点温度）
    dispatch(
        &mut runtime,
        json!({
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
        }),
    );
    // 电外部输入（unit 变体 = 裸字符串）
    dispatch(
        &mut runtime,
        json!({
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
        }),
    );
    // 火箭运力目标
    dispatch(
        &mut runtime,
        json!({
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
        }),
    );

    let factory = runtime.state.factory(project, factory).unwrap();
    assert_eq!(
        factory.targets[0].flow,
        metatorio_core::DualVar::Fluid {
            name: "water".to_string(),
            temperature: [15, 15]
        }
    );
    assert_eq!(
        factory.targets[1].flow,
        metatorio_core::DualVar::RocketWeightCapacity
    );
    assert_eq!(
        factory.external_inputs[0].flow,
        metatorio_core::DualVar::Electricity
    );
}
