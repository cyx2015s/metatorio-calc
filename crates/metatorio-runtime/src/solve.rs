use std::{collections::HashMap, fs::File, path::Path, sync::Arc};

use metatorio_core::{Accessibility, AccessibilityOptions, Accessible, Context, DualVar, Flow, GameState};
use metatorio_data::{FluidComponent, LabComponent, PrototypeBaseComponent};
use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use metatorio_solver::{AIndexMap, SolverData, SolverSolution, TargetSpec};
use serde::{Deserialize, Serialize};

use crate::document::{
    AppDocument, DOCUMENT_SCHEMA_VERSION, FactoryDocument, InfiniteTechLevel, ProjectDocument,
    ProjectSettings,
};
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
    /// 该变量的 Ruiz 均衡缩放系数。`amount / scale` 是内部缩放空间的
    /// 可比量（剔除逐变量缩放差异），判断"接近 0"应使用它而不是
    /// 表观 amount——单次产出大的配方表观量小但实际很重要。
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowBalance {
    pub flow: DualVar,
    pub amount: f64,
    /// 该物品平衡约束的 Ruiz 缩放系数（dual_scale）。
    /// `amount / scale` 是内部可比量，判断"接近 0"应使用它。
    pub scale: f64,
}

/// 面向前端的产能视图：区分**自动推算**与**用户指定**（用户值有虚线边框）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductivityView {
    /// 每个配方产能项：source = "auto"（自动推算）| "user"（用户指定）。
    pub recipes: Vec<RecipeProductivityView>,
    /// 自动推算的采矿产出加成。
    pub auto_mining: f64,
    /// 最终采矿产出加成（用户覆盖后）。
    pub mining: f64,
    /// 用户对无限科技的研究次数覆盖（2.b）。
    pub infinite_levels: Vec<InfiniteTechLevel>,
    /// 是否忽略自动推算（2.c）。
    pub ignore: bool,
}

/// 单个配方产能项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeProductivityView {
    pub recipe: String,
    pub value: f64,
    pub source: String,
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
    /// 项目 → 可达性结果缓存（settings 变化时在 dispatch 里整体失效；
    /// 计算按需进行，避免每次交互重算全图）。
    accessibilities: HashMap<ProjectId, Accessibility>,
    /// 上下文 → 可达性依赖图（一次构建缓存，供 milestone_order /
    /// compute_accessibility 复用，避免每交互重建全图）。
    graph_cache: HashMap<String, Arc<metatorio_core::GraphData>>,
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
            accessibilities: HashMap::new(),
            graph_cache: HashMap::new(),
        }
    }

    pub fn dispatch(&mut self, message: AppMessage) -> Result<DispatchResult, RuntimeError> {
        // 任何交互都可能改变 settings（时间缩放、里程碑、显式标记、无视可达性），
        // 简单起见整体失效缓存；计算本身发生在 project_accessibility 查询时。
        self.accessibilities.clear();
        self.state.dispatch(message)
    }

    /// Register a loaded prototype store under a stable context id.
    pub fn install_context(&mut self, context_id: String, prototype: PrototypeStore) {
        // 换了 store，之前的依赖图作废。
        self.graph_cache.remove(&context_id);
        self.contexts.insert(context_id, prototype);
    }

    /// Drop a context's in-memory store (the on-disk cache is untouched).
    pub fn remove_context(&mut self, context_id: &str) {
        self.contexts.remove(context_id);
        self.graph_cache.remove(context_id);
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

    /// Resolve the context id a project uses (pinned or active).
    fn context_id_for(&self, project_id: ProjectId) -> Result<String, RuntimeError> {
        let project = self.state.project(project_id)?;
        project
            .context_id
            .clone()
            .or_else(|| self.active_context.clone())
            .ok_or(RuntimeError::ContextNotFound(String::new()))
    }

    /// The cached, context-scoped accessibility dependency graph.  Built once
    /// per context and reused by `milestone_order` / `compute_accessibility`
    /// so interactions don't re-scan the whole prototype store.
    fn graph_for_context(&mut self, context_id: &str) -> Arc<metatorio_core::GraphData> {
        if let Some(graph) = self.graph_cache.get(context_id) {
            return graph.clone();
        }
        let store = self
            .contexts
            .get(context_id)
            .expect("graph_for_context: context not loaded");
        let graph = Arc::new(metatorio_core::build_graph(store));
        self.graph_cache
            .insert(context_id.to_string(), graph.clone());
        graph
    }

    /// Cached graph for a project's context (convenience).
    fn graph_for_project(&mut self, project_id: ProjectId) -> Result<Arc<metatorio_core::GraphData>, RuntimeError> {
        let context_id = self.context_id_for(project_id)?;
        Ok(self.graph_for_context(&context_id))
    }

    /// 计算并缓存项目的可达性结果（选择器过滤 / 自动规划过滤共用）。
    ///
    /// 里程碑来自 `ProjectSettings.milestones`，`unlocked` 是**强制覆盖**：
    /// - `unlocked = true` → 强制可达（并入根种子，最终结果保证可达）；
    /// - `unlocked = false` → 强制不可达（剪枝，最终结果保证不可达）；
    /// - `all_accessible`：无视一切，全可达。
    pub fn project_accessibility(
        &mut self,
        project_id: ProjectId,
    ) -> Result<Accessibility, RuntimeError> {
        if let Some(cached) = self.accessibilities.get(&project_id) {
            return Ok(cached.clone());
        }
        // 先取图（owned Arc，不借用 self），再取 store/settings，避免借用冲突。
        let graph = self.graph_for_project(project_id)?;
        let store = self.context_store(project_id)?;
        let options = {
            let settings = &self.state.project(project_id)?.settings;
            accessibility_options(settings)
        };
        let result =
            metatorio_core::compute_accessibility_with_graph(store, &options, &graph);
        self.accessibilities.insert(project_id, result.clone());
        Ok(result)
    }

    /// 面向前端的产能视图：自动推算 + 用户覆盖，按来源区分。
    pub fn project_productivity(
        &mut self,
        project_id: ProjectId,
    ) -> Result<ProductivityView, RuntimeError> {
        // 复用缓存的可达性结果（与 project_accessibility 同一 settings 推导）。
        let accessibility = self.project_accessibility(project_id)?;
        let store = self.context_store(project_id)?;
        let project = self.state.project(project_id)?.clone();
        let settings = &project.settings;

        let levels: Vec<(String, u32)> = settings
            .infinite_levels
            .iter()
            .map(|level| (level.tech.clone(), level.level))
            .collect();
        // 纯自动基准（不含用户无限等级）——用于展示"自动推算"。
        let pure_auto = metatorio_core::productivity::compute_productivity(
            store,
            &accessibility,
            &[],
            settings.ignore_productivity,
        );
        // 最终（含用户无限等级 2.b）。
        let with_levels = metatorio_core::productivity::compute_productivity(
            store,
            &accessibility,
            &levels,
            settings.ignore_productivity,
        );

        // 合并展示列表：with_levels 基准，用户 2.a 替换同名项。
        let mut recipes: Vec<RecipeProductivityView> = with_levels
            .recipe_productivity
            .iter()
            .map(|(recipe, value)| RecipeProductivityView {
                recipe: recipe.clone(),
                value: *value,
                source: "auto".to_string(),
            })
            .collect();
        for user in &settings.recipe_productivity {
            if let Some(entry) = recipes.iter_mut().find(|r| r.recipe == user.recipe) {
                entry.value = user.productivity;
                entry.source = "user".to_string();
            } else {
                recipes.push(RecipeProductivityView {
                    recipe: user.recipe.clone(),
                    value: user.productivity,
                    source: "user".to_string(),
                });
            }
        }
        recipes.sort_by(|a, b| a.recipe.cmp(&b.recipe));

        // 采矿：用户设定的固定值（非 0）替换自动推算值。
        let mining = if settings.mining_productivity != 0.0 {
            settings.mining_productivity
        } else {
            with_levels.mining_productivity
        };

        Ok(ProductivityView {
            recipes,
            auto_mining: pure_auto.mining_productivity,
            mining,
            infinite_levels: settings.infinite_levels.clone(),
            ignore: settings.ignore_productivity,
        })
    }

    /// 里程碑节点按依赖关系**拓扑排序**（依赖在前），供 UI 按序展示。
    /// `unlocked` 状态随节点保留；依赖环内的节点按原序附加。
    pub fn ordered_project_milestones(
        &mut self,
        project_id: ProjectId,
    ) -> Result<Vec<crate::document::Milestone>, RuntimeError> {
        let graph = self.graph_for_project(project_id)?;
        let store = self.context_store(project_id)?;
        let settings = &self.state.project(project_id)?.settings;
        let nodes: Vec<Accessible> = settings.milestones.iter().map(|m| m.node.clone()).collect();
        let order = metatorio_core::milestone_order_with_graph(store, &graph, &nodes);
        Ok(order
            .into_iter()
            .map(|node| {
                settings
                    .milestones
                    .iter()
                    .find(|m| m.node == node)
                    .cloned()
                    .unwrap_or_else(|| crate::document::Milestone {
                        node,
                        unlocked: true,
                    })
            })
            .collect())
    }

    /// 默认里程碑：把**科技瓶物品**（出现在实验室 LabComponent.inputs 的
    /// 物品，即 lab 消耗的科技瓶）设为里程碑，unlocked=true。
    ///
    /// 只提取**未被 hidden 标记**的 lab（mod 里不可见的实验室不参与默认
    /// 里程碑）；hidden 实验室的输入（如特殊科技瓶）不进入默认集合。
    ///
    /// 里程碑是可达性节点级（不限于科技）：锁定某个科技瓶（unlocked=false）
    /// 即剪枝该节点并阻断依赖它的对象，模拟"还没到这个科技阶段"。
    pub fn set_default_milestones(&mut self, project_id: ProjectId) -> Result<bool, RuntimeError> {
        let store = self.context_store(project_id)?;
        let mut science_packs: Vec<String> = Vec::new();
        for record in store.group(PrototypeGroup::Entity) {
            let Some(lab) = record.component::<LabComponent>() else {
                continue;
            };
            // 过滤隐藏实验室（不可见原型不参与默认里程碑）。
            if record
                .component::<PrototypeBaseComponent>()
                .map(|base| base.hidden)
                .unwrap_or(false)
            {
                continue;
            }
            for name in &lab.inputs {
                if !science_packs.contains(name) {
                    science_packs.push(name.clone());
                }
            }
        }
        science_packs.sort();
        let milestones = science_packs
            .into_iter()
            .map(|name| crate::document::Milestone {
                node: Accessible::Item(name),
                unlocked: true,
            })
            .collect();
        self.state.replace_milestones(project_id, milestones)
    }

    /// 从文件加载项目并导入当前文档（追加，不替换现有项目；
    /// 项目 id 冲突时自动重分配）。
    pub fn load_document_file(&mut self, path: impl AsRef<Path>) -> Result<(), RuntimeError> {
        let file =
            File::open(path.as_ref()).map_err(|error| RuntimeError::Io(error.to_string()))?;
        let document: AppDocument = serde_json::from_reader(file)
            .map_err(|error| RuntimeError::DataLoad(error.to_string()))?;
        self.state.import_projects(&document);
        Ok(())
    }

    /// 保存**当前选中的单个项目**到文件（项目级操作，不保存整个工程合集）。
    ///
    /// 文件格式仍是 `AppDocument`（只含这一个项目），与导入/打开兼容。
    pub fn save_document_file(
        &mut self,
        project: ProjectId,
        path: impl AsRef<Path>,
    ) -> Result<(), RuntimeError> {
        let project_doc = self.state.project(project)?.clone();
        let document = AppDocument {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            projects: vec![project_doc],
        };
        let file =
            File::create(path.as_ref()).map_err(|error| RuntimeError::Io(error.to_string()))?;
        serde_json::to_writer_pretty(file, &document)
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
    let mut game = make_game_state(prototype, project);
    apply_environment_to_game_state(
        prototype,
        &mut game,
        factory.settings.planet.as_deref(),
        factory.settings.surface.as_deref(),
    );
    let context = Context::new(prototype, &game);
    let expansion = metatorio_core::expand::expand(
        factory
            .mechanics
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| (entry.id, &entry.mechanic)),
        &context,
    );

    // 每台实例成本由展开阶段写入每个变量（ExpandedVariable.cost），
    // 太阳能会随表面倍率叠加蓄电器面积——作为 LP 目标系数。
    let mut variant_counts: HashMap<MechanicId, u16> = HashMap::new();
    let mut flows = AIndexMap::default();
    let mut variable_costs: HashMap<ExpandedVarId, f64> = HashMap::new();
    for variable in expansion.variables {
        let variant = variant_counts.entry(variable.prim_var.inner).or_default();
        let flow_id = ExpandedVarId {
            mechanic: variable.prim_var.inner,
            variant: *variant,
        };
        *variant = variant.saturating_add(1);
        variable_costs.insert(flow_id, variable.cost);
        flows.insert(flow_id, (variable.flow, variable.cost));
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
    // 星球自带资源免费可用（严格供给下也不例外），除非外部输入显式覆盖。
    let mut implicit_sources = Flow::default();
    if let Some(planet) = factory.settings.planet.as_deref() {
        implicit_sources = crate::planet::planet_autoplaced_flows(prototype, planet);
        for key in sources.keys() {
            implicit_sources.shift_remove(key);
        }
    }
    let mut all_sources = implicit_sources;
    all_sources.extend(sources.clone());
    // 零成本转换流（子类型关系，复刻原版 planner.rs:264-316 并扩展）：
    // 温度区间放宽、燃料子类型提升、filter 归并、定点温度互转（FluidHeat 平衡）。
    add_conversion_flows(&mut flows, prototype, &target, &all_sources);
    let mut problem = SolverData::new_simple(target, flows);
    problem.sources = all_sources;
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
            prim,
            prim_scale,
            dual_scale,
            sum,
            cost,
            ..
        } => SolveResult {
            project: project_id,
            factory: factory_id,
            status: SolveStatus::Solved {
                cost,
                mechanics: prim
                    .into_iter()
                    .map(|(id, amount)| {
                        let scale = prim_scale.get(&id).copied().unwrap_or(1.0);
                        MechanicSolution {
                            mechanic: id.mechanic,
                            variant: id.variant,
                            amount,
                            cost: variable_costs.get(&id).copied().unwrap_or(1.0),
                            scale,
                        }
                    })
                    .collect(),
                flows: sum
                    .into_iter()
                    .map(|(flow, amount)| {
                        let scale = dual_scale.get(&flow).copied().unwrap_or(1.0);
                        FlowBalance { flow, amount, scale }
                    })
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

/// 单台实例成本已移至 core（`metatorio_core::instance_cost`），此处 re-export
/// 以保持既有调用方（Tauri / auto_plan）的导入路径不变。
pub use metatorio_core::instance_cost;

/// 从项目设置构建求解用的 GameState（品质上限/采矿/配方产能加成）。
///
/// 产能 = **自动推算**（从可达性：可达的无限产能科技按等级贡献）+
/// 用户覆盖（2.a 固定配方值替换自动；2.b 无限科技等级替换默认等级；
/// 2.c 忽略时丢弃自动但保留用户值）。
pub fn make_game_state(prototype: &PrototypeStore, project: &ProjectDocument) -> GameState {
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
    let productivity = productivity_for_game(prototype, project);
    game.mining_productivity = productivity.mining_productivity;
    game.recipe_productivity = productivity.recipe_productivity;
    game
}

/// 里程碑/可达性选项（由项目设置派生），供 `project_accessibility` 与
/// 产能自动推算共用，避免两处构建不一致。
pub(crate) fn accessibility_options(settings: &ProjectSettings) -> AccessibilityOptions {
    AccessibilityOptions {
        forced_accessible: settings
            .milestones
            .iter()
            .filter(|milestone| milestone.unlocked)
            .map(|milestone| milestone.node.clone())
            .collect(),
        forced_inaccessible: settings
            .milestones
            .iter()
            .filter(|milestone| !milestone.unlocked)
            .map(|milestone| milestone.node.clone())
            .collect(),
        all_accessible: settings.all_accessible,
    }
}

/// 计算项目的最终配方/采矿产能（自动推算 + 用户覆盖）。
fn productivity_for_game(
    prototype: &PrototypeStore,
    project: &ProjectDocument,
) -> metatorio_core::ProductivityResult {
    let options = accessibility_options(&project.settings);
    let accessibility = metatorio_core::accessibility::compute_accessibility(prototype, &options);
    let levels: Vec<(String, u32)> = project
        .settings
        .infinite_levels
        .iter()
        .map(|level| (level.tech.clone(), level.level))
        .collect();
    let auto = metatorio_core::productivity::compute_productivity(
        prototype,
        &accessibility,
        &levels,
        project.settings.ignore_productivity,
    );

    // 2.a：用户固定配方值**替换**该配方的自动推算值。
    let mut recipe_productivity = auto.recipe_productivity;
    for user in &project.settings.recipe_productivity {
        recipe_productivity.insert(user.recipe.clone(), user.productivity);
    }

    // 采矿：用户设定的固定值（非 0）替换自动推算值。
    let mining_productivity = if project.settings.mining_productivity != 0.0 {
        project.settings.mining_productivity
    } else {
        auto.mining_productivity
    };

    metatorio_core::ProductivityResult {
        recipe_productivity,
        mining_productivity,
    }
}

/// 根据工厂环境（星球/地表）写入太阳能倍率与昼夜周期。
///
/// 规则（复刻需求设计）：
/// - 同时设置星球与地表（太空平台等）：用"太空中的太阳能"系数
///   （space-location 的 solar_power_in_space，是倍率，直接使用）；
/// - 否则（星球表面）：用"大气中的太阳能"系数。surface_properties
///   的 `solar-power` 是**百分比**（surface-property 默认值 100 即
///   nauvis 的 100%），需 ÷100 换算为倍率；缺失回退 1.0。
/// - day-night-cycle 来自星球 surface_properties，缺失回退 25200。
pub fn apply_environment_to_game_state(
    store: &PrototypeStore,
    game: &mut GameState,
    planet: Option<&str>,
    surface: Option<&str>,
) {
    if surface.is_some() {
        // 太空：solar_power_in_space（找不到 space-location 时回退 1.0）
        if let Some(planet_name) = planet {
            if let Some(record) =
                store.get(metatorio_data::store::PrototypeGroup::SpaceLocation, planet_name)
            {
                if let Some(component) =
                    record.component::<metatorio_data::SpaceLocationComponent>()
                {
                    game.solar_power_multiplier = component.solar_power_in_space;
                }
            }
        }
    } else if let Some(planet_name) = planet {
        if let Some(record) = store.get(metatorio_data::store::PrototypeGroup::Planet, planet_name) {
            if let Some(component) =
                record.component::<metatorio_data::PlanetComponent>()
            {
                if let Some(&value) = component.surface_properties.get("solar-power") {
                    // 百分比 → 倍率（nauvis 默认 100 → 1.0）
                    game.solar_power_multiplier = value / 100.0;
                }
                if let Some(&cycle) = component.surface_properties.get("day-night-cycle") {
                    game.day_night_cycle = cycle;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{MechanicKind, RecipeProductivity};
    use crate::message::{
        FactoryAction, FlowAction, MechanicAction, MechanicListAction, ProjectAction,
        RecipeMechanicAction, RuntimeCommand, SolveAction,
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

    /// 可达性测试专用合成 dump：科技链 + 解锁配方链 + 手工制造。
    fn load_accessibility_runtime() -> Runtime {
        let mut runtime = Runtime::new();
        let dump = json!({
            "item": {
                "iron-ore": { "type": "item", "name": "iron-ore" },
                "iron-plate": { "type": "item", "name": "iron-plate" },
                "steel-plate": { "type": "item", "name": "steel-plate" },
                "magic-item": { "type": "item", "name": "magic-item" },
                "assembling-machine-1": {
                    "type": "item", "name": "assembling-machine-1",
                    "place_result": "assembling-machine-1"
                }
            },
            "fluid": {},
            "technology": {
                "tech-base": {
                    "type": "technology", "name": "tech-base",
                    "prerequisites": [], "enabled": true,
                    "effects": [], "unit": { "count": 10, "time": 10, "ingredients": [] }
                },
                "tech-steel": {
                    "type": "technology", "name": "tech-steel",
                    "prerequisites": ["tech-base"], "enabled": false,
                    "effects": [{ "type": "unlock-recipe", "recipe": "steel-plate" }],
                    "unit": { "count": 10, "time": 10, "ingredients": [] }
                }
            },
            "recipe": {
                "iron-ore": {
                    "type": "recipe", "name": "iron-ore",
                    "energy_required": 1,
                    "ingredients": [],
                    "results": [{ "type": "item", "name": "iron-ore", "amount": 1 }],
                    "categories": ["crafting"], "enabled": true
                },
                "iron-plate": {
                    "type": "recipe", "name": "iron-plate",
                    "energy_required": 1,
                    "ingredients": [{ "type": "item", "name": "iron-ore", "amount": 1 }],
                    "results": [{ "type": "item", "name": "iron-plate", "amount": 1 }],
                    "categories": ["crafting"], "enabled": true
                },
                "steel-plate": {
                    "type": "recipe", "name": "steel-plate",
                    "energy_required": 1,
                    "ingredients": [{ "type": "item", "name": "iron-plate", "amount": 1 }],
                    "results": [{ "type": "item", "name": "steel-plate", "amount": 1 }],
                    "categories": ["crafting"], "enabled": false
                }
            },
            "assembling-machine": {
                "assembling-machine-1": {
                    "type": "assembling-machine", "name": "assembling-machine-1",
                    "crafting_categories": ["crafting"], "crafting_speed": 1,
                    "module_slots": 0, "energy_usage": "90kW",
                    "energy_source": { "type": "electric", "drain": "0J" }
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

    /// 产能测试专用合成 dump：两个无限产能科技（采矿 + 配方）。
    fn load_productivity_runtime() -> Runtime {
        let mut runtime = Runtime::new();
        let dump = json!({
            "item": {
                "iron-plate": { "type": "item", "name": "iron-plate" },
                "steel-plate": { "type": "item", "name": "steel-plate" }
            },
            "fluid": {},
            "technology": {
                "mining-prod": {
                    "type": "technology", "name": "mining-prod",
                    "prerequisites": [], "enabled": true, "max_level": "infinite",
                    "effects": [{ "type": "mining-drill-productivity-bonus", "modifier": 0.1 }],
                    "unit": { "count": 1, "time": 1, "ingredients": [] }
                },
                "steel-prod": {
                    "type": "technology", "name": "steel-prod",
                    "prerequisites": [], "enabled": true, "max_level": "infinite",
                    "effects": [{ "type": "change-recipe-productivity", "recipe": "steel-plate", "change": 0.1 }],
                    "unit": { "count": 1, "time": 1, "ingredients": [] }
                }
            },
            "recipe": {},
            "assembling-machine": {}
        });
        runtime.install_context(
            "test-context".to_string(),
            PrototypeStore::load(&dump).unwrap(),
        );
        runtime.set_active_context(Some("test-context".to_string()));
        runtime
    }

    fn new_project(runtime: &mut Runtime) -> ProjectId {
        runtime
            .dispatch(AppMessage::Application(
                crate::message::ApplicationAction::NewProject {
                    name: "test project".to_string(),
                },
            ))
            .unwrap();
        runtime.state.ui.selected_project.unwrap()
    }

    fn dispatch_project(runtime: &mut Runtime, project: ProjectId, action: ProjectAction) {
        runtime
            .dispatch(AppMessage::Project { project, action })
            .unwrap();
    }

    #[test]
    fn accessibility_milestones_are_forced_overrides() {
        let mut runtime = load_accessibility_runtime();
        let project = new_project(&mut runtime);

        // 默认自动层：iron-plate 可达（enabled 配方），steel-plate 可达
        // （tech-steel 由 tech-base 解锁），无来源的 magic-item 不可达。
        let result = runtime.project_accessibility(project).unwrap();
        assert!(result.is_item_accessible("iron-plate"));
        assert!(result.is_item_accessible("steel-plate"));
        assert!(!result.is_item_accessible("magic-item"));

        // 强制可达：magic-item 无任何来源，里程碑 unlocked=true → 强制可达，
        // 自动解析（无来源应不可达）被覆盖。
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::AddMilestone {
                node: Accessible::Item("magic-item".to_string()),
                unlocked: true,
            },
        );
        assert!(
            runtime
                .project_accessibility(project)
                .unwrap()
                .is_item_accessible("magic-item"),
            "里程碑 unlocked=true 应强制可达（覆盖自动解析）"
        );

        // 强制不可达：iron-plate 自动解析可达，但里程碑 unlocked=false 强制不可达，
        // 自动解析被覆盖，且阻断依赖它的对象（steel-plate）。
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::AddMilestone {
                node: Accessible::Item("iron-plate".to_string()),
                unlocked: false,
            },
        );
        let result = runtime.project_accessibility(project).unwrap();
        assert!(!result.is_item_accessible("iron-plate"), "强制不可达应覆盖自动可达");
        assert!(!result.is_item_accessible("steel-plate"), "强制不可达应阻断依赖");

        // 移除里程碑 → 恢复自动状态（iron-plate 重新可达）。
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::RemoveMilestone {
                node: Accessible::Item("iron-plate".to_string()),
            },
        );
        assert!(runtime.project_accessibility(project).unwrap().is_item_accessible("iron-plate"));

        // 科技里程碑 unlocked=false 剪枝科技子树。
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::AddMilestone {
                node: Accessible::Tech("tech-base".to_string()),
                unlocked: false,
            },
        );
        let result = runtime.project_accessibility(project).unwrap();
        assert!(!result.is_accessible(&Accessible::Tech("tech-steel".to_string())));
        assert!(!result.is_item_accessible("steel-plate"), "未解锁里程碑应阻断科技子树");

        // 无视可达性：全可达。
        dispatch_project(&mut runtime, project, ProjectAction::SetAllAccessible { enabled: true });
        let result = runtime.project_accessibility(project).unwrap();
        assert!(result.is_item_accessible("magic-item"));
        assert!(result.is_item_accessible("steel-plate"));
    }

    #[test]
    fn milestone_settings_persist_in_document() {
        let mut runtime = load_accessibility_runtime();
        let project = new_project(&mut runtime);
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::AddMilestone {
                node: Accessible::Item("magic-item".to_string()),
                unlocked: true,
            },
        );
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::AddMilestone {
                node: Accessible::Recipe("steel-plate".to_string()),
                unlocked: false,
            },
        );
        let settings = &runtime.state.project(project).unwrap().settings;
        assert_eq!(
            settings.milestones,
            vec![
                crate::document::Milestone {
                    node: Accessible::Item("magic-item".to_string()),
                    unlocked: true,
                },
                crate::document::Milestone {
                    node: Accessible::Recipe("steel-plate".to_string()),
                    unlocked: false,
                },
            ]
        );
        // 文档 roundtrip（serde 持久化）：Milestone 的 Accessible 外部标签格式。
        let encoded = serde_json::to_string(&runtime.state.document).unwrap();
        let decoded: crate::document::AppDocument = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, runtime.state.document);
    }

    /// 默认里程碑 = 科技瓶物品（实验室 LabComponent.inputs），unlocked=true；
    /// 锁定科技瓶物品里程碑 → 该物品剪枝。
    #[test]
    fn default_milestones_are_lab_science_packs() {
        let mut runtime = Runtime::new();
        let dump = json!({
            "item": {
                "automation-science-pack": { "type": "item", "name": "automation-science-pack" },
                "chemical-science-pack": { "type": "item", "name": "chemical-science-pack" },
                "secret-science-pack": { "type": "item", "name": "secret-science-pack" }
            },
            "fluid": {},
            "recipe": {
                "chemical-science-pack": {
                    "type": "recipe", "name": "chemical-science-pack",
                    "energy_required": 1, "ingredients": [],
                    "results": [{ "type": "item", "name": "chemical-science-pack", "amount": 1 }],
                    "categories": ["crafting"], "enabled": true
                }
            },
            "technology": {},
            "lab": {
                "lab": {
                    "type": "lab", "name": "lab",
                    "energy_usage": "60kW",
                    "energy_source": { "type": "electric", "drain": "0J" },
                    "researching_speed": 1,
                    "inputs": ["automation-science-pack", "chemical-science-pack"]
                },
                "hidden-lab": {
                    "type": "lab", "name": "hidden-lab",
                    "hidden": true,
                    "energy_usage": "60kW",
                    "energy_source": { "type": "electric", "drain": "0J" },
                    "researching_speed": 1,
                    "inputs": ["secret-science-pack"]
                }
            }
        });
        runtime.install_context(
            "test-context".to_string(),
            PrototypeStore::load(&dump).unwrap(),
        );
        runtime.set_active_context(Some("test-context".to_string()));
        let project = new_project(&mut runtime);

        assert!(runtime.set_default_milestones(project).unwrap());
        let settings = &runtime.state.project(project).unwrap().settings;
        let nodes: Vec<Accessible> = settings
            .milestones
            .iter()
            .map(|milestone| milestone.node.clone())
            .collect();
        assert_eq!(
            nodes,
            vec![
                Accessible::Item("automation-science-pack".to_string()),
                Accessible::Item("chemical-science-pack".to_string()),
            ],
            "默认里程碑应为实验室输入的科技瓶物品"
        );
        assert!(
            !nodes.contains(&Accessible::Item("secret-science-pack".to_string())),
            "隐藏实验室（hidden）的输入不应进入默认里程碑"
        );
        assert!(settings.milestones.iter().all(|m| m.unlocked));

        // 锁定科技瓶里程碑 → 该物品剪枝（不可达）。
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::SetMilestoneUnlocked {
                node: Accessible::Item("automation-science-pack".to_string()),
                unlocked: false,
            },
        );
        let result = runtime.project_accessibility(project).unwrap();
        assert!(!result.is_item_accessible("automation-science-pack"));
        assert!(result.is_item_accessible("chemical-science-pack"));

        // 移除里程碑 → 列表恢复。
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::RemoveMilestone {
                node: Accessible::Item("automation-science-pack".to_string()),
            },
        );
        assert_eq!(runtime.state.project(project).unwrap().settings.milestones.len(), 1);
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

    #[test]
    fn project_productivity_auto_and_user() {
        let mut runtime = load_productivity_runtime();
        let project = new_project(&mut runtime);
        // 全可达，让两个无限产能科技都可达（等级 1）。
        dispatch_project(&mut runtime, project, ProjectAction::SetAllAccessible { enabled: true });

        let view = runtime.project_productivity(project).unwrap();
        assert!(
            (view.auto_mining - 0.1).abs() < 1e-9,
            "自动采矿应为 0.1，实际 {}",
            view.auto_mining
        );
        assert!((view.mining - 0.1).abs() < 1e-9, "最终采矿应为 0.1");
        let steel = view.recipes.iter().find(|r| r.recipe == "steel-plate").unwrap();
        assert!((steel.value - 0.1).abs() < 1e-9, "steel-plate 自动应 0.1");
        assert_eq!(steel.source, "auto");

        // 2.b：用户把 mining-prod 研究到 50 级 → 最终采矿 = 0.1×50 = 5.0。
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::SetInfiniteTechLevel {
                level: InfiniteTechLevel { tech: "mining-prod".to_string(), level: 50 },
            },
        );
        let view = runtime.project_productivity(project).unwrap();
        // 自动采矿基准（无用户等级）仍是 0.1；最终采矿被 2.b 覆盖为 5.0。
        assert!((view.auto_mining - 0.1).abs() < 1e-9, "自动采矿基准应不变");
        assert!((view.mining - 5.0).abs() < 1e-9, "最终采矿应 5.0，实际 {}", view.mining);

        // 2.a：用户把 steel-plate 的产能固定为 0.5 → source=user，替换自动 0.1。
        dispatch_project(
            &mut runtime,
            project,
            ProjectAction::SetRecipeProductivity {
                productivity: RecipeProductivity { recipe: "steel-plate".to_string(), productivity: 0.5 },
            },
        );
        let view = runtime.project_productivity(project).unwrap();
        let steel = view.recipes.iter().find(|r| r.recipe == "steel-plate").unwrap();
        assert!((steel.value - 0.5).abs() < 1e-9, "2.a 用户值应替换自动");
        assert_eq!(steel.source, "user");

        // 2.c：忽略产能 → 自动采矿 0；但用户 2.b 等级仍生效（采矿 5.0）。
        dispatch_project(&mut runtime, project, ProjectAction::SetIgnoreProductivity { ignore: true });
        let view = runtime.project_productivity(project).unwrap();
        assert!((view.auto_mining - 0.0).abs() < 1e-9, "忽略时自动采矿应 0");
        assert!((view.mining - 5.0).abs() < 1e-9, "忽略时用户 2.b 仍生效");
        assert!(view.ignore);
    }
}
