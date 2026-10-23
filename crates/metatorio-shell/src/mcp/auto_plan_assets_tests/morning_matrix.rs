//! One isolated, resumable matrix case through the production GUI planning path.
use super::*;
use metatorio_runtime::{
    AppDocument, AppMessage, PlanningAction, ProjectAction, Runtime, RuntimeError,
};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "LP_MATRIX_CASE manifest; isolated release process per regression case"]
fn morning_matrix_case() {
    let path = PathBuf::from(std::env::var_os("LP_MATRIX_CASE").expect("LP_MATRIX_CASE"));
    let case: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let directory = PathBuf::from(case["directory"].as_str().unwrap());
    let write = |name: &str, value: &Value| {
        serde_json::to_writer(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(name))
                .unwrap(),
            value,
        )
        .unwrap();
    };
    let begun = Instant::now();
    let dump: Value =
        serde_json::from_slice(&fs::read(case["context_dump"].as_str().unwrap()).unwrap()).unwrap();
    let context = PrototypeStore::load(&dump).unwrap();
    drop(dump);
    let document: AppDocument = serde_json::from_value(case["document"].clone()).unwrap();
    let project = document.projects[0].id;
    let factory = document.projects[0].factories[0].id;
    let expected = document.projects[0].factories[0].clone();
    assert!(expected.strict_source);
    assert!(
        !expected
            .external_inputs
            .iter()
            .any(|input| expected.targets.iter().any(|t| t.flow == input.flow))
    );
    let mut runtime = Runtime::from_document(document);
    runtime.install_context("matrix".into(), context);
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::Planning(PlanningAction::UseBestModules {
                quality: case["module_quality"].as_str().unwrap().into(),
            }),
        })
        .unwrap();
    let configured = runtime.state.project(project).unwrap().clone();
    assert!(!configured.planning.enumerate_modules.is_empty());
    write(
        "configured.json",
        &json!({"case_id":case["id"],"load_ms":begun.elapsed().as_millis(),"project":configured}),
    );
    let begun = Instant::now();
    eprintln!("[MATRIX-START] {}", case["id"]);
    let outcome = runtime.auto_plan(project, factory);
    let elapsed_ms = begun.elapsed().as_millis();
    let record = match &outcome {
        Ok(result) => json!({"status":"returned","result":result}),
        Err(RuntimeError::Diagnostic(diagnostic)) => {
            json!({"status":"diagnostic","diagnostic":diagnostic})
        }
        Err(error) => json!({"status":"runtime-error","error":error.to_string()}),
    };
    write(
        "result.json",
        &json!({"case_id":case["id"],"elapsed_ms":elapsed_ms,"outcome":record}),
    );
    if let Ok(result) = outcome {
        let SolveStatus::Solved {
            cost,
            ref mechanics,
            ref flows,
        } = result.status
        else {
            return;
        };
        assert!(cost.is_finite() && cost >= 0.0);
        assert!(
            mechanics
                .iter()
                .all(|m| m.amount.is_finite() && m.amount >= 0.0)
        );
        assert!(
            flows
                .iter()
                .all(|f| f.amount.is_finite() && f.rate.is_finite())
        );
        assert_certified(&result);
        let written = runtime.state.factory(project, factory).unwrap();
        assert_eq!(written.settings, expected.settings);
        assert_eq!(written.targets, expected.targets);
        assert_eq!(written.external_inputs, expected.external_inputs);
        assert_eq!(written.strict_sink, expected.strict_sink);
        assert!(written.strict_source);
        if case["recheck"].as_bool() == Some(true) {
            let checked = runtime.solve_factory(project, factory).unwrap();
            assert_certified(&checked);
            let SolveStatus::Solved {
                cost: checked_cost, ..
            } = checked.status
            else {
                panic!("written factory not solved")
            };
            assert!((cost - checked_cost).abs() <= cost.abs().max(1.0) * 1e-12);
        }
        write(
            "verified.json",
            &json!({"returned_solution_checked":true,"writeback_checked":true,"independent_recompute":case["recheck"]}),
        );
    }
    eprintln!("[MATRIX-END] {} ms={elapsed_ms}", case["id"]);
}
