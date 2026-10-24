use crate::{
    Host,
    app::{self, AppState},
    solve_jobs::SolveJobs,
};
use metatorio_runtime::{
    FactoryId, ProjectId, Runtime, UniverseId, document::AppDocument, solve::UniverseSnapshot,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Default)]
struct TestHost {
    state: AppState,
    events: Mutex<Vec<(String, Value, bool)>>,
}
impl Host for TestHost {
    fn state(&self) -> &AppState {
        &self.state
    }
    fn app_data_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }
    fn emit(&self, event: &str, payload: Value) {
        let unlocked = self.state.runtime.try_lock().is_ok();
        self.events
            .lock()
            .unwrap()
            .push((event.into(), payload, unlocked));
    }
}
fn host() -> Arc<TestHost> {
    let host = Arc::new(TestHost::default());
    let doc:AppDocument=serde_json::from_value(json!({"projects":[{"id":1,"settings":{"solver":"microlp","all_accessible":true},"factories":[{"id":2}],"universes":[{"id":3,"targets":[{"id":10,"flow":{"location":{"kind":"planet","name":"nauvis"},"flow":{"Item":{"id":"plate","quality":"normal"}}},"amount":1}],"external_inputs":[{"id":11,"flow":{"location":{"kind":"planet","name":"nauvis"},"flow":{"Item":{"id":"ore","quality":"normal"}}},"penalty":0},{"id":12,"flow":{"location":{"kind":"planet","name":"nauvis"},"flow":"Electricity"},"penalty":0}]}]}]})).unwrap();
    let mut runtime = Runtime::from_document(doc);
    runtime.install_context("test".into(),metatorio_data::store::PrototypeStore::load(&json!({
 "item":{"ore":{"stack_size":100},"plate":{"stack_size":100}},"module":{"speed":{"stack_size":50,"category":"speed","tier":1,"effect":{"speed":0.2}}},
 "planet":{"nauvis":{}},"quality":{"normal":{"level":0,"next":"rare"},"rare":{"level":2}},
 "recipe":{"smelt":{"category":"crafting","energy_required":1,"ingredients":[{"type":"item","name":"ore","amount":1}],"results":[{"type":"item","name":"plate","amount":1}]},"advanced":{"category":"advanced","energy_required":1,"ingredients":[{"type":"item","name":"ore","amount":1}],"results":[{"type":"item","name":"plate","amount":1}]}},
 "assembling-machine":{"big":{"crafting_categories":["crafting"],"crafting_speed":1,"module_slots":2,"energy_usage":"100W","energy_source":{"type":"electric"},"collision_box":[[0,0],[1,1]]},"small":{"crafting_categories":["crafting"],"crafting_speed":1,"module_slots":0,"energy_usage":"100W","energy_source":{"type":"electric"},"collision_box":[[0,0],[1,1]]},"advanced":{"crafting_categories":["advanced"],"crafting_speed":2,"module_slots":1,"energy_usage":"100W","energy_source":{"type":"electric"},"collision_box":[[0,0],[1,1]]}}
 })).unwrap());
    runtime.set_active_context(Some("test".into()));
    *host.state.runtime.lock().unwrap() = runtime;
    host
}
fn add_configured(host: &TestHost) {
    let entry=serde_json::from_value(json!({"id":20,"enabled":true,"mechanic":{"type":"local","location":{"kind":"planet","name":"nauvis"},"mechanic":{"type":"recipe","recipe":{"id":"smelt","quality":"normal"},"machine":{"id":"big","quality":"normal"},"module_config":{"modules":[{"id":"speed","quality":"normal"},{"id":"speed","quality":"normal"}]}}}})).unwrap();
    host.state.runtime.lock().unwrap().state.document.projects[0].universes[0]
        .mechanics
        .push(entry);
}
#[tokio::test]
async fn universe_planning_runs_unlocked_persists_and_emits_scoped_progress() {
    let concrete = host();
    let app: Arc<dyn Host> = concrete.clone();
    let path = std::env::temp_dir().join(format!(
        "metatorio-universe-parity-{}-{}.metatorio",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    concrete
        .state
        .project_paths
        .lock()
        .unwrap()
        .insert(ProjectId(1), path.to_string_lossy().into());
    let result =
        app::auto_plan_universe_offlock(&app, &concrete.state, ProjectId(1), UniverseId(3))
            .await
            .unwrap();
    assert!(matches!(
        result.status,
        metatorio_runtime::universe::UniverseSolveStatus::Solved { .. }
    ));
    assert!(result.report.candidates_total > 0);
    let disk: AppDocument = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(disk.projects[0].universes[0].strict_source);
    assert!(!disk.projects[0].universes[0].mechanics.is_empty());
    let events = concrete.events.lock().unwrap();
    assert!(
        events
            .iter()
            .any(|(name, p, unlocked)| name == "auto-plan-progress"
                && p["stage"] == "enumerating"
                && *unlocked
                && p["universe"] == 3)
    );
    assert!(
        events
            .iter()
            .any(|(name, p, unlocked)| name == "auto-plan-progress"
                && p["stage"] == "solving"
                && p["variables"].as_u64().unwrap_or(0) > 0
                && *unlocked)
    );
    assert!(
        events
            .iter()
            .any(|(name, p, _)| name == "auto-plan-progress" && p["running"] == false)
    );
    drop(events);
    assert!(
        concrete
            .state
            .runtime
            .lock()
            .unwrap()
            .state
            .history_status()
            .can_undo
    );
    std::fs::remove_file(path).unwrap();
}
#[tokio::test]
async fn universe_dispatch_repairs_compatibility_and_clamps_before_recompute() {
    for (action, expected_machine, slots) in [
        (
            json!({"recipe":{"set-machine":{"machine":{"id":"small","quality":"normal"}}}}),
            "small",
            0,
        ),
        (
            json!({"recipe":{"set-recipe":{"recipe":{"id":"advanced","quality":"normal"}}}}),
            "advanced",
            1,
        ),
    ] {
        let concrete = host();
        add_configured(&concrete);
        let app: Arc<dyn Host> = concrete.clone();
        let message=serde_json::from_value(json!({"scope":"universe","action":{"project":1,"universe":3,"action":{"mechanic":{"mechanic":20,"action":action}}}})).unwrap();
        let response = app::dispatch(&app, message).await.unwrap();
        assert!(matches!(
            response.universe_solve.unwrap().status,
            metatorio_runtime::universe::UniverseSolveStatus::Solved { .. }
        ));
        let runtime = concrete.state.runtime.lock().unwrap();
        let metatorio_core::LocatedMechanic::Local(local) =
            &runtime.state.document.projects[0].universes[0].mechanics[0].mechanic
        else {
            panic!()
        };
        let metatorio_core::Mechanic::Recipe(recipe) = &local.mechanic else {
            panic!()
        };
        assert_eq!(recipe.machine.id, expected_machine);
        assert_eq!(recipe.module_config.modules.len(), slots);
    }
}
#[tokio::test]
async fn universe_jobs_reuse_and_retry_latest_snapshot_without_holding_runtime_lock() {
    let concrete = host();
    let jobs = SolveJobs::<f64, String, UniverseSnapshot>::default();
    let calls = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let read = concrete.clone();
        let compute = concrete.clone();
        let calls = calls.clone();
        let value=jobs.run((ProjectId(1),FactoryId(3)),move||read.state.runtime.lock().unwrap().universe_snapshot_inputs(ProjectId(1),UniverseId(3)).map_err(|e|e.to_string()),move|snapshot|{
   let count=calls.fetch_add(1,Ordering::SeqCst);let mut runtime=compute.state.runtime.try_lock().expect("compute must not hold runtime");
   if count==0{runtime.dispatch(serde_json::from_value(json!({"scope":"universe","action":{"project":1,"universe":3,"action":{"set-target-amount":{"target":10,"amount":2}}}})).unwrap()).unwrap();}
   Ok(snapshot.universe_doc.targets[0].amount)
  }).await.unwrap();
        assert_eq!(value, 2.0);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
#[test]
fn quality_limit_covers_ordinary_expressions_and_universe_sources() {
    for universe in [false, true] {
        let concrete = host();
        let mut runtime = concrete.state.runtime.lock().unwrap();
        let project = &mut runtime.state.document.projects[0];
        project.settings.all_accessible = false;
        project.settings.quality_limit = Some("normal".into());
        if universe {
            project.universes[0].external_inputs.push(serde_json::from_value(json!({"id":99,"flow":{"location":{"kind":"planet","name":"nauvis"},"flow":{"Item":{"id":"plate","quality":"rare"}}},"penalty":0})).unwrap());
        } else {
            project.factories[0].target_expressions.push(serde_json::from_value(json!({"id":98,"constant":1,"terms":[{"id":99,"flow":{"Item":{"id":"plate","quality":"rare"}},"coefficient":1}]})).unwrap());
        }
        app::ensure_quality_limit(&concrete.state, &mut runtime, ProjectId(1)).unwrap();
        assert_eq!(
            runtime.state.document.projects[0]
                .settings
                .quality_limit
                .as_deref(),
            Some("rare")
        );
    }
}
