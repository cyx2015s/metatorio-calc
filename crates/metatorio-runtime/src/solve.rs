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
use crate::message::{ApplicationAction, AppMessage, ProjectAction};
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
        // 只对可能改变可达性的消息失效 `accessibilities` 缓存（可达性计算耗时，
        // 不应每次交互都重算）。其余（改目标/机制/偏好等）保留缓存。
        if message_affects_accessibility(&message) {
            self.accessibilities.clear();
        }
        self.state.dispatch(message)
    }

    /// Register a loaded prototype store under a stable context id.
    pub fn install_context(&mut self, context_id: String, prototype: PrototypeStore) {
        // 换了 store，之前的依赖图与可达性结果作废。
        self.graph_cache.remove(&context_id);
        self.accessibilities.clear();
        self.contexts.insert(context_id, prototype);
    }

    /// Drop a context's in-memory store (the on-disk cache is untouched).
    pub fn remove_context(&mut self, context_id: &str) {
        self.contexts.remove(context_id);
        self.graph_cache.remove(context_id);
        self.accessibilities.clear();
        if self.active_context.as_deref() == Some(context_id) {
            self.active_context = None;
        }
    }

    /// The context used by projects that do not pin one.
    pub fn set_active_context(&mut self, context_id: Option<String>) {
        if self.active_context != context_id {
            self.accessibilities.clear();
        }
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
    ///
    /// 复用 runtime 缓存的可达性（`project_accessibility`）：求解内部依赖的
    /// 配方/采矿产能自动推算需要可达性，若每次重新 `compute_accessibility`
    /// 在 py 上下文下就要多花 ~2.5s（"创建新工厂都卡"的主因）。
    pub fn solve_factory(
        &mut self,
        project_id: ProjectId,
        factory_id: FactoryId,
    ) -> Result<SolveResult, RuntimeError> {
        let accessibility = self.project_accessibility(project_id)?;
        let prototype = self.context_store(project_id)?;
        let project = self.state.project(project_id)?;
        let factory = self.state.factory(project_id, factory_id)?;
        solve_document(
            prototype,
            project,
            factory,
            project_id,
            factory_id,
            &accessibility,
        )
    }

    /// 自动规划：完整状态空间枚举候选 → 构建 LP 求解 → 保留被选中的机制并
    /// 替换工厂机制，最后重求解。返回最终 SolveResult。
    pub fn auto_plan(
        &mut self,
        project_id: ProjectId,
        factory_id: FactoryId,
    ) -> Result<SolveResult, RuntimeError> {
        let store = self.context_store(project_id)?.clone();
        let project_doc = self.state.project(project_id)?.clone();
        let factory_doc = self.state.factory(project_id, factory_id)?.clone();
        let accessibility = self.project_accessibility(project_id)?;
        let game = make_game_state_with_accessibility(&store, &project_doc, &accessibility);
        let context = metatorio_core::Context::new(&store, &game);
        let quality_level =
            |name: &str| game.qualities.iter().position(|c| c == name).unwrap_or(0);
        let options = crate::auto_plan::EnumerateOptions {
            alternative_count: project_doc.planning.alternative_count,
            machine_preferences: project_doc.planning.machine_preferences.clone(),
            enumerate_modules: project_doc.planning.enumerate_modules.clone(),
            enumerate_beacons: project_doc.planning.enumerate_beacons.clone(),
            quality_limit: game.max_quality,
            major_quality: quality_level(&factory_doc.settings.major_quality),
            planet: factory_doc.settings.planet.clone(),
            surface: factory_doc.settings.surface.clone(),
            accessibility: Some(accessibility.clone()),
        };
        let candidates = crate::auto_plan::enumerate_all(&store, &context, &options);
        let (candidates, dropped): (Vec<_>, Vec<_>) = candidates
            .into_iter()
            .partition(|m| crate::auto_plan::mechanic_accessible(&store, &accessibility, m));
        if candidates.is_empty() {
            return Err(RuntimeError::InvalidValue(if dropped.is_empty() {
                "没有可枚举的机制候选".to_string()
            } else {
                format!(
                    "所有 {} 个候选机制都不可达（目标依赖的科技未解锁？可用\"无视可达性\"开关或显式标记可达）",
                    dropped.len()
                )
            }));
        }

        // 展开全部候选为一个 LP。
        let expansion = metatorio_core::expand::expand(
            candidates.iter().enumerate().map(|(index, mechanic)| (index as u64, mechanic)),
            &context,
        );
        let mut variant_counts: HashMap<MechanicId, u16> = HashMap::new();
        let mut flows = AIndexMap::default();
        for variable in expansion.variables {
            let config = MechanicId(variable.prim_var.inner);
            let variant = variant_counts.entry(config).or_default();
            let flow_id = ExpandedVarId {
                mechanic: config,
                variant: *variant,
            };
            *variant = variant.saturating_add(1);
            flows.insert(flow_id, (variable.flow, variable.cost));
        }
        let target = factory_doc
            .targets
            .iter()
            .fold(AIndexMap::default(), |mut target, item| {
                *target.entry(item.flow.clone()).or_insert(0.0) += item.amount;
                target
            });
        let sources: Flow = factory_doc
            .external_inputs
            .iter()
            .map(|input| (input.flow.clone(), input.penalty))
            .collect();
        let mut all_sources = sources.clone();
        if let Some(planet) = factory_doc.settings.planet.as_deref() {
            let mut implicit = crate::planet::planet_autoplaced_flows(&store, planet);
            for key in all_sources.keys() {
                implicit.shift_remove(key);
            }
            all_sources.extend(implicit);
        }
        add_conversion_flows(&mut flows, &store, &target, &all_sources);
        let mut problem = SolverData::new_simple(target, flows);
        problem.sources = all_sources;
        // 自动规划默认严格供给。
        problem.strict_source = true;
        problem.strict_sink = factory_doc.strict_sink;
        problem
            .target
            .extend(factory_doc.target_expressions.iter().map(|expression| TargetSpec {
                constant: expression.constant,
                coefficients: expression
                    .terms
                    .iter()
                    .map(|term| (term.flow.clone(), term.coefficient))
                    .collect(),
            }));

        let solution = problem.solve();
        let SolverSolution::Solved { prim, prim_scale, .. } = solution else {
            let SolverSolution::NotSolved { no_provider, .. } = solution else {
                return Err(RuntimeError::InvalidValue("自动规划求解失败".to_string()));
            };
            return Err(RuntimeError::InvalidValue(format!(
                "自动规划无解（目标不可达）：无供给 {no_provider:?}"
            )));
        };
        // 保留被选中的候选（用量 > 阈值），直接替换工厂机制。
        let mut used = crate::auto_plan::used_candidates(&candidates, prim, prim_scale);
        used.sort_by_key(|mechanic| crate::document::MechanicKind::of(mechanic) as u8);
        let ids: Vec<MechanicId> = (0..used.len()).map(|_| self.state.allocate_id()).collect();
        {
            let document = &mut self.state.document;
            let factory_doc = document
                .projects
                .iter_mut()
                .find(|candidate| candidate.id == project_id)
                .and_then(|candidate| {
                    candidate.factories.iter_mut().find(|factory_doc| factory_doc.id == factory_id)
                })
                .ok_or(RuntimeError::FactoryNotFound {
                    project: project_id,
                    factory: factory_id,
                })?;
            factory_doc.mechanics = used
                .into_iter()
                .zip(ids)
                .map(|(mechanic, id)| crate::document::MechanicEntry {
                    id,
                    enabled: true,
                    mechanic,
                })
                .collect();
        }
        self.solve_factory(project_id, factory_id)
    }
}

/// 该消息是否可能改变项目的可达性（里程碑/无视可达性/绑定上下文/换仓库）。
/// 若是则需失效 `accessibilities` 缓存；否则保留（可达性计算耗时，不每次重算）。
fn message_affects_accessibility(message: &AppMessage) -> bool {
    match message {
        AppMessage::Project { action, .. } => matches!(
            action,
            ProjectAction::SetAllAccessible { .. }
                | ProjectAction::AddMilestone { .. }
                | ProjectAction::SetMilestoneUnlocked { .. }
                | ProjectAction::RemoveMilestone { .. }
                | ProjectAction::SetContext { .. }
        ),
        AppMessage::Application(action) => matches!(
            action,
            ApplicationAction::LoadGameContext { .. } | ApplicationAction::LoadCachedContext
        ),
        _ => false,
    }
}

fn solve_document(
    prototype: &PrototypeStore,
    project: &ProjectDocument,
    factory: &FactoryDocument,
    project_id: ProjectId,
    factory_id: FactoryId,
    accessibility: &metatorio_core::Accessibility,
) -> Result<SolveResult, RuntimeError> {
    let mut game = make_game_state_with_accessibility(prototype, project, accessibility);
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

    // 燃料类别子集转换：窄类别燃料可满足包含它的更多类别的机器需求
    // （复刻流体温度区间子类型放宽）。如 coal(fuel_category=chemical) 可供给
    // fuel_categories=(chemical, kr-vehicle-fuel, processed-chemical) 的锅炉。
    // 类别集合以**精确顺序**的身份标识（ItemFuel 的身份就是 category Vec 本身），
    // 故按出现的确切类别列表逐对生成 子集→超 集 的零成本转换。
    {
        // 收集去重（精确向量相等）的燃料类别集合。
        let mut category_sets: Vec<Vec<String>> = Vec::new();
        for key in seen.keys() {
            if let DualVar::ItemFuel { category, .. } = key {
                if !category.is_empty() && !category_sets.contains(category) {
                    category_sets.push(category.clone());
                }
            }
        }
        let is_proper_subset =
            |a: &Vec<String>, b: &Vec<String>| a.len() < b.len() && a.iter().all(|x| b.contains(x));
        for s1 in &category_sets {
            for s2 in &category_sets {
                if s1 == s2 || !is_proper_subset(s1, s2) {
                    continue;
                }
                for burnt in [false, true] {
                    let mut flow = Flow::default();
                    flow.insert(
                        DualVar::ItemFuel {
                            category: s1.clone(),
                            has_burnt_result: burnt,
                        },
                        -1.0,
                    );
                    flow.insert(
                        DualVar::ItemFuel {
                            category: s2.clone(),
                            has_burnt_result: burnt,
                        },
                        1.0,
                    );
                    add_aux(flows, flow);
                }
            }
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
    let options = accessibility_options(&project.settings);
    let accessibility = metatorio_core::accessibility::compute_accessibility(prototype, &options);
    make_game_state_with_accessibility(prototype, project, &accessibility)
}

/// 与 `make_game_state` 等价，但复用**外部已算好**的可达性结果，而不是在
/// 调用处重新 `compute_accessibility`。py(Pyanodon) 上下文下该计算约 2.5s，
/// 是"创建新工厂都卡"的主因——求解/自动规划/悬停展开都走这里，必须复用
/// runtime 缓存的 `project_accessibility`。
pub fn make_game_state_with_accessibility(
    prototype: &PrototypeStore,
    project: &ProjectDocument,
    accessibility: &metatorio_core::Accessibility,
) -> GameState {
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
    let productivity = productivity_for_game(prototype, project, accessibility);
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
///
/// 复用调用方算好的 `accessibility`（避免在求解/悬停等高频路径重复
/// `compute_accessibility`）。
fn productivity_for_game(
    prototype: &PrototypeStore,
    project: &ProjectDocument,
    accessibility: &metatorio_core::Accessibility,
) -> metatorio_core::ProductivityResult {
    let levels: Vec<(String, u32)> = project
        .settings
        .infinite_levels
        .iter()
        .map(|level| (level.tech.clone(), level.level))
        .collect();
    let auto = metatorio_core::productivity::compute_productivity(
        prototype,
        accessibility,
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

    /// 燃料类别子集转换：单一化学燃料应能供给多类别（chemical + …）的锅炉。
    /// 回归：KR/SE 里燃料使用者 fuel_categories 为多类别，煤炭 fuel_category
    /// 只有 chemical，此前二者 ItemFuel 身份不相等 → 自动规划选不中燃煤发电。
    #[test]
    fn fuel_category_subset_conversion_allows_narrow_fuel() {
        let store = PrototypeStore::load(&serde_json::json!({})).expect("空 dump 应可加载");
        let narrow = vec!["chemical".to_string()];
        let wide = vec![
            "chemical".to_string(),
            "kr-vehicle-fuel".to_string(),
            "processed-chemical".to_string(),
        ];
        let mut flows = AIndexMap::default();
        let fuel_var = ExpandedVarId { mechanic: MechanicId(1), variant: 0 };
        let boiler_var = ExpandedVarId { mechanic: MechanicId(2), variant: 0 };
        let mut fuel_flow = Flow::default();
        fuel_flow.insert(
            DualVar::ItemFuel { category: narrow.clone(), has_burnt_result: false },
            100.0,
        );
        flows.insert(fuel_var, (fuel_flow, 1.0));
        let mut boiler_flow = Flow::default();
        boiler_flow.insert(
            DualVar::ItemFuel { category: wide.clone(), has_burnt_result: false },
            -100.0,
        );
        flows.insert(boiler_var, (boiler_flow, 1.0));
        add_conversion_flows(&mut flows, &store, &Flow::default(), &Flow::default());
        let has_conversion = flows.values().any(|(flow, _)| {
            flow.get(&DualVar::ItemFuel { category: narrow.clone(), has_burnt_result: false })
                .copied()
                .unwrap_or(0.0)
                < 0.0
                && flow
                    .get(&DualVar::ItemFuel { category: wide.clone(), has_burnt_result: false })
                    .copied()
                    .unwrap_or(0.0)
                    > 0.0
        });
        assert!(
            has_conversion,
            "应产出 化学 → 化学+kr-vehicle-fuel+processed-chemical 的零成本转换"
        );
    }

    /// 人体工学：新建工厂默认星球为 nauvis（避免"无星球"导致环境/隐式资源缺失）。
    #[test]
    fn new_factory_defaults_to_nauvis_planet() {
        let mut runtime = load_runtime();
        let project = new_project(&mut runtime);
        runtime
            .dispatch(AppMessage::Project {
                project,
                action: ProjectAction::AddFactory {
                    name: "test factory".to_string(),
                    template: crate::message::FactoryTemplate::Empty,
                },
            })
            .unwrap();
        let factory_id = runtime.state.ui.selected_factory.unwrap();
        let settings = &runtime.state.factory(project, factory_id).unwrap().settings;
        assert_eq!(settings.planet.as_deref(), Some("nauvis"));
    }

    /// 可达性缓存失效守卫：只有可能改变可达性的消息才清空 accessibilities。
    #[test]
    fn message_accessibility_guard_targets_only_relevant_actions() {
        use crate::message::ApplicationAction;
        let project = ProjectId(1);
        let proj = |action| AppMessage::Project { project, action };
        // 相关 → true
        assert!(message_affects_accessibility(&proj(ProjectAction::SetAllAccessible { enabled: true })));
        assert!(message_affects_accessibility(&proj(ProjectAction::AddMilestone {
            node: Accessible::Item("x".to_string()),
            unlocked: true,
        })));
        assert!(message_affects_accessibility(&proj(ProjectAction::SetMilestoneUnlocked {
            node: Accessible::Item("x".to_string()),
            unlocked: false,
        })));
        assert!(message_affects_accessibility(&proj(ProjectAction::RemoveMilestone {
            node: Accessible::Item("x".to_string()),
        })));
        assert!(message_affects_accessibility(&proj(ProjectAction::SetContext {
            context: Some("c".to_string()),
        })));
        assert!(message_affects_accessibility(&AppMessage::Application(
            ApplicationAction::LoadCachedContext
        )));
        // 无关 → false
        assert!(!message_affects_accessibility(&proj(ProjectAction::SetMiningProductivity {
            productivity: 1.0,
        })));
        assert!(!message_affects_accessibility(&proj(ProjectAction::SetRecipeProductivity {
            productivity: RecipeProductivity {
                recipe: "iron-plate".to_string(),
                productivity: 0.1,
            },
        })));
        assert!(!message_affects_accessibility(&AppMessage::Application(
            ApplicationAction::InstallUpdate
        )));
    }

    /// 临时基准：py(Pyanodon) 上下文下各命令的 Rust 侧耗时（诊断用）。
    /// 机器上没有该 dump 时跳过；本地运行看各 eprintln 的毫秒。
    #[test]
    fn py_context_command_timings() {
        let path = "C:\\Users\\mirac\\AppData\\Roaming\\com.mirac.metatorio-app\\contexts\\c3544821b3232cf9\\data-raw-dump.json";
        if !std::path::Path::new(path).exists() {
            eprintln!("[skip] 无 py dump（{path}），跳过");
            return;
        }
        let raw = std::fs::read(path).expect("读 dump");
        let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump");
        let store = PrototypeStore::load(&dump).expect("dump 加载失败");
        let mut runtime = Runtime::new();
        runtime.install_context("py".to_string(), store);
        runtime.set_active_context(Some("py".to_string()));
        let project = new_project(&mut runtime);
        runtime
            .dispatch(AppMessage::Project {
                project,
                action: ProjectAction::AddFactory {
                    name: "f".to_string(),
                    template: crate::message::FactoryTemplate::Empty,
                },
            })
            .unwrap();
        let factory = runtime.state.ui.selected_factory.unwrap();
        runtime
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: crate::message::FactoryAction::Context(
                    crate::message::FactoryContextAction::SetPlanet {
                        planet: Some("nauvis".to_string()),
                    },
                ),
            })
            .unwrap();

        let now = std::time::Instant::now;
        let t = now();
        runtime.set_default_milestones(project).unwrap();
        eprintln!("[py] set_default_milestones: {} ms", t.elapsed().as_millis());
        let t = now();
        runtime.project_accessibility(project).unwrap();
        eprintln!("[py] project_accessibility: {} ms", t.elapsed().as_millis());
        let t = now();
        runtime.ordered_project_milestones(project).unwrap();
        eprintln!("[py] ordered_project_milestones: {} ms", t.elapsed().as_millis());
        let t = now();
        runtime.project_productivity(project).unwrap();
        eprintln!("[py] project_productivity: {} ms", t.elapsed().as_millis());
        let t = now();
        runtime.solve_factory(project, factory).unwrap();
        eprintln!("[py] solve_factory(空工厂): {} ms", t.elapsed().as_millis());
        let t = now();
        let _ = crate::planet::planet_autoplaced_flows(
            runtime.context_store(project).unwrap(),
            "nauvis",
        );
        eprintln!("[py] planet_autoplaced_flows(nauvis): {} ms", t.elapsed().as_millis());
        let t = now();
        let _graph = metatorio_core::build_graph(runtime.context_store(project).unwrap());
        eprintln!("[py] build_graph: {} ms", t.elapsed().as_millis());
        let nodes: Vec<Accessible> = runtime
            .state
            .project(project)
            .unwrap()
            .settings
            .milestones
            .iter()
            .map(|m| m.node.clone())
            .collect();
        let t = now();
        let _ = metatorio_core::milestone_order(
            runtime.context_store(project).unwrap(),
            &nodes,
        );
        eprintln!(
            "[py] milestone_order ({} 个里程碑): {} ms",
            nodes.len(),
            t.elapsed().as_millis()
        );
    }

    /// 诊断:py 上下文下"默认设置"(新项目、未点"设置默认里程碑"、空里程碑,
    /// 即什么都不强制可达)时,研究中心输入物品(广义科技包)的可达性。
    ///
    /// 目的:找出哪些科技包不可达、以及默认设置下不可达的原因——为
    /// "py 下大量物品/配方不可达"定位根因(是不是科技包链路本身断了)。
    #[test]
    fn py_science_pack_reachability() {
        let path = "C:\\Users\\mirac\\AppData\\Roaming\\com.mirac.metatorio-app\\contexts\\c3544821b3232cf9\\data-raw-dump.json";
        if !std::path::Path::new(path).exists() {
            eprintln!("[skip] 无 py dump（{path}），跳过");
            return;
        }
        let raw = std::fs::read(path).expect("读 dump");
        let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump");
        let store = PrototypeStore::load(&dump).expect("dump 加载失败");
        use metatorio_data::store::PrototypeGroup;
        use metatorio_data::{RecipeComponent, TechnologyComponent};

        let mut runtime = Runtime::new();
        runtime.install_context("py".to_string(), store.clone());
        runtime.set_active_context(Some("py".to_string()));
        let project = new_project(&mut runtime);
        // 默认设置:空里程碑(未设置默认里程碑) → forced 空集。
        let access = runtime.project_accessibility(project).unwrap();

        // 研究中心输入物品 = 实验室 LabComponent.inputs(广义科技包)。
        let mut packs: Vec<String> = store
            .group(PrototypeGroup::Entity)
            .filter_map(|record| record.component::<LabComponent>().map(|lab| lab.inputs.clone()))
            .flatten()
            .collect();
        packs.sort();
        packs.dedup();

        // 物品全集(Item 组 ∪ 配方原料/产物),统计可达比例。
        let mut all_items: Vec<String> = store
            .group(PrototypeGroup::Item)
            .map(|record| record.name.clone())
            .collect();
        for record in store.group(PrototypeGroup::Recipe) {
            let Some(recipe) = record.component::<RecipeComponent>() else {
                continue;
            };
            for ingredient in &recipe.ingredients {
                if let metatorio_data::types::Ingredient::Item(item) = ingredient {
                    all_items.push(item.name.clone());
                }
            }
            for result in &recipe.results {
                if let metatorio_data::types::Product::Item(product) = result {
                    all_items.push(product.name.clone());
                }
            }
        }
        all_items.sort();
        all_items.dedup();
        let reachable_items = all_items
            .iter()
            .filter(|name| access.is_item_accessible(name))
            .count();
        eprintln!(
            "\n[py] 物品: 总 {} , 默认可达 {} ({:.1}%)",
            all_items.len(),
            reachable_items,
            100.0 * reachable_items as f64 / all_items.len().max(1) as f64
        );

        // 科技/配方总量回顾。
        let tech_count = store.group(PrototypeGroup::Technology).count();
        let recipe_count = store.group(PrototypeGroup::Recipe).count();
        let tech_reachable = store
            .group(PrototypeGroup::Technology)
            .filter(|r| r.name.len() > 0 && access.is_accessible(&metatorio_core::Accessible::Tech(r.name.clone())))
            .count();
        let recipe_reachable = store
            .group(PrototypeGroup::Recipe)
            .filter(|r| access.is_accessible(&metatorio_core::Accessible::Recipe(r.name.clone())))
            .count();
        eprintln!("[py] 科技: 总 {tech_count} , 默认可达 {tech_reachable};配方: 总 {recipe_count} , 默认可达 {recipe_reachable}");

        // 建 物品→产出配方 反查表(等价 GraphData.recipes_by_product)。
        use std::collections::HashMap;
        let mut recipes_by_product: HashMap<String, Vec<String>> = HashMap::new();
        for record in store.group(PrototypeGroup::Recipe) {
            let Some(recipe) = record.component::<RecipeComponent>() else {
                continue;
            };
            for result in &recipe.results {
                match result {
                    metatorio_data::types::Product::Item(product) => {
                        recipes_by_product.entry(product.name.clone()).or_default().push(record.name.clone());
                    }
                    metatorio_data::types::Product::Fluid(product) => {
                        recipes_by_product.entry(product.name.clone()).or_default().push(record.name.clone());
                    }
                    _ => {}
                }
            }
        }
        // 建 配方→解锁科技 表(等价 GraphData.techs_by_unlock)。
        let mut unlock_by_recipe: HashMap<String, Vec<String>> = HashMap::new();
        for record in store.group(PrototypeGroup::Technology) {
            let Some(tech) = record.component::<TechnologyComponent>() else {
                continue;
            };
            for effect in &tech.effects {
                if let metatorio_data::types::Modifier::UnlockRecipe(unlock) = effect {
                    unlock_by_recipe.entry(unlock.recipe.clone()).or_default().push(record.name.clone());
                }
            }
        }

        eprintln!("\n[py] 科技包(实验室输入物品)共 {} 个:", packs.len());
        let mut unreachable_packs: Vec<&String> = Vec::new();
        for pack in &packs {
            if access.is_item_accessible(pack) {
                eprintln!("  [可达] {pack}");
                continue;
            }
            unreachable_packs.push(pack);
            let producers = recipes_by_product.get(pack).cloned().unwrap_or_default();
            eprintln!("  [不可达] {pack}");
            if producers.is_empty() {
                eprintln!("      · 无产出配方(仅能经矿藏/变质/发射/星球获得或纯运行时机制)");
                continue;
            }
            for recipe_name in &producers {
                let Some(recipe) = store
                    .get(PrototypeGroup::Recipe, recipe_name)
                    .and_then(|r| r.component::<RecipeComponent>())
                else {
                    continue;
                };
                let enabled = recipe.enabled;
                let r_reachable = access.is_accessible(&metatorio_core::Accessible::Recipe(recipe_name.clone()));
                let unlock_techs = unlock_by_recipe.get(recipe_name).cloned().unwrap_or_default();
                let mut blocked: Vec<String> = Vec::new();
                for ingredient in &recipe.ingredients {
                    match ingredient {
                        metatorio_data::types::Ingredient::Item(item) if !access.is_item_accessible(&item.name) => {
                            blocked.push(format!("物品:{}", item.name));
                        }
                        metatorio_data::types::Ingredient::Fluid(fluid) if !access.is_accessible(&metatorio_core::Accessible::Fluid(fluid.name.clone())) => {
                            blocked.push(format!("流体:{}", fluid.name));
                        }
                        _ => {}
                    }
                }
                eprintln!(
                    "      · 配方 {recipe_name}: enabled={enabled} reachable={r_reachable} unlock_techs={unlock_techs:?} 缺原料={blocked:?}"
                );
            }
        }
        eprintln!("\n[py] 不可达科技包共 {} 个:\n  {}", unreachable_packs.len(), unreachable_packs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  "));
    }

    /// 诊断:沿"可达配方 → 第一个不可达原料"递归下钻,定位科技包不可达的
    /// 根因链(哪一环先断、以及为什么——无配方/配方不可达/原料不可达)。
    #[test]
    fn py_trace_science_pack_ingredients() {
        let path = "C:\\Users\\mirac\\AppData\\Roaming\\com.mirac.metatorio-app\\contexts\\c3544821b3232cf9\\data-raw-dump.json";
        if !std::path::Path::new(path).exists() {
            eprintln!("[skip] 无 py dump（{path}），跳过");
            return;
        }
        let raw = std::fs::read(path).expect("读 dump");
        let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump");
        let store = PrototypeStore::load(&dump).expect("dump 加载失败");
        use metatorio_data::store::PrototypeGroup;
        use metatorio_data::{BoilerComponent, RecipeComponent, TechnologyComponent};

        let mut runtime = Runtime::new();
        runtime.install_context("py".to_string(), store.clone());
        runtime.set_active_context(Some("py".to_string()));
        let project = new_project(&mut runtime);
        let access = runtime.project_accessibility(project).unwrap();

        use std::collections::HashMap;
        let mut recipes_by_product: HashMap<String, Vec<String>> = HashMap::new();
        for record in store.group(PrototypeGroup::Recipe) {
            let Some(recipe) = record.component::<RecipeComponent>() else {
                continue;
            };
            for result in &recipe.results {
                match result {
                    metatorio_data::types::Product::Item(product) => recipes_by_product
                        .entry(product.name.clone())
                        .or_default()
                        .push(record.name.clone()),
                    metatorio_data::types::Product::Fluid(product) => recipes_by_product
                        .entry(product.name.clone())
                        .or_default()
                        .push(record.name.clone()),
                    _ => {}
                }
            }
        }
        let mut unlock_by_recipe: HashMap<String, Vec<String>> = HashMap::new();
        for record in store.group(PrototypeGroup::Technology) {
            let Some(tech) = record.component::<TechnologyComponent>() else {
                continue;
            };
            for effect in &tech.effects {
                if let metatorio_data::types::Modifier::UnlockRecipe(unlock) = effect {
                    unlock_by_recipe.entry(unlock.recipe.clone()).or_default().push(record.name.clone());
                }
            }
        }

        // 递归链条:选一个"可用配方"(enable 或解锁科技可达),沿其第一个
        // 不可达原料下钻,直到无配方 / 配方不可达 / 深度耗尽。
        fn chain(
            access: &metatorio_core::Accessibility,
            store: &PrototypeStore,
            recipes_by_product: &HashMap<String, Vec<String>>,
            unlock_by_recipe: &HashMap<String, Vec<String>>,
            name: &str,
            depth: usize,
            seen: &mut Vec<String>,
        ) -> String {
            if access.is_item_accessible(name) {
                return "可达".to_string();
            }
            if depth > 5 {
                return format!("…(深度 {depth}, {name})");
            }
            if seen.contains(&name.to_string()) {
                return format!("→ 循环({name})");
            }
            seen.push(name.to_string());
            let producers = recipes_by_product.get(name).cloned().unwrap_or_default();
            if producers.is_empty() {
                return format!("({name}: 无产出配方/无矿藏变质发射起源)");
            }
            // 选可用配方:优先 enabled,其次解锁科技可达。
            let mut chosen: Option<String> = None;
            for recipe_name in &producers {
                let Some(recipe) = store
                    .get(PrototypeGroup::Recipe, recipe_name)
                    .and_then(|r| r.component::<RecipeComponent>())
                else {
                    continue;
                };
                if recipe.enabled {
                    chosen = Some(recipe_name.clone());
                    break;
                }
                if access.is_accessible(&metatorio_core::Accessible::Recipe(recipe_name.clone())) {
                    chosen = Some(recipe_name.clone());
                }
            }
            let Some(recipe_name) = chosen else {
                let reasons: Vec<String> = producers
                    .iter()
                    .map(|recipe_name| {
                        let unlock = unlock_by_recipe.get(recipe_name).cloned().unwrap_or_default();
                        format!("{recipe_name}(enabled=false, 解锁科技={unlock:?})")
                    })
                    .collect();
                return format!("({name}: 所有产出配方均不可达 → {})", reasons.join("; "));
            };
            let recipe = store
                .get(PrototypeGroup::Recipe, &recipe_name)
                .and_then(|r| r.component::<RecipeComponent>())
                .expect("选中的配方应存在");
            let mut blocked: Vec<String> = Vec::new();
            for ingredient in &recipe.ingredients {
                match ingredient {
                    metatorio_data::types::Ingredient::Item(item) => {
                        if !access.is_item_accessible(&item.name) {
                            blocked.push(item.name.clone());
                        }
                    }
                    metatorio_data::types::Ingredient::Fluid(fluid) => {
                        if !access.is_accessible(&metatorio_core::Accessible::Fluid(fluid.name.clone())) {
                            blocked.push(format!("流体:{}", fluid.name));
                        }
                    }
                    _ => {}
                }
            }
            if blocked.is_empty() {
                return format!("({name} ← {recipe_name}: 配方可用但不可达?)");
            }
            // 从第一个不可达原料下钻。
            let next = blocked.remove(0);
            let reason = chain(access, store, recipes_by_product, unlock_by_recipe, &next, depth + 1, seen);
            format!("{name} ← {recipe_name} 缺原料 → {reason}")
        }

        for target in [
            "py-science-pack-1",
            "logistic-science-pack",
            "chemical-science-pack",
        ] {
            let mut seen = Vec::new();
            let c = chain(&access, &store, &recipes_by_product, &unlock_by_recipe, target, 0, &mut seen);
            eprintln!("\n[chain] {target}:\n  {c}");
        }

        // 单独看 fawogae-substrate / flask:是否连"可用配方"都没有。
        for probe in ["fawogae-substrate", "flask", "alien-sample01", "solidified-sarcorus", "advanced-circuit", "optical-fiber", "workers-food-03", "nv-center", "pi-josephson-junction"] {
            let mut seen = Vec::new();
            let c = chain(&access, &store, &recipes_by_product, &unlock_by_recipe, probe, 0, &mut seen);
            eprintln!("[probe] {probe}:\n  {c}");
        }

        // 流体侧:steam / water 到底怎么来的?有没有产出配方、是否星球自动资源。
        let nauvis_flows = crate::planet::planet_autoplaced_flows(&store, "nauvis");
        let flu = |name: &str| {
            let producers = recipes_by_product.get(name).cloned().unwrap_or_default();
            let mut info = Vec::new();
            for recipe_name in &producers {
                let Some(recipe) = store
                    .get(PrototypeGroup::Recipe, recipe_name)
                    .and_then(|r| r.component::<RecipeComponent>())
                else {
                    continue;
                };
                let unlock = unlock_by_recipe.get(recipe_name).cloned().unwrap_or_default();
                info.push(format!(
                    "{recipe_name}(enabled={}, reachable={}, 解锁={unlock:?})",
                    recipe.enabled,
                    access.is_accessible(&metatorio_core::Accessible::Recipe(recipe_name.clone()))
                ));
            }
            let in_nauvis_any = nauvis_flows.iter().any(|(flow, _)| matches!(flow, metatorio_core::DualVar::Fluid { name: n, .. } if n == name));
            let node_reachable = access.is_accessible(&metatorio_core::Accessible::Fluid(name.to_string()));
            eprintln!(
                "[fluid] {name}: 可达(node)={node_reachable}, 产出配方={info:?}, nauvis自动流={in_nauvis_any}"
            );
        };
        flu("steam");
        flu("water");
        flu("pure-water");
        flu("hot-water");

        // 锅炉机制:steam 由锅炉加热水而来,不是普通配方。看锅炉实体本身
        // 及其 place_result 物品在默认设置下是否可达(可达则锅炉应能产 steam)。
        eprintln!("\n[py] 锅炉实体:");
        let mut boiler_count = 0usize;
        let mut boiler_reachable = 0usize;
        for record in store.group(PrototypeGroup::Entity) {
            let Some(boiler) = record.component::<BoilerComponent>() else {
                continue;
            };
            boiler_count += 1;
            let out_filter = boiler
                .output_fluid_box
                .filter
                .as_deref()
                .or(boiler.fluid_box.filter.as_deref())
                .unwrap_or("");
            // 该实体能否产出 steam。
            let makes_steam = out_filter.contains("steam");
            // 实体可达性:实体节点(通过 place_result/placeable_by)。
            let entity_reachable =
                access.is_accessible(&metatorio_core::Accessible::Entity(record.name.clone()));
            if makes_steam {
                eprintln!("  · {} (→ {out_filter}) 实体可达={entity_reachable}", record.name);
            }
            if entity_reachable {
                boiler_reachable += 1;
            }
        }
        eprintln!("[py] 锅炉实体: 总 {boiler_count}, 默认可达(实体节点) {boiler_reachable}");
    }
}
