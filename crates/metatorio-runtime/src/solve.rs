use std::{collections::HashMap, fs::File, path::Path};

use metatorio_core::{Context, DualVar, Flow, GameState, Mechanic, ModuleConfig};
use metatorio_data::generated_components::{EntityComponent, FluidComponent, ItemComponent};
use metatorio_data::store::{PrototypeGroup, PrototypeStore};
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
    /// 单台实例成本（机器碰撞箱面积；无数据时 16.0）。
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowBalance {
    pub flow: DualVar,
    pub amount: f64,
}

/// Tauri-independent application runtime.  Tauri commands can own this value
/// behind a mutex and forward its commands/events to the frontend.
///
/// The runtime holds one prototype store per game context (`contexts`), keyed
/// by a stable context id.  A project pins the context it was planned against
/// (`ProjectDocument::context_id`); solving falls back to the active context
/// when the project does not pin one.
#[derive(Debug, Default)]
pub struct Runtime {
    pub state: RuntimeState,
    contexts: HashMap<String, PrototypeStore>,
    active_context: Option<String>,
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_document(document: AppDocument) -> Self {
        Self {
            state: RuntimeState::new(document),
            contexts: HashMap::new(),
            active_context: None,
        }
    }

    pub fn dispatch(&mut self, message: AppMessage) -> Result<DispatchResult, RuntimeError> {
        self.state.dispatch(message)
    }

    /// Register a loaded prototype store under a stable context id.
    pub fn install_context(&mut self, context_id: String, prototype: PrototypeStore) {
        self.contexts.insert(context_id, prototype);
    }

    /// Drop a context's in-memory store (the on-disk cache is untouched).
    pub fn remove_context(&mut self, context_id: &str) {
        self.contexts.remove(context_id);
        if self.active_context.as_deref() == Some(context_id) {
            self.active_context = None;
        }
    }

    /// The context used by projects that do not pin one.
    pub fn set_active_context(&mut self, context_id: Option<String>) {
        self.active_context = context_id;
    }

    pub fn active_context(&self) -> Option<&str> {
        self.active_context.as_deref()
    }

    /// Ids of contexts whose store is currently in memory.
    pub fn context_ids(&self) -> impl Iterator<Item = &str> {
        self.contexts.keys().map(String::as_str)
    }

    /// The in-memory store for a context id, if loaded.
    pub fn context_store_by_id(&self, id: &str) -> Option<&PrototypeStore> {
        self.contexts.get(id)
    }

    /// The context the UI should display / browse: the selected project's
    /// pinned context, else the active context.
    pub fn effective_context_id(&self) -> Option<String> {
        self.state
            .ui
            .selected_project
            .and_then(|project| self.state.project(project).ok())
            .and_then(|project| project.context_id.clone())
            .or_else(|| self.active_context.clone())
    }

    /// Resolve the prototype store for a project: its pinned context first,
    /// then the active context.
    pub fn context_store(&self, project_id: ProjectId) -> Result<&PrototypeStore, RuntimeError> {
        let project = self.state.project(project_id)?;
        let context_id = project
            .context_id
            .as_ref()
            .or(self.active_context.as_ref());
        match context_id {
            Some(id) => self
                .contexts
                .get(id)
                .ok_or_else(|| RuntimeError::ContextNotFound(id.clone())),
            None => Err(RuntimeError::DataNotLoaded),
        }
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
        let prototype = self.context_store(project_id)?;
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

    // 每台实例成本（机器碰撞箱面积），作为 LP 目标系数。
    let costs: HashMap<MechanicId, f64> = factory
        .mechanics
        .iter()
        .map(|entry| (entry.id, instance_cost(prototype, &entry.mechanic)))
        .collect();

    let mut variant_counts: HashMap<MechanicId, u16> = HashMap::new();
    let mut flows = AIndexMap::default();
    for variable in expansion.variables {
        let variant = variant_counts.entry(variable.prim_var.inner).or_default();
        let flow_id = ExpandedVarId {
            mechanic: variable.prim_var.inner,
            variant: *variant,
        };
        *variant = variant.saturating_add(1);
        let cost = costs.get(&flow_id.mechanic).copied().unwrap_or(1.0);
        flows.insert(flow_id, (variable.flow, cost));
    }

    let target = factory
        .targets
        .iter()
        .fold(AIndexMap::default(), |mut target, item| {
            *target.entry(item.flow.clone()).or_insert(0.0) += item.amount;
            target
        });
    let sources: Flow = factory
        .external_inputs
        .iter()
        .map(|input| (input.flow.clone(), input.penalty))
        .collect();
    // 零成本转换流（子类型关系，复刻原版 planner.rs:264-316 并扩展）：
    // 温度区间放宽、燃料子类型提升、filter 归并、定点温度互转（FluidHeat 平衡）。
    add_conversion_flows(&mut flows, prototype, &target, &sources);
    let mut problem = SolverData::new_simple(target, flows);
    problem.sources = sources;
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
                        cost: costs.get(&id.mechanic).copied().unwrap_or(1.0),
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

/// 零成本转换流：表达流之间的子类型关系，复刻原版 planner.rs:264-316 并扩展。
///
/// 1. 温度区间子类型：窄区间流可放宽为包含它的宽区间流（[T,T] ⊆ [T1,T2]）。
/// 2. 定点温度互转：同种流体不同定点温度互转，消耗/产出对应 FluidHeat
///    平衡能量（加热消耗热量、冷却产出热量）。
/// 3. 燃料子类型：无燃尽产物的燃料可提升为带燃尽产物的燃料——能接受
///    带燃尽产物燃料的机器，自然也能接受无燃尽产物的燃料。
/// 4. filter 归并：带具体流体 filter 的 FluidHeat/FluidFuel 归并为空串
///    （"任意流体"）抽象流。
///
/// 全部使用 `MechanicId(u64::MAX)` 作为辅助变量身份（复刻 egui 的
/// `usize::MAX`），不落入任何真实机制的求解结果。
pub fn add_conversion_flows(
    flows: &mut AIndexMap<ExpandedVarId, (Flow, f64)>,
    prototype: &PrototypeStore,
    target: &Flow,
    sources: &Flow,
) {
    // 收集求解中实际出现的流键（含绝对值，避免正负相消漏掉）。
    let mut seen: Flow = target.clone();
    for (key, value) in sources {
        *seen.entry(key.clone()).or_insert(0.0) += value.abs();
    }
    for (flow, _) in flows.values() {
        for (key, value) in flow {
            *seen.entry(key.clone()).or_insert(0.0) += value.abs();
        }
    }

    let mut aux: u16 = 0;
    let mut add_aux = |flows: &mut AIndexMap<ExpandedVarId, (Flow, f64)>,
                       flow: Flow| {
        flows.insert(
            ExpandedVarId {
                mechanic: MechanicId(u64::MAX),
                variant: aux,
            },
            (flow, 0.0),
        );
        aux += 1;
    };

    // 温度区间子类型 + 定点温度互转
    let mut fluid_temps: HashMap<String, Vec<[i32; 2]>> = HashMap::new();
    for key in seen.keys() {
        if let DualVar::Fluid { name, temperature } = key {
            let list = fluid_temps.entry(name.clone()).or_default();
            if !list.contains(temperature) {
                list.push(*temperature);
            }
        }
    }
    for (name, temps) in &fluid_temps {
        for narrow in temps {
            for broad in temps {
                if narrow[0] >= broad[0] && narrow[1] <= broad[1] && narrow != broad {
                    let mut flow = Flow::default();
                    flow.insert(
                        DualVar::Fluid {
                            name: name.clone(),
                            temperature: *narrow,
                        },
                        -1.0,
                    );
                    flow.insert(
                        DualVar::Fluid {
                            name: name.clone(),
                            temperature: *broad,
                        },
                        1.0,
                    );
                    add_aux(flows, flow);
                }
            }
        }
        let fixed: Vec<i32> = temps
            .iter()
            .filter(|interval| interval[0] == interval[1])
            .map(|interval| interval[0])
            .collect();
        if fixed.len() > 1 {
            let heat_capacity = prototype
                .get(PrototypeGroup::Fluid, name)
                .and_then(|record| record.component::<FluidComponent>())
                .map(|fluid| fluid.heat_capacity().amount)
                .unwrap_or(0.0);
            for &t1 in &fixed {
                for &t2 in &fixed {
                    if t1 == t2 {
                        continue;
                    }
                    // 1 单位流体从 t1 变到 t2 的热量差（加热为正 → 消耗热量）。
                    let heat = heat_capacity * (f64::from(t2) - f64::from(t1));
                    let mut flow = Flow::default();
                    flow.insert(
                        DualVar::Fluid {
                            name: name.clone(),
                            temperature: [t1, t1],
                        },
                        -1.0,
                    );
                    flow.insert(
                        DualVar::Fluid {
                            name: name.clone(),
                            temperature: [t2, t2],
                        },
                        1.0,
                    );
                    flow.insert(DualVar::FluidHeat { filter: name.clone() }, -heat);
                    add_aux(flows, flow);
                }
            }
        }
    }

    // 燃料子类型：false → true（无燃尽产物燃料可满足带燃尽产物机器）。
    for key in seen.keys() {
        if let DualVar::ItemFuel {
            category,
            has_burnt_result: false,
        } = key
        {
            let mut flow = Flow::default();
            flow.insert(
                DualVar::ItemFuel {
                    category: category.clone(),
                    has_burnt_result: false,
                },
                -1.0,
            );
            flow.insert(
                DualVar::ItemFuel {
                    category: category.clone(),
                    has_burnt_result: true,
                },
                1.0,
            );
            add_aux(flows, flow);
        }
    }

    // filter 归并：FluidHeat{F} / FluidFuel{F} → 空串（任意流体）。
    for key in seen.keys() {
        let flow = match key {
            DualVar::FluidHeat { filter } if !filter.is_empty() => {
                let mut f = Flow::default();
                f.insert(DualVar::FluidHeat { filter: filter.clone() }, -1.0);
                f.insert(DualVar::FluidHeat { filter: String::new() }, 1.0);
                Some(f)
            }
            DualVar::FluidFuel { filter } if !filter.is_empty() => {
                let mut f = Flow::default();
                f.insert(DualVar::FluidFuel { filter: filter.clone() }, -1.0);
                f.insert(DualVar::FluidFuel { filter: String::new() }, 1.0);
                Some(f)
            }
            _ => None,
        };
        if let Some(flow) = flow {
            add_aux(flows, flow);
        }
    }
}

/// 实体碰撞箱面积（占地成本）。
fn entity_area(store: &PrototypeStore, name: &str) -> Option<f64> {
    let bb = store
        .entity(name)?
        .component::<EntityComponent>()?
        .collision_box
        .as_ref()?;
    Some((bb.1 .0 - bb.0 .0).ceil().abs() * (bb.1 .1 - bb.0 .1).ceil().abs())
}

/// 单台实例成本（复刻旧实现 + 信标占地）：
/// - 带机器/设备的机制：机器碰撞箱面积 + Σ(信标面积 × 信标数 / 共享比例)
///   （缺失回退 16.0）；
/// - 腐坏：spoil_ticks / stack_size / 16；
/// - 其余（种植/物品燃料/发射）：固定 16.0。
pub fn instance_cost(store: &PrototypeStore, mechanic: &Mechanic) -> f64 {
    let area = |name: &str| entity_area(store, name).unwrap_or(16.0);
    let beacon_area = |config: &ModuleConfig| -> f64 {
        config
            .beacons
            .iter()
            .map(|beacon| {
                area(&beacon.beacon.id) * beacon.count as f64 / beacon.share.max(1.0)
            })
            .sum()
    };
    match mechanic {
        Mechanic::Recipe(mechanic) => area(&mechanic.machine.id) + beacon_area(&mechanic.module_config),
        Mechanic::Mining(mechanic) => {
            area(&mechanic.machine.id) + beacon_area(&mechanic.module_config)
        }
        Mechanic::Generator(mechanic) => area(&mechanic.generator.id),
        Mechanic::Boiler(mechanic) => area(&mechanic.boiler.id),
        Mechanic::Reactor(mechanic) => area(&mechanic.reactor.id),
        Mechanic::Spoil(mechanic) => store
            .item(&mechanic.item.id)
            .and_then(|record| {
                let item = record.component::<ItemComponent>()?;
                Some(item.spoil_ticks? as f64 / item.stack_size.max(1) as f64 / 16.0)
            })
            .unwrap_or(16.0),
        // 流体燃料/流体热：转换机制，几乎无成本（复刻原版 cost()=0）。
        Mechanic::FluidFuel(_) | Mechanic::FluidHeat(_) => 0.0,
        _ => 16.0,
    }
}

/// 从项目设置构建求解用的 GameState（品质上限/采矿/配方产能加成）。
pub fn make_game_state(prototype: &PrototypeStore, project: &ProjectDocument) -> GameState {    let mut game = GameState::default();
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
        FactoryAction, FlowAction, MechanicAction, MechanicListAction, RecipeMechanicAction,
        RuntimeCommand, SolveAction,
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
        runtime.install_context(
            "test-context".to_string(),
            PrototypeStore::load(&dump).unwrap(),
        );
        runtime.set_active_context(Some("test-context".to_string()));
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
                    action: MechanicAction::Recipe(RecipeMechanicAction::SetRecipe {
                        recipe: IdWithQuality::new("iron-gear-wheel", "normal"),
                    }),
                },
            })
            .unwrap();
        runtime
            .dispatch(AppMessage::Factory {
                project: project_id,
                factory: factory_id,
                action: FactoryAction::Mechanic {
                    mechanic: mechanic_id,
                    action: MechanicAction::Recipe(RecipeMechanicAction::SetMachine {
                        machine: IdWithQuality::new("assembling-machine-1", "normal"),
                    }),
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

    #[test]
    fn project_pinned_context_wins_over_active_context() {
        let mut runtime = load_runtime();
        // A second, distinct context becomes active.
        let other = json!({ "item": {} });
        runtime.install_context("other".to_string(), PrototypeStore::load(&other).unwrap());
        runtime.set_active_context(Some("other".to_string()));

        let project_id = ProjectId(1);
        runtime
            .dispatch(AppMessage::Application(
                crate::message::ApplicationAction::NewProject {
                    name: "pinned".to_string(),
                },
            ))
            .unwrap();
        let project_id = runtime.state.ui.selected_project.unwrap_or(project_id);

        // Unpinned project resolves to the active context.
        assert!(runtime.context_store(project_id).is_ok());

        // Pinning the project to "test-context" overrides the active context.
        runtime
            .dispatch(AppMessage::Project {
                project: project_id,
                action: crate::message::ProjectAction::SetContext {
                    context: Some("test-context".to_string()),
                },
            })
            .unwrap();
        assert_eq!(
            runtime
                .state
                .project(project_id)
                .unwrap()
                .context_id
                .as_deref(),
            Some("test-context")
        );
        assert!(runtime.context_store(project_id).is_ok());

        // A pinned context that is not loaded → ContextNotFound.
        runtime
            .dispatch(AppMessage::Project {
                project: project_id,
                action: crate::message::ProjectAction::SetContext {
                    context: Some("nope".to_string()),
                },
            })
            .unwrap();
        assert_eq!(
            runtime.context_store(project_id).unwrap_err(),
            RuntimeError::ContextNotFound("nope".to_string())
        );
    }
}
