//! Diagnostic collection, deliberately separate from the feasibility regressions.
use super::*;
use std::{
    collections::BTreeSet,
    io::Write,
    path::{Path, PathBuf},
};

/// User's two-mechanism factory, evaluated without changing its external inputs.
/// The mod dump is deliberately supplied outside the repository fixture.
#[tokio::test]
#[ignore = "requires AMMONIA_CONTEXT_DUMP; real modded quantum-dot regression"]
async fn ammonia_quantum_dot_existing_and_auto_plan() {
    use metatorio_runtime::{AppDocument, Runtime};
    let _permit = GATE.acquire().await.unwrap();
    let path = std::env::var_os("AMMONIA_CONTEXT_DUMP").expect("AMMONIA_CONTEXT_DUMP");
    let dump = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let context = PrototypeStore::load(&dump).unwrap();
    let template: AppDocument =
        serde_json::from_str(include_str!("fixtures/ammonia-quantum-dot.json")).unwrap();
    let report = std::env::var_os("AMMONIA_TEST_REPORT").expect("AMMONIA_TEST_REPORT");
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report)
        .unwrap();
    let mut failures = Vec::new();
    let space_control = std::env::var("AMMONIA_TEST_SURFACE").is_ok();
    let preserve_control = std::env::var("AMMONIA_PRESERVE_EXISTING").as_deref() == Ok("1");
    for strict in [false, true] {
        for auto in [false, true] {
            // The saved non-strict case is enough to prove the environment
            // mismatch. Strict original-environment search is a separate hard LP.
            if !space_control && !preserve_control && strict && auto {
                continue;
            }
            if std::env::var("AMMONIA_EXISTING_ONLY").as_deref() == Ok("1") && auto {
                continue;
            }
            let mut document = template.clone();
            let project = &mut document.projects[0];
            let project_id = project.id;
            project.planning.preserve_existing_mechanics = preserve_control;
            project.context_id = Some("asset".into());
            let factory = &mut project.factories[0];
            let factory_id = factory.id;
            factory.strict_sink = strict;
            if let Ok(surface) = std::env::var("AMMONIA_TEST_SURFACE") {
                factory.settings.surface = Some(surface);
                // The space control must not inherit free planetary deposits.
                factory.settings.planet = None;
            }
            let mut runtime = Runtime::from_document(document);
            runtime.install_context("asset".into(), context.clone());
            let begun = Instant::now();
            eprintln!("[AMMONIA] strict={strict} auto={auto}");
            let result = if auto {
                runtime.auto_plan(project_id, factory_id)
            } else {
                runtime.solve_factory(project_id, factory_id)
            };
            let record = match result {
                Ok(result) => {
                    if matches!(result.status, SolveStatus::Solved { .. }) {
                        assert_certified(&result);
                    } else {
                        failures.push(format!("strict={strict} auto={auto}: {:?}", result.status));
                    }
                    json!({"result":result})
                }
                Err(error) => {
                    if !space_control && !preserve_control && auto {
                        assert!(error.to_string().contains("quantum-dot-catalyst"));
                        assert!(error.to_string().contains("地表条件"));
                    } else {
                        failures.push(format!("strict={strict} auto={auto}: {error}"));
                    }
                    json!({"error":error.to_string()})
                }
            };
            let record = json!({"strict":strict,"auto":auto,"elapsed_ms":begun.elapsed().as_millis(),"outcome":record});
            serde_json::to_writer(&mut output, &record).unwrap();
            writeln!(output).unwrap();
            output.flush().unwrap();
            eprintln!(
                "[AMMONIA] finished strict={strict} auto={auto} ms={}",
                begun.elapsed().as_millis()
            );
        }
    }
    assert!(failures.is_empty(), "quantum-dot failures: {failures:?}");
}

fn snapshots(directory: &Path) -> BTreeSet<PathBuf> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect()
}

/// Read-only project replay. Overrides change only the first target's item and
/// quality; all enumeration, imports, costs and strictness stay in the fixture.
#[tokio::test]
#[ignore = "requires unique LP_BENCH_REPORT; optional LP_BENCH_PROJECT; serialized real-asset benchmark"]
async fn project_file_auto_plan_benchmark() {
    use metatorio_runtime::{AppDocument, Runtime};
    let _permit = GATE.acquire().await.unwrap();
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var_os("LP_BENCH_REPORT").expect("LP_BENCH_REPORT"))
        .unwrap();
    let begun = Instant::now();
    let context = if let Some(path) = std::env::var_os("LP_BENCH_CONTEXT_DUMP") {
        let dump = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        PrototypeStore::load(&dump).unwrap()
    } else {
        store(BASE).clone()
    };
    let mut source: Value = if let Some(path) = std::env::var_os("LP_BENCH_PROJECT") {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    } else {
        serde_json::from_str(include_str!("fixtures/legendary-biter-prod.json")).unwrap()
    };
    let target = &mut source["projects"][0]["factories"][0]["targets"][0]["flow"]["Item"];
    if let Ok(item) = std::env::var("LP_BENCH_TARGET") {
        target["id"] = item.into();
    }
    if let Ok(quality) = std::env::var("LP_BENCH_QUALITY") {
        target["quality"] = quality.into();
    }
    let mut document: AppDocument = serde_json::from_value(source).unwrap();
    let project = &mut document.projects[0];
    project.context_id = Some("asset".into());
    let project_id = project.id;
    let factory_id = project.factories[0].id;
    let input = serde_json::to_value(&project).unwrap();
    let mut runtime = Runtime::from_document(document);
    runtime.install_context("asset".into(), context);
    let load_ms = begun.elapsed().as_millis();
    eprintln!(
        "[PROJECT-BENCH] load_ms={load_ms} targets={}",
        input["factories"][0]["targets"]
    );
    let begun = Instant::now();
    let result = runtime.auto_plan(project_id, factory_id);
    let outcome = match &result {
        Ok(value) => json!({"result": value}),
        Err(error) => json!({"error": error.to_string()}),
    };
    let elapsed_ms = begun.elapsed().as_millis();
    serde_json::to_writer(
        &mut output,
        &json!({"load_ms":load_ms,"elapsed_ms":elapsed_ms,"input":input,"outcome":outcome,
            "document_after":runtime.document_for_save(project_id).ok()}),
    )
    .unwrap();
    output.flush().unwrap();
    eprintln!("[PROJECT-BENCH] auto_plan_ms={elapsed_ms}");
    let result = result.expect("project replay must return a certified result");
    assert_certified(&result);
    assert!(matches!(result.status, SolveStatus::Solved { .. }));
}

#[tokio::test]
#[ignore = "full lengden legendary LDS 120/s; serialized real-asset optimization benchmark"]
async fn lengden_legendary_lds_auto_plan() {
    use metatorio_runtime::{AppDocument, Runtime};
    let _permit = GATE.acquire().await.unwrap();
    let context = if let Some(path) = std::env::var_os("LENGDEN_CONTEXT_DUMP") {
        let dump = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        PrototypeStore::load(&dump).unwrap()
    } else {
        store(BASE).clone()
    };
    let mut document: AppDocument =
        serde_json::from_str(include_str!("fixtures/lengden-legendary-lds.json")).unwrap();
    let project = &mut document.projects[0];
    project.context_id = Some("asset".into());
    let project_id = project.id;
    let factory_id = project.factories[0].id;
    let missing_machines: Vec<_> = project
        .planning
        .machine_preferences
        .iter()
        .filter(|m| {
            context
                .get(metatorio_data::store::PrototypeGroup::Entity, &m.id)
                .is_none()
        })
        .map(|m| m.id.clone())
        .collect();
    let settings = serde_json::to_value(&project.settings).unwrap();
    eprintln!(
        "[LENGDEN] missing machine preferences={} target=legendary low-density-structure 120/s",
        missing_machines.len()
    );
    let mut runtime = Runtime::from_document(document);
    runtime.install_context("asset".into(), context);
    let begun = Instant::now();
    let result = runtime.auto_plan(project_id, factory_id);
    let record = match &result {
        Ok(result) => json!({"result":result}),
        Err(error) => json!({"error":error.to_string()}),
    };
    let record = json!({"elapsed_ms":begun.elapsed().as_millis(),"missing_machine_preferences":missing_machines,"project_settings":settings,"outcome":record});
    let path = std::env::var_os("LENGDEN_TEST_REPORT").expect("LENGDEN_TEST_REPORT");
    serde_json::to_writer_pretty(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap(),
        &record,
    )
    .unwrap();
    let result = result.expect("lengden auto-plan must have a certified result");
    assert_certified(&result);
    assert!(matches!(result.status, SolveStatus::Solved { .. }));
    eprintln!("[LENGDEN] solved ms={}", begun.elapsed().as_millis());
}

#[tokio::test]
#[ignore = "captures historical failing requests; diagnostic outcomes are not pass-rate assertions"]
async fn experiment_capture_failure_requests() {
    let _permit = GATE.acquire().await.unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let request_path = std::env::var_os("LP_REQUESTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("crates/metatorio-shell/src/mcp/auto_plan_assets_tests/fixtures/lp-failure-requests.jsonl"));
    let directory = PathBuf::from(
        std::env::var_os("METATORIO_LP_CAPTURE_DIR").expect("METATORIO_LP_CAPTURE_DIR is required"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let offset = std::env::var("LP_REQUEST_OFFSET")
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(0);
    let limit = std::env::var("LP_REQUEST_LIMIT")
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(usize::MAX);
    let requests: Vec<Value> = std::fs::read_to_string(request_path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| {
            row["error_summary"].as_str().unwrap().contains("校验")
                || row["error_summary"].as_str().unwrap().contains("无界")
        })
        .skip(offset)
        .take(limit)
        .collect();
    assert!(!requests.is_empty(), "no selected diagnostic requests");
    let output_path = directory.join(format!("request-outcomes-{offset}.jsonl"));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .unwrap();
    let mut counts = std::collections::BTreeMap::new();
    for (index, request) in requests.iter().enumerate() {
        let asset = request["asset"].as_str().unwrap();
        assert!(matches!(asset, BASE | MODDED));
        eprintln!(
            "[LP-REQUEST] {} {asset}/{}",
            offset + index,
            request["item"]
        );
        let before = snapshots(&directory);
        let mcp = handler(asset);
        let start_time = Instant::now();
        let ids = start(&mcp, request["arguments"].clone()).await;
        let state = finish(&mcp, &ids).await;
        let status = state["auto_plan"]["status"].as_str().unwrap();
        let captured: Vec<_> = snapshots(&directory).difference(&before).cloned().collect();
        let error = state["auto_plan"]["error"]
            .as_str()
            .map(|s| s.split('；').next().unwrap());
        let record = json!({"request_index":offset+index,"request":request,"status":status,"error_summary":error,
            "elapsed_ms":start_time.elapsed().as_millis(),"snapshot_paths":captured,
            "report":state["auto_plan"]["result"]["report"]});
        writeln!(output, "{record}").unwrap();
        output.flush().unwrap();
        assert!(!captured.is_empty(), "LP capture did not occur");
        assert!(matches!(status, "done" | "failed"));
        *counts.entry(status.to_string()).or_insert(0usize) += 1;
        if status == "done" {
            assert_solved(&mcp, &ids, &state);
            let backend =
                std::env::var("METATORIO_LP_BACKEND").unwrap_or_else(|_| "rust-certified".into());
            if matches!(
                backend.as_str(),
                "hybrid-exact" | "rust-exact" | "rust-verified"
            ) {
                assert_eq!(
                    state["auto_plan"]["result"]["report"]["exact_primal_verified"],
                    true
                );
                if matches!(backend.as_str(), "rust-exact" | "rust-verified") {
                    assert_eq!(
                        state["auto_plan"]["result"]["report"]["exact_optimality_verified"],
                        true
                    );
                }
            }
        } else {
            assert!(
                state["auto_plan"]["error"]
                    .as_str()
                    .unwrap()
                    .starts_with("自动规划未求解："),
                "infrastructure failure: {state}"
            );
        }
    }
    eprintln!("[LP-REQUEST-OUTCOMES] {counts:?}; diagnostic collection only");
    if std::env::var("LP_REQUIRE_SOLVED").as_deref() == Ok("1") {
        assert_eq!(
            counts.get("failed").copied().unwrap_or(0),
            0,
            "strict experimental pipeline regression failed; inspect saved request outcomes"
        );
    }
}
