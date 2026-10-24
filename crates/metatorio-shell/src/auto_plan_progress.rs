//! Per-run progress events shared by GUI and MCP auto-plan entry points.
use crate::Host;
use metatorio_runtime::{FactoryId, ProjectId, solve::AutoPlanProgress};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

struct State {
    active: bool,
    progress: AutoPlanProgress,
}

/// Closing the command (including errors/timeouts/cancellation) closes progress.
/// A timed-out blocking worker may continue, but cannot resurrect its events.
pub(crate) struct PlanningProgress {
    host: Arc<dyn Host>,
    project: ProjectId,
    factory: FactoryId,
    universe: Option<metatorio_runtime::UniverseId>,
    run_id: u64,
    state: Arc<Mutex<State>>,
}

fn send(
    host: &dyn Host,
    project: ProjectId,
    factory: FactoryId,
    universe: Option<metatorio_runtime::UniverseId>,
    run_id: u64,
    state: &State,
) {
    host.emit(
        "auto-plan-progress",
        serde_json::json!({
            "project": project, "factory": factory, "universe":universe, "run_id": run_id,
            "running": state.active, "stage": state.progress.stage,
            "variables": state.progress.variables, "constraints": state.progress.constraints,
        }),
    );
}

impl PlanningProgress {
    pub(crate) fn start(host: Arc<dyn Host>, project: ProjectId, factory: FactoryId) -> Self {
        Self::start_scoped(host, project, factory, None)
    }
    pub(crate) fn start_universe(
        host: Arc<dyn Host>,
        project: ProjectId,
        universe: metatorio_runtime::UniverseId,
    ) -> Self {
        Self::start_scoped(host, project, FactoryId(universe.0), Some(universe))
    }
    fn start_scoped(
        host: Arc<dyn Host>,
        project: ProjectId,
        factory: FactoryId,
        universe: Option<metatorio_runtime::UniverseId>,
    ) -> Self {
        let state = State {
            active: true,
            progress: AutoPlanProgress::preparing(),
        };
        let run_id = NEXT_RUN.fetch_add(1, Ordering::Relaxed);
        send(host.as_ref(), project, factory, universe, run_id, &state);
        Self {
            host,
            project,
            factory,
            universe,
            run_id,
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub(crate) fn observer(&self) -> Box<dyn Fn(AutoPlanProgress) + Send + Sync> {
        let (host, project, factory, universe, run_id, state) = (
            self.host.clone(),
            self.project,
            self.factory,
            self.universe,
            self.run_id,
            self.state.clone(),
        );
        Box::new(move |progress| {
            if let Ok(mut state) = state.lock()
                && state.active
            {
                state.progress = progress;
                send(host.as_ref(), project, factory, universe, run_id, &state);
            }
        })
    }
}

impl Drop for PlanningProgress {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.active = false;
            send(
                self.host.as_ref(),
                self.project,
                self.factory,
                self.universe,
                self.run_id,
                &state,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use metatorio_runtime::{message::RuntimeCommand, solve::AutoPlanStage};
    use std::path::PathBuf;

    #[derive(Default)]
    struct EventHost {
        state: AppState,
        events: Mutex<Vec<serde_json::Value>>,
    }
    impl Host for EventHost {
        fn state(&self) -> &AppState {
            &self.state
        }
        fn app_data_dir(&self) -> PathBuf {
            std::env::temp_dir()
        }
        fn emit(&self, event: &str, payload: serde_json::Value) {
            if event == "auto-plan-progress" {
                self.events.lock().unwrap().push(payload);
            }
        }
    }

    #[test]
    fn closing_a_run_preserves_dimensions_and_blocks_late_worker_events() {
        let host = Arc::new(EventHost::default());
        let run = PlanningProgress::start(host.clone(), ProjectId(1), FactoryId(2));
        let observe = run.observer();
        observe(AutoPlanProgress {
            stage: AutoPlanStage::Solving,
            variables: Some(1_000_000),
            constraints: Some(10_000),
        });
        drop(run);
        observe(AutoPlanProgress::preparing());
        let events = host.events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["variables"], serde_json::Value::Null);
        assert_eq!(events[1]["running"], true);
        assert_eq!(events[2]["running"], false);
        assert_eq!(events[2]["variables"], 1_000_000);
        assert_eq!(events[2]["constraints"], 10_000);
        assert_eq!(events[0]["run_id"], events[2]["run_id"]);
    }

    #[tokio::test]
    async fn auto_plan_failure_before_model_construction_closes_progress() {
        let concrete = Arc::new(EventHost::default());
        let host: Arc<dyn Host> = concrete.clone();
        crate::app::execute_command(
            &host,
            host.state(),
            &RuntimeCommand::AutoPlan {
                project: ProjectId(999),
                factory: FactoryId(999),
            },
        )
        .await;
        let events = concrete.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["running"], true);
        assert_eq!(events[1]["running"], false);
    }
}
