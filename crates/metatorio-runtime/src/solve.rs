use std::{collections::HashMap, fs::File, path::Path};

use metatorio_core::{Context, DualVar, GameState};
use metatorio_data::store::PrototypeStore;
use metatorio_solver::{AIndexMap, SolverData, SolverSolution, TargetSpec};
use serde::{Deserialize, Serialize};

use crate::document::{AppDocument, FactoryDocument, ProjectDocument};
use crate::id::{FactoryId, MechanicId, ProjectId};
use crate::message::AppMessage;
use crate::state::{DispatchResult, RuntimeError, RuntimeState};

/// The solver variable identity used by the application adapter.
///
/// One mechanism may expand into several variables because of temperature
/// variants.  The variant index is therefore part of the identity while the
/// persisted document still only stores the mechanism ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExpandedVarId {
    pub mechanic: MechanicId,
    pub variant: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolveResult {
    pub project: ProjectId,
    pub factory: FactoryId,
    pub status: SolveStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SolveStatus {
    Solved {
        cost: f64,
        mechanics: Vec<MechanicSolution>,
        flows: Vec<FlowBalance>,
    },
    NotSolved {
        no_provider: Vec<DualVar>,
        no_consumer: Vec<DualVar>,
        description: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MechanicSolution {
    pub mechanic: MechanicId,
    pub variant: u16,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowBalance {
    pub flow: DualVar,
    pub amount: f64,
}

/// Tauri-independent application runtime.  Tauri commands can own this value
/// behind a mutex and forward its commands/events to the frontend.
#[derive(Debug, Default)]
pub struct Runtime {
    pub state: RuntimeState,
    prototype: Option<PrototypeStore>,
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_document(document: AppDocument) -> Self {
        Self {
            state: RuntimeState::new(document),
            prototype: None,
        }
    }

    pub fn dispatch(&mut self, message: AppMessage) -> Result<DispatchResult, RuntimeError> {
        self.state.dispatch(message)
    }

    pub fn install_prototype_store(&mut self, prototype: PrototypeStore) {
        self.prototype = Some(prototype);
    }

    pub fn load_dump_file(&mut self, path: impl AsRef<Path>) -> Result<(), RuntimeError> {
        let file =
            File::open(path.as_ref()).map_err(|error| RuntimeError::Io(error.to_string()))?;
        let dump: serde_json::Value = serde_json::from_reader(file)
            .map_err(|error| RuntimeError::DataLoad(error.to_string()))?;
        let prototype = PrototypeStore::load(&dump)
            .map_err(|error| RuntimeError::DataLoad(error.to_string()))?;
        self.install_prototype_store(prototype);
        Ok(())
    }

    pub fn load_document_file(&mut self, path: impl AsRef<Path>) -> Result<(), RuntimeError> {
        let file =
            File::open(path.as_ref()).map_err(|error| RuntimeError::Io(error.to_string()))?;
        let document: AppDocument = serde_json::from_reader(file)
            .map_err(|error| RuntimeError::DataLoad(error.to_string()))?;
        self.state = RuntimeState::new(document);
        Ok(())
    }

    pub fn save_document_file(
        &mut self,
        project: ProjectId,
        path: impl AsRef<Path>,
    ) -> Result<(), RuntimeError> {
        self.state.project(project)?;
        let file =
            File::create(path.as_ref()).map_err(|error| RuntimeError::Io(error.to_string()))?;
        serde_json::to_writer_pretty(file, &self.state.document)
            .map_err(|error| RuntimeError::Io(error.to_string()))?;
        self.state.dirty_projects.remove(&project);
        Ok(())
    }

    /// Solve a factory synchronously.  The outer Tauri layer should call this
    /// from its dedicated worker rather than from the command thread.
    pub fn solve_factory(
        &self,
        project_id: ProjectId,
        factory_id: FactoryId,
    ) -> Result<SolveResult, RuntimeError> {
        let prototype = self.prototype.as_ref().ok_or(RuntimeError::DataNotLoaded)?;
        let project = self.state.project(project_id)?;
        let factory = self.state.factory(project_id, factory_id)?;
        solve_document(prototype, project, factory, project_id, factory_id)
    }
}

fn solve_document(
    prototype: &PrototypeStore,
    project: &ProjectDocument,
    factory: &FactoryDocument,
    project_id: ProjectId,
    factory_id: FactoryId,
) -> Result<SolveResult, RuntimeError> {
    let game = make_game_state(prototype, project);
    let context = Context::new(prototype, &game);
    let expansion = metatorio_core::expand::expand(
        factory
            .mechanics
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| (entry.id, &entry.mechanic)),
        &context,
    );

    let mut variant_counts: HashMap<MechanicId, u16> = HashMap::new();
    let mut flows = AIndexMap::default();
    for variable in expansion.variables {
        let variant = variant_counts.entry(variable.prim_var.inner).or_default();
        let flow_id = ExpandedVarId {
            mechanic: variable.prim_var.inner,
            variant: *variant,
        };
        *variant = variant.saturating_add(1);

        // The current core expansion does not expose the old per-instance
        // area cost yet.  A positive neutral cost keeps zero-cost conversion
        // flow rules meaningful until that domain value is added to core.
        flows.insert(flow_id, (variable.flow, 1.0));
    }

    let target = factory
        .targets
        .iter()
        .fold(AIndexMap::default(), |mut target, item| {
            *target.entry(item.flow.clone()).or_insert(0.0) += item.amount;
            target
        });
    let mut problem = SolverData::new_simple(target, flows);
    problem.sources = factory
        .external_inputs
        .iter()
        .map(|input| (input.flow.clone(), input.penalty))
        .collect();
    problem.strict_source = factory.strict_source;
    problem.strict_sink = factory.strict_sink;
    problem
        .target
        .extend(factory.target_expressions.iter().map(|expression| {
            TargetSpec {
                constant: expression.constant,
                coefficients: expression
                    .terms
                    .iter()
                    .map(|term| (term.flow.clone(), term.coefficient))
                    .collect(),
            }
        }));

    let solution = problem.solve();
    Ok(match solution {
        SolverSolution::Solved {
            prim, sum, cost, ..
        } => SolveResult {
            project: project_id,
            factory: factory_id,
            status: SolveStatus::Solved {
                cost,
                mechanics: prim
                    .into_iter()
                    .map(|(id, amount)| MechanicSolution {
                        mechanic: id.mechanic,
                        variant: id.variant,
                        amount,
                    })
                    .collect(),
                flows: sum
                    .into_iter()
                    .map(|(flow, amount)| FlowBalance { flow, amount })
                    .collect(),
            },
        },
        SolverSolution::NotSolved {
            no_provider,
            no_consumer,
            description,
        } => SolveResult {
            project: project_id,
            factory: factory_id,
            status: SolveStatus::NotSolved {
                no_provider,
                no_consumer,
                description,
            },
        },
    })
}

fn make_game_state(prototype: &PrototypeStore, project: &ProjectDocument) -> GameState {
    let mut game = GameState::default();
    let qualities = prototype.quality_order();
    if !qualities.is_empty() {
        game.qualities = qualities.to_vec();
    }
    game.max_quality = if project.settings.all_accessible {
        game.qualities.len().saturating_sub(1)
    } else {
        project
            .settings
            .quality_limit
            .as_deref()
            .and_then(|quality| {
                game.qualities
                    .iter()
                    .position(|candidate| candidate == quality)
            })
            .unwrap_or(0)
    };
    game.mining_productivity = project.settings.mining_productivity;
    if !project.settings.ignore_productivity {
        for productivity in &project.settings.recipe_productivity {
            game.recipe_productivity
                .insert(productivity.recipe.clone(), productivity.productivity);
        }
    }
    game
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::MechanicKind;
    use crate::message::{
        FactoryAction, FlowAction, MechanicAction, MechanicListAction, RuntimeCommand, SolveAction,
    };
    use metatorio_core::IdWithQuality;
    use metatorio_data::store::PrototypeStore;
    use serde_json::json;

    fn load_runtime() -> Runtime {
        let mut runtime = Runtime::new();
        let dump = json!({
            "item": {
                "iron-plate": {"type": "item", "name": "iron-plate", "stack_size": 100},
                "iron-gear-wheel": {"type": "item", "name": "iron-gear-wheel", "stack_size": 100}
            },
            "recipe": {
                "iron-gear-wheel": {
                    "type": "recipe",
                    "name": "iron-gear-wheel",
                    "category": "crafting",
                    "energy_required": 0.5,
                    "ingredients": [{"type": "item", "name": "iron-plate", "amount": 2}],
                    "results": [{"type": "item", "name": "iron-gear-wheel", "amount": 1}]
                }
            },
            "assembling-machine": {
                "assembling-machine-1": {
                    "type": "assembling-machine",
                    "name": "assembling-machine-1",
                    "crafting_categories": ["crafting"],
                    "crafting_speed": 0.5,
                    "energy_usage": "90kW",
                    "energy_source": {"type": "electric"}
                }
            }
        });
        runtime.install_prototype_store(PrototypeStore::load(&dump).unwrap());
        runtime
    }

    #[test]
    fn recipe_target_dispatch_and_solve_form_one_closed_loop() {
        let mut runtime = load_runtime();
        let project_id = ProjectId(1);
        runtime
            .dispatch(AppMessage::Application(
                crate::message::ApplicationAction::NewProject {
                    name: "test project".to_string(),
                },
            ))
            .unwrap();
        let project_id = runtime.state.ui.selected_project.unwrap_or(project_id);
        runtime
            .dispatch(AppMessage::Project {
                project: project_id,
                action: crate::message::ProjectAction::AddFactory {
                    name: "test factory".to_string(),
                    template: crate::message::FactoryTemplate::Empty,
                },
            })
            .unwrap();
        let factory_id = runtime.state.ui.selected_factory.unwrap();
        runtime
            .dispatch(AppMessage::Factory {
                project: project_id,
                factory: factory_id,
                action: FactoryAction::MechanicList(MechanicListAction::Add {
                    kind: MechanicKind::Recipe,
                }),
            })
            .unwrap();
        let mechanic_id = runtime
            .state
            .factory(project_id, factory_id)
            .unwrap()
            .mechanics[0]
            .id;
        runtime
            .dispatch(AppMessage::Factory {
                project: project_id,
                factory: factory_id,
                action: FactoryAction::Mechanic {
                    mechanic: mechanic_id,
                    action: MechanicAction::SetRecipe {
                        recipe: IdWithQuality::new("iron-gear-wheel", "normal"),
                    },
                },
            })
            .unwrap();
        runtime
            .dispatch(AppMessage::Factory {
                project: project_id,
                factory: factory_id,
                action: FactoryAction::Mechanic {
                    mechanic: mechanic_id,
                    action: MechanicAction::SetMachine {
                        machine: IdWithQuality::new("assembling-machine-1", "normal"),
                    },
                },
            })
            .unwrap();
        runtime
            .dispatch(AppMessage::Factory {
                project: project_id,
                factory: factory_id,
                action: FactoryAction::Flow(FlowAction::AddToTarget {
                    flow: DualVar::Item(IdWithQuality::new("iron-gear-wheel", "normal")),
                    amount: 1.0,
                }),
            })
            .unwrap();

        let result = runtime.solve_factory(project_id, factory_id).unwrap();
        let SolveStatus::Solved {
            flows, mechanics, ..
        } = result.status
        else {
            panic!("expected the one-recipe factory to solve");
        };
        assert!(
            mechanics
                .iter()
                .any(|item| item.mechanic == mechanic_id && item.amount > 0.0)
        );
        assert!(flows.iter().any(|item| item.amount > 0.0));

        let update = runtime
            .dispatch(AppMessage::Factory {
                project: project_id,
                factory: factory_id,
                action: FactoryAction::Solve(SolveAction::Recompute),
            })
            .unwrap();
        assert!(update.commands.contains(&RuntimeCommand::Recompute {
            project: project_id,
            factory: factory_id,
        }));
    }
}
