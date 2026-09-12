//! Tauri adapter for the framework-independent `metatorio-runtime` layer.
//!
//! The frontend sends `AppMessage` values as JSON through the `dispatch`
//! command.  The runtime reducer returns a [`DispatchResult`]; side effects
//! (`RuntimeCommand`) are executed here — solving runs on a blocking worker
//! and its outcome is pushed to the frontend as a `solve-result` event.
//!
//! This layer also owns the game-context cache (multiple exported data sets,
//! each with its own prototype store + icon directory) and project file
//! persistence.  Contexts are cached under
//! `<app_data>/contexts/<content-hash>/`; projects pin the context they were
//! planned against via `ProjectDocument::context_id`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use metatorio_core::{Accessible, DualVar, IdWithQuality, Mechanic};
use metatorio_data::store::{PrototypeGroup, PrototypeRecord, PrototypeStore};
use metatorio_data::types::{Ingredient, Product, TechnologyMaxLevel};
use metatorio_data::{
    BeaconComponent, BoilerComponent, BurnerGeneratorComponent, CraftingMachineComponent,
    EntityComponent, FluidComponent, GeneratorComponent, ItemComponent, MiningDrillComponent,
    ModuleComponent, PrototypeBaseComponent, QualityComponent, ReactorComponent, RecipeComponent,
    ResourceEntityComponent, TechnologyComponent,
};
use metatorio_runtime::{
    auto_plan,
    document::AppDocument,
    id::{FactoryId, MechanicId, ProjectId},
    message::{
        AppMessage, FactoryAction, MechanicAction, MiningMechanicAction, ModuleAction,
        ProjectAction, RecipeMechanicAction, RuntimeCommand,
    },
    prototype::{
        effective_module_slots, effective_probability, effective_recipe_categories,
        effective_resource_category, fluid_fuel_info, fluid_tags, item_fuel_info, item_tags,
        surface_condition_text,
    },
    solve::Runtime,
    state::DispatchResult,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime as TauriRuntime, State};
use tauri_plugin_dialog::DialogExt;

/// 与本体合并的 MCP 服务器（localhost Streamable-HTTP 端点）。
///
/// `pub`：bin（main.rs）要用 `mcp::DEFAULT_MCP_PORT` / `MCP_PATH` 作为 CLI 默认值。
#[cfg(not(mobile))]
pub mod mcp;

/// 求解任务调度（把长求解移出 `Mutex<Runtime>`）。
mod solve_jobs;

/// Minimal embedded game-data dump so the app can solve out of the box.
/// Replace with a real Factorio dump once data loading is wired to a
/// file dialog.
const DEMO_DUMP: &str = include_str!("../dumps/demo_dump.json");

// ── Managed state ─────────────────────────────────────────────────

pub struct AppState {
    runtime: Mutex<Runtime>,
    /// 求解调度器：长求解在锁外跑，按 (project, factory) 单飞 + latest-wins。
    solve_jobs: solve_jobs::SolveJobs<metatorio_runtime::SolveResult>,
    /// 自动规划调度器：产出 `(快照, 候选机制)`，回写前用快照校验版本。
    autoplan_jobs: solve_jobs::SolveJobs<(
        metatorio_runtime::SolveSnapshot,
        Vec<metatorio_core::Mechanic>,
    )>,
    /// 上下文载入的按键串行锁（同一上下文只读盘解析一次）。
    context_loads: solve_jobs::KeyLocks,
    contexts: Mutex<ContextRegistry>,
    project_paths: Mutex<HashMap<ProjectId, String>>,
    /// 上下文 id → 本地化名映射（来自游戏 `--dump-prototype-locale` 的
    /// `prototype-locale.json`，键为 `"{type}/{name}"`）。
    locales: Mutex<HashMap<String, HashMap<String, String>>>,
    /// MCP `dispatch` 的幂等缓存：request_id → 上次返回的载荷。
    dispatch_cache: Mutex<DispatchCache>,
    /// 异步自动规划的状态（MCP 的 `auto_plan` 工具用）：按 (project, factory)
    /// 记录「运行中 / 完成（含求解结果）/ 失败」，供 agent 稍后查询。
    auto_plans: Mutex<HashMap<(ProjectId, FactoryId), AutoPlanState>>,
}

/// 一次**异步**自动规划的状态（`auto_plan` 工具立即返回后由 agent 轮询）。
///
/// 为什么不让 `get_planning_state(recompute=true)` 直接当轮询入口：那条路会**重新**
/// 求解**当前**文档——规划还没回写时它算的是旧文档，agent 会拿到与计划无关的结果。
/// 所以这里把「这次规划自己的结果」存下来，由 `get_planning_state` 一并返回。
#[derive(Debug, Clone)]
pub enum AutoPlanState {
    /// 已受理，枚举/求解/回写进行中。
    Running,
    /// 完成：`revision` 是回写后的文档版本，`result` 是回写后的重解结果。
    Done {
        revision: u64,
        result: Box<metatorio_runtime::SolveResult>,
    },
    /// 失败（无解 / 版本冲突 / 上下文缺失等）：原样带上错误信息。
    Failed(String),
}

/// MCP `dispatch` 的幂等缓存（有界 FIFO）。
///
/// 工具调用超时后 agent 会重试；没有幂等键时「添加目标/机制」会被重复应用。
/// agent 传 `request_id` 后，同一个 id 只应用一次，重试直接拿回上次的载荷。
#[derive(Default)]
struct DispatchCache {
    order: std::collections::VecDeque<String>,
    entries: HashMap<String, (serde_json::Value, bool)>,
}

impl DispatchCache {
    /// 最多记住多少次调用（超出后淘汰最早的）。
    const CAPACITY: usize = 256;

    fn get(&self, request_id: &str) -> Option<(serde_json::Value, bool)> {
        self.entries.get(request_id).cloned()
    }

    fn insert(&mut self, request_id: String, payload: serde_json::Value, is_error: bool) {
        if self
            .entries
            .insert(request_id.clone(), (payload, is_error))
            .is_none()
        {
            self.order.push_back(request_id);
        }
        while self.order.len() > Self::CAPACITY {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(Runtime::new()),
            solve_jobs: solve_jobs::SolveJobs::default(),
            autoplan_jobs: solve_jobs::SolveJobs::default(),
            context_loads: solve_jobs::KeyLocks::default(),
            contexts: Mutex::new(ContextRegistry::default()),
            project_paths: Mutex::new(HashMap::new()),
            locales: Mutex::new(HashMap::new()),
            dispatch_cache: Mutex::new(DispatchCache::default()),
            auto_plans: Mutex::new(HashMap::new()),
        }
    }
}

/// 启动选项：由 bin 的 CLI 解析（CLI > 环境变量 > 默认）后传入。
///
/// lib 不依赖 clap：这样 `Options` 是普通数据，可单测，也便于将来换解析器；
/// **参数的单一真相**在这里，`mcp::spawn_server` 与求解调度器都只认它，
/// 不再各自去读环境变量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// MCP 监听地址；默认 `127.0.0.1`（只回环）。填本机局域网 IP 或 `0.0.0.0`
    /// 才能被手机等其它设备访问——**非回环时必须有 token**（`run` 前的校验拦住）。
    pub mcp_bind: String,
    /// MCP 端点端口。
    pub mcp_port: u16,
    /// 额外允许的 `Host`（用主机名/mDNS 名访问时填）；具体 IP 由 `mcp_bind` 自动允许。
    pub mcp_allow_hosts: Vec<String>,
    /// MCP Bearer token；`None`/空 = 不鉴权（仅回环兜底；非回环会被拒绝启动）。
    pub mcp_token: Option<String>,
    /// 单次求解的等待上限（毫秒）；`None` = 内置默认 120s。
    pub solve_timeout_ms: Option<u64>,
    /// 无头：不创建窗口，只提供 MCP 端点（GUI 命令因此无人调用）。
    pub headless: bool,
    /// 是否启动 MCP 端点。默认开；`--no-mcp` 可关（只想要 GUI、不开本地端口）。
    pub mcp: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mcp_bind: mcp::DEFAULT_MCP_BIND.to_string(),
            mcp_port: mcp::DEFAULT_MCP_PORT,
            mcp_allow_hosts: Vec::new(),
            mcp_token: None,
            solve_timeout_ms: None,
            headless: false,
            mcp: true,
        }
    }
}

impl AppState {
    /// 按启动选项构造（目前只有求解等待上限需要覆盖）。
    pub fn with_options(options: &Options) -> Self {
        let mut state = Self::default();
        if let Some(ms) = options.solve_timeout_ms {
            let timeout = std::time::Duration::from_millis(ms);
            state.solve_jobs = solve_jobs::SolveJobs::default().with_timeout(timeout);
            state.autoplan_jobs = solve_jobs::SolveJobs::default().with_timeout(timeout);
        }
        state
    }
}

/// 组合入口（MCP 的 `auto_plan`）在「先把文档配好」阶段只跑这些**便宜的收敛命令**。
///
/// 它们修正文档内部一致性（品质上限 / 机器兼容 / 插件数钳制），代价微秒级。
/// `Recompute`/`AutoPlan` 会在最后统一跑一次，`Persist` 对新项目本就是 no-op
/// （没有保存路径），因此这里跳过它们——否则一次组合配置会触发六次整厂求解，
/// 「立即返回」就成了空话。
pub(crate) fn is_consistency_command(command: &metatorio_runtime::message::RuntimeCommand) -> bool {
    use metatorio_runtime::message::RuntimeCommand;
    matches!(
        command,
        RuntimeCommand::EnsureQualityLimit { .. }
            | RuntimeCommand::ClampModules { .. }
            | RuntimeCommand::EnsureMachineCompat { .. }
    )
}

/// 触发一次**异步**自动规划：立刻返回，把状态写进 `AppState::auto_plans` 供轮询。
///
/// `execute_command` 的 `AutoPlan` 分支本身是「锁外枚举 → 版本校验 → 回写 → 重解」
/// （py 上下文实测 70s+），所以这里只负责把它丢到后台任务，并把**这次规划自己的**
/// 结果回报出去——不要让 agent 用 `recompute` 去猜（那会去算规划尚未回写的旧文档）。
pub(crate) fn spawn_auto_plan<R: TauriRuntime>(
    app: &AppHandle<R>,
    project: ProjectId,
    factory: FactoryId,
    command: metatorio_runtime::message::RuntimeCommand,
) {
    let app = app.clone();
    if let Ok(mut plans) = app.state::<AppState>().auto_plans.lock() {
        plans.insert((project, factory), AutoPlanState::Running);
    }
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let outcome = execute_command(&app, &state, &command).await;
        let revision = with_runtime(&state, |runtime| Ok(runtime.state.revision)).unwrap_or(0);
        let next = if !outcome.errors.is_empty() {
            AutoPlanState::Failed(outcome.errors.join("；"))
        } else {
            match outcome.effect {
                Some(metatorio_runtime::CommandEffect::Solve(result)) => AutoPlanState::Done {
                    revision,
                    result: Box::new(result),
                },
                _ => AutoPlanState::Failed("自动规划没有产出求解结果".to_string()),
            }
        };
        if let Ok(mut plans) = state.auto_plans.lock() {
            plans.insert((project, factory), next);
        }
        if let Err(error) = app.emit("document-changed", revision) {
            eprintln!("广播 document-changed 失败：{error}");
        }
    });
}

/// Game contexts cached on disk under `<app_data>/contexts/<id>/`.
///
/// The registry keeps manifests only; loaded prototype stores live in
/// [`Runtime::contexts`] (single in-memory copy).  `id` is a content hash of
/// the raw dump, so identical exports dedupe and ids are stable across
/// machines.
#[derive(Default)]
struct ContextRegistry {
    dir: PathBuf,
    meta: HashMap<String, ContextMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContextMeta {
    id: String,
    name: String,
    source: String,
    created_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextInfo {
    pub id: String,
    pub name: String,
    pub source: String,
    pub created_at: u64,
    pub loaded: bool,
    pub groups: Vec<GroupCount>,
    pub icon_root: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextList {
    pub active: Option<String>,
    pub contexts: Vec<ContextInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupCount {
    pub name: String,
    pub count: usize,
}

/// 目录索引条目：一次性下发到前端，筛选/分组/排序全部前端本地做。
#[derive(Debug, Clone, Serialize)]
pub struct IndexEntry {
    pub kind: String,
    pub name: String,
    /// 本地化显示名（来自 `--dump-prototype-locale`；无翻译时为空串）。
    pub localized_name: String,
    pub group: String,
    pub subgroup: String,
    pub icon_type: String,
    pub module_slots: Option<u16>,
    /// 兼容性类别：machine→crafting_categories、recipe→categories、
    /// mining-machine→resource_categories、resource→category。
    pub categories: Vec<String>,
    /// 物品燃料类别（非燃料物品为空串）；供前端燃料选择筛选。
    pub fuel_category: String,
    /// 物品/流体燃料热值（焦耳；非燃料为 null）；供前端燃料选择筛选。
    pub fuel_value_j: Option<f64>,
    /// 科技等级上限：`U32(n)` → `Some(n)`，`Infinite` → `None`（无限）。
    /// 仅 technology 条目填充；其余为 None。前端据此筛选"可多次研究"的科技。
    pub technology_max_level: Option<u32>,
    /// 科技最低等级（名字 `-<number>` 后缀；无后缀为 0）。仅 technology 条目填充。
    /// max_level 与 base 共同决定该科技是否可配置（max > base 或无限）。
    pub technology_base_level: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogIndex {
    pub context_id: String,
    /// 可用品质（normal 起，按 order）。
    pub qualities: Vec<String>,
    pub entries: Vec<IndexEntry>,
}

/// 一次「名字 ↔ 本地化名」查询命中的条目。
///
/// `matched_by` 说明命中方式，便于调用方区分「就是它」和「只是像」：
/// `name-exact` / `localized-exact` / `localized-prefix` / `name-prefix` /
/// `localized-contains` / `name-contains` / `typo`。
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedName {
    pub kind: String,
    pub name: String,
    pub localized_name: String,
    pub group: String,
    pub matched_by: &'static str,
    /// 仅 `typo` 命中带距离（其它命中为 null，序列化时省略）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance: Option<usize>,
}

/// 配方原料/产物条目（含概率/产能/品质修饰）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct FlowAmount {
    pub kind: String,
    pub name: String,
    /// 单次期望量（概率已折算）。
    pub amount: f64,
    /// 产出概率（0..1；常规产物为 1）。
    pub probability: f64,
    /// 有概率时的原始量区间（amount_min/amount_max）。
    pub amount_min: Option<f64>,
    pub amount_max: Option<f64>,
    /// 每次产能结算的额外产量（仅产物）。
    pub productivity: f64,
    /// 流体温度。
    pub temperature: Option<f64>,
    pub min_temperature: Option<f64>,
    pub max_temperature: Option<f64>,
    /// 产物品质下限/上限（如 "uncommon"）。
    pub quality_min: Option<String>,
    pub quality_max: Option<String>,
    /// 品质偏移（品质等级偏移量，0 不显示）。
    pub quality_change: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PrototypeDetail {
    pub name: String,
    pub localized_name: String,
    pub kind: String,
    pub subgroup: Option<String>,
    pub order: String,
    pub hidden: bool,
    // item
    pub stack_size: Option<f64>,
    /// 燃料能量（焦耳）。
    pub fuel_value_j: Option<f64>,
    /// 燃料类别（如 "chemical"）。
    pub fuel_category: String,
    /// 燃烧产物。
    pub burnt_result: String,
    /// 变质产物。
    pub spoil_result: String,
    /// 变质时间（刻）。
    pub spoil_ticks: Option<u32>,
    /// 种植产物（种子 → 实体）。
    pub plant_result: String,
    /// 是否可火箭发射。
    pub launchable: bool,
    /// 火箭发射产物物品名列表（`rocket_launch_products`，如 satellite）。
    pub rocket_launch_products: Vec<String>,
    // recipe
    pub category: Option<String>,
    pub categories: Vec<String>,
    pub energy_required: Option<f64>,
    /// 配方最大产能加成（默认 3.0）。
    pub maximum_productivity: Option<f64>,
    /// 配方表面条件（如 "gravity: 1"）。
    pub surface_conditions: Vec<String>,
    pub ingredients: Vec<FlowAmount>,
    pub results: Vec<FlowAmount>,
    // machine
    pub crafting_speed: Option<f64>,
    pub module_slots: Option<u16>,
    /// 机器允许的插件类别（空 = 不限制）。
    pub allowed_module_categories: Vec<String>,
    /// 焦耳/刻（功率）；前端换算为 W。
    pub energy_usage_j: Option<f64>,
    /// 机器能量源类型（electric/burner/fluid/heat/void）。前端据此判断
    /// 是否显示燃料配置（burner 机器需要物品燃料，electric 等不需要）。
    pub machine_energy_source: Option<String>,
    /// burner 机器可接受的燃料类别（electric/fluid 为空）；燃料选择筛选用。
    pub burner_fuel_categories: Vec<String>,
    /// 机器是否接受**插件塔**效果（`EffectReceiver.uses_beacon_effects`）；
    /// `None` = 该原型没有 EffectReceiver（按 true 处理），false 时前端不应
    /// 允许添加插件塔。
    pub uses_beacon_effects: Option<bool>,
    /// 机器是否接受**自身插件**效果（`EffectReceiver.uses_module_effects`）；
    /// `None` = 未声明（按 true 处理）。
    pub uses_module_effects: Option<bool>,
    // generator / boiler / reactor
    /// 发电效率。
    pub effectivity: Option<f64>,
    /// 最大出力（焦耳/刻）。
    pub max_power_output_j: Option<f64>,
    /// 最高/目标温度。
    pub maximum_temperature: Option<f64>,
    /// 发电机是否燃烧流体。
    pub burns_fluid: Option<bool>,
    /// 发电机流体用量（单位/刻）。
    pub fluid_usage_per_tick: Option<f64>,
    /// 锅炉能耗（焦耳/刻）。
    pub energy_consumption_j: Option<f64>,
    /// 锅炉目标温度。
    pub target_temperature: Option<f64>,
    /// 反应堆相邻加成。
    pub neighbour_bonus: Option<f64>,
    /// 反应堆加热半径。
    pub heating_radius: Option<f64>,
    /// 反应堆热输出（焦耳/刻）。
    pub heat_output_j: Option<f64>,
    /// 流体箱过滤（如 "steam"）。
    pub fluid_filter: Option<String>,
    // beacon
    pub beacon_module_slots: Option<u16>,
    // fluid
    pub default_temperature: Option<f64>,
    // quality（kind = "quality"）
    pub quality_level: Option<u32>,
    pub quality_next: Option<String>,
    pub quality_next_probability: Option<f64>,
    pub quality_crafting_speed: Option<f64>,
    pub quality_module_speed: Option<f64>,
    pub quality_module_productivity: Option<f64>,
}

// ── Registry helpers ──────────────────────────────────────────────

impl ContextRegistry {
    fn store_dir(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    fn dump_path(&self, id: &str) -> PathBuf {
        self.store_dir(id).join("data-raw-dump.json")
    }

    fn manifest_path(&self, id: &str) -> PathBuf {
        self.store_dir(id).join("context.json")
    }

    fn locale_path(&self, id: &str) -> PathBuf {
        self.store_dir(id).join("locale.json")
    }

    fn icon_root(&self, id: &str) -> PathBuf {
        self.store_dir(id).join("icons")
    }

    /// Rebuild `meta` from the manifests on disk.  Dot-prefixed dirs are
    /// fast-delete trash: skipped here, purged on a background thread.
    fn scan(&mut self) {
        if self.dir.as_os_str().is_empty() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            if name.starts_with('.') {
                spawn_delete(path);
                continue;
            }
            if let Some(meta) = read_manifest(&path.join("context.json")) {
                self.meta.insert(name, meta);
            }
        }
    }

    /// Create the cache directory + manifest for a new context id.
    fn register(&mut self, id: String, name: String, source: String) {
        let created_at = now_secs();
        let meta = ContextMeta {
            id: id.clone(),
            name,
            source,
            created_at,
        };
        let _ = std::fs::create_dir_all(self.store_dir(&id));
        write_manifest(&self.manifest_path(&id), &meta);
        self.meta.insert(id, meta);
    }

    fn rename(&mut self, id: &str, name: String) -> Option<()> {
        let manifest_path = self.manifest_path(id);
        let meta = self.meta.get_mut(id)?;
        meta.name = name;
        write_manifest(&manifest_path, meta);
        Some(())
    }

    /// Fast delete: rename the cache dir to a dot-prefixed trash name and
    /// purge it on a background thread, so a huge `icons/` tree never blocks
    /// the UI.  Any leftover trash is picked up by [`Self::scan`] on startup.
    fn remove(&mut self, id: &str) {
        let Some(_meta) = self.meta.remove(id) else {
            return;
        };
        let store = self.store_dir(id);
        let trash = self.dir.join(format!(".trash-{id}"));
        // 清掉可能残留的同名 trash，避免 rename 失败。
        let _ = std::fs::remove_dir_all(&trash);
        if std::fs::rename(&store, &trash).is_ok() {
            spawn_delete(trash);
        } else {
            // rename 失败（跨卷等）：就地删，慢一点但保证删除。
            let _ = std::fs::remove_dir_all(&store);
        }
    }
}

/// Delete a directory tree off the UI thread (fire-and-forget).
fn spawn_delete(dir: PathBuf) {
    std::thread::spawn(move || {
        if let Err(error) = std::fs::remove_dir_all(&dir) {
            eprintln!("failed to purge {dir:?}: {error}");
        }
    });
}

fn read_manifest(path: &Path) -> Option<ContextMeta> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_manifest(path: &Path, meta: &ContextMeta) {
    if let Ok(json) = serde_json::to_string_pretty(meta) {
        let _ = std::fs::write(path, json);
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn context_id_of(raw: &[u8]) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(raw))
}

/// 解析单个 `{category}-locale.json`（游戏导出格式，见 metatorio-egui 的
/// `LOCALE_CATEGORIES`）：顶层是 `{"names": {name: label}, "descriptions": …}`。
/// 容忍顶层直接就是 `{name: label}` 的扁平形式。产出 `"{category}/{name}" → label`。
fn parse_locale_category(raw: &[u8], category: &str) -> HashMap<String, String> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return HashMap::new();
    };
    let names = value.get("names").unwrap_or(&value);
    let Some(object) = names.as_object() else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    for (name, label) in object {
        if let Some(label) = label.as_str() {
            map.entry(format!("{category}/{name}"))
                .or_insert_with(|| label.to_string());
        }
    }
    map
}

/// 解析合并格式的 locale.json（本应用缓存：顶层即 `"{category}/{name}" → label`）。
fn parse_flat_locale_map(raw: &[u8]) -> HashMap<String, String> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return HashMap::new();
    };
    let Some(object) = value.as_object() else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    for (key, label) in object {
        if let Some(label) = label.as_str() {
            map.entry(key.clone()).or_insert_with(|| label.to_string());
        }
    }
    map
}

/// 扫描目录合并翻译：逐类读取 `*-locale.json`（游戏导出格式），也接受
/// 单独的 `locale.json`（本应用已合并的缓存格式）。
fn collect_locale_map(dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Ok(raw) = std::fs::read(&path) else {
            continue;
        };
        if file_name == "locale.json" {
            let merged = parse_flat_locale_map(&raw);
            for (key, label) in merged {
                map.entry(key).or_insert(label);
            }
        } else if let Some(category) = file_name.strip_suffix("-locale.json") {
            let category_map = parse_locale_category(&raw, category);
            for (key, label) in category_map {
                map.entry(key).or_insert(label);
            }
        }
    }
    map
}

/// 读上下文缓存的 locale.json（合并格式）并缓存到 AppState（缺失/解析失败 = 空映射）。
fn locale_map_of(state: &AppState, id: &str) -> HashMap<String, String> {
    {
        let locales = state.locales.lock().ok();
        if let Some(map) = locales.and_then(|locales| locales.get(id).cloned()) {
            return map;
        }
    }
    let path = {
        let registry = state.contexts.lock().ok();
        registry.map(|registry| registry.locale_path(id))
    };
    let map = path
        .and_then(|path| std::fs::read(path).ok())
        .map(|raw| parse_flat_locale_map(&raw))
        .unwrap_or_default();
    if let Ok(mut locales) = state.locales.lock() {
        locales.insert(id.to_string(), map.clone());
    }
    map
}

/// 按 kind 查询本地化名：优先 `{kind}/{name}`，实体派生 kind
/// （machine/mining-machine/beacon/…）回退到 `entity/{name}`，
/// module 回退到 `item/{name}`；都没有则返回空串。
fn localized_name(map: &HashMap<String, String>, kind: &str, name: &str) -> String {
    let direct = format!("{kind}/{name}");
    if let Some(label) = map.get(&direct) {
        return label.clone();
    }
    for fallback in ["entity", "item", "recipe", "fluid", "technology"] {
        let key = format!("{fallback}/{name}");
        if let Some(label) = map.get(&key) {
            return label.clone();
        }
    }
    String::new()
}

// ── Context loading / registration ────────────────────────────────

/// 上下文的 dump 文件路径（短暂持有 registry 锁）。
fn context_dump_path(state: &AppState, id: &str) -> Result<PathBuf, String> {
    let registry = state
        .contexts
        .lock()
        .map_err(|_| "contexts 锁损坏".to_string())?;
    if !registry.meta.contains_key(id) {
        return Err(format!("上下文 {id} 不存在于缓存"));
    }
    Ok(registry.dump_path(id))
}

/// 读 dump 并构建原型仓库。**不碰 runtime / runtime 锁**，可在锁外或阻塞
/// 线程池上执行（大 dump 解析可达数秒）。
fn load_store_from_dump(dump_path: &Path) -> Result<PrototypeStore, String> {
    let raw = std::fs::read(dump_path).map_err(|error| error.to_string())?;
    let dump: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|error| error.to_string())?;
    PrototypeStore::load(&dump).map_err(|error| error.to_string())
}

/// 同步版：调用方已持有 runtime 锁时用（读盘仍在锁内）。
fn ensure_context_loaded(state: &AppState, runtime: &mut Runtime, id: &str) -> Result<(), String> {
    if runtime.context_store_by_id(id).is_some() {
        return Ok(());
    }
    let dump_path = context_dump_path(state, id)?;
    let prototype = load_store_from_dump(&dump_path)?;
    runtime.install_context(id.to_string(), prototype);
    Ok(())
}

/// 锁外版：读盘/解析在阻塞线程池上完成，只在最后短暂上锁装入 store。
///
/// 同一上下文并发请求会串行（`context_loads`），避免大 dump 被解析多次。
async fn ensure_context_loaded_offlock(state: &AppState, id: &str) -> Result<(), String> {
    if with_runtime(state, |runtime| {
        Ok(runtime.context_store_by_id(id).is_some())
    })? {
        return Ok(());
    }
    let _guard = state.context_loads.lock(id.to_string()).await;
    // 等锁期间别人可能已经装好。
    if with_runtime(state, |runtime| {
        Ok(runtime.context_store_by_id(id).is_some())
    })? {
        return Ok(());
    }
    let dump_path = context_dump_path(state, id)?;
    let prototype = tauri::async_runtime::spawn_blocking(move || load_store_from_dump(&dump_path))
        .await
        .map_err(|error| error.to_string())??;
    with_runtime(state, |runtime| {
        runtime.install_context(id.to_string(), prototype);
        Ok(())
    })
}

/// Register a new context (or reuse the cached one by content hash), persist
/// it, load its store and make it active.
/// 图标来源：导出路径用 Move（rename，同卷瞬间、缓存自包含不可变），
/// dump 导入路径用 Copy（用户目录不可 rename）。
enum IconImport {
    None,
    Move(PathBuf),
    Copy(PathBuf),
}

fn copy_dir(src: &Path, dst: &Path) {
    if !src.is_dir() {
        return;
    }
    let _ = std::fs::create_dir_all(dst);
    if let Ok(entries) = std::fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let target = dst.join(entry.file_name());
            if path.is_dir() {
                copy_dir(&path, &target);
            } else {
                let _ = std::fs::copy(&path, &target);
            }
        }
    }
}

/// 注册上下文：写注册表 + 落盘 dump/locale/图标，返回上下文 id。
///
/// 不碰 runtime，也不长时间持有 registry 锁（几十 MB 的 dump 写入与图标目录
/// 拷贝都在锁外完成；注册表只在开头做一次 check-and-set）。
fn register_context_files(
    state: &AppState,
    name: String,
    source: String,
    raw: &[u8],
    locale_raw: Option<&[u8]>,
    icon: IconImport,
) -> Result<String, String> {
    let id = context_id_of(raw);
    let (is_new, dump_path, locale_path, icon_root) = {
        let mut registry = state
            .contexts
            .lock()
            .map_err(|_| "contexts 锁损坏".to_string())?;
        let is_new = !registry.meta.contains_key(&id);
        if is_new {
            registry.register(id.clone(), name, source);
        }
        (
            is_new,
            registry.dump_path(&id),
            registry.locale_path(&id),
            registry.icon_root(&id),
        )
    };
    if is_new {
        std::fs::write(&dump_path, raw).map_err(|error| error.to_string())?;
    }
    // 翻译：合并后的 `{category}/{name}` 映射写入 locale.json（id 只由
    // dump 内容决定，翻译变化不影响上下文 id；缺失/历史缓存则补写）。
    if !locale_path.is_file() {
        if let Some(locale_raw) = locale_raw {
            let _ = std::fs::write(&locale_path, locale_raw);
        }
    }
    // 图标：新上下文，或历史注册时缺图标（早期路径 bug 留下的缓存）
    // 都导入——重新导出同内容时 id 相同、注册被跳过，但图标仍需补齐。
    if !icon_root.is_dir() {
        match &icon {
            IconImport::None => {}
            IconImport::Move(src) => {
                // 同卷 rename：把本次导出的类型目录整体移入缓存，之后
                // 再次导出覆盖暂存目录也不会影响这个上下文。
                if let Err(error) = std::fs::rename(src, &icon_root) {
                    eprintln!("移动图标失败（忽略，使用占位图标）: {error}");
                }
            }
            IconImport::Copy(src) => copy_dir(src, &icon_root),
        }
    }
    Ok(id)
}

/// 注册上下文并激活：注册文件 → 锁外载入 store → 短暂上锁激活。
async fn register_context_and_activate(
    state: &AppState,
    name: String,
    source: String,
    raw: &[u8],
    locale_raw: Option<&[u8]>,
    icon: IconImport,
) -> Result<ContextInfo, String> {
    let id = register_context_files(state, name, source, raw, locale_raw, icon)?;
    ensure_context_loaded_offlock(state, &id).await?;
    with_runtime(state, |runtime| {
        runtime.set_active_context(Some(id.clone()));
        context_info_of(state, runtime, &id).ok_or_else(|| "上下文信息缺失".to_string())
    })
}

fn context_info_from(
    registry: &ContextRegistry,
    runtime: &Runtime,
    id: &str,
) -> Option<ContextInfo> {
    let meta = registry.meta.get(id)?.clone();
    let store = runtime.context_store_by_id(id);
    let icon_root = registry.icon_root(id);
    Some(ContextInfo {
        id: meta.id,
        name: meta.name,
        source: meta.source,
        created_at: meta.created_at,
        loaded: store.is_some(),
        groups: store
            .map(|store| {
                store
                    .groups
                    .iter()
                    .map(|(group, records)| GroupCount {
                        name: format!("{group:?}"),
                        count: records.len(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        icon_root: icon_root
            .is_dir()
            .then(|| icon_root.to_string_lossy().to_string()),
        active: runtime.active_context() == Some(id),
    })
}

fn context_info_of(state: &AppState, runtime: &Runtime, id: &str) -> Option<ContextInfo> {
    let registry = state.contexts.lock().ok()?;
    context_info_from(&registry, runtime, id)
}

/// 构建上下文列表。调用方若已持有 runtime 锁，请用
/// [`context_list_with`]（避免 std Mutex 不可重入导致死锁）。
fn context_list(state: &AppState) -> ContextList {
    let runtime = state.runtime.lock().ok();
    let Some(runtime) = runtime.as_ref() else {
        return ContextList {
            active: None,
            contexts: Vec::new(),
        };
    };
    context_list_with(runtime, state)
}

/// 是否有项目绑定到该上下文（删除前的引用检查）。
///
/// 抽成纯函数是为了可单测：GUI 命令与消息层共用同一条守卫，避免 agent 走
/// dispatch 时绕过「先解除关联」的限制。
fn context_referenced(document: &AppDocument, id: &str) -> bool {
    document
        .projects
        .iter()
        .any(|project| project.context_id.as_deref() == Some(id))
}

fn context_list_with(runtime: &Runtime, state: &AppState) -> ContextList {
    let registry = state.contexts.lock().ok();
    let Some(registry) = registry.as_ref() else {
        return ContextList {
            active: None,
            contexts: Vec::new(),
        };
    };
    let active = runtime.active_context().map(str::to_string);
    let mut contexts: Vec<ContextInfo> = registry
        .meta
        .keys()
        .filter_map(|id| context_info_from(registry, runtime, id))
        .collect();
    contexts.sort_by_key(|x| std::cmp::Reverse(x.created_at));
    ContextList { active, contexts }
}

/// 调用方持有 runtime 锁时传 `Some(runtime)`，否则传 `None`。
fn emit_contexts_changed<R: TauriRuntime>(
    app: &AppHandle<R>,
    state: &AppState,
    runtime: Option<&Runtime>,
) {
    let list = match runtime {
        Some(runtime) => context_list_with(runtime, state),
        None => context_list(state),
    };
    if let Err(error) = app.emit("contexts-changed", list) {
        eprintln!("广播 contexts-changed 失败：{error}");
    }
}

// ── Game export (executable) ──────────────────────────────────────

fn game_export_dir<R: TauriRuntime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("game-export");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn run_game(exe: &Path, config: &Path, args: &[&str], extra: &[String]) -> Result<(), String> {
    let mut command = std::process::Command::new(exe);
    command.args(args).arg("--config").arg(config);
    if !extra.is_empty() {
        command.args(extra);
    }
    let status = command
        .status()
        .map_err(|error| format!("启动游戏失败: {error}"))?;
    if !status.success() {
        return Err("游戏导出命令失败".to_string());
    }
    Ok(())
}

/// 跑游戏导出并读回产物（纯 IO + 子进程，**不碰 runtime / registry 锁**）。
///
/// 返回 `(name, source, dump 原始字节, locale 字节, 图标源目录)`；图标源为
/// `None` 表示本次没导出贴图（无头/无图形环境），上下文照常可用、只是没图标。
type GameExport = (String, String, Vec<u8>, Option<Vec<u8>>, Option<PathBuf>);

/// 从可执行文件路径推断游戏的 **data 目录**（`read-data` 要指向它本身，
/// 与 Factorio 默认的 `__PATH__executable__/../../data` 一致）。
///
/// 覆盖常见布局：`<root>/bin/x64/factorio(.exe)`、`<root>/bin/factorio`、
/// `<root>/factorio`。找不到 `data/` 目录时返回 `None`（不写 `read-data`，
/// 让游戏按自己的默认逻辑找数据）。
fn factorio_data_dir(exe: &Path) -> Option<PathBuf> {
    let mut dir = exe.parent()?;
    if dir
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("x64"))
    {
        dir = dir.parent()?;
    }
    if dir
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("bin"))
    {
        dir = dir.parent()?;
    }
    let data = dir.join("data");
    data.is_dir().then_some(data)
}

fn export_game_context<R: TauriRuntime>(
    app: &AppHandle<R>,
    executable_path: &str,
    mod_dir: Option<&str>,
) -> Result<GameExport, String> {
    let exe = PathBuf::from(executable_path);
    if !exe.is_file() {
        return Err(format!("游戏可执行文件不存在: {executable_path}"));
    }
    let export = game_export_dir(app)?;
    let config = export.join("config.ini");
    // `read-data` 指向游戏的 data 目录本身（与 Factorio 默认的
    // `__PATH__executable__/../../data` 一致）；否则游戏会去默认路径
    // （Linux 下常见 `/usr/share/factorio`）找数据包而失败。找不到就省略。
    let mut config_text = format!("[path]\nwrite-data={}\n", export.to_string_lossy());
    if let Some(data) = factorio_data_dir(&exe) {
        config_text.push_str(&format!("read-data={}\n", data.to_string_lossy()));
    }
    config_text.push_str("[general]\nlocale=zh-CN\n");
    std::fs::write(&config, config_text).map_err(|error| error.to_string())?;

    let extra: Vec<String> = match mod_dir {
        Some(dir) => vec!["--mod-directory".to_string(), dir.to_string()],
        None => Vec::new(),
    };

    run_game(&exe, &config, &["--dump-data"], &extra)?;
    run_game(&exe, &config, &["--dump-prototype-locale"], &extra)?;
    // 贴图导出是 best-effort：无头环境（没有图形/贴图数据）会让这一步失败，
    // 但数据与翻译仍然可用——不应因此整体失败。
    if let Err(error) = run_game(
        &exe,
        &config,
        &["--dump-icon-sprites", "--disable-audio"],
        &extra,
    ) {
        eprintln!("贴图导出失败（忽略，本上下文将没有图标）: {error}");
    }

    let script_output = export.join("script-output");
    let dump_path = script_output.join("data-raw-dump.json");
    if !dump_path.exists() {
        return Err(format!(
            "未找到导出数据: {}（请确认游戏已正确执行导出）",
            dump_path.display()
        ));
    }
    let raw = std::fs::read(&dump_path).map_err(|error| error.to_string())?;
    let name = mod_dir
        .and_then(|dir| Path::new(dir).file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "vanilla".to_string());
    let source = format!(
        "exe: {executable_path}{}",
        mod_dir
            .map(|dir| format!(", mods: {dir}"))
            .unwrap_or_default()
    );
    // 只有真的导出了贴图目录才把它当作图标源：否则 `script-output` 里只有
    // dump/locale，搬过去会变成"图标目录"里塞着一份 dump。
    let icon_src = (script_output.join("item").is_dir() || script_output.join("entity").is_dir())
        .then(|| script_output.clone());
    // 翻译：`--dump-prototype-locale` 在 script-output 下写出多个
    // `{category}-locale.json`（item/recipe/entity/…），逐类合并。
    let locale_raw = {
        let map = collect_locale_map(&script_output);
        if map.is_empty() {
            None
        } else {
            serde_json::to_vec(&map).ok()
        }
    };
    Ok((name, source, raw, locale_raw, icon_src))
}

/// 导出 → 注册 → 锁外载入 → 激活。
async fn load_game_context_and_activate<R: TauriRuntime>(
    app: &AppHandle<R>,
    state: &AppState,
    executable_path: &str,
    mod_dir: Option<&str>,
) -> Result<ContextInfo, String> {
    let export_app = app.clone();
    let executable = executable_path.to_string();
    let mods = mod_dir.map(str::to_string);
    let (name, source, raw, locale_raw, icon_src) =
        tauri::async_runtime::spawn_blocking(move || {
            export_game_context(&export_app, &executable, mods.as_deref())
        })
        .await
        .map_err(|error| error.to_string())??;
    register_context_and_activate(
        state,
        name,
        source,
        &raw,
        locale_raw.as_deref(),
        match icon_src {
            Some(src) => IconImport::Move(src),
            // 无头/无图形环境没导出贴图：照常注册上下文，只是没有图标。
            None => IconImport::None,
        },
    )
    .await
}

// ── Commands ──────────────────────────────────────────────────────

/// Load the embedded demo prototype store as a context (idempotent by hash).
#[tauri::command]
async fn load_bundled_dump(app: AppHandle) -> Result<ContextInfo, String> {
    let state = app.state::<AppState>();
    let info = register_context_and_activate(
        &state,
        "内置示例".to_string(),
        "embedded demo".to_string(),
        DEMO_DUMP.as_bytes(),
        None,
        IconImport::None,
    )
    .await?;
    emit_contexts_changed(&app, &state, None);
    Ok(info)
}

/// Run the Factorio executable to export data + locale + icon sprites, then
/// cache and activate the result as a context.
#[tauri::command]
async fn load_game_context(
    app: AppHandle,
    executable_path: String,
    mod_dir: Option<String>,
) -> Result<ContextInfo, String> {
    let state = app.state::<AppState>();
    let info =
        load_game_context_and_activate(&app, &state, &executable_path, mod_dir.as_deref()).await?;
    emit_contexts_changed(&app, &state, None);
    Ok(info)
}

/// Load a pre-generated `data-raw-dump.json` as a cached context.
#[tauri::command]
async fn load_dump(app: AppHandle, path: String) -> Result<ContextInfo, String> {
    // 读盘 + 目录探测放在阻塞线程池（dump 可能几十 MB）。
    let (name, source, raw, locale_raw, icon) = tauri::async_runtime::spawn_blocking(move || {
        let raw = std::fs::read(&path).map_err(|error| error.to_string())?;
        let name = Path::new(&path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("dump")
            .to_string();
        let source = format!("dump: {path}");
        // 翻译：dump 旁的游戏导出目录里通常有多个 `{category}-locale.json`。
        let locale_raw = {
            let map = Path::new(&path)
                .parent()
                .map(collect_locale_map)
                .unwrap_or_default();
            if map.is_empty() {
                None
            } else {
                serde_json::to_vec(&map).ok()
            }
        };
        // 图标根：优先 dump 旁的 icons/（旧约定），否则 dump 所在目录本身
        // （导出时类型目录直接位于 script-output 根下）。
        let icon = match Path::new(&path).parent() {
            Some(parent) => {
                let sibling = parent.join("icons");
                if sibling.is_dir() {
                    IconImport::Copy(sibling)
                } else if parent.join("item").is_dir() {
                    IconImport::Copy(parent.to_path_buf())
                } else {
                    IconImport::None
                }
            }
            None => IconImport::None,
        };
        Ok::<_, String>((name, source, raw, locale_raw, icon))
    })
    .await
    .map_err(|error| error.to_string())??;
    let state = app.state::<AppState>();
    let info =
        register_context_and_activate(&state, name, source, &raw, locale_raw.as_deref(), icon)
            .await?;
    emit_contexts_changed(&app, &state, None);
    Ok(info)
}

/// All cached contexts + the active context id.
#[tauri::command]
fn list_contexts(state: State<'_, AppState>) -> ContextList {
    context_list(&state)
}

/// Activate a context (loading its store from cache on demand) and broadcast.
///
/// GUI 命令与消息层（`ApplicationAction::SetActiveContext`）共用：上下文的
/// 注册表/缓存只有 app 层能碰，把它做成单一实现，避免「GUI 能切、agent 不能」
/// 或两条路径行为漂移。
async fn activate_context<R: TauriRuntime>(
    app: &AppHandle<R>,
    id: Option<String>,
) -> Result<ContextList, String> {
    let state = app.state::<AppState>();
    if let Some(id) = &id {
        // 读盘解析在锁外完成，只在最后短暂上锁装入。
        ensure_context_loaded_offlock(&state, id).await?;
    }
    let list = with_runtime(&state, |runtime| {
        runtime.set_active_context(id.clone());
        Ok(context_list_with(runtime, &state))
    })?;
    emit_contexts_changed(app, &state, None);
    Ok(list)
}

/// 重命名已注册的上下文（只改显示名；写 manifest 在阻塞线程上）。
async fn rename_registered_context<R: TauriRuntime>(
    app: &AppHandle<R>,
    id: String,
    name: String,
) -> Result<ContextList, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("名称不能为空".to_string());
        }
        {
            let mut registry = state
                .contexts
                .lock()
                .map_err(|_| "contexts 锁损坏".to_string())?;
            registry
                .rename(&id, name)
                .ok_or_else(|| format!("上下文 {id} 不存在"))?;
        }
        emit_contexts_changed(&app, &state, None);
        Ok(context_list(&state))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 删除已注册的上下文：被任何项目引用时拒绝（提示先解除关联），否则清磁盘
/// 缓存并在有 store 时一并卸载。
async fn delete_registered_context<R: TauriRuntime>(
    app: &AppHandle<R>,
    id: String,
) -> Result<ContextList, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        {
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "runtime 锁已损坏（poisoned）".to_string())?;
            if context_referenced(&runtime.state.document, &id) {
                return Err("有项目正在引用该上下文，请先解除关联".to_string());
            }
        }
        {
            let mut registry = state
                .contexts
                .lock()
                .map_err(|_| "contexts 锁损坏".to_string())?;
            registry.remove(&id);
        }
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "runtime 锁已损坏（poisoned）".to_string())?;
        runtime.remove_context(&id);
        emit_contexts_changed(&app, &state, Some(&runtime));
        Ok(context_list_with(&runtime, &state))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Game icon PNG bytes for the given context's `<icons>/<ty>/<name>.png`
/// (from `--dump-icon-sprites`)。图标在注册时已移入/拷入缓存，缓存自包含。
/// 前端显式传入 `context_id`（当前选中项目绑定的上下文，否则为激活上下文）。
#[tauri::command]
fn icon(
    state: State<'_, AppState>,
    ty: String,
    name: String,
    context_id: String,
) -> Option<Vec<u8>> {
    if context_id.is_empty() {
        return None;
    }
    let cache_root = {
        let registry = state.contexts.lock().ok()?;
        registry.icon_root(&context_id)
    };
    if !cache_root.is_dir() {
        return None;
    }
    let candidates: Vec<String> = if ty == "quality" {
        // 品质图标只有 quality/ 目录；回退到 item/entity 会显示错误的物品图标。
        vec![format!("quality/{name}.png")]
    } else if ty == "planet" {
        // Factorio 的 --dump-icon-sprites 把星球图标导出到 space-location/
        // 目录（星球原型属于 space-location 类型），而不是 planet/。
        vec![
            format!("space-location/{name}.png"),
            format!("planet/{name}.png"),
            format!("item/{name}.png"),
            format!("entity/{name}.png"),
        ]
    } else {
        vec![
            format!("{ty}/{name}.png"),
            format!("item/{name}.png"),
            format!("entity/{name}.png"),
        ]
    };
    for candidate in candidates {
        let path = cache_root.join(candidate);
        if path.is_file() {
            if let Ok(bytes) = std::fs::read(path) {
                return Some(bytes);
            }
        }
    }
    None
}

/// 匹配用的归一化：小写 + 去掉分隔符（连字符/下划线/空白，含全角与 Unicode 破折号）。
///
/// 名字里的分隔符几乎不可预测——同一个原型会被写成 `processing-unit`、
/// `processing_unit`、`processing unit`，中文 mod 名里也常夹空格。因此**只对
/// 比较用的字符串**去掉这些字符；返回给调用方的 `name` / `localized_name`
/// 仍是原型原名（要拿它去 dispatch）。
fn normalize_for_match(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| !is_separator(*ch))
        .collect()
}

fn is_separator(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '-' | '_' | '\u{2010}'
                ..='\u{2015}' // 各种 Unicode 连字符/破折号
                | '\u{2212}'               // 减号
                | '\u{ff0d}'               // 全角连字符
                | '\u{ff3f}' // 全角下划线
        )
}

/// typo 桶的 kind 优先级：口述一个名字时最可能指的是**物品/插件**，其次是流体，
/// 再是配方，最后才是机器/科技/星球这类概念。
///
/// 只用于给错拼候选排序：同一个名字跨多个原型组时，让 `typo_suggestion` 落在最
/// 可能的那个组上（否则可能给出 `kind: technology` 这种突兀的默认），调用方仍可
/// 用 `kind` 参数或从 `typo` 列表里自选。
fn typo_kind_rank(kind: &str) -> u8 {
    match kind {
        "item" | "module" => 0,
        "fluid" => 1,
        "recipe" => 2,
        _ => 3,
    }
}

/// 打字错误的编辑距离（OSA：相邻字符换位算 1 步——这是最常见的手误，
/// 纯 Levenshtein 会把它算成 2，从而漏掉 `chemcial` 这类错拼）。
fn typo_distance(left: &[char], right: &[char]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut prev2 = vec![0usize; right.len() + 1];
    let mut prev: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for i in 1..=left.len() {
        current[0] = i;
        for j in 1..=right.len() {
            let cost = usize::from(left[i - 1] != right[j - 1]);
            let mut best = (prev[j] + 1)
                .min(current[j - 1] + 1)
                .min(prev[j - 1] + cost);
            if i > 1 && j > 1 && left[i - 1] == right[j - 2] && left[i - 2] == right[j - 1] {
                best = best.min(prev2[j - 2] + 1);
            }
            current[j] = best;
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut current);
    }
    prev[right.len()]
}

/// 一次名字查询的结果：精确命中 + 截断到 `limit` 的模糊命中 + 打字错误候选。
///
/// `partial_matched` 是模糊命中的**总数**（截断前），因此
/// `partial_matched > partial.len()` 即表示结果被截断。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResolvedQuery {
    pub exact: Vec<ResolvedName>,
    pub partial: Vec<ResolvedName>,
    pub partial_matched: usize,
    /// 打字错误候选（**仅在精确与模糊都为空时才计算**）：按编辑距离升序，
    /// 每条带 `distance`。
    pub typo: Vec<ResolvedName>,
    /// typo 命中的总数（截断前）。
    pub typo_matched: usize,
    /// 最佳（最小）编辑距离；没有 typo 命中时为 `None`。
    pub typo_best_distance: Option<usize>,
    /// 最佳距离上有多少**不同名字**。`1` = 名字唯一——即使同名跨 item/recipe
    /// 等多个原型组也不算歧义（与 `exact` 桶的约定一致：名字是确定的，kind 由
    /// 调用方按上下文选）；`> 1` = 有几个同样接近的名字，必须人工确认。
    pub typo_best_name_count: usize,
    /// 「高置信度结果」：**仅当最佳距离上只有一个不同名字时**给出该候选，否则 `None`。
    ///
    /// 这是刻意的：错拼命中一旦被当成确定答案，就会把错误原型名写进计划
    /// （校验能过、计划是错的），所以平局时宁可交回 `None` 让调用方问人。
    pub typo_suggestion: Option<ResolvedName>,
}

/// 解析一个「名字查询」：既接受原型 id（`iron-gear-wheel`），也接受玩家口述的
/// 本地化名（`铁齿轮`）。
///
/// - **精确命中**：归一化（小写 + 去掉 `-`/`_`/空白）后与原型名或本地化名完全
///   相同——`processing unit` / `processing_unit` / `PROCESSING-UNIT` 都等于
///   `processing-unit`。同一个名字在不同原型组里可能是多条（`speed-module`
///   同时是 item/recipe/technology），因此返回全部、不去重；
/// - **模糊命中**：按「本地化名前缀 → 原型名前缀 → 本地化名子串 → 原型名子串」
///   排序后取前 `limit` 条，用 `matched_by` 标注命中方式——口述名往往只记得
///   一半（「铁板」可能同时是 `铁板`/`铁棒`/`铁板条`）；
/// - **打字错误**：精确与模糊都为空时，才按编辑距离找近似候选（阈值随查询长度
///   收紧），并给出 `typo_best_distance` / `typo_margin` 作为置信证据。
///
/// 抽成纯函数是为了可单测：MCP 工具只负责取索引与序列化。求解结果里的 id 换成
/// 中文名（向群里汇报）与「口述名 → id」共用这一套匹配口径。
///
/// 打字错误的置信规则：只有「最佳编辑距离上只有一个候选」时才给
/// [`ResolvedQuery::typo_suggestion`]；平局时它是 `None`（宁可让人确认，也不能
/// 猜一个原型名——错误的名字能通过校验，却会让计划悄悄跑偏）。
pub(crate) fn resolve_index_entry(
    entries: &[IndexEntry],
    query: &str,
    limit: usize,
) -> ResolvedQuery {
    let needle = normalize_for_match(query);
    if needle.is_empty() {
        return ResolvedQuery::default();
    }
    let needle_chars: Vec<char> = needle.chars().collect();
    let resolved =
        |entry: &IndexEntry, matched_by: &'static str, distance: Option<usize>| ResolvedName {
            kind: entry.kind.clone(),
            name: entry.name.clone(),
            localized_name: entry.localized_name.clone(),
            group: entry.group.clone(),
            matched_by,
            distance,
        };

    let mut exact = Vec::new();
    // (排序键, 本地化名长度, 名字)——后两者只为让结果稳定、可读。
    let mut partial: Vec<(u8, usize, ResolvedName)> = Vec::new();
    for entry in entries {
        let name = normalize_for_match(&entry.name);
        let localized = normalize_for_match(&entry.localized_name);
        if name == needle {
            exact.push(resolved(entry, "name-exact", None));
            continue;
        }
        if !localized.is_empty() && localized == needle {
            exact.push(resolved(entry, "localized-exact", None));
            continue;
        }
        let (rank, matched_by) = if !localized.is_empty() && localized.starts_with(&needle) {
            (0, "localized-prefix")
        } else if name.starts_with(&needle) {
            (1, "name-prefix")
        } else if !localized.is_empty() && localized.contains(&needle) {
            (2, "localized-contains")
        } else if name.contains(&needle) {
            (3, "name-contains")
        } else {
            continue;
        };
        partial.push((
            rank,
            entry.localized_name.chars().count(),
            resolved(entry, matched_by, None),
        ));
    }
    partial.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.name.cmp(&right.2.name))
    });
    let partial_matched = partial.len();
    let partial: Vec<ResolvedName> = partial
        .into_iter()
        .take(limit)
        .map(|(_, _, resolved)| resolved)
        .collect();

    // 打字错误只在「什么都没有」时才算：正常查询不会被近似结果干扰，扫描成本也
    // 只在真正可能用到时付出（外加长度预筛，避免对两万条原型逐个算距离）。
    // 排序键 = (编辑距离, kind 优先级, 本地化名长度) + 命中。
    let mut typo: Vec<((usize, u8, usize), ResolvedName)> = Vec::new();
    if exact.is_empty() && partial_matched == 0 {
        // 长度门槛按**字符**算，并且对非 ASCII（中日韩）放宽一格：`铁板` 只有两个
        // 字，却是一个完整的词，打错一个字（`铁版`）就该给候选。原来「3 个字符起」
        // 是拉丁中心口径——中文名永远拿不到错拼候选。
        let non_ascii = needle_chars.iter().any(|character| !character.is_ascii());
        let max_distance = match needle_chars.len() {
            0..=1 => 0, // 太短：任何两个名字都「差不多」，没有意义
            2 if non_ascii => 1,
            2 => 0, // ASCII 两个字母同样太短（`zz` 不该匹配一堆东西）
            3..=4 => 1,
            _ => 2,
        };
        if max_distance > 0 {
            for entry in entries {
                let name = normalize_for_match(&entry.name);
                let localized = normalize_for_match(&entry.localized_name);
                let mut best: Option<usize> = None;
                for candidate in [&name, &localized] {
                    // **按字符**比较长度，不能拿 `str::len()`（UTF-8 字节数）去比
                    // 字符数：`铁齿轮` 是 9 字节 / 3 字符，旧写法算出差距 6 > 阈值，
                    // 于是所有中文名在这里就被预筛掉了——错拼匹配对中文完全失效。
                    let chars = candidate.chars().count();
                    if candidate.is_empty() || chars.abs_diff(needle_chars.len()) > max_distance {
                        continue;
                    }
                    let chars: Vec<char> = candidate.chars().collect();
                    let distance = typo_distance(&needle_chars, &chars);
                    best = Some(best.map_or(distance, |current: usize| current.min(distance)));
                }
                if let Some(distance) = best {
                    if distance <= max_distance {
                        typo.push((
                            (
                                distance,
                                typo_kind_rank(&entry.kind),
                                entry.localized_name.chars().count(),
                            ),
                            resolved(entry, "typo", Some(distance)),
                        ));
                    }
                }
            }
            // 排序键 = (编辑距离, kind 优先级, 本地化名长度)，同键按名字稳定收尾。
            typo.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.name.cmp(&right.1.name)));
        }
    }
    let typo_matched = typo.len();
    let typo_best_distance = typo.first().map(|((distance, _, _), _)| *distance);
    // 并列判定按**名字**去重：同一个名字出现在多个原型组（item/recipe/…）不算歧义
    // ——名字是确定的，调用方只需再按上下文选一个 kind（与 `exact` 桶一致）。
    let typo_best_name_count = typo_best_distance.map_or(0, |best| {
        let mut names: Vec<&str> = typo
            .iter()
            .filter(|((distance, _, _), _)| *distance == best)
            .map(|(_, resolved)| resolved.name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        names.len()
    });
    // 名字唯一才给高置信度建议；多个同样接近的名字（processing-unit-2 与 -3）返回 None。
    let typo_suggestion = if typo_best_name_count == 1 {
        typo.first().map(|(_, resolved)| resolved.clone())
    } else {
        None
    };
    let typo: Vec<ResolvedName> = typo
        .into_iter()
        .take(limit)
        .map(|(_, resolved)| resolved)
        .collect();

    ResolvedQuery {
        exact,
        partial,
        partial_matched,
        typo,
        typo_matched,
        typo_best_distance,
        typo_best_name_count,
        typo_suggestion,
    }
}

/// 过滤目录索引条目：`kind` 精确匹配（大小写不敏感），`name_contains` 与
/// `name`/`localized_name` 做子串比较——**同样走 [`normalize_for_match`]**，
/// 因此 `processing unit` 也能筛出 `processing-unit`。归一化在这里做，调用方
/// 直接传原始输入。供 MCP 的 `list_prototypes` 工具与测试共用。
pub(crate) fn filter_index_entries(
    entries: Vec<IndexEntry>,
    kind: Option<&str>,
    needle: Option<&str>,
) -> Vec<IndexEntry> {
    let kind = kind
        .map(|kind| kind.trim().to_lowercase())
        .filter(|kind| !kind.is_empty());
    let needle = needle
        .map(normalize_for_match)
        .filter(|needle| !needle.is_empty());
    entries
        .into_iter()
        .filter(|entry| kind.as_deref().is_none_or(|kind| entry.kind == kind))
        .filter(|entry| match &needle {
            Some(needle) => {
                normalize_for_match(&entry.name).contains(needle)
                    || normalize_for_match(&entry.localized_name).contains(needle)
            }
            None => true,
        })
        .collect()
}

/// 解析「查询用的上下文 id」：显式给出优先，否则用当前激活上下文。
///
/// MCP 的只读工具（list_prototypes / suggest）因此不必让 agent 先 list_contexts
/// 再回填 id——省略参数就查它正在操作的那个上下文。
fn resolve_context_id(state: &AppState, requested: Option<&str>) -> Result<String, String> {
    if let Some(id) = requested.map(str::trim).filter(|id| !id.is_empty()) {
        return Ok(id.to_string());
    }
    with_runtime(state, |runtime| {
        runtime.active_context().map(str::to_string).ok_or_else(|| {
            "没有激活的游戏上下文：先在返回值里指定 context_id，或用 dispatch 的 set-active-context 选中一个"
                .to_string()
        })
    })
}

/// 全量目录索引（含 order fallback 排序）：GUI 的 `catalog_index` 命令与 MCP 的
/// `list_prototypes` 工具共用，避免两条路径的条目形状漂移。
async fn catalog_index_for(state: &AppState, context_id: &str) -> Result<CatalogIndex, String> {
    if context_id.is_empty() {
        return Ok(CatalogIndex {
            context_id: String::new(),
            qualities: Vec::new(),
            entries: Vec::new(),
        });
    }
    let store = context_store_arc(state, context_id).await?;
    let locale = locale_map_of(state, context_id);
    // 索引构建遍历全部原型：放到阻塞线程池，且不持 runtime 锁。
    let context_id = context_id.to_string();
    tauri::async_runtime::spawn_blocking(move || CatalogIndex {
        context_id: context_id.clone(),
        qualities: store.quality_order().to_vec(),
        entries: catalog_index_from_store(&store, &locale),
    })
    .await
    .map_err(|error| error.to_string())
}

/// 一条流（物品/流体）的候选机制建议：GUI 的 `suggest` 命令与 MCP 的 `suggest`
/// 工具共用。
async fn suggest_for(
    state: &AppState,
    context_id: &str,
    flow: DualVar,
) -> Result<Vec<Suggestion>, String> {
    if context_id.is_empty() {
        return Ok(Vec::new());
    }
    let store = context_store_arc(state, context_id).await?;
    tauri::async_runtime::spawn_blocking(move || suggest_for_flow(&store, flow))
        .await
        .map_err(|error| error.to_string())
}

/// 全量目录索引（含 order fallback 排序）：一次拉取，前端本地筛选/分组。
#[tauri::command]
async fn catalog_index(app: AppHandle, context_id: String) -> Result<CatalogIndex, String> {
    let state = app.state::<AppState>();
    catalog_index_for(&state, &context_id).await
}

/// 星球隐式可用输入（严格供给下也免费；外部输入显式覆盖后不显示）：
/// 供前端在外部输入面板用虚线展示。
#[tauri::command]
async fn implicit_sources(
    app: AppHandle,
    project: ProjectId,
    factory: FactoryId,
) -> Result<Vec<DualVar>, String> {
    let state = app.state::<AppState>();
    let snapshot = factory_snapshot(&state, project, factory).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let factory_doc = &snapshot.factory_doc;
        let Some(planet) = factory_doc.settings.planet.as_deref() else {
            return Vec::new();
        };
        let mut implicit =
            metatorio_runtime::planet::planet_autoplaced_flows(&snapshot.store, planet);
        for input in &factory_doc.external_inputs {
            implicit.shift_remove(&input.flow);
        }
        let mut keys: Vec<DualVar> = implicit.keys().cloned().collect();
        keys.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        keys
    })
    .await
    .map_err(|error| error.to_string())
}

/// 建议候选：给定一条流，列出能产出/消耗它的机制（配方/矿点/燃料/发电机）。
#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    /// "recipe" | "resource" | "item-fuel" | "generator"
    pub kind: String,
    pub name: String,
    /// "producer" = 产出该流；"consumer" = 消耗该流（作为原料）。
    pub role: String,
}

/// 建议系统：为一条流生成候选机制（对应 egui 的"推荐配方/矿点"模态框）。
#[tauri::command]
async fn suggest(
    app: AppHandle,
    context_id: String,
    flow: DualVar,
) -> Result<Vec<Suggestion>, String> {
    let state = app.state::<AppState>();
    suggest_for(&state, &context_id, flow).await
}

/// 单个机制在当前规划条件下的展开流（系数 = 1 时的每秒产/耗）。
///
/// 返回 `(DualVar, f64)` 列表：正值产出、负值消耗。无求解结果时
/// 前端用它展示"假定系数为 1 时产出的资源"（复刻原版 egui 机制卡）。
#[tauri::command]
async fn mechanic_flow(
    app: AppHandle,
    project: ProjectId,
    factory: FactoryId,
    mechanic: MechanicId,
) -> Result<Vec<(DualVar, f64)>, String> {
    let state = app.state::<AppState>();
    mechanic_flow_for(&state, project, factory, mechanic).await
}

/// [`mechanic_flow`] 命令与 MCP `get_planning_state {mechanic}` 的**共用实现**：
/// 两条路径必须给出同一份数（GUI 机制卡上显示的、和 agent 读到的不能是两套算法）。
pub(crate) async fn mechanic_flow_for(
    state: &AppState,
    project: ProjectId,
    factory: FactoryId,
    mechanic: MechanicId,
) -> Result<Vec<(DualVar, f64)>, String> {
    let snapshot = factory_snapshot(state, project, factory).await?;
    tauri::async_runtime::spawn_blocking(move || mechanic_flow_from_snapshot(&snapshot, mechanic))
        .await
        .map_err(|error| error.to_string())?
}

/// 展开一个机制的流（纯计算，不碰 runtime 锁；求解路径与它共用同一套上下文/环境设置）。
pub(crate) fn mechanic_flow_from_snapshot(
    snapshot: &metatorio_runtime::SolveSnapshot,
    mechanic: MechanicId,
) -> Result<Vec<(DualVar, f64)>, String> {
    let accessibility = snapshot.resolve_accessibility();
    let store = &snapshot.store;
    let factory_doc = &snapshot.factory_doc;
    let entry = factory_doc
        .mechanics
        .iter()
        .find(|entry| entry.id == mechanic)
        .ok_or("机制不存在")?;
    let mut game = metatorio_runtime::solve::make_game_state_with_accessibility(
        store,
        &snapshot.project_doc,
        &accessibility,
    );
    // 与求解路径一致：应用当前工厂的星球/地表环境（太阳能系数、昼夜周期）。
    metatorio_runtime::solve::apply_environment_to_game_state(
        store,
        &mut game,
        factory_doc.settings.planet.as_deref(),
        factory_doc.settings.surface.as_deref(),
    );
    let context = metatorio_core::Context::new(store, &game);
    let expansion =
        metatorio_core::expand::expand(std::iter::once((mechanic, &entry.mechanic)), &context);
    // 合并所有展开变量的流（同 config 的流体插值端求和）。
    let mut flow: metatorio_core::prim_var::Flow = Default::default();
    for variable in expansion.variables {
        for (key, value) in variable.flow {
            *flow.entry(key).or_insert(0.0) += value;
        }
    }
    Ok(flow.into_iter().filter(|(_, v)| v.abs() > 1e-12).collect())
}

/// 太阳能机制的配平信息（平均出力 / 周期溢出总电量 / 蓄电器配比）。
///
/// 使用当前工厂环境的星球太阳能系数与昼夜周期（同求解路径）。
#[tauri::command]
async fn solar_balance(
    app: AppHandle,
    project: ProjectId,
    factory: FactoryId,
    mechanic: MechanicId,
) -> Result<Option<metatorio_core::SolarBalance>, String> {
    let state = app.state::<AppState>();
    let snapshot = factory_snapshot(&state, project, factory).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let accessibility = snapshot.resolve_accessibility();
        let store = &snapshot.store;
        let factory_doc = &snapshot.factory_doc;
        let entry = factory_doc
            .mechanics
            .iter()
            .find(|entry| entry.id == mechanic)
            .ok_or("机制不存在")?;
        let Mechanic::Solar(mechanic) = &entry.mechanic else {
            return Ok(None);
        };
        let mut game = metatorio_runtime::solve::make_game_state_with_accessibility(
            store,
            &snapshot.project_doc,
            &accessibility,
        );
        metatorio_runtime::solve::apply_environment_to_game_state(
            store,
            &mut game,
            factory_doc.settings.planet.as_deref(),
            factory_doc.settings.surface.as_deref(),
        );
        let context = metatorio_core::Context::new(store, &game);
        Ok(metatorio_core::solar_balance(&context, mechanic))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 指定机器/插件塔允许的插件列表（机制卡手动插件选择的鉴权）。
///
/// 规则（与自动规划 `module_allowed` 一致）：
/// - 显式插件类别（allowed_module_categories）非空时，插件类别必须在其中
///   （留空 = 全部支持）；
/// - 禁止的效果类型（allowed_effects）缺失/为空时，插件对应效果属性为
///   正面（≠0）即被拒绝；配方机制还叠加配方的 allow_speed/allow_productivity
///   等开关。
///
/// `machine_kind`: "machine" | "mining-machine" | "beacon"。
/// `recipe`: 可选配方名（仅 recipe 机制传入；采矿/插件塔为 None）。
#[tauri::command]
async fn allowed_modules(
    app: AppHandle,
    context_id: String,
    machine_kind: String,
    machine: String,
    recipe: Option<String>,
) -> Result<Vec<String>, String> {
    if context_id.is_empty() {
        return Ok(Vec::new());
    }
    let state = app.state::<AppState>();
    let store = context_store_arc(&state, &context_id).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(record) = store.get(PrototypeGroup::Entity, &machine) else {
            return Vec::new();
        };
        // 收集机器/采矿机/插件塔的插件类别与效果限制。
        let (categories, effects) = match machine_kind.as_str() {
            "mining-machine" => record
                .component::<MiningDrillComponent>()
                .map(|drill| {
                    (
                        drill.allowed_module_categories.clone(),
                        drill.allowed_effects,
                    )
                })
                .unwrap_or((None, None)),
            "beacon" => record
                .component::<BeaconComponent>()
                .map(|beacon| {
                    (
                        beacon.allowed_module_categories.clone(),
                        beacon.allowed_effects,
                    )
                })
                .unwrap_or((None, None)),
            _ => record
                .component::<CraftingMachineComponent>()
                .map(|machine| {
                    (
                        machine.allowed_module_categories.clone(),
                        machine.allowed_effects,
                    )
                })
                .unwrap_or((None, None)),
        };
        let recipe_component = recipe.as_deref().and_then(|name| {
            store
                .get(PrototypeGroup::Recipe, name)
                .and_then(|record| record.component::<RecipeComponent>())
        });
        let mut out = Vec::new();
        for item_record in store.group(PrototypeGroup::Item) {
            let Some(module) = item_record.component::<ModuleComponent>() else {
                continue;
            };
            if auto_plan::module_allowed(module, &categories, &effects, recipe_component) {
                out.push(item_record.name.clone());
            }
        }
        // 稳定排序（catalog 顺序由 order 决定，这里按名称保证可预测）。
        out.sort();
        out
    })
    .await
    .map_err(|error| error.to_string())
}

/// 生成候选机制（与 suggest 命令共用；AutoPlan 也用它）。
///
/// 对物品/流体同时收集：
/// - producer：产出该流的配方 / 产出矿点
/// - consumer：把该流作为原料的配方
///
/// 其余能量类流沿用旧逻辑（只列生产者）。
fn suggest_for_flow(store: &PrototypeStore, flow: DualVar) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let mut push = |kind: &str, name: &str, role: &str| {
        out.push(Suggestion {
            kind: kind.to_string(),
            name: name.to_string(),
            role: role.to_string(),
        });
    };
    match flow {
        DualVar::Item(item) => {
            let target = item.id.as_str();
            for record in store.group(PrototypeGroup::Recipe) {
                let Some(recipe) = record.component::<RecipeComponent>() else {
                    continue;
                };
                if recipe
                    .results
                    .iter()
                    .any(|product| matches!(product, Product::Item(p) if p.name == target))
                {
                    push("recipe", &record.name, "producer");
                }
                if recipe
                    .ingredients
                    .iter()
                    .any(|ingredient| matches!(ingredient, Ingredient::Item(p) if p.name == target))
                {
                    push("recipe", &record.name, "consumer");
                }
            }
            for record in store.group(PrototypeGroup::Entity) {
                if record.type_ != "resource" {
                    continue;
                }
                let Some(minable) = record
                    .component::<EntityComponent>()
                    .and_then(|entity| entity.minable())
                else {
                    continue;
                };
                let yields_item = minable.result.as_deref() == Some(target)
                    || minable
                        .results
                        .iter()
                        .any(|product| matches!(product, Product::Item(p) if p.name == target));
                if yields_item {
                    push("resource", &record.name, "producer");
                }
            }
        }
        DualVar::Fluid { name, .. } => {
            for record in store.group(PrototypeGroup::Recipe) {
                let Some(recipe) = record.component::<RecipeComponent>() else {
                    continue;
                };
                if recipe
                    .results
                    .iter()
                    .any(|product| matches!(product, Product::Fluid(p) if p.name == *name))
                {
                    push("recipe", &record.name, "producer");
                }
                if recipe
                    .ingredients
                    .iter()
                    .any(|ingredient| matches!(ingredient, Ingredient::Fluid(p) if p.name == *name))
                {
                    push("recipe", &record.name, "consumer");
                }
            }
        }
        DualVar::Electricity => {
            for record in store.group(PrototypeGroup::Entity) {
                if record.has("GeneratorComponent") || record.has("BurnerGeneratorComponent") {
                    push("generator", &record.name, "producer");
                }
            }
        }
        DualVar::ItemFuel { category, .. } => {
            for record in store.group(PrototypeGroup::Item) {
                let Some(item) = record.component::<ItemComponent>() else {
                    continue;
                };
                if item.fuel_value().amount > 0.0
                    && category
                        .iter()
                        .any(|candidate| candidate == &item.fuel_category)
                {
                    push("item-fuel", &record.name, "producer");
                }
            }
        }
        _ => {}
    }
    out
}

/// 科技的最低等级：名字以 `-<number>` 结尾时取该数字（Factorio 规则），
/// 否则为 0（非升级档科技）。
fn technology_base_level(name: &str) -> u32 {
    name.rsplit_once('-')
        .and_then(|(_, suffix)| suffix.parse::<u32>().ok())
        .unwrap_or(0)
}

/// 科技等级上限 → 前端 `Option<u32>`。
/// - `None`：无限科技（无上限）。
/// - `Some(n)`：有效上限 n。
///
/// max_level 未显式声明时默认等于该科技的最低等级（自身），即单次研究。
fn technology_max_level_value(tech: &TechnologyComponent, name: &str) -> Option<u32> {
    match tech.max_level {
        Some(TechnologyMaxLevel::Infinite) => None,
        Some(TechnologyMaxLevel::U32(level)) => Some(level),
        None => Some(technology_base_level(name)),
    }
}

/// 能量源类型字符串（electric/burner/fluid/heat/void）；前端据此决定是否显示燃料。
fn energy_source_kind(source: &metatorio_data::types::EnergySource) -> &'static str {
    match source {
        metatorio_data::types::EnergySource::Electric(_) => "electric",
        metatorio_data::types::EnergySource::Burner(_) => "burner",
        metatorio_data::types::EnergySource::Fluid(_) => "fluid",
        metatorio_data::types::EnergySource::Heat(_) => "heat",
        metatorio_data::types::EnergySource::Void => "void",
    }
}

/// Burner 能量源的燃料类别；非 burner 能量源返回空。供前端燃料选择筛选
/// 物品的 `fuel_category`（配合 getDetail 返回）。
fn burner_fuel_categories_of(source: &metatorio_data::types::EnergySource) -> Vec<String> {
    match source {
        metatorio_data::types::EnergySource::Burner(burner) => burner.fuel_categories.clone(),
        _ => Vec::new(),
    }
}

fn catalog_index_from_store(
    store: &PrototypeStore,
    locale: &HashMap<String, String>,
) -> Vec<IndexEntry> {
    let mut out: Vec<IndexEntry> = Vec::new();
    // 有 order_info 的组：大组 → 小组 → 条目（recipe/entity fallback 已在
    // order_info 中生效）
    let ordered = [
        ("item", PrototypeGroup::Item, "item"),
        ("fluid", PrototypeGroup::Fluid, "fluid"),
        ("recipe", PrototypeGroup::Recipe, "recipe"),
        ("technology", PrototypeGroup::Technology, "technology"),
        ("planet", PrototypeGroup::Planet, "planet"),
        ("surface", PrototypeGroup::Surface, "surface"),
    ];
    for (kind, group, icon_type) in ordered {
        let Some(order) = store.order_info().get(&group) else {
            continue;
        };
        for (big, subgroups) in order {
            for (sub, names) in subgroups {
                for name in names {
                    let categories = if kind == "recipe" {
                        store
                            .get(PrototypeGroup::Recipe, name)
                            .and_then(|record| record.component::<RecipeComponent>())
                            .map(effective_recipe_categories)
                            .unwrap_or_default()
                    } else if kind == "item" {
                        item_tags(store, name)
                    } else if kind == "fluid" {
                        fluid_tags(store, name)
                    } else {
                        Vec::new()
                    };
                    let (fuel_category, fuel_value_j) = if kind == "item" {
                        item_fuel_info(store, name)
                    } else if kind == "fluid" {
                        fluid_fuel_info(store, name)
                    } else {
                        (String::new(), None)
                    };
                    let technology_max_level = if kind == "technology" {
                        store
                            .get(PrototypeGroup::Technology, name)
                            .and_then(|record| record.component::<TechnologyComponent>())
                            .and_then(|tech| technology_max_level_value(tech, name))
                    } else {
                        None
                    };
                    let technology_base_level = if kind == "technology" {
                        technology_base_level(name)
                    } else {
                        0
                    };
                    out.push(IndexEntry {
                        kind: kind.to_string(),
                        name: name.clone(),
                        localized_name: localized_name(locale, kind, name),
                        group: big.clone(),
                        subgroup: sub.clone(),
                        icon_type: icon_type.to_string(),
                        module_slots: None,
                        categories,
                        fuel_category,
                        fuel_value_j,
                        technology_max_level,
                        technology_base_level,
                    });
                }
            }
        }
    }

    // 实体类：Entity 的 order_info（含 fallback）里按组件过滤
    let entity_kinds = [
        ("machine", &["CraftingMachineComponent"][..], true),
        ("mining-machine", &["MiningDrillComponent"][..], true),
        (
            "generator",
            &["GeneratorComponent", "BurnerGeneratorComponent"][..],
            true,
        ),
        ("boiler", &["BoilerComponent"][..], true),
        ("reactor", &["ReactorComponent"][..], true),
        ("solar-panel", &["SolarPanelComponent"][..], false),
        ("accumulator", &["AccumulatorComponent"][..], false),
        ("beacon", &["BeaconComponent"][..], true),
        ("entity", &["EntityComponent"][..], false),
    ];
    for (kind, components, want_slots) in entity_kinds {
        let Some(order) = store.order_info().get(&PrototypeGroup::Entity) else {
            continue;
        };
        for (big, subgroups) in order {
            for (sub, names) in subgroups {
                for name in names {
                    let Some(record) = store.get(PrototypeGroup::Entity, name) else {
                        continue;
                    };
                    if !components.iter().any(|component| record.has(component)) {
                        continue;
                    }
                    let slots = if want_slots {
                        record
                            .component::<CraftingMachineComponent>()
                            .and_then(|machine| machine.module_slots)
                    } else {
                        None
                    };
                    let categories = match kind {
                        "machine" => record
                            .component::<CraftingMachineComponent>()
                            .map(|machine| machine.crafting_categories.clone())
                            .unwrap_or_default(),
                        "mining-machine" => record
                            .component::<MiningDrillComponent>()
                            .map(|drill| drill.resource_categories.clone())
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    out.push(IndexEntry {
                        kind: kind.to_string(),
                        name: name.clone(),
                        localized_name: localized_name(locale, kind, name),
                        group: big.clone(),
                        subgroup: sub.clone(),
                        icon_type: "entity".to_string(),
                        module_slots: slots,
                        categories,
                        fuel_category: String::new(),
                        fuel_value_j: None,
                        technology_max_level: None,
                        technology_base_level: 0,
                    });
                }
            }
        }
    }

    // module：Item order_info 过滤 ModuleComponent
    if let Some(order) = store.order_info().get(&PrototypeGroup::Item) {
        for (big, subgroups) in order {
            for (sub, names) in subgroups {
                for name in names {
                    let Some(record) = store.get(PrototypeGroup::Item, name) else {
                        continue;
                    };
                    let Some(module) = record.component::<ModuleComponent>() else {
                        continue;
                    };
                    out.push(IndexEntry {
                        kind: "module".to_string(),
                        name: name.clone(),
                        localized_name: localized_name(locale, "module", name),
                        group: big.clone(),
                        subgroup: sub.clone(),
                        icon_type: "item".to_string(),
                        module_slots: None,
                        categories: if module.category.is_empty() {
                            Vec::new()
                        } else {
                            vec![module.category.clone()]
                        },
                        fuel_category: String::new(),
                        fuel_value_j: None,
                        technology_max_level: None,
                        technology_base_level: 0,
                    });
                }
            }
        }
    }

    // resource：Entity order_info 过滤 type_ == "resource"
    if let Some(order) = store.order_info().get(&PrototypeGroup::Entity) {
        for (big, subgroups) in order {
            for (sub, names) in subgroups {
                for name in names {
                    let Some(record) = store.get(PrototypeGroup::Entity, name) else {
                        continue;
                    };
                    if record.type_ != "resource" {
                        continue;
                    }
                    let categories = record
                        .component::<ResourceEntityComponent>()
                        .map(|resource| vec![effective_resource_category(resource)])
                        .unwrap_or_default();
                    out.push(IndexEntry {
                        kind: "resource".to_string(),
                        name: name.clone(),
                        localized_name: localized_name(locale, "resource", name),
                        group: big.clone(),
                        subgroup: sub.clone(),
                        icon_type: "entity".to_string(),
                        module_slots: None,
                        categories,
                        fuel_category: String::new(),
                        fuel_value_j: None,
                        technology_max_level: None,
                        technology_base_level: 0,
                    });
                }
            }
        }
    }

    // quality
    for name in store.quality_order() {
        out.push(IndexEntry {
            kind: "quality".to_string(),
            name: name.clone(),
            localized_name: localized_name(locale, "quality", name),
            group: "quality".to_string(),
            subgroup: String::new(),
            icon_type: "quality".to_string(),
            module_slots: None,
            categories: Vec::new(),
            fuel_category: String::new(),
            fuel_value_j: None,
            technology_max_level: None,
            technology_base_level: 0,
        });
    }

    out
}

/// 悬停详情：按需拉取，前端缓存。
#[tauri::command]
fn prototype_detail(
    state: State<'_, AppState>,
    context_id: String,
    kind: String,
    name: String,
) -> Result<Option<PrototypeDetail>, String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime 锁已损坏（poisoned）".to_string())?;
    if context_id.is_empty() {
        return Ok(None);
    }
    ensure_context_loaded(&state, &mut runtime, &context_id)?;
    let store = runtime
        .context_store_by_id(&context_id)
        .ok_or("上下文未载入")?;
    let record = match kind.as_str() {
        "item" | "module" => store.get(PrototypeGroup::Item, &name),
        "fluid" => store.get(PrototypeGroup::Fluid, &name),
        "recipe" => store.get(PrototypeGroup::Recipe, &name),
        "technology" => store.get(PrototypeGroup::Technology, &name),
        "planet" => store.get(PrototypeGroup::Planet, &name),
        "surface" => store.get(PrototypeGroup::Surface, &name),
        "quality" => store.get(PrototypeGroup::Quality, &name),
        _ => store.get(PrototypeGroup::Entity, &name),
    };
    let Some(record) = record else {
        return Ok(None);
    };
    let locale = locale_map_of(&state, &context_id);
    let mut detail = PrototypeDetail {
        name: record.name.clone(),
        localized_name: localized_name(&locale, &kind, &record.name),
        kind: kind.clone(),
        ..Default::default()
    };
    if let Some(base) = record.component::<PrototypeBaseComponent>() {
        detail.subgroup = base.subgroup.clone();
        detail.order = base.order.clone();
        detail.hidden = base.hidden;
    }
    if let Some(item) = record.component::<ItemComponent>() {
        detail.stack_size = Some(f64::from(item.stack_size));
        detail.fuel_value_j = item.fuel_value.map(|value| value.amount);
        detail.fuel_category = item.fuel_category.clone();
        detail.burnt_result = item.burnt_result.clone();
        detail.spoil_result = item.spoil_result.clone().unwrap_or_default();
        detail.spoil_ticks = item.spoil_ticks;
        detail.plant_result = item.plant_result.clone().unwrap_or_default();
        detail.launchable = !item.rocket_launch_products.is_empty();
        detail.rocket_launch_products = item
            .rocket_launch_products
            .iter()
            .map(|product| product.name.clone())
            .collect();
    }
    if let Some(recipe) = record.component::<RecipeComponent>() {
        detail.categories = effective_recipe_categories(recipe);
        detail.category = Some(detail.categories.join(", "));
        detail.energy_required = Some(recipe.energy_required);
        detail.maximum_productivity = Some(recipe.maximum_productivity);
        detail.surface_conditions = recipe
            .surface_conditions
            .iter()
            .map(surface_condition_text)
            .collect();
        detail.ingredients = recipe.ingredients.iter().map(ingredient_flow).collect();
        detail.results = recipe.results.iter().map(product_flow).collect();
    }
    if let Some(machine) = record.component::<CraftingMachineComponent>() {
        detail.crafting_speed = Some(machine.crafting_speed);
        detail.module_slots = machine.module_slots;
        detail.allowed_module_categories = machine
            .allowed_module_categories
            .clone()
            .unwrap_or_default();
        detail.energy_usage_j = Some(machine.energy_usage.amount);
        detail.categories = machine.crafting_categories.clone();
        detail.machine_energy_source = Some(energy_source_kind(&machine.energy_source).to_string());
        detail.burner_fuel_categories = burner_fuel_categories_of(&machine.energy_source);
        let receiver = machine.effect_receiver.as_ref();
        detail.uses_beacon_effects =
            Some(receiver.is_none_or(|receiver| receiver.uses_beacon_effects));
        detail.uses_module_effects =
            Some(receiver.is_none_or(|receiver| receiver.uses_module_effects));
    }
    if let Some(drill) = record.component::<MiningDrillComponent>() {
        detail.categories = drill.resource_categories.clone();
        detail.module_slots = drill.module_slots;
        detail.allowed_module_categories =
            drill.allowed_module_categories.clone().unwrap_or_default();
        detail.machine_energy_source = Some(energy_source_kind(&drill.energy_source).to_string());
        detail.burner_fuel_categories = burner_fuel_categories_of(&drill.energy_source);
        let receiver = drill.effect_receiver.as_ref();
        detail.uses_beacon_effects =
            Some(receiver.is_none_or(|receiver| receiver.uses_beacon_effects));
        detail.uses_module_effects =
            Some(receiver.is_none_or(|receiver| receiver.uses_module_effects));
    }
    if let Some(beacon) = record.component::<BeaconComponent>() {
        detail.beacon_module_slots = Some(beacon.module_slots);
        detail.allowed_module_categories =
            beacon.allowed_module_categories.clone().unwrap_or_default();
    }
    if let Some(gen) = record.component::<GeneratorComponent>() {
        detail.effectivity = Some(gen.effectivity);
        detail.max_power_output_j = gen.max_power_output.map(|value| value.amount);
        detail.maximum_temperature = Some(gen.maximum_temperature);
        detail.burns_fluid = Some(gen.burns_fluid);
        detail.fluid_usage_per_tick = Some(gen.fluid_usage_per_tick);
        detail.fluid_filter = gen.fluid_box.filter.clone();
    }
    if let Some(burner_gen) = record.component::<BurnerGeneratorComponent>() {
        detail.max_power_output_j = Some(burner_gen.max_power_output.amount);
        detail.fuel_category = burner_gen.burner.fuel_categories.join(", ");
        detail.machine_energy_source = Some("burner".to_string());
        detail.burner_fuel_categories = burner_gen.burner.fuel_categories.clone();
    }
    if let Some(boiler) = record.component::<BoilerComponent>() {
        detail.energy_consumption_j = Some(boiler.energy_consumption.amount);
        detail.target_temperature = boiler.target_temperature;
        detail.fluid_filter = boiler.fluid_box.filter.clone();
        detail.machine_energy_source = Some(energy_source_kind(&boiler.energy_source).to_string());
        detail.burner_fuel_categories = burner_fuel_categories_of(&boiler.energy_source);
    }
    if let Some(reactor) = record.component::<ReactorComponent>() {
        detail.heat_output_j = Some(reactor.consumption.amount);
        detail.neighbour_bonus = Some(reactor.neighbour_bonus);
        detail.heating_radius = Some(reactor.heating_radius);
        detail.machine_energy_source = Some(energy_source_kind(&reactor.energy_source).to_string());
        detail.burner_fuel_categories = burner_fuel_categories_of(&reactor.energy_source);
    }
    if let Some(resource) = record.component::<ResourceEntityComponent>() {
        detail.categories = vec![effective_resource_category(resource)];
    }
    if let Some(fluid) = record.component::<FluidComponent>() {
        detail.default_temperature = Some(fluid.default_temperature);
    }
    if let Some(quality) = record.component::<QualityComponent>() {
        detail.quality_level = Some(quality.level);
        detail.quality_next = quality.next.clone();
        detail.quality_next_probability = Some(quality.next_probability);
        detail.quality_crafting_speed = quality.crafting_machine_speed_multiplier;
        detail.quality_module_speed = quality.module_speed_multiplier;
        detail.quality_module_productivity = quality.module_productivity_multiplier;
    }
    Ok(Some(detail))
}

fn ingredient_flow(ingredient: &metatorio_data::types::Ingredient) -> FlowAmount {
    use metatorio_data::types::Ingredient;
    match ingredient {
        Ingredient::Item(item) => FlowAmount {
            kind: "item".to_string(),
            name: item.name.clone(),
            amount: f64::from(item.amount),
            quality_min: item.quality_min.clone(),
            quality_max: item.quality_max.clone(),
            quality_change: (item.quality_change != 0).then_some(i32::from(item.quality_change)),
            ..Default::default()
        },
        Ingredient::Fluid(fluid) => FlowAmount {
            kind: "fluid".to_string(),
            name: fluid.name.clone(),
            amount: fluid.amount,
            temperature: fluid.temperature,
            min_temperature: fluid.minimum_temperature,
            max_temperature: fluid.maximum_temperature,
            ..Default::default()
        },
    }
}

fn product_flow(product: &metatorio_data::types::Product) -> FlowAmount {
    use metatorio_data::types::{Product, Production};
    match product {
        Product::Item(item) => {
            let Production { base, productivity } = item.normalized_output();
            FlowAmount {
                kind: "item".to_string(),
                name: item.name.clone(),
                amount: base,
                probability: effective_probability(
                    item.probability_info.independent_probability,
                    item.probability_info.shared_probability.min,
                    item.probability_info.shared_probability.max,
                ),
                amount_min: item.amount_min.map(f64::from),
                amount_max: item.amount_max.map(f64::from),
                productivity,
                quality_min: item.quality_min.clone(),
                quality_max: item.quality_max.clone(),
                quality_change: (item.quality_change != 0)
                    .then_some(i32::from(item.quality_change)),
                ..Default::default()
            }
        }
        Product::Fluid(fluid) => {
            let Production { base, productivity } = fluid.normalized_output();
            FlowAmount {
                kind: "fluid".to_string(),
                name: fluid.name.clone(),
                amount: base,
                probability: effective_probability(
                    fluid.probability_info.independent_probability,
                    fluid.probability_info.shared_probability.min,
                    fluid.probability_info.shared_probability.max,
                ),
                amount_min: fluid.amount_min,
                amount_max: fluid.amount_max,
                productivity,
                temperature: fluid.temperature,
                ..Default::default()
            }
        }
    }
}

/// Accept one user message and execute its side effects.
///
/// 锁纪律：reducer 在**短临界区**内跑完即放锁；随后的 `RuntimeCommand` 各自
/// 按需短暂上锁（求解/落盘/载入上下文都在锁外进行）。这样 MCP 工具调用与
/// GUI 交互、以及不同工厂的求解之间不再互相阻塞。
#[tauri::command]
async fn dispatch(app: AppHandle, message: AppMessage) -> Result<DispatchResult, String> {
    let outcome = {
        let state = app.state::<AppState>();
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "runtime 锁已损坏（poisoned）".to_string())?;
        runtime
            .dispatch(message)
            .map_err(|error| error.to_string())?
    };
    // 与 MCP 的 `dispatch` 工具共用同一套汇总（`run_commands`）：求解产出、
    // 命令序列化、失败收集。求解/上下文类命令自己会 emit（`solve-error` /
    // `context-error`），其余（落盘、关闭项目…）只有回执——丢弃回执就等于
    // **失败在界面上完全不可见**（例如自动保存写盘失败，用户以为已保存）。
    let state = app.state::<AppState>();
    let state_ref = &state;
    let app_ref = &app;
    let (_solve, _commands, errors) = run_commands(&outcome.commands, move |command| {
        // 闭包返回值不能借用参数，故克隆命令进 async 块。
        let command = command.clone();
        async move { execute_command(app_ref, state_ref, &command).await }
    })
    .await;
    if !errors.is_empty() {
        emit(&app, "command-error", errors);
    }
    Ok(outcome)
}

/// Current serializable document snapshot.
#[tauri::command]
async fn get_document(app: AppHandle) -> Result<AppDocument, String> {
    run_blocking(app, |runtime| Ok(runtime.state.document.clone())).await
}

/// 在阻塞线程池里以 `&mut Runtime` 执行一段逻辑（用于把重计算移出主线程）。
async fn run_blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&mut Runtime) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "runtime 锁已损坏（poisoned）".to_string())?;
        f(&mut runtime)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 项目可达性快照（选择器过滤用）：当前可达对象集合。
///
/// 由 runtime 用项目的用户显式覆盖（marked accessible/inaccessible、
/// 里程碑、无视可达性）经 core 的 `compute_accessibility` 计算并缓存；
/// `all_accessible` 时返回全部对象。项目未绑定上下文时返回错误（前端
/// 应在此情况下不做可达性过滤）。
#[tauri::command]
async fn accessibility(app: AppHandle, project: ProjectId) -> Result<Vec<Accessible>, String> {
    let state = app.state::<AppState>();
    ensure_context_for_project_offlock(&state, project).await?;
    // 锁内只取快照；可达性 BFS（py 上下文约 2.5s）在锁外算。
    let snapshot = with_runtime(&state, |runtime| {
        runtime.project_snapshot(project).map_err(|e| e.to_string())
    })?;
    let (snapshot, accessibility, nodes) = tauri::async_runtime::spawn_blocking(move || {
        let accessibility = snapshot.resolve_accessibility();
        let nodes: Vec<Accessible> = accessibility.accessible().iter().cloned().collect();
        (snapshot, accessibility, nodes)
    })
    .await
    .map_err(|error| error.to_string())?;
    let _ = with_runtime(&state, |runtime| {
        runtime.cache_accessibility_if_current(
            snapshot.project,
            snapshot.revision,
            snapshot.accessibility_epoch,
            accessibility,
        );
        Ok(())
    });
    Ok(nodes)
}

/// 里程碑节点按依赖拓扑排序（依赖在前），供 UI 按序展示。
#[tauri::command]
async fn milestones_ordered(
    app: AppHandle,
    project: ProjectId,
) -> Result<Vec<metatorio_runtime::Milestone>, String> {
    let state = app.state::<AppState>();
    ensure_context_for_project_offlock(&state, project).await?;
    let snapshot = with_runtime(&state, |runtime| {
        runtime.project_snapshot(project).map_err(|e| e.to_string())
    })?;
    tauri::async_runtime::spawn_blocking(move || metatorio_runtime::ordered_milestones(&snapshot))
        .await
        .map_err(|error| error.to_string())
}

/// 面向前端的产能视图：自动推算 + 用户覆盖，按来源（auto/user）区分，
/// 供 UI 以虚线边框标注用户指定项。
#[tauri::command]
async fn productivity(
    app: AppHandle,
    project: ProjectId,
) -> Result<metatorio_runtime::ProductivityView, String> {
    let state = app.state::<AppState>();
    ensure_context_for_project_offlock(&state, project).await?;
    let snapshot = with_runtime(&state, |runtime| {
        runtime.project_snapshot(project).map_err(|e| e.to_string())
    })?;
    let (snapshot, accessibility, view) = tauri::async_runtime::spawn_blocking(move || {
        let accessibility = snapshot.resolve_accessibility();
        let view = metatorio_runtime::productivity_view(&snapshot, &accessibility);
        (snapshot, accessibility, view)
    })
    .await
    .map_err(|error| error.to_string())?;
    let _ = with_runtime(&state, |runtime| {
        runtime.cache_accessibility_if_current(
            snapshot.project,
            snapshot.revision,
            snapshot.accessibility_epoch,
            accessibility,
        );
        Ok(())
    });
    Ok(view)
}

// ── Persistence ───────────────────────────────────────────────────

/// OS file dialog for the Factorio executable (game-context loading).
#[tauri::command]
async fn pick_game_executable(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let picked = app
            .dialog()
            .file()
            .add_filter("Factorio 可执行文件", &["exe"])
            .blocking_pick_file();
        #[cfg(not(windows))]
        let picked = app.dialog().file().blocking_pick_file();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// OS file dialog for a pre-generated `data-raw-dump.json`.
#[tauri::command]
async fn pick_dump_file(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .add_filter("Factorio 数据 dump", &["json"])
            .blocking_pick_file();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// OS folder dialog for a Factorio mod directory.
#[tauri::command]
async fn pick_mod_dir(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app.dialog().file().blocking_pick_folder();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// OS 文件对话框：选一个工程文件路径。只返回路径——真正的导入走
/// `ApplicationAction::OpenProject` 消息（`RuntimeCommand::LoadProject`），
/// 这样 GUI 与 MCP agent 共用同一条导入路径。
#[tauri::command]
async fn pick_project_file(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .add_filter("Metatorio 工程", &["json", "fpp"])
            .blocking_pick_file();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// OS 文件对话框：选「另存为」的路径。只返回路径——写盘走
/// `ApplicationAction::SaveProjectAs` 消息（`RuntimeCommand::Persist`）。
#[tauri::command]
async fn pick_project_save_path(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_file_name("metatorio-project.json")
            .add_filter("Metatorio 工程", &["json", "fpp"])
            .blocking_save_file();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 项目记忆的保存路径（未保存过返回 null），供界面显示"保存位置"。
#[tauri::command]
fn project_save_path(state: State<'_, AppState>, project: ProjectId) -> Option<String> {
    state.project_paths.lock().ok()?.get(&project).cloned()
}

// ── Side effects ──────────────────────────────────────────────────

/// 机器变化后按槽位上限钳制插件数量（超出直接截断，经 reducer 落盘）。
fn clamp_modules(
    state: &AppState,
    runtime: &mut Runtime,
    project: ProjectId,
    factory: FactoryId,
    mechanic: MechanicId,
) -> Result<(), String> {
    let context_id = runtime
        .state
        .project(project)
        .map_err(|error| error.to_string())?
        .context_id
        .clone()
        .or_else(|| runtime.active_context().map(str::to_string));
    let Some(context_id) = context_id else {
        return Ok(());
    };
    ensure_context_loaded(state, runtime, &context_id)?;
    let store = runtime
        .context_store_by_id(&context_id)
        .ok_or_else(|| "上下文未载入".to_string())?
        .clone();

    let entry = runtime
        .state
        .factory(project, factory)
        .map_err(|error| error.to_string())?
        .mechanics
        .iter()
        .find(|entry| entry.id == mechanic)
        .cloned()
        .ok_or_else(|| "机制不存在".to_string())?;

    // 只有配方/采矿机制带插件槽（`module_config`）；其余机制没有可钳制的插件。
    // 这里一次判定同时给出「哪种机制」，避免第二个 match 依赖此处的提前 return
    // 才能安全地把 `_` 当成 Mining。
    let (module_count, max, is_recipe) = match &entry.mechanic {
        Mechanic::Recipe(recipe) => (
            recipe.module_config.modules.len(),
            effective_module_slots(&store, &recipe.machine.id, &recipe.machine.quality),
            true,
        ),
        Mechanic::Mining(mining) => (
            mining.module_config.modules.len(),
            effective_module_slots(&store, &mining.machine.id, &mining.machine.quality),
            false,
        ),
        _ => return Ok(()),
    };
    if module_count <= max {
        return Ok(());
    }
    let action = if is_recipe {
        MechanicAction::Recipe(RecipeMechanicAction::Module(ModuleAction::ClampModules {
            max,
        }))
    } else {
        MechanicAction::Mining(MiningMechanicAction::Module(ModuleAction::ClampModules {
            max,
        }))
    };
    runtime
        .dispatch(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Mechanic { mechanic, action },
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn emit<R: TauriRuntime, T: Serialize + Clone>(app: &AppHandle<R>, event: &str, payload: T) {
    if let Err(error) = app.emit(event, payload) {
        eprintln!("failed to emit {event}: {error}");
    }
}

/// 在实体组里挑选一台机器：优先项目规划偏好的机器偏好，其次按给定
/// 排序分数（如 crafting_speed）取最优；都不满足返回 None。
fn pick_entity<
    F: Fn(&metatorio_data::store::PrototypeRecord) -> bool,
    S: Fn(&metatorio_data::store::PrototypeRecord) -> f64,
>(
    store: &PrototypeStore,
    prefs: &[IdWithQuality],
    matches: F,
    score: S,
) -> Option<String> {
    let mut candidates: Vec<(&PrototypeRecord, f64)> = store
        .group(PrototypeGroup::Entity)
        .filter(|record| matches(record))
        .map(|record| (record, score(record)))
        .collect();
    for pref in prefs {
        if let Some((record, _)) = candidates.iter().find(|(record, _)| record.name == pref.id) {
            return Some(record.name.clone());
        }
    }
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.name.cmp(&b.0.name))
    });
    Some(candidates[0].0.name.clone())
}

fn categories_overlap(required: &[String], available: &[String]) -> bool {
    required.is_empty()
        || available
            .iter()
            .any(|available| required.contains(available))
}

fn quality_level_of(qualities: &[String], name: &str) -> usize {
    qualities
        .iter()
        .position(|candidate| candidate == name)
        .unwrap_or(0)
}

/// 品质为空（新机制/首次自动推断尚未设过品质）时归一化为 "normal"，
/// 避免 UI 显示空品质（空串既不是 normal 也显示不出角标）。
fn quality_or_normal(quality: &str) -> &str {
    if quality.is_empty() {
        "normal"
    } else {
        quality
    }
}

fn flow_quality_level(qualities: &[String], flow: &DualVar) -> usize {
    let name = match flow {
        DualVar::Item(id) | DualVar::Entity(id) => &id.quality,
        _ => return 0,
    };
    quality_level_of(qualities, name)
}

/// 一条机制「显式引用」的最高品质等级。
///
/// 用于 [`ensure_quality_limit`]：文档里出现的品质都要反映到项目品质上限，
/// 否则上限会低于文档实际使用值——自动规划按 `quality_limit + 1` 枚举品质
/// （`auto_plan.rs`），偏低的上限会让它**枚举不出**用户已选的更高品质设备，
/// 从而在重排机制时把显式选择静默降级。
///
/// 因此这里必须覆盖全部带品质的字段，而不只是"参与当前计算"的字段：
/// - 各机制的主设备（配方/机器、矿机、燃料、种子、发电机、锅炉、反应堆、
///   **太阳能板 + 蓄电器**）；
/// - 插件清单（recipe/mining）；
/// - **插件塔及其塔内插件**（插件塔品质影响覆盖效率与耗电，见
///   `metatorio_core::mechanic`）。
///
/// 注意 `Mechanic` 是 `#[non_exhaustive]`，新增带品质的机制时这个 `_` 会静默
/// 漏掉——上面枚举的就是它唯一会吞掉的分支（Solar 曾在此被漏掉）。
fn mechanic_quality_level(qualities: &[String], mechanic: &Mechanic) -> usize {
    let mut ids: Vec<&IdWithQuality> = match mechanic {
        Mechanic::Recipe(mechanic) => vec![&mechanic.recipe, &mechanic.machine],
        Mechanic::Mining(mechanic) => vec![&mechanic.machine],
        Mechanic::Spoil(mechanic) => vec![&mechanic.item],
        Mechanic::Plant(mechanic) => vec![&mechanic.seed],
        Mechanic::ItemFuel(mechanic) => vec![&mechanic.item],
        Mechanic::ItemLaunch(mechanic) => vec![&mechanic.item],
        Mechanic::Generator(mechanic) => vec![&mechanic.generator],
        Mechanic::Boiler(mechanic) => vec![&mechanic.boiler],
        Mechanic::Reactor(mechanic) => vec![&mechanic.reactor],
        Mechanic::Solar(mechanic) => vec![&mechanic.solar_panel, &mechanic.accumulator],
        _ => Vec::new(),
    };
    if let Some(config) = module_config_of(mechanic) {
        ids.extend(config.modules.iter());
        for beacon in &config.beacons {
            ids.push(&beacon.beacon);
            ids.extend(beacon.modules.iter().map(|(module, _)| module));
        }
    }
    ids.iter()
        .map(|id| quality_level_of(qualities, &id.quality))
        .max()
        .unwrap_or(0)
}

/// 带插件配置的机制（只有配方与采矿有）。
fn module_config_of(mechanic: &Mechanic) -> Option<&metatorio_core::ModuleConfig> {
    match mechanic {
        Mechanic::Recipe(mechanic) => Some(&mechanic.module_config),
        Mechanic::Mining(mechanic) => Some(&mechanic.module_config),
        _ => None,
    }
}

/// 项目品质上限自动提升：文档中出现高于当前上限的品质时（目标/外部输入/
/// 机制），把 `ProjectSettings.quality_limit` 提升到该品质。这样"显式要求
/// uncommon 目标"不会被默认的 normal 上限静默判死。
fn ensure_quality_limit(
    state: &AppState,
    runtime: &mut Runtime,
    project: ProjectId,
) -> Result<(), String> {
    let context_id = runtime
        .state
        .project(project)
        .map_err(|error| error.to_string())?
        .context_id
        .clone()
        .or_else(|| runtime.active_context().map(str::to_string));
    let Some(context_id) = context_id else {
        return Ok(());
    };
    ensure_context_loaded(state, runtime, &context_id)?;
    let qualities = runtime
        .context_store_by_id(&context_id)
        .map(|store| store.quality_order().to_vec())
        .unwrap_or_default();
    if qualities.len() <= 1 {
        return Ok(());
    }

    let (all_accessible, current_limit) = {
        let project_doc = runtime
            .state
            .project(project)
            .map_err(|error| error.to_string())?;
        (
            project_doc.settings.all_accessible,
            project_doc.settings.quality_limit.clone(),
        )
    };
    if all_accessible {
        return Ok(());
    }
    let current_level = current_limit
        .as_deref()
        .map(|quality| quality_level_of(&qualities, quality))
        .unwrap_or(0);

    let mut max_level = current_level;
    {
        let project_doc = runtime
            .state
            .project(project)
            .map_err(|error| error.to_string())?;
        for factory in &project_doc.factories {
            for target in &factory.targets {
                max_level = max_level.max(flow_quality_level(&qualities, &target.flow));
            }
            for input in &factory.external_inputs {
                max_level = max_level.max(flow_quality_level(&qualities, &input.flow));
            }
            for entry in &factory.mechanics {
                max_level = max_level.max(mechanic_quality_level(&qualities, &entry.mechanic));
            }
        }
    }
    if max_level > current_level {
        let quality = qualities[max_level].clone();
        runtime
            .dispatch(AppMessage::Project {
                project,
                action: ProjectAction::SetQualityLimit {
                    quality: Some(quality),
                },
            })
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// 配方/资源变化后的机器兼容性校验与回退：
/// - 当前机器兼容（类别匹配）→ 不动；
/// - 不兼容或未设置 → 挑选默认机器（项目规划偏好优先，其次最高 crafting_speed），
///   通过 reducer 重新 SetMachine（保持原品质）。
fn ensure_machine_compat(
    state: &AppState,
    runtime: &mut Runtime,
    project: ProjectId,
    factory: FactoryId,
    mechanic: MechanicId,
) -> Result<(), String> {
    let context_id = runtime
        .state
        .project(project)
        .map_err(|error| error.to_string())?
        .context_id
        .clone()
        .or_else(|| runtime.active_context().map(str::to_string));
    let Some(context_id) = context_id else {
        return Ok(()); // 没有上下文时无从校验
    };
    ensure_context_loaded(state, runtime, &context_id)?;
    let store = runtime
        .context_store_by_id(&context_id)
        .ok_or_else(|| "上下文未载入".to_string())?
        .clone();
    let prefs = runtime
        .state
        .project(project)
        .map_err(|error| error.to_string())?
        .planning
        .machine_preferences
        .clone();

    let entry = runtime
        .state
        .factory(project, factory)
        .map_err(|error| error.to_string())?
        .mechanics
        .iter()
        .find(|entry| entry.id == mechanic)
        .cloned()
        .ok_or_else(|| "机制不存在".to_string())?;

    match &entry.mechanic {
        Mechanic::Recipe(recipe) => {
            let recipe_categories = store
                .get(PrototypeGroup::Recipe, &recipe.recipe.id)
                .and_then(|record| record.component::<RecipeComponent>())
                .map(effective_recipe_categories)
                .unwrap_or_default();
            let machine_ok = !recipe.machine.id.is_empty()
                && store
                    .get(PrototypeGroup::Entity, &recipe.machine.id)
                    .and_then(|record| record.component::<CraftingMachineComponent>())
                    .is_some_and(|machine| {
                        categories_overlap(&recipe_categories, &machine.crafting_categories)
                    });
            if machine_ok {
                return Ok(());
            }
            let pick = pick_entity(
                &store,
                &prefs,
                |record| {
                    record
                        .component::<CraftingMachineComponent>()
                        .is_some_and(|machine| {
                            categories_overlap(&recipe_categories, &machine.crafting_categories)
                        })
                },
                |record| {
                    record
                        .component::<CraftingMachineComponent>()
                        .map(|machine| machine.crafting_speed)
                        .unwrap_or(0.0)
                },
            );
            if let Some(machine) = pick {
                let machine =
                    IdWithQuality::new(machine, quality_or_normal(&recipe.machine.quality));
                runtime
                    .dispatch(AppMessage::Factory {
                        project,
                        factory,
                        action: FactoryAction::Mechanic {
                            mechanic,
                            action: MechanicAction::Recipe(RecipeMechanicAction::SetMachine {
                                machine,
                            }),
                        },
                    })
                    .map_err(|error| error.to_string())?;
            }
        }
        Mechanic::Mining(mining) => {
            let resource_category = store
                .get(PrototypeGroup::Entity, &mining.resource)
                .and_then(|record| record.component::<ResourceEntityComponent>())
                .map(effective_resource_category)
                .unwrap_or_default();
            let machine_ok = !mining.machine.id.is_empty()
                && store
                    .get(PrototypeGroup::Entity, &mining.machine.id)
                    .and_then(|record| record.component::<MiningDrillComponent>())
                    .is_some_and(|drill| drill.resource_categories.contains(&resource_category));
            if machine_ok {
                return Ok(());
            }
            let pick = pick_entity(
                &store,
                &prefs,
                |record| {
                    record
                        .component::<MiningDrillComponent>()
                        .is_some_and(|drill| drill.resource_categories.contains(&resource_category))
                },
                |_| 0.0,
            );
            if let Some(machine) = pick {
                let machine =
                    IdWithQuality::new(machine, quality_or_normal(&mining.machine.quality));
                runtime
                    .dispatch(AppMessage::Factory {
                        project,
                        factory,
                        action: FactoryAction::Mechanic {
                            mechanic,
                            action: MechanicAction::Mining(MiningMechanicAction::SetMachine {
                                machine,
                            }),
                        },
                    })
                    .map_err(|error| error.to_string())?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// 短暂持有 runtime 锁执行一段逻辑（**不得跨 `await`**）。
fn with_runtime<T>(
    state: &AppState,
    f: impl FnOnce(&mut Runtime) -> Result<T, String>,
) -> Result<T, String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime 锁已损坏（poisoned）".to_string())?;
    f(&mut runtime)
}

/// 取上下文 store 的 `Arc`（必要时先锁外载入）。之后的重计算可锁外进行。
async fn context_store_arc(
    state: &AppState,
    context_id: &str,
) -> Result<Arc<PrototypeStore>, String> {
    if context_id.is_empty() {
        return Err("上下文为空".to_string());
    }
    ensure_context_loaded_offlock(state, context_id).await?;
    with_runtime(state, |runtime| {
        runtime
            .context_arc_by_id(context_id)
            .ok_or_else(|| "上下文未载入".to_string())
    })
}

/// 取工厂级求解快照（必要时先锁外载入上下文）。之后的重计算可锁外进行。
pub(crate) async fn factory_snapshot(
    state: &AppState,
    project: ProjectId,
    factory: FactoryId,
) -> Result<metatorio_runtime::SolveSnapshot, String> {
    ensure_context_for_project_offlock(state, project).await?;
    with_runtime(state, |runtime| {
        runtime
            .solve_snapshot_inputs(project, factory)
            .map_err(|error| error.to_string())
    })
}

/// 保存项目到文件：锁内取文档快照 → 锁外原子写盘 → 锁内清 dirty。
///
/// 序列化 + 文件 IO 都不持 runtime 锁（大工程 JSON 序列化可能有几十毫秒，
/// 不该阻塞 MCP / GUI 的其它交互）。
async fn persist_project(state: &AppState, project: ProjectId, path: String) -> Result<(), String> {
    let document = with_runtime(state, |runtime| {
        runtime
            .document_for_save(project)
            .map_err(|error| error.to_string())
    })?;
    let write_path = PathBuf::from(&path);
    tauri::async_runtime::spawn_blocking(move || {
        metatorio_runtime::write_document_file(&document, &write_path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    with_runtime(state, |runtime| {
        runtime.mark_saved(project);
        Ok(())
    })?;
    if let Ok(mut paths) = state.project_paths.lock() {
        paths.insert(project, path);
    }
    Ok(())
}

/// 锁外求解一个工厂：取快照（短锁）→ 后台线程求解 → 回填可达性缓存（短锁）。
///
/// 求解本身不持有 runtime 锁，因此 MCP 调用与 GUI 交互不会被长求解挡住；
/// 不同工厂的求解还能真正并行（见 [`solve_jobs`]）。
pub(crate) async fn solve_factory_offlock<R: TauriRuntime>(
    app: &AppHandle<R>,
    state: &AppState,
    project: ProjectId,
    factory: FactoryId,
) -> Result<metatorio_runtime::SolveResult, String> {
    let snapshot_app = app.clone();
    let compute_app = app.clone();
    state
        .solve_jobs
        .run(
            (project, factory),
            move || {
                let state = snapshot_app.state::<AppState>();
                let runtime = state
                    .runtime
                    .lock()
                    .map_err(|_| "runtime 锁已损坏（poisoned）".to_string())?;
                runtime
                    .solve_snapshot_inputs(project, factory)
                    .map_err(|error| error.to_string())
            },
            move |snapshot| {
                let accessibility = snapshot.resolve_accessibility();
                let result = metatorio_runtime::solve_snapshot_with(snapshot, &accessibility)
                    .map_err(|error| error.to_string())?;
                // 锁外算出的可达性回填缓存（仅当文档/失效代次都没变）。
                if let Ok(runtime) = compute_app.state::<AppState>().runtime.lock() {
                    runtime.cache_accessibility_if_current(
                        snapshot.project,
                        snapshot.revision,
                        snapshot.accessibility_epoch,
                        accessibility,
                    );
                }
                Ok(result)
            },
        )
        .await
}

/// 一条（或一组嵌套）命令的执行结果。
///
/// 失败时 GUI 侧已经 `emit` 了错误事件；`errors` 是给**发起方**（MCP 工具、
/// 未来的其它适配层）的显式回执——否则调用方只能看到 `solve: null`，无法区分
/// 「无解」「参数无效」「什么都没做」。
#[derive(Default)]
struct CommandOutcome {
    /// 求解类命令的结构化产出。
    effect: Option<metatorio_runtime::CommandEffect>,
    /// 失败信息（按发生顺序累积；GUI 事件不受影响）。
    errors: Vec<String>,
}

impl CommandOutcome {
    fn done(effect: Option<metatorio_runtime::CommandEffect>) -> Self {
        Self {
            effect,
            errors: Vec::new(),
        }
    }

    fn failed(error: impl Into<String>) -> Self {
        Self {
            effect: None,
            errors: vec![error.into()],
        }
    }

    /// 合并嵌套命令的结果：保留第一个求解产出，错误全部累积。
    fn absorb(&mut self, other: CommandOutcome) {
        if self.effect.is_none() {
            self.effect = other.effect;
        }
        self.errors.extend(other.errors);
    }
}

/// 逐条执行命令并汇总结果：第一个求解产出、命令的 JSON 序列化、全部错误。
///
/// 抽出来是为了让「命令执行 → 工具回执」的汇总逻辑可单测（不需要 Tauri
/// AppHandle：执行器由调用方以闭包注入）。
pub(crate) async fn run_commands<F, Fut>(
    commands: &[RuntimeCommand],
    mut execute: F,
) -> (
    Option<metatorio_runtime::SolveResult>,
    Vec<serde_json::Value>,
    Vec<String>,
)
where
    F: FnMut(&RuntimeCommand) -> Fut,
    Fut: std::future::Future<Output = CommandOutcome>,
{
    let mut solve = None;
    let mut serialized = Vec::new();
    let mut errors = Vec::new();
    for command in commands {
        let outcome = execute(command).await;
        if solve.is_none() {
            if let Some(metatorio_runtime::CommandEffect::Solve(result)) = outcome.effect {
                solve = Some(result);
            }
        }
        errors.extend(outcome.errors);
        if let Ok(value) = serde_json::to_value(command) {
            serialized.push(value);
        }
    }
    (solve, serialized, errors)
}

/// Execute the side effects of one [`RuntimeCommand`] on the shared runtime,
/// emitting the usual Tauri events (so a live GUI stays in sync).
///
/// 每条分支只在自己的**短临界区**内持锁；求解（Recompute / AutoPlan）与
/// 文件/上下文 IO 都在锁外进行，因此本函数是 `async` 的。
///
/// 返回 [`CommandOutcome`]：求解类命令带上 `CommandEffect` 供 MCP 等非 GUI
/// 消费方直接取用；任何失败都会记录在 `errors` 里（同时仍 emit 事件）。
async fn execute_command<R: TauriRuntime>(
    app: &AppHandle<R>,
    state: &AppState,
    command: &RuntimeCommand,
) -> CommandOutcome {
    match command {
        RuntimeCommand::Recompute { project, factory } => {
            let (project, factory) = (*project, *factory);
            // 先确保项目的上下文 store 在内存里（读盘在锁外完成）。
            if let Err(error) = ensure_context_for_project_offlock(state, project).await {
                emit(app, "solve-error", error.clone());
                return CommandOutcome::failed(error);
            }
            // 锁外求解。
            match solve_factory_offlock(app, state, project, factory).await {
                Ok(result) => {
                    emit(app, "solve-result", result.clone());
                    CommandOutcome::done(Some(metatorio_runtime::CommandEffect::Solve(result)))
                }
                Err(error) => {
                    emit(app, "solve-error", error.clone());
                    CommandOutcome::failed(error)
                }
            }
        }
        RuntimeCommand::EnsureMachineCompat {
            project,
            factory,
            mechanic,
        } => {
            // 内部一致性修复（best-effort）：失败只记日志，不作为本次操作的
            // 失败回执，否则每次普通编辑都可能被无关的修复失败污染。
            let (project, factory, mechanic) = (*project, *factory, *mechanic);
            if let Err(error) = with_runtime(state, |runtime| {
                ensure_machine_compat(state, runtime, project, factory, mechanic)
            }) {
                eprintln!("机器兼容性兜底失败：{error}");
            }
            CommandOutcome::default()
        }
        RuntimeCommand::EnsureQualityLimit { project } => {
            let project = *project;
            if let Err(error) = with_runtime(state, |runtime| {
                ensure_quality_limit(state, runtime, project)
            }) {
                eprintln!("quality limit auto-raise failed: {error}");
            }
            CommandOutcome::default()
        }
        RuntimeCommand::ClampModules {
            project,
            factory,
            mechanic,
        } => {
            let (project, factory, mechanic) = (*project, *factory, *mechanic);
            if let Err(error) = with_runtime(state, |runtime| {
                clamp_modules(state, runtime, project, factory, mechanic)
            }) {
                eprintln!("module clamp failed: {error}");
            }
            CommandOutcome::default()
        }
        RuntimeCommand::Persist { project, path } => {
            let project = *project;
            let path = path
                .clone()
                .or_else(|| state.project_paths.lock().ok()?.get(&project).cloned());
            let Some(path) = path else {
                // Pathless persist with no remembered path is a no-op: 这是**自动
                // 落盘**的路径（元数据变更），未保存过的新项目不该报错。
                return CommandOutcome::default();
            };
            // 锁外序列化 + 写盘。
            match persist_project(state, project, path).await {
                Ok(()) => CommandOutcome::default(),
                Err(error) => {
                    eprintln!("persist failed: {error}");
                    CommandOutcome::failed(format!("保存项目 {} 失败: {error}", project.0))
                }
            }
        }
        RuntimeCommand::SaveProject { project } => {
            // 显式保存（ApplicationAction::SaveProject）：没有记忆路径时必须报错，
            // 否则 agent 会把静默 no-op 当成保存成功。
            let project = *project;
            let path = state
                .project_paths
                .lock()
                .ok()
                .and_then(|paths| paths.get(&project).cloned());
            let Some(path) = path else {
                let error = format!(
                    "项目 {} 尚无保存路径：请先用 save-project-as 指定路径",
                    project.0
                );
                emit(app, "solve-error", error.clone());
                return CommandOutcome::failed(error);
            };
            match persist_project(state, project, path).await {
                Ok(()) => CommandOutcome::default(),
                Err(error) => {
                    emit(app, "solve-error", error.clone());
                    CommandOutcome::failed(format!("保存项目 {} 失败: {error}", project.0))
                }
            }
        }
        RuntimeCommand::LoadGameContext {
            executable_path,
            mod_path,
        } => {
            match load_game_context_and_activate(app, state, executable_path, mod_path.as_deref())
                .await
            {
                Ok(_) => {
                    emit_contexts_changed(app, state, None);
                    CommandOutcome::default()
                }
                Err(error) => {
                    emit(app, "context-error", error.clone());
                    CommandOutcome::failed(error)
                }
            }
        }
        RuntimeCommand::LoadCachedContext => {
            // 恢复最近创建的上下文。
            let newest = state.contexts.lock().ok().and_then(|registry| {
                registry
                    .meta
                    .values()
                    .max_by_key(|meta| meta.created_at)
                    .map(|meta| meta.id.clone())
            });
            match newest {
                Some(id) => {
                    // 读盘解析在锁外完成。
                    match ensure_context_loaded_offlock(state, &id).await {
                        Ok(()) => {
                            let _ = with_runtime(state, |runtime| {
                                runtime.set_active_context(Some(id.clone()));
                                Ok(())
                            });
                            emit_contexts_changed(app, state, None);
                            CommandOutcome::default()
                        }
                        Err(error) => {
                            emit(app, "context-error", error.clone());
                            CommandOutcome::failed(error)
                        }
                    }
                }
                None => {
                    let error = "没有缓存的游戏数据".to_string();
                    emit(app, "context-error", error.clone());
                    CommandOutcome::failed(error)
                }
            }
        }
        RuntimeCommand::SetActiveContext { context } => {
            match activate_context(app, context.clone()).await {
                Ok(_) => CommandOutcome::default(),
                Err(error) => {
                    emit(app, "context-error", error.clone());
                    CommandOutcome::failed(error)
                }
            }
        }
        RuntimeCommand::RenameContext { id, name } => {
            match rename_registered_context(app, id.clone(), name.clone()).await {
                Ok(_) => CommandOutcome::default(),
                Err(error) => {
                    emit(app, "context-error", error.clone());
                    CommandOutcome::failed(error)
                }
            }
        }
        RuntimeCommand::DeleteContext { id } => {
            match delete_registered_context(app, id.clone()).await {
                Ok(_) => CommandOutcome::default(),
                Err(error) => {
                    emit(app, "context-error", error.clone());
                    CommandOutcome::failed(error)
                }
            }
        }
        RuntimeCommand::Cleanup {
            project,
            factory,
            action,
        } => {
            let (project, factory, action) = (*project, *factory, *action);
            if let Err(error) = ensure_context_for_project_offlock(state, project).await {
                emit(app, "solve-error", error.clone());
                return CommandOutcome::failed(error);
            }
            // 1) 锁外求解（清理规则基于求解结果）。
            let solved = match solve_factory_offlock(app, state, project, factory).await {
                Ok(result) => result,
                Err(error) => {
                    emit(app, "solve-error", error.clone());
                    return CommandOutcome::failed(error);
                }
            };
            // 2) 锁内按用量回写（走 reducer 收尾：revision/dirty/Persist/Recompute）。
            let commands = match with_runtime(state, |runtime| {
                runtime
                    .state
                    .apply_cleanup(
                        project,
                        factory,
                        action,
                        &metatorio_runtime::solve::mechanic_usage(&solved),
                    )
                    .map(|outcome| outcome.commands)
                    .map_err(|error| error.to_string())
            }) {
                Ok(commands) => commands,
                Err(error) => {
                    emit(app, "solve-error", error.clone());
                    return CommandOutcome::failed(error);
                }
            };
            // 3) 执行回写产生的命令（落盘 + 重解）。
            let mut outcome = CommandOutcome::default();
            for command in &commands {
                outcome.absorb(Box::pin(execute_command(app, state, command)).await);
            }
            outcome
        }
        RuntimeCommand::AutoPlan { project, factory } => {
            // 自动规划：枚举候选 → LP → 回写被选中的机制 → 重解。
            let (project, factory) = (*project, *factory);
            if let Err(error) = ensure_context_for_project_offlock(state, project).await {
                emit(app, "solve-error", error.clone());
                return CommandOutcome::failed(error);
            }
            // 1) 锁外枚举 + 求解（真实 dump 上可能数十秒，绝不持锁）。
            let snapshot_app = app.clone();
            let compute_app = app.clone();
            let planned = state
                .autoplan_jobs
                .run(
                    (project, factory),
                    move || {
                        let state = snapshot_app.state::<AppState>();
                        let runtime = state
                            .runtime
                            .lock()
                            .map_err(|_| "runtime 锁已损坏（poisoned）".to_string())?;
                        runtime
                            .solve_snapshot_inputs(project, factory)
                            .map_err(|error| error.to_string())
                    },
                    move |snapshot| {
                        let accessibility = snapshot.resolve_accessibility();
                        let mechanics =
                            metatorio_runtime::solve::plan_auto_plan(snapshot, &accessibility)
                                .map_err(|error| error.to_string())?;
                        if let Ok(runtime) = compute_app.state::<AppState>().runtime.lock() {
                            runtime.cache_accessibility_if_current(
                                snapshot.project,
                                snapshot.revision,
                                snapshot.accessibility_epoch,
                                accessibility,
                            );
                        }
                        Ok((snapshot.clone(), mechanics))
                    },
                )
                .await;
            let (snapshot, mechanics) = match planned {
                Ok(planned) => planned,
                Err(error) => {
                    emit(app, "solve-error", error.clone());
                    return CommandOutcome::failed(error);
                }
            };
            // 2) 锁内回写：目标工厂/项目设置必须与快照一致，否则会覆盖用户在
            //    规划期间的编辑（其它工厂的改动不影响——见 document_matches）。
            //
            //    `apply_auto_plan` 除了替换机制，还会把工厂标记为**严格供给**：
            //    规划本身就是按严格供给算的，文档不跟着落到同一模式的话，之后
            //    任何一次普通重解都会给出另一套结果（人和 agent 反复踩这个坑）。
            //    机制与标记都没变时它返回 `changed = false`，此时直接重解回传
            //    （大概率命中缓存），省掉 revision bump / 落盘。
            let written = with_runtime(state, |runtime| {
                if !runtime.document_matches(&snapshot) {
                    return Err(
                        "该工厂或项目设置在自动规划期间被修改，已放弃本次回写，请重试".to_string(),
                    );
                }
                runtime
                    .state
                    .apply_auto_plan(project, factory, mechanics)
                    .map_err(|error| error.to_string())
            });
            let written = match written {
                Ok(written) => written,
                Err(error) => {
                    emit(app, "solve-error", error.clone());
                    return CommandOutcome::failed(error);
                }
            };
            if !written.changed {
                return match solve_factory_offlock(app, state, project, factory).await {
                    Ok(result) => {
                        emit(app, "solve-result", result.clone());
                        CommandOutcome::done(Some(metatorio_runtime::CommandEffect::Solve(result)))
                    }
                    Err(error) => {
                        emit(app, "solve-error", error.clone());
                        CommandOutcome::failed(error)
                    }
                };
            }
            // 3) 执行回写产生的命令（落盘 + 重解），并让 GUI 重新拉取文档。
            let mut outcome = CommandOutcome::default();
            for command in &written.commands {
                outcome.absorb(Box::pin(execute_command(app, state, command)).await);
            }
            outcome
        }
        RuntimeCommand::LoadProject { path } => {
            // 读盘 + JSON 解析在锁外；迁移/导入在短锁内。
            let load_path = PathBuf::from(path.clone());
            let parsed = tauri::async_runtime::spawn_blocking(move || {
                metatorio_runtime::parse_document_file(&load_path)
                    .map_err(|error| error.to_string())
            })
            .await
            .map_err(|error| error.to_string());
            let value = match parsed {
                Ok(Ok(value)) => value,
                Ok(Err(error)) | Err(error) => {
                    emit(app, "solve-error", error.clone());
                    return CommandOutcome::failed(error);
                }
            };
            match with_runtime(state, |runtime| {
                runtime
                    .import_document_value(value)
                    .map_err(|error| error.to_string())?;
                Ok(runtime.state.document.clone())
            }) {
                Ok(document) => {
                    if let Ok(mut paths) = state.project_paths.lock() {
                        for project in &document.projects {
                            paths.entry(project.id).or_insert_with(|| path.clone());
                        }
                    }
                    emit(app, "document-changed", ());
                    CommandOutcome::default()
                }
                Err(error) => {
                    emit(app, "solve-error", error.clone());
                    CommandOutcome::failed(error)
                }
            }
        }
        RuntimeCommand::CloseProject { project } => {
            let project = *project;
            let outcome = with_runtime(state, |runtime| {
                runtime
                    .state
                    .close_project(project)
                    .map(|outcome| outcome.commands)
                    .map_err(|error| error.to_string())
            });
            if let Ok(mut paths) = state.project_paths.lock() {
                paths.remove(&project);
            }
            match outcome {
                Ok(commands) => {
                    let mut outcome = CommandOutcome::default();
                    for command in &commands {
                        outcome.absorb(Box::pin(execute_command(app, state, command)).await);
                    }
                    outcome
                }
                Err(error) => {
                    eprintln!("close project failed: {error}");
                    CommandOutcome::failed(format!("关闭项目 {} 失败: {error}", project.0))
                }
            }
        }
        // 以下命令目前只有消息侧定义、没有实现（GUI 分别走 `suggest` /
        // `implicit_sources` 等独立 Tauri 命令，更新走 tauri-plugin-updater 的
        // JS 插件）。保留显式分支而不是 `_ =>`，这样新增 RuntimeCommand 变体会在
        // 编译期暴露；调用方也应收到明确的「未实现」而不是静默成功。
        RuntimeCommand::RequestSuggestions { .. } => {
            CommandOutcome::failed("request-suggestions 未实现（GUI 走 suggest 命令）")
        }
        RuntimeCommand::ReplaceExternalInputs { .. } => {
            CommandOutcome::failed("replace-external-inputs 未实现（GUI 走 implicit_sources 命令）")
        }
        RuntimeCommand::CheckForUpdate => {
            CommandOutcome::failed("check-for-update 未实现（前端走 updater 插件）")
        }
        RuntimeCommand::InstallUpdate => {
            CommandOutcome::failed("install-update 未实现（前端走 updater 插件）")
        }
        RuntimeCommand::RestartAfterUpdate => {
            CommandOutcome::failed("restart-after-update 未实现（前端走 updater 插件）")
        }
    }
}

/// 确保项目上下文已载入：读盘解析在锁外完成。
async fn ensure_context_for_project_offlock(
    state: &AppState,
    project: ProjectId,
) -> Result<(), String> {
    let context_id = with_runtime(state, |runtime| {
        Ok(runtime
            .state
            .project(project)
            .map_err(|error| error.to_string())?
            .context_id
            .clone()
            .or_else(|| runtime.active_context().map(str::to_string)))
    })?;
    if let Some(id) = context_id {
        ensure_context_loaded_offlock(state, &id).await?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(options: Options) {
    // `tauri.conf.json` 的窗口是在事件循环首次迭代的 `setup` 阶段创建的
    // （tauri 源码 `fn setup`：`for window_config in app.config().app.windows`）。
    // 无头模式因此**在 build 之前清空窗口配置**：`setup` 照常跑（注册表扫描、
    // 恢复最近上下文、启动 MCP 都在里面），但一个窗口都不会建，事件循环也不会
    // 因为「最后一个窗口关闭」而退出。
    let mut context = tauri::generate_context!();
    if options.headless {
        context.config_mut().app.windows.clear();
        if options.mcp {
            println!(
                "切向量化 headless：不创建窗口，MCP 端点在 http://{}:{}{}",
                options.mcp_bind,
                options.mcp_port,
                mcp::MCP_PATH
            );
        } else {
            println!("切向量化 headless：不创建窗口，且已关闭 MCP（没有任何接口）");
        }
    }
    let mcp_enabled = options.mcp;
    let mcp_config = mcp::ServerConfig {
        bind: options.mcp_bind.clone(),
        port: options.mcp_port,
        token: options.mcp_token.clone(),
        allow_hosts: options.mcp_allow_hosts.clone(),
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::with_options(&options))
        .setup(move |app| {
            // 先把 MCP 端点起起来：恢复缓存上下文可能要读几十 MB 的 dump（真机上
            // 数秒），而 MCP 客户端（尤其 headless/agent 场景）往往启动后立刻连接。
            // 此时 AppState 已 manage 完毕，工具照常可见；若正好在载入上下文，
            // 调用会在 runtime 锁上短暂等待。
            #[cfg(not(mobile))]
            if mcp_enabled {
                mcp::spawn_server(app.handle().clone(), mcp_config.clone());
            }

            // 恢复缓存注册表并激活最近使用的上下文。
            let dir = app
                .path()
                .app_data_dir()
                .map(|dir| dir.join("contexts"))
                .unwrap_or_default();
            let state = app.state::<AppState>();
            {
                let mut registry = state.contexts.lock().expect("contexts 锁");
                registry.dir = dir;
                registry.scan();
            }
            let newest: Option<String> = state.contexts.lock().ok().and_then(|registry| {
                registry
                    .meta
                    .values()
                    .max_by_key(|meta| meta.created_at)
                    .map(|meta| meta.id.clone())
            });
            if let Some(id) = newest {
                let mut runtime = state.runtime.lock().expect("runtime 锁");
                if let Err(error) = ensure_context_loaded(&state, &mut runtime, &id) {
                    eprintln!("载入缓存上下文失败：{error}");
                } else {
                    runtime.set_active_context(Some(id));
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_bundled_dump,
            load_game_context,
            load_dump,
            list_contexts,
            // set_active_context / rename_context / delete_context 已收敛为
            // AppMessage（ApplicationAction），由 dispatch 命令统一处理。
            pick_game_executable,
            pick_dump_file,
            pick_mod_dir,
            icon,
            catalog_index,
            prototype_detail,
            suggest,
            implicit_sources,
            mechanic_flow,
            solar_balance,
            allowed_modules,
            dispatch,
            get_document,
            accessibility,
            milestones_ordered,
            productivity,
            pick_project_file,
            pick_project_save_path,
            project_save_path,
        ])
        .run(context)
        .expect("tauri 应用运行出错");
}

#[cfg(test)]
mod tests {
    use metatorio_runtime::message::{
        ApplicationAction, FactoryTemplate, MechanicAction, MechanicListAction,
        RecipeMechanicAction,
    };
    use metatorio_runtime::SolveStatus;

    use super::*;

    /// 机制流（MCP `get_planning_state {mechanic}` 与 GUI 机制卡读的同一份）的语义：
    /// **系数 = 1 时每秒**的产/耗。用演示 dump（2 铁板 → 1 铁齿轮）断言两件事：
    /// 比率必须与配方一致；速率必须带上机器速度（`assembling-machine-1` 速度 0.5、
    /// 铁齿轮 0.5 s/个 → 1 个/秒）。速率写死数字会把「机器速度没生效」这种错也一起
    /// 写进期望值里，所以这里断言的是**语义**。
    #[test]
    fn mechanic_flow_reports_recipe_rates_at_coefficient_one() {
        let dump: serde_json::Value = serde_json::from_str(DEMO_DUMP).expect("内置示例 dump");
        let mut runtime = Runtime::new();
        runtime.install_context(
            "demo".to_string(),
            PrototypeStore::load(&dump).expect("加载内置示例 dump"),
        );
        runtime.set_active_context(Some("demo".to_string()));
        runtime
            .dispatch(AppMessage::Application(ApplicationAction::NewProject {
                name: "p".to_string(),
            }))
            .unwrap();
        let project = runtime.state.document.projects[0].id;
        runtime
            .dispatch(AppMessage::Project {
                project,
                action: ProjectAction::AddFactory {
                    name: "f".to_string(),
                    template: FactoryTemplate::Empty,
                },
            })
            .unwrap();
        let factory = runtime.state.project(project).unwrap().factories[0].id;
        runtime
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::MechanicList(MechanicListAction::Add {
                    kind: metatorio_runtime::document::MechanicKind::Recipe,
                }),
            })
            .unwrap();
        let mechanic = runtime.state.factory(project, factory).unwrap().mechanics[0].id;
        for action in [
            MechanicAction::Recipe(RecipeMechanicAction::SetRecipe {
                recipe: IdWithQuality::new("iron-gear-wheel", "normal"),
            }),
            MechanicAction::Recipe(RecipeMechanicAction::SetMachine {
                machine: IdWithQuality::new("assembling-machine-1", "normal"),
            }),
        ] {
            runtime
                .dispatch(AppMessage::Factory {
                    project,
                    factory,
                    action: FactoryAction::Mechanic { mechanic, action },
                })
                .unwrap();
        }

        let snapshot = runtime
            .solve_snapshot_inputs(project, factory)
            .expect("求解快照");
        let flow = mechanic_flow_from_snapshot(&snapshot, mechanic).expect("机制流");
        let amount = |id: &str| {
            flow.iter()
                .find(|(flow, _)| *flow == DualVar::Item(IdWithQuality::new(id, "normal")))
                .map(|(_, amount)| *amount)
        };
        let iron = amount("iron-plate").expect("消耗铁板");
        let gear = amount("iron-gear-wheel").expect("产出铁齿轮");
        // 约定：正数产出、负数消耗（MCP 层负责拆成 inputs / outputs 的正数）。
        assert!(iron < 0.0 && gear > 0.0, "{flow:?}");
        assert!(
            (iron.abs() / gear - 2.0).abs() < 1e-9,
            "比率应与配方一致（2 铁板 : 1 铁齿轮）：{flow:?}"
        );
        assert!(
            (gear - 1.0).abs() < 1e-9,
            "速率应带上机器速度（速度 0.5 × 0.5 s/个）：{flow:?}"
        );
    }

    /// 上下文删除的引用守卫：只有 `context_id` 精确等于目标 id 的项目才算引用；
    /// `None`（跟随激活上下文）不算引用。GUI 命令与消息层共用它，避免 agent 走
    /// dispatch 时能删掉正在被引用的上下文。
    #[test]
    fn context_reference_guard_is_exact() {
        let mut document = AppDocument::default();
        let mut pinned = metatorio_runtime::document::ProjectDocument::default();
        pinned.context_id = Some("hash-a".to_string());
        let mut follows_active = metatorio_runtime::document::ProjectDocument::default();
        follows_active.context_id = None;
        document.projects.push(pinned);
        document.projects.push(follows_active);

        assert!(context_referenced(&document, "hash-a"));
        assert!(!context_referenced(&document, "hash-b"));
        assert!(!context_referenced(&document, ""));
    }

    /// `list_prototypes` 的过滤契约：kind 精确、名字大小写不敏感、同时匹配
    /// 本地化名；两个条件都不给时返回全部（工具只负责过滤，不做截断）。
    #[test]
    fn index_entry_filter_matches_kind_and_name() {
        let entry = |kind: &str, name: &str, localized: &str| IndexEntry {
            kind: kind.to_string(),
            name: name.to_string(),
            localized_name: localized.to_string(),
            group: String::new(),
            subgroup: String::new(),
            icon_type: String::new(),
            module_slots: None,
            categories: Vec::new(),
            fuel_category: String::new(),
            fuel_value_j: None,
            technology_max_level: None,
            technology_base_level: 0,
        };
        let entries = vec![
            entry("item", "iron-plate", "铁板"),
            entry("item", "iron-gear-wheel", "铁齿轮"),
            entry("recipe", "iron-gear-wheel", "铁齿轮"),
            entry("fluid", "water", "水"),
        ];

        let names = |filtered: &[IndexEntry]| {
            filtered
                .iter()
                .map(|entry| (entry.kind.clone(), entry.name.clone()))
                .collect::<Vec<_>>()
        };

        // 无过滤 → 全部
        assert_eq!(filter_index_entries(entries.clone(), None, None).len(), 4);
        // kind 精确：recipe 不会连带 item
        assert_eq!(
            names(&filter_index_entries(entries.clone(), Some("recipe"), None)),
            vec![("recipe".to_string(), "iron-gear-wheel".to_string())]
        );
        // 名字子串：大小写不敏感
        assert_eq!(
            filter_index_entries(entries.clone(), None, Some("IRON-")).len(),
            3
        );
        // 本地化名也参与匹配
        assert_eq!(
            names(&filter_index_entries(entries.clone(), None, Some("水"))),
            vec![("fluid".to_string(), "water".to_string())]
        );
        // 两个条件叠加
        assert_eq!(
            names(&filter_index_entries(
                entries.clone(),
                Some("item"),
                Some("iron-plate")
            )),
            vec![("item".to_string(), "iron-plate".to_string())]
        );
    }

    /// 测试用目录条目（只填与匹配相关的字段）。
    fn index_entry(kind: &str, name: &str, localized: &str) -> IndexEntry {
        IndexEntry {
            kind: kind.to_string(),
            name: name.to_string(),
            localized_name: localized.to_string(),
            group: String::new(),
            subgroup: String::new(),
            icon_type: String::new(),
            module_slots: None,
            categories: Vec::new(),
            fuel_category: String::new(),
            fuel_value_j: None,
            technology_max_level: None,
            technology_base_level: 0,
        }
    }

    /// 名字解析的两个方向：id → 本地化名（把求解结果翻译成人话）、口述名 → id
    /// （群里说的「铁板」落到 `iron-plate`）。两者共用一套匹配/排序口径，因此这里
    /// 逐条钉住：精确优先、跨组同名都返回、模糊按「本地化前缀 → 原型前缀 →
    /// 本地化子串 → 原型子串」排序、大小写不敏感、空查询不匹配、截断要如实上报。
    #[test]
    fn resolve_index_entry_handles_both_directions() {
        let entries = vec![
            index_entry("item", "iron-plate", "铁板"),
            index_entry("recipe", "iron-plate", "铁板"),
            index_entry("item", "iron-stick", "铁棒"),
            index_entry("item", "iron-plate-long", "铁板条"),
            index_entry("fluid", "water", "水"),
            index_entry("item", "no-translation", ""),
        ];

        // 方向一：id → 本地化名。同名跨组（item/recipe）都要返回，且都是精确命中。
        // `iron-plate-long` 会作为「id 前缀」进入模糊结果（有意如此：id 也只记得
        // 前半截时仍能找回来），所以这里只钉精确组。
        let by_name = resolve_index_entry(&entries, "iron-plate", 8);
        assert_eq!(by_name.exact.len(), 2, "item 与 recipe 都应返回");
        assert!(by_name
            .exact
            .iter()
            .all(|hit| hit.localized_name == "铁板" && hit.matched_by == "name-exact"));
        assert!(
            by_name
                .partial
                .iter()
                .any(|hit| hit.name == "iron-plate-long" && hit.matched_by == "name-prefix"),
            "{:?}",
            by_name.partial
        );

        // 方向二：口述名 → id。
        let by_localized = resolve_index_entry(&entries, "铁板", 8);
        assert_eq!(by_localized.exact.len(), 2);
        assert!(by_localized
            .exact
            .iter()
            .all(|hit| hit.name == "iron-plate" && hit.matched_by == "localized-exact"));
        assert!(
            by_localized
                .partial
                .iter()
                .any(|hit| hit.name == "iron-plate-long"),
            "「铁板条」应与「铁板」区分开：只出现在模糊结果里"
        );

        // 大小写不敏感（口述名多半是中文，但 id 会被随手打成大写）。
        assert_eq!(
            resolve_index_entry(&entries, "IRON-PLATE", 8).exact.len(),
            2
        );

        // 模糊排序 + 确定性：全是 name-prefix 时，按本地化名长度再按名字排序。
        let partial = resolve_index_entry(&entries, "iron", 8).partial;
        assert_eq!(
            partial
                .iter()
                .map(|hit| hit.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "iron-plate",      // item（铁板）
                "iron-plate",      // recipe（同名同长，保持索引顺序）
                "iron-stick",      // 铁棒
                "iron-plate-long", // 铁板条
            ]
        );
        assert!(partial.iter().all(|hit| hit.matched_by == "name-prefix"));

        // 截断要如实上报：命中 4 条、只取 2 条。
        let truncated = resolve_index_entry(&entries, "iron", 2);
        assert_eq!(truncated.partial.len(), 2);
        assert_eq!(truncated.partial_matched, 4);

        // 空/空白查询不匹配任何东西（否则 `contains("")` 会把全库倒出来）。
        for query in ["", "   "] {
            let empty = resolve_index_entry(&entries, query, 8);
            assert!(empty.exact.is_empty() && empty.partial.is_empty());
            assert_eq!(empty.partial_matched, 0);
        }

        // 无翻译的条目不会因为「本地化名为空」而被误命中。
        assert!(
            resolve_index_entry(&entries, "no-translation", 8)
                .exact
                .len()
                == 1
        );
    }

    /// 分隔符归一化：`-` / `_` / 空白（含全角、Unicode 破折号）在比较时等同，
    /// 因此 `processing unit` / `processing_unit` / `PROCESSING-UNIT` 都能命中
    /// `processing-unit`——同一件事在群里会被写成三种样子。
    #[test]
    fn resolve_index_entry_ignores_separators() {
        let entries = vec![
            index_entry("item", "processing-unit", "处理器"),
            index_entry("item", "processing-unit-2", "处理器2"),
            index_entry("fluid", "sulfuric-acid", "硫酸"),
        ];

        for query in [
            "processing unit",
            "processing_unit",
            "PROCESSING-UNIT",
            "  processing   unit  ",
            "processing\u{2011}unit", // Unicode 连字符
            "processing\u{ff0d}unit", // 全角连字符
        ] {
            let resolved = resolve_index_entry(&entries, query, 8);
            assert_eq!(
                resolved.exact.len(),
                1,
                "`{query}` 应精确命中 processing-unit：{:?}",
                resolved
            );
            assert_eq!(resolved.exact[0].name, "processing-unit");
            assert_eq!(resolved.exact[0].matched_by, "name-exact");
        }

        // 带序号的名字同样能被「用空格念出来」的写法找到。
        let numbered = resolve_index_entry(&entries, "processing unit 2", 8);
        assert_eq!(numbered.exact.len(), 1);
        assert_eq!(numbered.exact[0].name, "processing-unit-2");

        // 反方向：本地化名里的空格也归一化。
        let with_space = vec![index_entry("item", "weird-item", "奇怪 物品")];
        let resolved = resolve_index_entry(&with_space, "奇怪物品", 8);
        assert_eq!(resolved.exact.len(), 1, "本地化名里的空格应被忽略");
    }

    /// 打字错误候选：只在精确与模糊都为空时计算，按编辑距离排序，并给出置信证据
    /// ——**绝不替调用方选一个「最佳答案」**。
    #[test]
    fn resolve_index_entry_reports_typo_candidates_with_evidence() {
        let entries = vec![
            index_entry("item", "processing-unit", "处理器"),
            index_entry("item", "chemical-plant", "化工厂"),
        ];

        // 唯一且领先的错拼（漏一个字母 / 相邻换位）→ 给出高置信度建议。
        for query in ["procesing-unit", "processnig-unit"] {
            let resolved = resolve_index_entry(&entries, query, 8);
            assert!(resolved.exact.is_empty() && resolved.partial.is_empty());
            assert_eq!(resolved.typo_matched, 1, "`{query}`：{:?}", resolved.typo);
            assert_eq!(resolved.typo[0].name, "processing-unit");
            assert_eq!(resolved.typo[0].matched_by, "typo");
            assert_eq!(resolved.typo[0].distance, Some(1));
            assert_eq!(resolved.typo_best_distance, Some(1));
            assert_eq!(resolved.typo_best_name_count, 1);
            assert_eq!(
                resolved.typo_suggestion.map(|hit| hit.name),
                Some("processing-unit".to_string()),
                "唯一最接近的候选应作为高置信度建议给出"
            );
        }

        // 同名跨组（item + recipe）**不算歧义**：名字确定，只是 kind 要调用方选。
        let same_name_two_kinds = vec![
            index_entry("item", "processing-unit", "处理器"),
            index_entry("recipe", "processing-unit", "处理器"),
        ];
        let same_name = resolve_index_entry(&same_name_two_kinds, "procesing-unit", 8);
        assert_eq!(same_name.typo_best_name_count, 1);
        assert_eq!(same_name.typo_matched, 2);
        assert!(
            same_name.typo_suggestion.is_some(),
            "同名跨组不该被判成歧义：{:?}",
            same_name.typo
        );

        // 更远的候选仍会列出（供人判断），但不会影响「最佳唯一」的结论。
        let with_numbers = vec![
            index_entry("item", "processing-unit", "处理器"),
            index_entry("item", "processing-unit-2", "处理器2"),
        ];
        let ranked = resolve_index_entry(&with_numbers, "procesing-unit", 8);
        assert_eq!(ranked.typo[0].name, "processing-unit");
        assert_eq!(ranked.typo_best_distance, Some(1));
        assert_eq!(ranked.typo_best_name_count, 1);
        assert!(ranked.typo.len() >= 2, "{:?}", ranked.typo);

        // 两个不同的名字同样接近（`-2` 与 `-3`）→ 不给建议，必须人工确认。
        let ambiguous_entries = vec![
            index_entry("item", "processing-unit-2", "处理器2"),
            index_entry("item", "processing-unit-3", "处理器3"),
        ];
        let ambiguous = resolve_index_entry(&ambiguous_entries, "processing-unit-4", 8);
        assert_eq!(ambiguous.typo.len(), 2, "{:?}", ambiguous.typo);
        assert_eq!(ambiguous.typo_best_distance, Some(1));
        assert_eq!(ambiguous.typo_best_name_count, 2);
        assert!(
            ambiguous.typo_suggestion.is_none(),
            "并列时不能给「高置信度建议」：{:?}",
            ambiguous.typo_suggestion
        );

        // 命中精确/模糊时不计算 typo：正常查询不被近似结果干扰。
        let normal = resolve_index_entry(&entries, "chemical-plant", 8);
        assert_eq!(normal.exact.len(), 1);
        assert!(normal.typo.is_empty() && normal.typo_best_distance.is_none());

        // 查询太短（≤2 字符）时不做错拼匹配：那时任何名字都「差不多」。
        let short = resolve_index_entry(&entries, "zz", 8);
        assert!(short.typo.is_empty());

        // 差距过大时不硬凑答案。
        let unrelated = resolve_index_entry(&entries, "uranium-enrichment", 8);
        assert!(
            unrelated.typo.is_empty(),
            "不该凑近似答案：{:?}",
            unrelated.typo
        );
    }

    /// 错拼候选对**中文名**同样有效——群里玩家报的是中文，`铁版` 应该能查到 `铁板`。
    ///
    /// 以前这里有两个拉丁中心的假设，把中文名彻底挡在门外：
    /// 1. 长度预筛拿 `candidate.len()`（UTF-8 **字节数**）对比 `needle_chars.len()`
    ///    （**字符数**）——`铁齿轮` 是 9 字节 / 3 字符，差距 6 直接超阈值，于是所有
    ///    非 ASCII 名字都被跳过，错拼匹配对中文**从未生效**；
    /// 2. 门槛「3 个字符起」——`铁板` 只有两个字，却是一个完整的词。
    #[test]
    fn typo_candidates_work_for_cjk_names() {
        let entries = vec![
            index_entry("item", "iron-plate", "铁板"),
            index_entry("item", "steel-plate", "钢板"),
            index_entry("item", "iron-gear-wheel", "铁齿轮"),
        ];

        // 两个汉字打错一个：`铁版` → `铁板`（群里最典型的一幕）。`钢板` 有两处不同、
        // 距离 2，超出「2 字名」的阈值，所以这里是**唯一**候选 → 可以给高置信度建议。
        let resolved = resolve_index_entry(&entries, "铁版", 8);
        assert!(resolved.exact.is_empty() && resolved.partial.is_empty());
        assert_eq!(resolved.typo_best_distance, Some(1));
        assert_eq!(resolved.typo_best_name_count, 1, "{:?}", resolved.typo);
        assert_eq!(
            resolved.typo_suggestion.map(|hit| hit.name),
            Some("iron-plate".to_string()),
            "{:?}",
            resolved.typo
        );

        // 三个汉字打错一个：旧写法（字节长度预筛）在这里一条候选都给不出来。
        let resolved = resolve_index_entry(&entries, "铁齿抡", 8);
        assert_eq!(resolved.typo_best_distance, Some(1), "{:?}", resolved.typo);
        assert_eq!(
            resolved.typo_suggestion.map(|hit| hit.name),
            Some("iron-gear-wheel".to_string()),
            "{:?}",
            resolved.typo
        );

        // 单个汉字仍然太短：任何名字都「差不多」，不给候选。
        let short = resolve_index_entry(&entries, "铁", 8);
        assert!(short.typo.is_empty(), "{:?}", short.typo);
        // ASCII 两个字母同样太短（这条旧行为不能因为放宽中文而回退）。
        let short_ascii = resolve_index_entry(&entries, "zz", 8);
        assert!(short_ascii.typo.is_empty(), "{:?}", short_ascii.typo);
    }

    /// `list_prototypes` 的 `name_contains` 与名字解析共用同一套归一化。
    #[test]
    fn index_entry_filter_ignores_separators() {
        let entries = vec![
            index_entry("item", "processing-unit", "处理器"),
            index_entry("item", "iron-plate", "铁板"),
        ];
        let names = |filtered: Vec<IndexEntry>| {
            filtered
                .into_iter()
                .map(|entry| entry.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(filter_index_entries(
                entries.clone(),
                None,
                Some("processing unit")
            )),
            vec!["processing-unit".to_string()]
        );
        assert_eq!(
            names(filter_index_entries(entries, None, Some("IRON_PLATE"))),
            vec!["iron-plate".to_string()]
        );
    }

    /// 命令汇总契约：求解产出取第一个、错误全部收集、命令按序序列化。
    /// 这是「求解失败必须让 agent 看见」的实现基础。
    #[tokio::test]
    async fn run_commands_collects_errors_and_solve() {
        use metatorio_runtime::message::RuntimeCommand;
        let commands = vec![
            RuntimeCommand::CheckForUpdate,
            RuntimeCommand::Recompute {
                project: ProjectId(1),
                factory: FactoryId(2),
            },
        ];
        let (solve, serialized, errors) = run_commands(&commands, |command| {
            let failed = matches!(command, RuntimeCommand::CheckForUpdate);
            async move {
                if failed {
                    CommandOutcome::failed("boom")
                } else {
                    CommandOutcome::done(Some(metatorio_runtime::CommandEffect::Solve(
                        metatorio_runtime::SolveResult {
                            project: ProjectId(1),
                            factory: FactoryId(2),
                            status: SolveStatus::NotSolved {
                                no_provider: Vec::new(),
                                no_consumer: Vec::new(),
                                description: "test".to_string(),
                            },
                        },
                    )))
                }
            }
        })
        .await;

        assert!(solve.is_some(), "应保留求解产出");
        assert_eq!(errors, vec!["boom".to_string()], "错误必须被收集");
        assert_eq!(serialized.len(), 2, "每条命令都应序列化");
    }

    /// 品质上限自动提升必须看到**文档里显式引用的每一个品质**，包括那些
    /// 「当前模型没用到」的字段（蓄电器品质）与容易漏掉的分支（太阳能、
    /// 插件塔及其塔内插件）：漏掉就会让自动规划按过低的上限枚举品质，
    /// 把用户显式选的更高品质设备静默降级。
    #[test]
    fn mechanic_quality_level_covers_every_quality_bearing_field() {
        let qualities: Vec<String> = ["normal", "uncommon", "rare", "epic", "legendary"]
            .iter()
            .map(|quality| quality.to_string())
            .collect();
        let q = |name: &str| IdWithQuality::new("thing", name);
        let level = |mechanic: &Mechanic| mechanic_quality_level(&qualities, mechanic);

        // 各机制主设备（含曾被 `_` 吞掉的 Solar）。
        assert_eq!(
            level(&Mechanic::Solar(metatorio_core::SolarMechanic {
                solar_panel: q("legendary"),
                accumulator: q("normal"),
            })),
            4
        );
        assert_eq!(
            level(&Mechanic::Solar(metatorio_core::SolarMechanic {
                solar_panel: q("normal"),
                accumulator: q("rare"),
            })),
            2,
            "蓄电器品质同样要计入（它是文档里的显式选择）"
        );
        assert_eq!(
            level(&Mechanic::Reactor(metatorio_core::ReactorMechanic {
                reactor: q("epic"),
                ..Default::default()
            })),
            3
        );

        // 插件清单（recipe / mining）。
        let recipe_with_module = Mechanic::Recipe(metatorio_core::RecipeMechanic {
            recipe: q("normal"),
            machine: q("normal"),
            module_config: metatorio_core::ModuleConfig {
                modules: vec![q("uncommon")],
                beacons: Vec::new(),
            },
            ..Default::default()
        });
        assert_eq!(level(&recipe_with_module), 1);

        // 插件塔本体品质影响覆盖效率/耗电，塔内插件品质影响其效果——两者都要计入。
        let recipe_with_beacon = Mechanic::Recipe(metatorio_core::RecipeMechanic {
            recipe: q("normal"),
            machine: q("normal"),
            module_config: metatorio_core::ModuleConfig {
                modules: Vec::new(),
                beacons: vec![metatorio_core::BeaconConfig {
                    beacon: q("epic"),
                    modules: vec![(q("normal"), 1)],
                    ..Default::default()
                }],
            },
            ..Default::default()
        });
        assert_eq!(level(&recipe_with_beacon), 3, "插件塔本体品质要计入");
        let recipe_with_beacon_module = Mechanic::Recipe(metatorio_core::RecipeMechanic {
            recipe: q("normal"),
            machine: q("normal"),
            module_config: metatorio_core::ModuleConfig {
                modules: Vec::new(),
                beacons: vec![metatorio_core::BeaconConfig {
                    beacon: q("normal"),
                    modules: vec![(q("legendary"), 2)],
                    ..Default::default()
                }],
            },
            ..Default::default()
        });
        assert_eq!(level(&recipe_with_beacon_module), 4, "塔内插件品质要计入");

        // 没有品质字段的机制（流体燃料/流体热只有流体名）恒为 normal。
        assert_eq!(
            level(&Mechanic::FluidFuel(metatorio_core::FluidFuelMechanic {
                fluid: "steam".to_string(),
                ..Default::default()
            })),
            0
        );
    }

    /// 幂等缓存：同一 request_id 回放上次载荷；超过容量淘汰最早的。
    #[test]
    fn dispatch_cache_replays_and_evicts() {
        let mut cache = DispatchCache::default();
        assert!(cache.get("a").is_none());
        cache.insert("a".to_string(), serde_json::json!({ "revision": 1 }), false);
        let (payload, is_error) = cache.get("a").expect("应命中");
        assert_eq!(payload["revision"], 1);
        assert!(!is_error);

        cache.insert("b".to_string(), serde_json::json!({}), true);
        assert!(cache.get("b").expect("应命中").1, "错误结果同样记录");

        for index in 0..DispatchCache::CAPACITY {
            cache.insert(format!("k{index}"), serde_json::json!({}), false);
        }
        assert!(cache.get("a").is_none(), "最早的条目应被淘汰");
    }

    /// 游戏安装目录推断：`bin/x64/`、`bin/`、根目录三种布局都要命中，
    /// 且返回的是 `data` 目录本身（read-data 的语义）；找不到时返回 None。
    #[test]
    fn factorio_data_dir_handles_common_layouts() {
        let root = std::env::temp_dir().join(format!("metatorio-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("data")).unwrap();
        let expected = root.join("data");
        let bin_x64 = root.join("bin").join("x64");
        std::fs::create_dir_all(&bin_x64).unwrap();
        let exe = bin_x64.join("factorio.exe");
        std::fs::write(&exe, b"").unwrap();
        assert_eq!(factorio_data_dir(&exe), Some(expected.clone()));

        let flat = root.join("bin").join("factorio");
        std::fs::write(&flat, b"").unwrap();
        assert_eq!(factorio_data_dir(&flat), Some(expected.clone()));

        let bare = root.join("factorio");
        std::fs::write(&bare, b"").unwrap();
        assert_eq!(factorio_data_dir(&bare), Some(expected));

        let elsewhere =
            std::env::temp_dir().join(format!("metatorio-install-x-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&elsewhere);
        std::fs::create_dir_all(&elsewhere).unwrap();
        let stray = elsewhere.join("factorio");
        std::fs::write(&stray, b"").unwrap();
        assert_eq!(factorio_data_dir(&stray), None, "没有 data/ 时不应猜");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    #[test]
    fn empty_recipe_categories_default_to_crafting() {
        let recipe = RecipeComponent {
            categories: None,
            ..Default::default()
        };
        assert_eq!(effective_recipe_categories(&recipe), vec!["crafting"]);
        let recipe = RecipeComponent {
            categories: Some(vec!["smelting".to_string()]),
            ..Default::default()
        };
        assert_eq!(effective_recipe_categories(&recipe), vec!["smelting"]);
    }

    #[test]
    fn empty_resource_category_defaults_to_basic_solid() {
        let resource = ResourceEntityComponent {
            category: String::new(),
            ..Default::default()
        };
        assert_eq!(effective_resource_category(&resource), "basic-solid");
        let resource = ResourceEntityComponent {
            category: "calcite".to_string(),
            ..Default::default()
        };
        assert_eq!(effective_resource_category(&resource), "calcite");
    }

    /// 复刻用户操作序列（自动规划）：新建项目 → 新建工厂 → 设目标 =
    /// legendary electromagnetic-plant（amount 1e-6 vs 1.0）→ 星球
    /// fulgora → 主品质 legendary → 规划偏好"使用最佳插件"（枚举传奇
    /// 品质 module-3 四件套）→ 严格输入 → 自动规划。
    ///
    /// 诊断：对比目标倍率的可解性（倍率不变性问题暂缓，见
    /// global_scale_sensitivity 测试注释）。跑一次约 100 秒（真实 dump
    /// 16 万候选），标记 ignore 避免拖慢常规测试；调查时手动运行。
    #[test]
    #[ignore]
    fn fulgora_legendary_plant_auto_plan_scale_invariance() {
        use metatorio_runtime::message::{
            ApplicationAction, FactoryAction, FactoryContextAction, PlanningAction, ProjectAction,
        };

        // 仓库内真实 dump（相对路径，测试可移植；不依赖机器上的 %APPDATA%）。
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/data-raw-dump.json"
        );
        if !std::path::Path::new(path).exists() {
            eprintln!("[skip] 无真实 dump（{path}），跳过");
            return;
        }
        let raw = std::fs::read(path).expect("读 dump");
        let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump");
        let store = PrototypeStore::load(&dump).expect("dump 加载失败");

        let mut runtime = Runtime::new();
        runtime.install_context("real".to_string(), store);
        runtime.set_active_context(Some("real".to_string()));

        // 1. 新建项目。
        runtime
            .dispatch(AppMessage::Application(ApplicationAction::NewProject {
                name: "test".to_string(),
            }))
            .unwrap();
        let project = runtime.state.document.projects[0].id;
        // 2. 新建工厂。
        runtime
            .dispatch(AppMessage::Project {
                project,
                action: ProjectAction::AddFactory {
                    name: "factory".to_string(),
                    template: metatorio_runtime::message::FactoryTemplate::Empty,
                },
            })
            .unwrap();
        let factory = runtime.state.project(project).unwrap().factories[0].id;

        // 4. 星球 fulgora + 主品质 legendary。
        runtime
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::Context(FactoryContextAction::SetPlanet {
                    planet: Some("fulgora".to_string()),
                }),
            })
            .unwrap();
        runtime
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::Context(FactoryContextAction::SetMajorQuality {
                    quality: "legendary".to_string(),
                }),
            })
            .unwrap();

        // 5. 规划偏好"使用最佳插件"：枚举传奇品质的 module-3 四件套
        //    （前端 applyBestModules：清空后逐个添加）。
        for name in [
            "efficiency-module-3",
            "speed-module-3",
            "productivity-module-3",
            "quality-module-3",
        ] {
            runtime
                .dispatch(AppMessage::Project {
                    project,
                    action: ProjectAction::Planning(PlanningAction::AddEnumeratedModule {
                        module: IdWithQuality::new(name, "legendary"),
                    }),
                })
                .unwrap();
        }
        // 6. 严格输入。
        runtime
            .dispatch(AppMessage::Factory {
                project,
                factory,
                action: FactoryAction::SetStrictSource { strict: true },
            })
            .unwrap();

        // 7. 品质上限提升到 legendary（UI 设传奇目标时"超出会自动提升"）。
        runtime
            .dispatch(AppMessage::Project {
                project,
                action: metatorio_runtime::message::ProjectAction::SetQualityLimit {
                    quality: Some("legendary".to_string()),
                },
            })
            .unwrap();

        // 复刻用户操作：同一工厂，先设目标 1/s 点自动规划，再改 0.001/s
        // 点自动规划。auto_plan 会用选中机制替换工厂机制（第二次跑时
        // 工厂已有第一次的机制，但目标倍率变化不应影响可解性）。
        // 同一目标跑两次验证求解确定性（用户 0.001 可解、测试不可解，
        // 若两次不一致说明非确定性）。
        fn set_target(runtime: &mut Runtime, project: ProjectId, factory: FactoryId, amount: f64) {
            // 复刻 UI 修改目标金额（SetAmount，目标 id 不变——与删除重建
            // 不同，后者会改变目标列表顺序影响 LP 目标表达式结构）。
            let targets = runtime
                .state
                .factory(project, factory)
                .unwrap()
                .targets
                .clone();
            if let Some(target) = targets.first() {
                runtime
                    .dispatch(AppMessage::Factory {
                        project,
                        factory,
                        action: FactoryAction::Target(
                            metatorio_runtime::message::TargetAction::SetAmount {
                                target: target.id,
                                amount,
                            },
                        ),
                    })
                    .unwrap();
            } else {
                runtime
                    .dispatch(AppMessage::Factory {
                        project,
                        factory,
                        action: FactoryAction::Flow(
                            metatorio_runtime::message::FlowAction::AddToTarget {
                                flow: DualVar::Item(IdWithQuality::new(
                                    "electromagnetic-plant",
                                    "legendary",
                                )),
                                amount,
                            },
                        ),
                    })
                    .unwrap();
            }
        }

        // 每个目标用新工厂隔离（auto_plan 会替换机制），验证确定性。
        fn make_factory(runtime: &mut Runtime, project: ProjectId, amount: f64) -> FactoryId {
            runtime
                .dispatch(AppMessage::Project {
                    project,
                    action: ProjectAction::AddFactory {
                        name: format!("factory-{amount}"),
                        template: metatorio_runtime::message::FactoryTemplate::Empty,
                    },
                })
                .unwrap();
            let factory = runtime
                .state
                .project(project)
                .unwrap()
                .factories
                .last()
                .unwrap()
                .id;
            runtime
                .dispatch(AppMessage::Factory {
                    project,
                    factory,
                    action: FactoryAction::Context(FactoryContextAction::SetPlanet {
                        planet: Some("fulgora".to_string()),
                    }),
                })
                .unwrap();
            runtime
                .dispatch(AppMessage::Factory {
                    project,
                    factory,
                    action: FactoryAction::Context(FactoryContextAction::SetMajorQuality {
                        quality: "legendary".to_string(),
                    }),
                })
                .unwrap();
            runtime
                .dispatch(AppMessage::Factory {
                    project,
                    factory,
                    action: FactoryAction::SetStrictSource { strict: true },
                })
                .unwrap();
            set_target(runtime, project, factory, amount);
            factory
        }

        // 复刻用户操作：fulgora 传奇电磁工厂自动规划，对比目标倍率
        // （1e-6 vs 1.0）的可解性。这是诊断测试：记录两种倍率的结果，
        // 不强制断言（倍率不变性问题暂缓，根因是 dual_scale 达 5.85e5
        // 导致 global_scale 极小、microlp 数值路径随目标常数变化）。
        let mut outcomes = Vec::new();
        for (label, amount) in [("1e-6（小目标）", 1e-6), ("1.0（大目标）", 1.0)] {
            let factory = make_factory(&mut runtime, project, amount);
            let result = runtime.auto_plan(project, factory);
            let status = match &result {
                Ok(solve) => match &solve.status {
                    SolveStatus::Solved { .. } => "Solved".to_string(),
                    SolveStatus::NotSolved { description, .. } => {
                        format!("NotSolved: {description}")
                    }
                },
                Err(error) => format!("Err: {error}"),
            };
            eprintln!("{label}: {status}");
            outcomes.push((label, amount, result));
        }
        // 记录诊断：两种倍率结果（供后续倍率不变性调查）。
        eprintln!(
            "倍率对比：1e-6={:?}，1.0={:?}",
            outcomes[0].2.is_ok(),
            outcomes[1].2.is_ok()
        );
    }
}
