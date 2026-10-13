//! Real-context tests of the actual MCP handlers, including asynchronous writeback.
//! No GUI, running game, network listener or user application directory is needed.
use super::*;
use metatorio_runtime::SolveStatus;
use serde_json::{Value, json};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

mod import_location_tests;
#[cfg(feature = "lp-experiments")]
mod lp_experiment_tests;

const BASE: &str = "data-raw-dump.json";
const MODDED: &str = "data-raw-dump-heavily-modded.json";
// Each solve enumerates the full context. Serialize tests to keep memory bounded
// even when Cargo's test runner starts many cases simultaneously.
static GATE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static BASE_STORE: OnceLock<PrototypeStore> = OnceLock::new();
static MODDED_STORE: OnceLock<PrototypeStore> = OnceLock::new();

fn store(asset: &str) -> &'static PrototypeStore {
    let cache = if asset == BASE {
        &BASE_STORE
    } else {
        &MODDED_STORE
    };
    cache.get_or_init(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(asset);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let dump = serde_json::from_slice(&bytes).expect("asset JSON");
        PrototypeStore::load(&dump).expect("asset prototypes")
    })
}

struct TestHost {
    state: AppState,
}
impl Host for TestHost {
    fn state(&self) -> &AppState {
        &self.state
    }
    fn app_data_dir(&self) -> std::path::PathBuf {
        // New, unsaved projects must never persist or export contexts.
        panic!("auto_plan test unexpectedly requested an application directory")
    }
    fn emit(&self, _: &str, _: Value) {}
}

fn handler(asset: &str) -> MetatorioMcp {
    let state = AppState::default();
    {
        let mut runtime = state.runtime.lock().unwrap();
        runtime.install_context("asset".into(), store(asset).clone());
        runtime.set_active_context(Some("asset".into()));
    }
    MetatorioMcp {
        host: Arc::new(TestHost { state }),
    }
}

fn payload(result: CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.expect("MCP structured content")
}

async fn start(mcp: &MetatorioMcp, args: Value) -> Value {
    payload(
        mcp.auto_plan(Parameters(serde_json::from_value(args).unwrap()))
            .await
            .expect("auto_plan should accept valid configuration"),
    )
}

async fn poll(mcp: &MetatorioMcp, ids: &Value, limit: usize, offset: usize) -> Value {
    payload(
        mcp.get_planning_state(Parameters(
            serde_json::from_value(json!({
                "project": ids["project"], "factory": ids["factory"],
                "limit": limit, "offset": offset
            }))
            .unwrap(),
        ))
        .await
        .unwrap(),
    )
}

async fn finish(mcp: &MetatorioMcp, ids: &Value) -> Value {
    assert_eq!(ids["auto_plan"]["status"], "running");
    assert_eq!(ids["poll"]["arguments"]["project"], ids["project"]);
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let state = poll(mcp, ids, 1000, 0).await;
        match state["auto_plan"]["status"].as_str() {
            Some("done" | "failed") => return state,
            Some("running") => {}
            other => panic!("unexpected async status: {other:?}: {state}"),
        }
        assert!(
            Instant::now() < deadline,
            "auto_plan did not finish within 180s: {ids}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn assert_solved(mcp: &MetatorioMcp, ids: &Value, state: &Value) -> f64 {
    assert_eq!(state["auto_plan"]["status"], "done", "{state}");
    let result: metatorio_runtime::SolveResult =
        serde_json::from_value(state["auto_plan"]["result"].clone()).unwrap();
    assert!(
        result.report.writeback_verified,
        "writeback verification: {:?}",
        result.report
    );
    assert!(result.report.solution_violation.is_finite());
    assert!(
        result.report.solution_violation <= 1e-5,
        "{:?}",
        result.report
    );
    let SolveStatus::Solved {
        cost,
        mechanics,
        flows,
    } = &result.status
    else {
        panic!("auto_plan returned an unsolved result: {result:?}");
    };
    assert!(cost.is_finite() && *cost >= 0.0, "cost={cost}");
    let mut runtime = mcp.host.state().runtime.lock().unwrap();
    let project = ProjectId(ids["project"].as_u64().unwrap());
    let factory = FactoryId(ids["factory"].as_u64().unwrap());
    let doc = runtime.state.factory(project, factory).unwrap();
    assert!(doc.strict_source, "auto_plan must enforce strict supply");
    for mechanic in mechanics {
        assert!(mechanic.amount.is_finite() && mechanic.amount >= -1e-9);
        assert!(mechanic.rate.is_finite() && mechanic.scale.is_finite());
        if !mechanic.is_virtual {
            assert!(
                doc.mechanics
                    .iter()
                    .any(|entry| entry.id == mechanic.mechanic)
            );
        }
    }
    for flow in flows {
        assert!(flow.amount.is_finite() && flow.rate.is_finite());
    }
    for target in &doc.targets {
        let supplied: f64 = flows
            .iter()
            .filter(|f| f.flow == target.flow)
            .map(|f| f.amount)
            .sum();
        assert!(
            supplied + target.amount.abs().max(1e-9) * 1e-5 >= target.amount,
            "target {:?}: wanted {}, got {supplied}",
            target.flow,
            target.amount
        );
    }
    // Independently solve the persisted factory rather than trusting the report flag.
    let rechecked = runtime
        .solve_factory(project, factory)
        .expect("solve written document");
    let SolveStatus::Solved { cost: new_cost, .. } = rechecked.status else {
        panic!("written factory is infeasible: {rechecked:?}");
    };
    assert!((new_cost - cost).abs() <= cost.abs().max(1.0) * 1e-5);
    *cost
}

async fn success(asset: &str, args: Value) {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(asset);
    let ids = start(&mcp, args.clone()).await;
    assert_eq!(
        ids["created"]["targets"].as_array().unwrap().len(),
        args["targets"].as_array().unwrap().len()
    );
    let state = finish(&mcp, &ids).await;
    assert_solved(&mcp, &ids, &state);
    let runtime = mcp.host.state().runtime.lock().unwrap();
    assert_eq!(runtime.state.document.projects.len(), 1);
    assert_eq!(runtime.state.document.projects[0].factories.len(), 1);
    let project = &runtime.state.document.projects[0];
    let factory = &project.factories[0];
    if let Some(name) = args["project_name"].as_str() {
        assert_eq!(project.name, name);
    }
    if let Some(name) = args["factory_name"].as_str() {
        assert_eq!(factory.name, name);
    }
    if let Some(context) = args["context_id"].as_str() {
        assert_eq!(project.context_id.as_deref(), Some(context));
    }
    if let Some(inputs) = args["external_inputs"].as_array() {
        assert_eq!(factory.external_inputs.len(), inputs.len());
        assert_eq!(
            ids["created"]["external_inputs"].as_array().unwrap().len(),
            inputs.len()
        );
    }
    if let Some(excluded) = args["modules"]["exclude"].as_array() {
        for entry in excluded {
            let module: IdWithQuality = serde_json::from_value(entry.clone()).unwrap();
            assert!(
                !project.planning.enumerate_modules.contains(&module),
                "excluded module still enumerated: {module:?}"
            );
        }
    }
    if let Some(beacons) = args["beacons"].as_array() {
        assert_eq!(project.planning.enumerate_beacons.len(), beacons.len());
    }
}

macro_rules! item_cases {
    ($asset:expr; $($name:ident => $item:literal),* $(,)?) => {$ (
        #[tokio::test]
        async fn $name() {
            success($asset, json!({"targets":[{"item":$item,"amount":1.0}]})).await;
        }
    )*};
}

item_cases!(BASE;
    base_iron => "iron-plate", base_copper => "copper-plate",
    base_steel => "steel-plate", base_gears => "iron-gear-wheel",
    base_cable => "copper-cable", base_green_circuit => "electronic-circuit",
    base_red_circuit => "advanced-circuit", base_blue_circuit => "processing-unit",
    base_plastic => "plastic-bar", base_sulfur => "sulfur", base_battery => "battery",
    base_engine => "engine-unit", base_electric_engine => "electric-engine-unit",
    base_robot_frame => "flying-robot-frame", base_rocket_fuel => "rocket-fuel",
    base_low_density => "low-density-structure", base_concrete => "concrete",
    base_red_science => "automation-science-pack", base_green_science => "logistic-science-pack",
    base_blue_science => "chemical-science-pack", base_military_science => "military-science-pack",
    base_purple_science => "production-science-pack", base_yellow_science => "utility-science-pack",
    base_transport_belt => "transport-belt", base_fast_belt => "fast-transport-belt",
    base_express_belt => "express-transport-belt", base_inserter => "inserter",
    base_piercing_ammo => "piercing-rounds-magazine", base_explosives => "explosives",
    base_solar_panel => "solar-panel", base_accumulator => "accumulator",
);

item_cases!(MODDED;
    modded_iron => "iron-plate", modded_copper => "copper-plate",
    modded_steel => "steel-plate", modded_gears => "iron-gear-wheel",
    modded_green_circuit => "electronic-circuit", modded_plastic => "plastic-bar",
    modded_sulfur => "sulfur", modded_battery => "battery",
    modded_red_circuit => "advanced-circuit", modded_blue_circuit => "processing-unit",
    modded_engine => "engine-unit", modded_low_density => "low-density-structure",
);

macro_rules! planet_cases {
    ($($name:ident => ($planet:literal, $item:literal)),* $(,)?) => {$ (
        #[tokio::test]
        async fn $name() {
            let mut args = json!({"planet":$planet,"targets":[{"item":$item,"amount":1.0}]});
            // Aquilo has no native iron/copper/oil production. Supply imports
            // explicitly; never silently relax the planner's strict supply.
            if $planet == "aquilo" {
                args["external_inputs"] = json!([
                    {"flow":"Electricity"}, {"flow":"Heat"},
                    {"flow":{"Item":{"id":"iron-plate","quality":"normal"}}},
                    {"flow":{"Item":{"id":"copper-plate","quality":"normal"}}},
                    {"flow":{"Item":{"id":"coal","quality":"normal"}}},
                    {"flow":{"Item":{"id":"stone","quality":"normal"}}},
                    {"flow":{"Item":{"id":"calcite","quality":"normal"}}},
                    {"flow":{"Item":{"id":"holmium-plate","quality":"normal"}}},
                    {"flow":{"Item":{"id":"processing-unit","quality":"normal"}}},
                    {"flow":{"Item":{"id":"low-density-structure","quality":"normal"}}},
                    {"flow":{"Fluid":{"name":"water","temperature":[15,15]}}},
                    {"flow":{"Fluid":{"name":"crude-oil","temperature":[25,25]}}}
                ]);
            }
            success(BASE, args).await;
        }
    )*};
}
planet_cases!(
    vulcanus_tungsten => ("vulcanus", "tungsten-plate"),
    vulcanus_carbide => ("vulcanus", "tungsten-carbide"),
    vulcanus_science => ("vulcanus", "metallurgic-science-pack"),
    fulgora_holmium => ("fulgora", "holmium-plate"),
    fulgora_science => ("fulgora", "electromagnetic-science-pack"),
    gleba_bioflux => ("gleba", "bioflux"),
    gleba_science => ("gleba", "agricultural-science-pack"),
    aquilo_lithium => ("aquilo", "lithium-plate"),
    aquilo_science => ("aquilo", "cryogenic-science-pack"),
);

macro_rules! imported_module_cases {
    ($($name:ident => ($item:literal, $import:literal)),* $(,)?) => {$ (
        #[tokio::test]
        async fn $name() {
            success(BASE, json!({"targets":[{"item":$item,"amount":1.0}],
                "external_inputs":[{"flow":{"Item":{"id":$import,"quality":"normal"}}}]})).await;
        }
    )*};
}
imported_module_cases!(
    base_speed_module => ("speed-module-3", "tungsten-carbide"),
    base_productivity_module => ("productivity-module-3", "biter-egg"),
    base_efficiency_module => ("efficiency-module-3", "spoilage"),
    base_quality_module => ("quality-module-3", "superconductor"),
);

#[tokio::test]
async fn simultaneous_same_request_id_creates_only_one_project() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let args =
        json!({"targets":[{"item":"iron-plate","amount":1.0}],"request_id":"concurrent-retry"});
    let (a, b) = tokio::join!(start(&mcp, args.clone()), start(&mcp, args));
    // Drain both tasks even if the assertion below detects duplicate creation.
    let (a_state, b_state) = tokio::join!(finish(&mcp, &a), finish(&mcp, &b));
    assert_solved(&mcp, &a, &a_state);
    assert_solved(&mcp, &b, &b_state);
    assert_eq!(
        a["project"], b["project"],
        "concurrent retries created duplicate projects"
    );
    assert_eq!(a["factory"], b["factory"]);
    assert_eq!(
        mcp.host
            .state()
            .runtime
            .lock()
            .unwrap()
            .state
            .document
            .projects
            .len(),
        1
    );
}

#[tokio::test]
async fn concurrent_distinct_requests_do_not_overwrite_each_other() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let (a, b) = tokio::join!(
        start(
            &mcp,
            json!({"targets":[{"item":"iron-plate","amount":1.0}],"request_id":"a"})
        ),
        start(
            &mcp,
            json!({"targets":[{"item":"copper-plate","amount":7.0}],"request_id":"b"})
        )
    );
    assert_ne!(a["project"], b["project"]);
    let (a_state, b_state) = tokio::join!(finish(&mcp, &a), finish(&mcp, &b));
    assert_solved(&mcp, &a, &a_state);
    assert_solved(&mcp, &b, &b_state);
    let runtime = mcp.host.state().runtime.lock().unwrap();
    assert_eq!(runtime.state.document.projects.len(), 2);
    for project in &runtime.state.document.projects {
        assert_eq!(project.factories.len(), 1);
        assert_eq!(project.factories[0].targets.len(), 1);
    }
}

/// dispatch 的并发幂等：同一个 request_id 同时进来，也只能应用一次。
/// （auto_plan 那条路径见 simultaneous_same_request_id_creates_only_one_project。）
#[tokio::test]
async fn concurrent_same_request_id_applies_dispatch_once() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let args = json!({
        "message": {"scope": "application", "action": {"new-project": {"name": "并发幂等"}}},
        "request_id": "dispatch-concurrent-retry",
    });
    let call = |args: Value| async {
        payload(
            mcp.dispatch(Parameters(serde_json::from_value(args).unwrap()))
                .await
                .unwrap(),
        )
    };
    let (a, b) = tokio::join!(call(args.clone()), call(args));
    // 一个真正执行、一个回放：两个回执指向同一批新建对象。
    assert_eq!(a["created"]["projects"], b["created"]["projects"]);
    assert!(
        a["idempotent_replay"].as_bool().unwrap_or(false)
            || b["idempotent_replay"].as_bool().unwrap_or(false),
        "同 id 的第二个调用必须回放：a={a} b={b}"
    );
    let projects = mcp
        .host
        .state()
        .runtime
        .lock()
        .unwrap()
        .state
        .document
        .projects
        .len();
    assert_eq!(projects, 1, "并发同 request_id 只应建一个项目");
}

#[tokio::test]
async fn repeated_planning_keeps_equivalent_document_revision() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let ids = start(
        &mcp,
        json!({"targets":[{"item":"iron-plate","amount":1.0}]}),
    )
    .await;
    assert_solved(&mcp, &ids, &finish(&mcp, &ids).await);
    let project = ProjectId(ids["project"].as_u64().unwrap());
    let factory = FactoryId(ids["factory"].as_u64().unwrap());
    let (revision, before) = {
        let runtime = mcp.host.state().runtime.lock().unwrap();
        (
            runtime.state.revision,
            runtime.state.factory(project, factory).unwrap().clone(),
        )
    };
    spawn_auto_plan(
        mcp.host.clone(),
        project,
        factory,
        RuntimeCommand::AutoPlan { project, factory },
    );
    assert_solved(&mcp, &ids, &finish(&mcp, &ids).await);
    let runtime = mcp.host.state().runtime.lock().unwrap();
    assert_eq!(runtime.state.factory(project, factory).unwrap(), &before);
    assert_eq!(runtime.state.revision, revision);
}

#[tokio::test]
async fn target_order_and_duplicate_targets_preserve_cost() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let configurations = [
        json!([{"item":"iron-plate","amount":2.0},{"item":"copper-plate","amount":1.0}]),
        json!([{"item":"copper-plate","amount":1.0},{"item":"iron-plate","amount":2.0}]),
        json!([{"item":"iron-plate","amount":1.0},{"item":"iron-plate","amount":1.0},{"item":"copper-plate","amount":1.0}]),
    ];
    let mut baseline = None;
    for targets in configurations {
        let ids = start(&mcp, json!({"targets":targets})).await;
        let cost = assert_solved(&mcp, &ids, &finish(&mcp, &ids).await);
        let first: f64 = *baseline.get_or_insert(cost);
        assert!(
            (cost - first).abs() <= first.abs().max(1.0) * 1e-5,
            "ordering or target splitting changed cost: {first} -> {cost}"
        );
    }
}

#[tokio::test]
async fn nonfinite_target_rates_are_rejected_before_mutation() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    for amount in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0] {
        let mut params: AutoPlanParams = serde_json::from_value(json!({
            "targets":[{"item":"iron-plate","amount":1.0}]
        }))
        .unwrap();
        params.targets[0].amount = amount;
        assert!(mcp.auto_plan(Parameters(params)).await.is_err());
        assert!(
            mcp.host
                .state()
                .runtime
                .lock()
                .unwrap()
                .state
                .document
                .projects
                .is_empty()
        );
    }
}

#[tokio::test]
async fn missing_active_context_rejects_without_creating_documents() {
    let mcp = MetatorioMcp {
        host: Arc::new(TestHost {
            state: AppState::default(),
        }),
    };
    let params =
        serde_json::from_value(json!({"targets":[{"item":"iron-plate","amount":1.0}]})).unwrap();
    assert!(mcp.auto_plan(Parameters(params)).await.is_err());
    assert!(
        mcp.host
            .state()
            .runtime
            .lock()
            .unwrap()
            .state
            .document
            .projects
            .is_empty()
    );
}

#[tokio::test]
async fn explicit_context_survives_active_context_change_during_planning() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let ids = start(
        &mcp,
        json!({"context_id":"asset","targets":[{"item":"steel-plate","amount":1.0}]}),
    )
    .await;
    mcp.host
        .state()
        .runtime
        .lock()
        .unwrap()
        .set_active_context(None);
    assert_solved(&mcp, &ids, &finish(&mcp, &ids).await);
}

#[tokio::test]
async fn cheaper_external_supply_cannot_increase_optimal_cost() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let mut baseline: Option<f64> = None;
    for inputs in [
        json!([]),
        json!([{"flow":"Electricity","penalty":0.0}]),
        json!([{"flow":"Electricity","penalty":0.0},
            {"flow":{"Item":{"id":"iron-ore","quality":"normal"}},"penalty":0.0}]),
    ] {
        let ids = start(
            &mcp,
            json!({"targets":[{"item":"iron-plate","amount":1.0}],"external_inputs":inputs}),
        )
        .await;
        let cost = assert_solved(&mcp, &ids, &finish(&mcp, &ids).await);
        if let Some(previous) = baseline {
            assert!(cost <= previous + previous.max(1.0) * 1e-5);
        }
        baseline = Some(cost);
    }
}

#[tokio::test]
async fn fluid_interval_target_and_explicit_import() {
    success(BASE, json!({"targets":[{"item":"steam","temperature":[165,500],"amount":10.0}],
        "external_inputs":[{"flow":{"Fluid":{"name":"water","temperature":[15,15]}},"penalty":0.1}]})).await;
}

#[tokio::test]
async fn module_exclusion_is_applied_after_best_selection() {
    success(
        BASE,
        json!({"targets":[{"item":"electronic-circuit","amount":1.0}],
        "modules":{"best":true,"exclude":[
            {"id":"speed-module-3","quality":"normal"},
            {"id":"productivity-module-3","quality":"normal"},
            {"id":"efficiency-module-3","quality":"normal"},
            {"id":"quality-module-3","quality":"normal"}
        ]}}),
    )
    .await;
}

#[tokio::test]
async fn impossible_temperature_is_reported_as_async_failure() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let ids = start(
        &mcp,
        json!({"targets":[{"item":"steam","temperature":[20000,20000],"amount":1.0}]}),
    )
    .await;
    let state = finish(&mcp, &ids).await;
    assert_eq!(state["auto_plan"]["status"], "failed", "{state}");
    assert!(!state["auto_plan"]["error"].as_str().unwrap().is_empty());
    assert!(state["auto_plan"].get("result").is_none());
}

/// Only explicitly declared import locations, using the item's own planet.
/// Aquilo is excluded. The report retains every unavailable target for review.
#[tokio::test]
#[ignore = "real import-location sweep; run with --release; supports offset/limit"]
async fn stress_import_location_products() {
    import_location_tests::run_import_location_sweep().await;
}

#[tokio::test]
async fn fluid_temperature_matrix() {
    for (item, temperature) in [
        ("water", None),
        ("steam", Some([165, 165])),
        ("steam", Some([500, 500])),
        ("petroleum-gas", None),
        ("lubricant", None),
        ("sulfuric-acid", None),
    ] {
        let mut target = json!({"item":item,"amount":1.0});
        if let Some(temperature) = temperature {
            target["temperature"] = json!(temperature);
        }
        success(BASE, json!({"targets":[target]})).await;
    }
}

#[tokio::test]
async fn multi_target_shared_upstream() {
    success(
        BASE,
        json!({"targets":[
            {"item":"iron-plate","amount":3.0}, {"item":"steel-plate","amount":0.25},
            {"item":"electronic-circuit","amount":2.0}, {"item":"advanced-circuit","amount":0.1}
        ]}),
    )
    .await;
}

#[tokio::test]
async fn external_input_supply_and_penalties() {
    success(
        BASE,
        json!({"context_id":"asset", "project_name":"外部供给", "factory_name":"test",
        "targets":[{"item":"processing-unit","amount":2.0}], "external_inputs":[
            {"flow":{"Item":{"id":"electronic-circuit","quality":"normal"}},"penalty":1.0},
            {"flow":{"Item":{"id":"advanced-circuit","quality":"normal"}},"penalty":100.0}
        ]}),
    )
    .await;
}

#[tokio::test]
async fn request_id_replay_before_and_after_completion() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let args = json!({"targets":[{"item":"iron-plate","amount":1.0}],"request_id":"retry"});
    let ids = start(&mcp, args.clone()).await;
    let mut expected = ids.clone();
    expected["idempotent_replay"] = json!(true);
    assert_eq!(start(&mcp, args.clone()).await, expected);
    let state = finish(&mcp, &ids).await;
    assert_solved(&mcp, &ids, &state);
    let revision = mcp.host.state().runtime.lock().unwrap().state.revision;
    assert_eq!(start(&mcp, args).await, expected);
    let runtime = mcp.host.state().runtime.lock().unwrap();
    assert_eq!(runtime.state.revision, revision);
    assert_eq!(runtime.state.document.projects.len(), 1);
}

#[tokio::test]
async fn invalid_configuration_is_atomic_and_retryable() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let valid = json!({"targets":[{"item":"iron-plate","amount":1.0}],"request_id":"invalid"});
    let mut cases = vec![
        json!({"targets":[]}),
        json!({"targets":[{"item":"iron-plte","amount":1.0}]}),
        json!({"targets":[{"item":"iron-plate","amount":0.0}]}),
        json!({"targets":[{"item":"iron-plate","amount":-1.0}]}),
        json!({"targets":[{"item":"steam","amount":1.0,"temperature":[500,15]}]}),
        json!({"targets":[{"item":"iron-plate","amount":1.0,"temperature":[15,500]}]}),
    ];
    for (field, value) in [
        ("planet", json!("does-not-exist")),
        ("major_quality", json!("does-not-exist")),
        ("context_id", json!("missing-context")),
        ("modules", json!({"best":true,"quality":"does-not-exist"})),
        (
            "modules",
            json!({"exclude":[{"id":"bad-module","quality":"normal"}]}),
        ),
        (
            "beacons",
            json!([{"beacon":{"id":"bad-beacon","quality":"normal"},"modules":[]}]),
        ),
        (
            "external_inputs",
            json!([{"flow":{"Item":{"id":"bad-input","quality":"normal"}}}]),
        ),
    ] {
        let mut args = valid.clone();
        args[field] = value;
        cases.push(args);
    }
    for mut args in cases {
        args["request_id"] = json!("invalid");
        let result = mcp
            .auto_plan(Parameters(serde_json::from_value(args.clone()).unwrap()))
            .await;
        assert!(
            result.is_err(),
            "accepted invalid arguments: {args}: {result:?}"
        );
        assert!(
            mcp.host
                .state()
                .runtime
                .lock()
                .unwrap()
                .state
                .document
                .projects
                .is_empty(),
            "invalid call left a partial project: {args}"
        );
    }
    let ids = start(&mcp, valid).await;
    assert_ne!(ids["idempotent_replay"], true);
    assert_solved(&mcp, &ids, &finish(&mcp, &ids).await);
}

#[tokio::test]
async fn result_pagination_preserves_totals_and_revision() {
    let _permit = GATE.acquire().await.unwrap();
    let mcp = handler(BASE);
    let ids = start(
        &mcp,
        json!({"targets":[{"item":"processing-unit","amount":1.0}]}),
    )
    .await;
    let full = finish(&mcp, &ids).await;
    assert_solved(&mcp, &ids, &full);
    let small = poll(&mcp, &ids, 1, 0).await;
    let second = poll(&mcp, &ids, 1, 1).await;
    let all = full["auto_plan"]["result"]["status"]["solved"]["mechanics"]
        .as_array()
        .unwrap();
    assert!(all.len() > 1);
    for (page, index) in [(&small, 0), (&second, 1)] {
        assert_eq!(page["auto_plan"]["revision"], full["auto_plan"]["revision"]);
        assert_eq!(
            page["auto_plan"]["result"]["status"]["solved"]["mechanics"],
            json!([all[index]])
        );
        assert_eq!(page["page"]["totals"]["auto_plan.mechanics"], all.len());
    }
}

#[tokio::test]
#[ignore = "full-context module, beacon and quality enumeration; run explicitly"]
async fn stress_modules_beacons_quality_and_scale() {
    let limit = std::env::var("AUTOPLAN_STRESS_LIMIT")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(usize::MAX);
    let mut tested = 0;
    for quality in ["normal", "uncommon", "rare", "epic", "legendary"] {
        for amount in [1e-6, 1.0, 1e6] {
            for beacons in [
                json!([]),
                json!([{"beacon":{"id":"beacon","quality":quality},
                "count":8,"share":8.0,"modules":[{"module":{"id":"speed-module-3","quality":quality},"count":16}]}]),
            ] {
                if tested >= limit {
                    return;
                }
                eprintln!("module stress: quality={quality}, amount={amount}, beacons={beacons}");
                success(BASE, json!({"targets":[{"item":"electronic-circuit","quality":quality,"amount":amount}],
                    "major_quality":quality,"modules":{"best":true,"quality":quality},"beacons":beacons})).await;
                tested += 1;
            }
        }
    }
}

#[tokio::test]
#[ignore = "scale metamorphism over both complete asset contexts"]
async fn stress_scale_invariance() {
    let _permit = GATE.acquire().await.unwrap();
    for asset in [BASE, MODDED] {
        let mcp = handler(asset);
        for item in ["iron-plate", "electronic-circuit", "plastic-bar"] {
            let mut unit_cost = None;
            for amount in [1e-9, 1e-6, 0.001, 1.0, 1000.0, 1e6, 1e9] {
                let ids = start(&mcp, json!({"targets":[{"item":item,"amount":amount}]})).await;
                let cost = assert_solved(&mcp, &ids, &finish(&mcp, &ids).await) / amount;
                let baseline: f64 = *unit_cost.get_or_insert(cost);
                assert!(
                    (cost - baseline).abs() <= baseline.abs().max(1e-9) * 1e-4,
                    "{asset}/{item}/{amount}: unit cost {cost}, baseline {baseline}"
                );
            }
        }
    }
}
