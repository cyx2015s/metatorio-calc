//! 求解任务调度：把长求解移出 `Mutex<Runtime>`。
//!
//! 背景：`execute_command` 里的 `Recompute` / `AutoPlan` 会占用 runtime 锁
//! 数毫秒到上百秒，期间 MCP 工具调用、GUI 的其它交互、甚至只读快照全部排队。
//! 求解本身只依赖一份[快照](metatorio_runtime::SolveSnapshot)（原型仓库 +
//! 依赖图 + 项目/工厂文档 + 可达性），因此可以脱离锁在后台线程上跑。
//!
//! 本模块提供的就是这层调度，语义为：
//!
//! - **同 `(project, factory)` 串行、不同工厂并行**：每个键一把异步锁，
//!   求解在 `spawn_blocking` 上执行，不持有 runtime 锁。
//! - **latest-wins 合并**：快照在**拿到键锁之后**才取，排队中的请求因此
//!   看到的是最新文档；若其身份（文档版本 + 可达性代次 + store 实例）与上次
//!   求解相同，直接复用结果，不再重算——拖滑块产生的连串重算合并成一次。
//! - **求解期间文档又变了 → 再算一轮**（最多 [`MAX_ROUNDS`] 轮，避免活锁），
//!   保证返回结果尽量新鲜。
//! - 失败结果不缓存（避免一次瞬时失败被后续请求反复命中）。

use std::collections::HashMap;
use std::sync::Arc;

use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{FactoryId, ProjectId, SolveSnapshot};
use tokio::sync::Mutex;

/// 求解的调度键：一个工厂的求解彼此无关，因此按工厂并行。
pub type SolveKey = (ProjectId, FactoryId);

/// 求解期间文档最多重算几轮（防止持续编辑导致的活锁）。
const MAX_ROUNDS: usize = 3;

/// 每键一份的求解槽位。
struct Slot<R> {
    /// 上次成功求解的身份与结果。
    last: Option<Cached<R>>,
}

/// 一次成功求解的缓存条目。
///
/// 身份 = 文档版本 + 可达性失效代次 + 原型仓库实例。三者都相同意味着求解
/// 输入完全相同（求解是纯函数），可以安全复用结果。
struct Cached<R> {
    revision: u64,
    accessibility_epoch: u64,
    store: Arc<PrototypeStore>,
    result: R,
}

impl<R> Cached<R> {
    fn matches(&self, snapshot: &SolveSnapshot) -> bool {
        self.revision == snapshot.revision
            && self.accessibility_epoch == snapshot.accessibility_epoch
            && Arc::ptr_eq(&self.store, &snapshot.store)
    }
}

/// 按字符串键串行化一段异步操作（「同一资源只加载一次」用）。
///
/// 例：两个并发请求要载入同一个游戏上下文时，只有一个真正读盘解析，
/// 另一个等它装好后直接返回。
#[derive(Default)]
pub struct KeyLocks {
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl KeyLocks {
    pub async fn lock(&self, key: String) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = self
            .locks
            .lock()
            .await
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        lock.lock_owned().await
    }
}

/// 求解调度器（挂在 `AppState` 上，按用途各一份）。
///
/// `R` 是这次「求解」的产出：重算产 [`SolveResult`]，自动规划产
/// `(快照, 候选机制)`（快照用于回写前的版本校验）。
pub struct SolveJobs<R> {
    slots: Mutex<HashMap<SolveKey, Arc<Mutex<Slot<R>>>>>,
}

impl<R> Default for SolveJobs<R> {
    fn default() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
        }
    }
}

impl<R: Clone + Send + 'static> SolveJobs<R> {
    /// 取某个工厂的键锁（不同键可并行）。
    async fn slot(&self, key: SolveKey) -> Arc<Mutex<Slot<R>>> {
        self.slots
            .lock()
            .await
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(Slot { last: None })))
            .clone()
    }

    /// 跑一次（或复用一次）求解。
    ///
    /// - `take_snapshot`：在 runtime 锁内取快照（微秒级），每次调用都会重新取。
    /// - `compute`：锁外纯计算，跑在阻塞线程池上；应自行回填可达性缓存。
    pub async fn run<F, G>(&self, key: SolveKey, take_snapshot: F, compute: G) -> Result<R, String>
    where
        F: Fn() -> Result<SolveSnapshot, String> + Send + Sync + 'static,
        G: Fn(&SolveSnapshot) -> Result<R, String> + Send + Sync + 'static,
    {
        let slot = self.slot(key).await;
        // 同键串行：等锁期间文档可能已被别人算过，因此快照要在拿到锁之后取。
        let mut slot = slot.lock().await;

        let compute = Arc::new(compute);
        let mut snapshot = take_snapshot()?;
        let mut round = 0usize;
        loop {
            if let Some(cached) = &slot.last {
                if cached.matches(&snapshot) {
                    return Ok(cached.result.clone());
                }
            }
            let result = {
                let task_snapshot = snapshot.clone();
                let compute = compute.clone();
                tauri::async_runtime::spawn_blocking(move || compute(&task_snapshot))
                    .await
                    .map_err(|error| format!("求解任务 join 失败: {error}"))?
            };
            match result {
                Ok(result) => {
                    slot.last = Some(Cached {
                        revision: snapshot.revision,
                        accessibility_epoch: snapshot.accessibility_epoch,
                        store: snapshot.store.clone(),
                        result: result.clone(),
                    });
                    // 求解期间文档又变了：用新快照再算一轮（latest-wins）。
                    let next = take_snapshot()?;
                    if next.revision == snapshot.revision || round + 1 >= MAX_ROUNDS {
                        return Ok(result);
                    }
                    snapshot = next;
                    round += 1;
                }
                // 失败不缓存：瞬时失败（如上下文尚未载入）不应粘住后续请求。
                Err(error) => return Err(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use metatorio_runtime::message::{
        AppMessage, ApplicationAction, FactoryAction, FactoryTemplate, ProjectAction,
    };
    use metatorio_runtime::{ProjectId, Runtime, SolveResult, SolveStatus};

    /// 造一个带项目/工厂的最小 runtime（内置示例 dump）。
    fn demo_runtime() -> (StdMutex<Runtime>, ProjectId, FactoryId) {
        let dump: serde_json::Value =
            serde_json::from_str(crate::DEMO_DUMP).expect("内置示例 dump 应可解析");
        let mut runtime = Runtime::new();
        runtime.install_context(
            "demo".to_string(),
            PrototypeStore::load(&dump).expect("内置示例 dump 应可加载"),
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
        (StdMutex::new(runtime), project, factory)
    }

    fn snapshot_fn(
        runtime: &Arc<StdMutex<Runtime>>,
        project: ProjectId,
        factory: FactoryId,
    ) -> impl Fn() -> Result<SolveSnapshot, String> + Send + Sync + 'static {
        let runtime = runtime.clone();
        move || {
            let runtime = runtime.lock().unwrap();
            runtime
                .solve_snapshot_inputs(project, factory)
                .map_err(|error| error.to_string())
        }
    }

    /// 一个假的「求解」：只计数并返回固定结果，避免测试真的跑 LP。
    fn fake_result(project: ProjectId, factory: FactoryId, tag: f64) -> SolveResult {
        SolveResult {
            project,
            factory,
            status: SolveStatus::Solved {
                cost: tag,
                mechanics: Vec::new(),
                flows: Vec::new(),
            },
        }
    }

    fn cost_of(result: &SolveResult) -> f64 {
        match &result.status {
            SolveStatus::Solved { cost, .. } => *cost,
            other => panic!("期望 Solved，得到 {other:?}"),
        }
    }

    /// 同一文档版本重复请求：只算一次。
    #[tokio::test]
    async fn identical_requests_reuse_the_cached_result() {
        let (runtime, project, factory) = demo_runtime();
        let runtime = Arc::new(runtime);
        let jobs = SolveJobs::default();
        let calls = Arc::new(AtomicUsize::new(0));
        for _ in 0..3 {
            let counter = calls.clone();
            let result = jobs
                .run(
                    (project, factory),
                    snapshot_fn(&runtime, project, factory),
                    move |_| {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok(fake_result(project, factory, 1.0))
                    },
                )
                .await
                .unwrap();
            assert_eq!(cost_of(&result), 1.0);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "同版本应只算一次");
    }

    /// 文档变化后必须重算。
    #[tokio::test]
    async fn new_revision_recomputes() {
        let (runtime, project, factory) = demo_runtime();
        let runtime = Arc::new(runtime);
        let jobs = SolveJobs::default();
        let calls = Arc::new(AtomicUsize::new(0));

        let counter = calls.clone();
        jobs.run(
            (project, factory),
            snapshot_fn(&runtime, project, factory),
            move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(fake_result(project, factory, 1.0))
            },
        )
        .await
        .unwrap();

        // 改文档 → revision 变化。
        {
            let mut runtime = runtime.lock().unwrap();
            runtime
                .dispatch(AppMessage::Factory {
                    project,
                    factory,
                    action: FactoryAction::SetName {
                        name: "renamed".to_string(),
                    },
                })
                .unwrap();
        }

        let counter = calls.clone();
        jobs.run(
            (project, factory),
            snapshot_fn(&runtime, project, factory),
            move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(fake_result(project, factory, 2.0))
            },
        )
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "新版本必须重算");
    }

    /// 并发请求同一个键：单飞，第二次命中缓存。
    #[tokio::test]
    async fn concurrent_requests_for_one_key_run_once() {
        let (runtime, project, factory) = demo_runtime();
        let runtime = Arc::new(runtime);
        let jobs = Arc::new(SolveJobs::<SolveResult>::default());
        let calls = Arc::new(AtomicUsize::new(0));

        let make_task = |jobs: Arc<SolveJobs<SolveResult>>, calls: Arc<AtomicUsize>| {
            let runtime = runtime.clone();
            tokio::spawn(async move {
                jobs.run(
                    (project, factory),
                    snapshot_fn(&runtime, project, factory),
                    move |_| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(50));
                        Ok(fake_result(project, factory, 1.0))
                    },
                )
                .await
                .unwrap()
            })
        };
        let (a, b) = tokio::join!(
            make_task(jobs.clone(), calls.clone()),
            make_task(jobs, calls.clone())
        );
        assert_eq!(cost_of(&a.unwrap()), 1.0);
        assert_eq!(cost_of(&b.unwrap()), 1.0);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "并发同键应单飞");
    }

    /// 不同工厂的求解必须能真正并行（这正是「同时规划多个工厂」的前提）。
    #[tokio::test]
    async fn different_keys_solve_in_parallel() {
        let (runtime, project, factory_a) = demo_runtime();
        let factory_b = {
            let mut runtime = runtime.lock().unwrap();
            runtime
                .dispatch(AppMessage::Project {
                    project,
                    action: ProjectAction::AddFactory {
                        name: "f2".to_string(),
                        template: FactoryTemplate::Empty,
                    },
                })
                .unwrap();
            runtime.state.project(project).unwrap().factories[1].id
        };
        let runtime = Arc::new(runtime);
        let jobs = Arc::new(SolveJobs::<SolveResult>::default());
        // 记录同时进行的求解数峰值：串行实现下峰值只会是 1。
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let make_task = |jobs: Arc<SolveJobs<SolveResult>>, factory: FactoryId| {
            let runtime = runtime.clone();
            let running = running.clone();
            let peak = peak.clone();
            tokio::spawn(async move {
                jobs.run(
                    (project, factory),
                    snapshot_fn(&runtime, project, factory),
                    move |_| {
                        let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(100));
                        running.fetch_sub(1, Ordering::SeqCst);
                        Ok(fake_result(project, factory, 1.0))
                    },
                )
                .await
                .unwrap()
            })
        };
        let (a, b) = tokio::join!(
            make_task(jobs.clone(), factory_a),
            make_task(jobs, factory_b)
        );
        assert_eq!(cost_of(&a.unwrap()), 1.0);
        assert_eq!(cost_of(&b.unwrap()), 1.0);
        assert!(
            peak.load(Ordering::SeqCst) >= 2,
            "不同工厂应并行求解，实测峰值 {}",
            peak.load(Ordering::SeqCst)
        );
    }

    /// 求解期间文档被改动 → 再算一轮，返回新鲜结果。
    #[tokio::test]
    async fn document_change_during_solve_triggers_another_round() {
        let (runtime, project, factory) = demo_runtime();
        let runtime = Arc::new(runtime);
        let jobs = SolveJobs::default();
        let calls = Arc::new(AtomicUsize::new(0));

        let counter = calls.clone();
        let editor = runtime.clone();
        let result = jobs
            .run(
                (project, factory),
                snapshot_fn(&runtime, project, factory),
                move |_| {
                    let n = counter.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        // 第一轮：模拟求解期间用户改了文档。
                        let mut runtime = editor.lock().unwrap();
                        runtime
                            .dispatch(AppMessage::Factory {
                                project,
                                factory,
                                action: FactoryAction::SetName {
                                    name: "edited".to_string(),
                                },
                            })
                            .unwrap();
                        Ok(fake_result(project, factory, 1.0))
                    } else {
                        Ok(fake_result(project, factory, 2.0))
                    }
                },
            )
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "文档变更应触发第二轮");
        assert_eq!(cost_of(&result), 2.0, "应返回最新一轮的结果");
    }

    /// 失败不缓存：下一次请求必须重新尝试。
    #[tokio::test]
    async fn failures_are_not_cached() {
        let (runtime, project, factory) = demo_runtime();
        let runtime = Arc::new(runtime);
        let jobs = SolveJobs::default();
        let calls = Arc::new(AtomicUsize::new(0));

        let counter = calls.clone();
        let first = jobs
            .run(
                (project, factory),
                snapshot_fn(&runtime, project, factory),
                move |_| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err("boom".to_string())
                },
            )
            .await;
        assert!(first.is_err());

        let counter = calls.clone();
        let second = jobs
            .run(
                (project, factory),
                snapshot_fn(&runtime, project, factory),
                move |_| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(fake_result(project, factory, 1.0))
                },
            )
            .await
            .unwrap();
        assert_eq!(cost_of(&second), 1.0);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
