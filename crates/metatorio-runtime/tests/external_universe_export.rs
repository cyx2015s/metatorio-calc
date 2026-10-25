//! Opt-in fixture bridge: load a saved project inside test code, expand/solve it
//! with real prototypes, then let the browser test render that exact topology.
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{LpSolver, Runtime};
use serde_json::json;

#[test]
#[ignore = "requires METATORIO_PROJECT_FIXTURE and METATORIO_NETWORK_FIXTURE_OUT"]
fn prepare_saved_universe_network_fixture() {
    assert!(
        cfg!(feature = "soplex"),
        "fixture solve requires the soplex feature"
    );
    let input = std::env::var("METATORIO_PROJECT_FIXTURE").expect("saved project path");
    let output =
        std::env::var("METATORIO_NETWORK_FIXTURE_OUT").expect("browser fixture output path");
    let dump = std::env::var("METATORIO_CONTEXT_DUMP").unwrap_or_else(|_| {
        format!(
            "{}/../../assets/data-raw-dump.json",
            env!("CARGO_MANIFEST_DIR")
        )
    });
    let mut runtime = Runtime::new();
    runtime
        .import_document_value(serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap())
        .unwrap();
    let store =
        PrototypeStore::load(&serde_json::from_slice(&std::fs::read(dump).unwrap()).unwrap())
            .unwrap();
    let contexts: Vec<_> = runtime
        .state
        .document
        .projects
        .iter()
        .map(|p| p.context_id.clone().unwrap_or_else(|| "fixture".into()))
        .collect();
    for context in &contexts {
        runtime.install_context(context.clone(), store.clone());
    }
    runtime.set_active_context(contexts.first().cloned());
    for project in &mut runtime.state.document.projects {
        project.settings.solver = LpSolver::Soplex;
        project
            .settings
            .orbital_transport_costs
            .orbital_drop_spoil_penalty = 216000.0;
    }
    let mut universes = Vec::new();
    for project in runtime.state.document.projects.clone() {
        let snapshot = runtime.project_snapshot(project.id).unwrap();
        let accessibility = snapshot.resolve_accessibility();
        for universe in &project.universes {
            let preview = metatorio_runtime::universe::preview_universe(
                &snapshot.store,
                &project,
                universe,
                &accessibility,
            );
            let errors: Vec<_> = preview
                .mechanics
                .iter()
                .filter_map(|m| m.error.as_ref())
                .collect();
            assert!(
                errors.is_empty(),
                "fixture expansion errors: {}",
                errors
                    .iter()
                    .take(3)
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            let solved = runtime.solve_universe(project.id, universe.id).unwrap();
            let status = if matches!(
                solved.status,
                metatorio_runtime::UniverseSolveStatus::Solved { .. }
            ) {
                "solved"
            } else {
                "not-solved"
            };
            println!(
                "fixture project={} universe={} context={:?} mechanics={} expanded_flows={} status={}",
                project.id.0,
                universe.id.0,
                project.context_id,
                universe.mechanics.len(),
                preview
                    .mechanics
                    .iter()
                    .map(|m| m.flows.len())
                    .sum::<usize>(),
                status
            );
            universes.push(json!({"project":project.id,"universe":universe.id,"preview":preview,"solved":solved}));
        }
    }
    assert!(
        !universes.is_empty(),
        "fixture contains no universe workspaces"
    );
    std::fs::write(
        output,
        serde_json::to_vec(&json!({"document":runtime.state.document,"universes":universes}))
            .unwrap(),
    )
    .unwrap();
}
