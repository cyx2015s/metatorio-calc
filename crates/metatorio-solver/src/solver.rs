use good_lp::{IntoAffineExpression, variable};

use crate::concept::{AIndexMap, AIndexSet, Flow, ItemIdent};
use crate::lp::{SolveReport, solve_lp};
use core::f64;

use std::fmt::Debug;
use std::hash::Hash;
use std::sync::mpsc::*;
use std::time::Instant;

#[must_use]
pub fn flow_add<T>(a: &Flow<T>, b: &Flow<T>, c: f64) -> Flow<T>
where
    T: Eq + Hash + Clone,
{
    let mut result = a.clone();
    for (key, value) in b {
        let entry = result.entry(key.clone()).or_insert(0.0);
        *entry += value * c;
    }
    result
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TargetSpec<I: ItemIdent> {
    pub constant: f64,
    pub coefficients: Flow<I>,
}

#[derive(Debug, Clone)]
pub struct FlowSpec<I: ItemIdent> {
    pub coefficients: Flow<I>,
    pub cost: f64,
    pub fixed: Option<f64>, // 如果是Some(v)，表示这个原始变量必须是v
}

#[derive(Debug, Clone)]
pub struct SolverData<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub target: Vec<TargetSpec<I>>,
    pub flows: AIndexMap<R, FlowSpec<I>>,
    pub sources: Flow<I>, //  输入特定物品消耗的价值
    pub sinks: Flow<I>,   //  产生额外物品的惩罚
    // 我还不知道怎么称呼，目前规定如下：
    // 如果是严格模式，相比普通模式有如下限制：只能使用来自external的输入
    pub strict_source: bool,
    // 如果是严格模式，相比普通模式有如下限制：没有出现在target中的物品必须配平
    pub strict_sink: bool,
    /// 剪枝阶段（trim_flows）因缺少提供者而被移除配方所对应的缺失
    /// 物品/流。用于失败诊断：这些物品没有任何配方能产出、外部也不
    /// 供给，因此其消耗配方被整体剪掉——NoProvider 应如实报告它们，
    /// 而不是在剪枝后静默消失。
    pub pruned_missing: Vec<I>,
    /// **不受配平约束**的坐标：污染这类量是「指标」而不是「物质」——它既不参与
    /// 守恒，被吸收也不构成需求（没有污染时，吸收污染的建筑照样正常工作）。
    ///
    /// 由**调用方按类型语义**填（`DualVar::Pollution`），求解器不猜：`I` 是泛型，
    /// 内核不认识具体变体。这是「平衡规则来自语义、不来自系数符号」的第一步。
    pub unconstrained: AIndexSet<I>,
}

// TODO: warning: large size difference between variants
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SolverSolution<I, R> {
    Solved {
        prim: Flow<R>,
        dual: Option<Flow<I>>,
        prim_scale: Flow<R>,
        dual_scale: Flow<I>,
        global_scale: f64,
        sum: Flow<I>,
        cost: f64,
        /// 本次求解的剪枝/后端记录（见 [`SolveReport`]）。
        report: SolveReport,
    },
    NotSolved {
        no_provider: Vec<I>,
        no_consumer: Vec<I>,
        description: String,
    },
}

impl<I, R> Default for SolverSolution<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    fn default() -> Self {
        SolverSolution::NotSolved {
            no_provider: vec![],
            no_consumer: vec![],
            description: "未求解".to_string(),
        }
    }
}

impl<I, R> SolverSolution<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub fn get_prim_raw_of(&self, i: &R) -> Option<f64> {
        match self {
            SolverSolution::Solved {
                prim,
                prim_scale,
                global_scale,
                ..
            } => match (prim.get(i), prim_scale.get(i)) {
                (Some(v), Some(s)) => Some(*v / s * global_scale),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn get_prim_of(&self, i: &R) -> Option<f64> {
        match self {
            SolverSolution::Solved { prim, .. } => prim.get(i).cloned(),
            _ => None,
        }
    }

    pub fn get_dual_raw_of_of(&self, i: &I) -> Option<f64> {
        match self {
            SolverSolution::Solved {
                dual: Some(dual),
                dual_scale,
                ..
            } => dual
                .get(i)
                .cloned()
                .map(|v| v * dual_scale.get(i).cloned().unwrap_or(1.0)),

            _ => None,
        }
    }

    pub fn get_dual_of(&self, i: &I) -> Option<f64> {
        match self {
            SolverSolution::Solved {
                dual: Some(dual), ..
            } => dual.get(i).cloned(),
            _ => None,
        }
    }

    pub fn get_cost(&self) -> Option<f64> {
        match self {
            SolverSolution::Solved { cost, .. } => Some(*cost),
            _ => None,
        }
    }

    pub fn get_sum(&self) -> Option<&Flow<I>> {
        match self {
            SolverSolution::Solved { sum, .. } => Some(sum),
            _ => None,
        }
    }

    pub fn get_sum_of(&self, i: &I) -> Option<f64> {
        match self {
            SolverSolution::Solved { sum, .. } => sum.get(i).cloned(),
            _ => None,
        }
    }

    pub fn get_sum_raw_of(&self, i: &I) -> Option<f64> {
        match self {
            SolverSolution::Solved {
                sum,
                dual_scale,
                global_scale,
                ..
            } => sum
                .get(i)
                .map(|v| *v * dual_scale.get(i).cloned().unwrap_or(1.0) * global_scale),
            _ => None,
        }
    }
}

impl<I, R> SolverData<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub fn new_simple(target: Flow<I>, flows: AIndexMap<R, (Flow<I>, f64)>) -> Self {
        Self {
            target: target
                .into_iter()
                .map(|(item_id, constant)| TargetSpec {
                    constant,
                    coefficients: [(item_id, 1.0)].into_iter().collect(),
                })
                .collect(),
            flows: flows
                .into_iter()
                .map(|(flow_id, (coefficients, cost))| {
                    (
                        flow_id,
                        FlowSpec {
                            coefficients,
                            cost,
                            fixed: None,
                        },
                    )
                })
                .collect(),
            sources: AIndexMap::default(),
            sinks: AIndexMap::default(),
            strict_source: false,
            strict_sink: false,
            pruned_missing: Vec::new(),
            unconstrained: AIndexSet::default(),
        }
    }

    pub fn with_sources(mut self, sources: Flow<I>) -> Self {
        self.sources.extend(sources);
        self
    }

    pub fn with_sinks(mut self, sinks: Flow<I>) -> Self {
        self.sinks.extend(sinks);
        self
    }

    pub fn with_strict_source(mut self, strict: bool) -> Self {
        self.strict_source = strict;
        self
    }

    pub fn with_strict_sink(mut self, strict: bool) -> Self {
        self.strict_sink = strict;
        self
    }

    /// 剪枝一轮。`record_missing` 为 true 时把本轮因缺少提供者而剪掉的
    /// 物品记录进 `pruned_missing`（调用方应只在第一轮传 true：后续轮次
    /// 缺少的物品往往是被第一轮剪枝连坐的间接原因，不是根因）。
    pub fn trim_flows(&mut self, record_missing: bool) -> bool {
        if self.strict_source {
            // 在strict_source模式下，移除所有无法使用的配方
            let instant = std::time::Instant::now();
            let mut status = AIndexMap::default();
            enum ItemStatus<R> {
                Pending {
                    providers: AIndexSet<R>,
                    consumers: AIndexSet<R>,
                },
                Usable,
            }

            for (
                f_id,
                FlowSpec {
                    coefficients,
                    cost: _,
                    fixed: _,
                },
            ) in &self.flows
            {
                for (item_id, &amount) in coefficients {
                    let entry =
                        status
                            .entry(item_id.clone())
                            .or_insert_with(|| ItemStatus::Pending {
                                providers: AIndexSet::default(),
                                consumers: AIndexSet::default(),
                            });
                    if amount > 0.0 {
                        // 生产这个物品的配方
                        match entry {
                            ItemStatus::Pending { providers, .. } => {
                                providers.insert(f_id.clone());
                            }
                            ItemStatus::Usable => {}
                        }
                    }
                    if amount < 0.0 {
                        // 消耗这个物品的配方
                        match entry {
                            ItemStatus::Pending { consumers, .. } => {
                                consumers.insert(f_id.clone());
                            }
                            ItemStatus::Usable => {}
                        }
                    }
                    match entry {
                        ItemStatus::Pending {
                            providers,
                            consumers,
                        } if !providers.is_empty() && !consumers.is_empty() => {
                            *entry = ItemStatus::Usable;
                        }
                        _ => {}
                    }
                }
            }

            let needed_by_target = self.target.iter().fold(
                AIndexSet::default(),
                |mut acc,
                 TargetSpec {
                     constant,
                     coefficients,
                 }| {
                    for (i_id, coef) in coefficients {
                        if *coef * constant > 0.0 {
                            // 系数与常数同号，说明目标需要这个物品
                            acc.insert(i_id.clone());
                        }
                    }
                    acc
                },
            );

            // **只报告，不删除。**
            //
            // 删除以前在这里做，而且会**级联**：某物品没有生产者 → 删掉它的消费流 →
            // 那条流产出的东西又没了生产者 → 再删……实测把「1W 电力」这种目标的整条链
            // 剪空（燃料棒没生产者 → 剪反应堆 → 热量没生产者 → 剪热交换机 → 蒸汽没生产者
            // → 剪汽轮机 → 电力没生产者），最后报出误导性的
            // 「无供给：Electricity, uranium-fuel-cell」。
            //
            // 而且删除本来就是**不必要**的：严格供给下，一个「有消费者、没有生产者、
            // 也不在 sources」的物品，其配平约束是 `-coef·var >= 0`（strict_sink 时
            // `== 0`），变量非负，这个约束自己就把那些流的变量压成 0；HiGHS 返回的是
            // 精确顶点解，不需要我们提前剪掉。省下的那点 LP 规模不值得这个风险。
            //
            // 保留**记录**（`pruned_missing`）：它是「哪些物品没有任何生产者」的唯一
            // 来源，求解失败的诊断仍然需要它。
            for (i_id, entry) in &status {
                if let ItemStatus::Pending {
                    providers,
                    consumers,
                } = entry
                    && providers.is_empty() // 没有生产这个物品的配方
                        && !self.sources.contains_key(i_id) // 外部也不能提供
                        && !needed_by_target.contains(i_id) // 目标也不需要
                        && record_missing
                        && !consumers.is_empty()
                        && !self.pruned_missing.contains(i_id)
                {
                    self.pruned_missing.push(i_id.clone());
                }
            }
            log::debug!(
                "求解器：统计无生产者物品耗时 {} ms",
                instant.elapsed().as_millis()
            );
        }
        // 不再有任何「剪枝」变更：恒返回 false，调用方的 while 循环只跑一轮。
        false
    }

    pub fn solve(mut self) -> SolverSolution<I, R> {
        if self.flows.is_empty() {
            return SolverSolution::NotSolved {
                no_provider: vec![],
                no_consumer: vec![],
                description: "没有可用的配方".to_string(),
            };
        }
        if self.target.is_empty() {
            return SolverSolution::NotSolved {
                no_provider: vec![],
                no_consumer: vec![],
                description: "没有目标物品。".to_string(),
            };
        }

        log::info!("求解器：开始剪枝");
        let mut count = 0;
        let instant = Instant::now();
        let len_before = self.flows.len();
        // 只在第一轮记录缺失物品（直接原因）；后续轮次缺失是连锁反应。
        let mut first_round = true;
        while self.trim_flows(first_round) {
            count += 1;
            first_round = false;
        }
        let len_after = self.flows.len();
        log::info!(
            "求解器：剪枝完成，共执行了 {} 次剪枝操作，移除了 {} 个配方 ({} -> {})，耗时 {:.2?}",
            count,
            len_before - len_after,
            len_before,
            len_after,
            instant.elapsed()
        );

        let mut problem_variables = good_lp::ProblemVariables::new();
        let mut item_in_targets = AIndexSet::default();
        for target in &self.target {
            for (i_id, _coef) in target.coefficients.iter() {
                item_in_targets.insert(i_id.clone());
            }
        }
        // 用户提供的流编号 -> 变量的映射
        let mut flow_vars = AIndexMap::default();
        // 物品源变量
        let mut source_vars = AIndexMap::default();
        // 物品汇变量
        let mut sink_vars = AIndexMap::default();
        for f_id in self.flows.keys() {
            let var = problem_variables.add(variable().min(0));
            flow_vars.insert(f_id.clone(), var);
        }

        let mut item_balances = AIndexMap::default();

        log::info!(
            "求解器：开始构建物品平衡表达式：一共有 {} 个配方变量",
            self.flows.len()
        );

        // 辅助转换流的产物**不再强制配平**（原来这里把「零成本流的正系数产物」收集成
        // `force_zero_items`，再对它们加 `expr == 0`）。那条约束是错的：
        //
        // 1. 辅助流全是 **1:1 重贴标签**（温度区间子类型、定点降温、燃料类别桥接、
        //    filter 归并），变量非负、消耗 1 才能产出 1，凭空造物质不可能——它想防的
        //    事不存在。
        // 2. 它让任何**用宽键命名的目标**必然无解：目标的 broad 键正是由子类型边
        //    （零成本、正系数）产出的，于是 `balance(目标键) == 0` 与
        //    `target_expr == amount` 直接冲突。实测 `steam@[15,500]` 目标恒不可行，
        //    而 `steam@[500,500]` 因为不存在 narrow≠broad 的自类型边反而可行。
        // 3. 判据本身也坏：用 `cost == 0.0` 代理「这是辅助流」，但 `cost` 同时还在表达
        //    「这个机制便宜」——`FluidFuel`/`FluidHeat` 的实例成本本来就是 0，于是两个
        //    **真实机制**的产物也被强制配平了。辅助变量的正确标识是
        //    `MechanicId(u64::MAX)`（`used_candidates` 已经在用）。
        for (f_id, flow_spec) in &self.flows {
            let var = flow_vars.get(f_id).unwrap();
            for (item_id, &amount) in &flow_spec.coefficients {
                *item_balances
                    .entry(item_id.clone())
                    .or_insert(good_lp::Expression::from(0.0)) += amount * *var;
            }
        }
        log::info!("求解器：一共有 {} 个物品需要平衡", item_balances.len(),);

        for (item_id, _) in &self.sources {
            let var = problem_variables.add(variable().min(0));
            source_vars.insert(item_id.clone(), var);
            let entry = item_balances
                .entry(item_id.clone())
                .or_insert(good_lp::Expression::from(0.0));
            *entry += 1.0 * var;
        }
        for (item_id, _) in &self.sinks {
            let var = problem_variables.add(variable().min(0));
            sink_vars.insert(item_id.clone(), var);
            let entry = item_balances
                .entry(item_id.clone())
                .or_insert(good_lp::Expression::from(0.0));
            *entry -= 1.0 * var;
        }
        let mut no_providers: AIndexSet<I> = item_balances.keys().cloned().collect();
        let mut no_consumers: AIndexSet<I> = item_balances.keys().cloned().collect();
        for (_flow, flow_spec) in &self.flows {
            for (item_id, &amount) in &flow_spec.coefficients {
                if amount > 0.0 {
                    no_providers.swap_remove(item_id);
                }
                if amount < 0.0 {
                    no_consumers.swap_remove(item_id);
                }
            }
        }
        for item in self.sources.keys() {
            no_providers.swap_remove(item);
        }
        for item in self.sinks.keys() {
            no_consumers.swap_remove(item);
        }
        let mut constraints = Vec::new();
        let mut item_to_constraint = AIndexMap::default();
        let mut add_constraint = |item_id: &I, constraint: good_lp::Constraint| {
            constraints.push(constraint);
            item_to_constraint.insert(item_id.clone(), constraints.len() - 1);
        };
        for (item_id, expr) in &item_balances {
            // 污染这类「指标」完全不受约束：没有污染时吸收污染的建筑照样正常工作，
            // 有污染时也不要求"吸收量 = 排放量"（那会把吸收变成了需求）。
            if self.unconstrained.contains(item_id) {
                continue;
            }
            // **目标物品由目标约束独占管辖**（下面 `target_expr == constant`），
            // 不在这里配平。否则严格产出会对同一个表达式同时要求 `== 0` 和
            // `== constant`，任何非零目标都必然不可行——严格产出模式因此从来没有
            // 真正跑通过（这是它一直坏着的根因，也是 `force_zero_items` 当初被加进来的
            // 原因：它在局部模拟「不允许剩余」，因为全局的严格产出是坏的）。
            if !item_in_targets.contains(item_id) {
                // 严格模式下，不能凭空输入。非严格模式下，有来源的物品不能有凭空输入。
                // 非目标物品，不能为负
                if self.strict_source {
                    // 不能从外部借用
                    if self.strict_sink {
                        // 必须配平
                        add_constraint(item_id, expr.clone().eq(0.0));
                    } else {
                        // 不用配平
                        add_constraint(item_id, expr.clone().geq(0.0));
                    }
                } else if no_providers.contains(item_id) {
                    // 需要借用，不用限制
                } else if self.strict_sink {
                    // 必须配平
                    add_constraint(item_id, expr.clone().eq(0.0));
                } else {
                    // 不用配平
                    add_constraint(item_id, expr.clone().geq(0.0));
                }
            }
        }
        for source_var in source_vars.values() {
            constraints.push(source_var.into_expression().geq(0.0));
        }
        // 添加求解目标的限制

        let mut target_exprs = vec![good_lp::Expression::from(0.0); self.target.len()];
        for item in &item_in_targets {
            for (target_idx, target) in self.target.iter().enumerate() {
                if let Some(&coef) = target.coefficients.get(item) {
                    if coef == 0.0 {
                        continue;
                    }
                    // 分离数量级平衡和问题构造后就成平凡的了
                    target_exprs[target_idx] += coef
                        * item_balances
                            .get(item)
                            .cloned()
                            .unwrap_or(good_lp::Expression::from(0.0));
                }
            }
        }
        for (t_idx, target) in self.target.iter().enumerate() {
            let target_expr = &target_exprs[t_idx];
            let constant = target.constant;
            constraints.push(target_expr.clone().eq(constant));
        }
        let mut optimization_expr = good_lp::Expression::from(0.0);
        for (flow_id, flow_spec) in &self.flows {
            let var = flow_vars.get(flow_id).unwrap();
            optimization_expr += flow_spec.cost * *var;
        }
        for (item_id, cost) in &self.sources {
            let var = source_vars.get(item_id).unwrap();
            optimization_expr += *cost * *var;
        }
        for (item_id, cost) in &self.sinks {
            let var = sink_vars.get(item_id).unwrap();
            optimization_expr += *cost * *var;
        }
        if !no_providers.is_empty() {
            log::warn!("没有来源的物品：{:?}个", no_providers.len());
        }
        if !no_consumers.is_empty() {
            log::warn!("没有去处的物品：{:?}个", no_consumers.len());
        }
        if constraints.len() < 8 {
            log::debug!("求解器：构建的约束表达式: {:?}", constraints);

            log::debug!("求解器：对应流变量: {:?}", flow_vars);
        }
        // 求解策略见 solve_lp：HiGHS 单后端，直接给顶点解。
        let solution = solve_lp(optimization_expr.clone(), constraints, problem_variables);

        match solution {
            Ok(sol) => {
                log::info!("求解器：求解成功，开始构建结果");
                let global_scale = sol.global_scale;
                let mut sum = Flow::default();
                let mut prim = Flow::default();
                let mut prim_scale = Flow::default();

                for (f_id, var) in &flow_vars {
                    let cur_prim_scale = sol.prim_scale(*var);
                    // value 已是原问题空间的取值（后端与缩放都对调用方透明）。
                    let value = sol.value(*var);

                    prim.insert(f_id.clone(), value);

                    prim_scale.insert(f_id.clone(), cur_prim_scale);
                    for (item_id, &amount) in &self.flows[f_id].coefficients {
                        let entry = sum.entry(item_id.clone()).or_insert(0.0);
                        *entry += amount * value;
                    }
                }
                SolverSolution::Solved {
                    prim,
                    prim_scale,
                    dual: None,
                    dual_scale: item_to_constraint
                        .iter()
                        .map(|(item_id, &c_idx)| {
                            let dual_scale = sol.dual_scales[c_idx];
                            (item_id.clone(), dual_scale)
                        })
                        .collect(),
                    sum,
                    cost: sol.cost,
                    global_scale,
                    report: sol.report,
                }
            }
            Err(err) => {
                log::error!("求解器：求解失败，错误信息: {:?}", err);
                let err_string = match err {
                    good_lp::ResolutionError::Unbounded => "求解无界（目标可无限增大）".to_string(),
                    good_lp::ResolutionError::Infeasible => "无可行解（目标不可达）".to_string(),
                    // 内部数值失败**不等于**无可行解：把二者区分开，界面/agent
                    // 才不会把求解器算不动说成「配方不可解」。
                    good_lp::ResolutionError::Other(_) => {
                        "求解器内部错误（不代表问题无可行解，可重试）".to_string()
                    }
                    good_lp::ResolutionError::Str(s) => {
                        format!("求解器内部错误（不代表问题无可行解）：{s}")
                    }
                };
                // 剪枝阶段缺的物品并入 NoProvider：这些流没有任何配方
                // 能产出且外部也不供给，是求解失败的根因之一。
                let mut no_providers = no_providers;
                for missing in &self.pruned_missing {
                    no_providers.insert(missing.clone());
                }
                let mut description = err_string;
                if !self.pruned_missing.is_empty() {
                    description.push_str(&format!(
                        "；剪枝阶段缺少供给的物品/流 {} 个：{}",
                        self.pruned_missing.len(),
                        self.pruned_missing
                            .iter()
                            .map(|item| format!("{item:?}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                SolverSolution::NotSolved {
                    no_provider: no_providers.iter().cloned().collect(),
                    no_consumer: no_consumers.iter().cloned().collect(),
                    description,
                }
            }
        }
    }

    pub fn make_dedicated_solver_thread(
        solution_tx: Sender<SolverSolution<I, R>>,
        problem_rx: Receiver<SolverData<I, R>>,
    ) {
        std::thread::spawn(move || {
            log::info!("求解线程启动");
            loop {
                let mut last_req = match problem_rx.recv() {
                    Ok(req) => req,
                    Err(_) => break,
                };
                // 尽可能多地丢弃后续请求，只保留最新
                while let Ok(req) = problem_rx.try_recv() {
                    // 虽然不太可能，因为每次算都很快。
                    log::info!("丢弃了一个过时的求解请求");

                    last_req = req;
                }
                if solution_tx.send(last_req.solve()).is_err() {
                    // 接收方已关闭，退出线程
                    break;
                }
            }
            log::info!("求解线程退出");
        });
    }

    pub fn make_solver_thread(
        solution_tx: Sender<(usize, SolverSolution<I, R>)>,
        problem_rx: Receiver<(usize, SolverData<I, R>)>,
    ) {
        std::thread::spawn(move || {
            log::info!("求解线程启动");
            loop {
                let mut reqs = AIndexMap::default();
                std::thread::sleep(std::time::Duration::from_millis(50));
                while let Ok((req_id, req)) = problem_rx.try_recv() {
                    reqs.insert(req_id, req);
                }
                for (req_id, req) in reqs.into_iter() {
                    let result = req.solve();

                    if solution_tx.send((req_id, result)).is_err() {
                        // 接收方已关闭，退出线程
                        log::info!("求解线程退出");
                        break;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最简单的冶炼：1 铁矿 → 1 铁板，成本 1；目标产出 1 铁板
    fn smelting_problem() -> SolverData<&'static str, &'static str> {
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);

        let mut flows = AIndexMap::default();
        let mut smelt = AIndexMap::default();
        smelt.insert("iron-ore", -1.0);
        smelt.insert("iron-plate", 1.0);
        flows.insert("smelt", (smelt, 1.0));

        SolverData::new_simple(target, flows)
    }

    #[test]
    fn solve_simple_recipe() {
        let solution = smelting_problem().solve();
        match solution {
            SolverSolution::Solved {
                prim, sum, cost, ..
            } => {
                assert!((prim["smelt"] - 1.0).abs() < 1e-6, "prim: {prim:?}");
                assert!((sum["iron-plate"] - 1.0).abs() < 1e-6, "sum: {sum:?}");
                assert!((sum["iron-ore"] + 1.0).abs() < 1e-6, "sum: {sum:?}");
                assert!((cost - 1.0).abs() < 1e-6, "cost: {cost}");
            }
            SolverSolution::NotSolved { description, .. } => {
                panic!("求解失败: {description}")
            }
        }
    }

    #[test]
    fn flow_add_combines_with_scale() {
        let mut a = AIndexMap::default();
        a.insert("x", 1.0);
        a.insert("y", 2.0);
        let mut b = AIndexMap::default();
        b.insert("y", 3.0);
        b.insert("z", 1.0);
        let r = flow_add(&a, &b, 2.0);
        assert_eq!(r.len(), 3);
        assert!((r["x"] - 1.0).abs() < 1e-9);
        assert!((r["y"] - 8.0).abs() < 1e-9);
        assert!((r["z"] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn flow_add_empty_base_and_negative_scale() {
        let a = AIndexMap::default();
        let mut b = AIndexMap::default();
        b.insert("x", 4.0);
        let r = flow_add(&a, &b, -0.5);
        assert!((r["x"] + 2.0).abs() < 1e-9);
    }

    #[test]
    fn solve_chooses_cheapest_recipe() {
        // 两条配方产出同一物品，成本不同：求解器应选择成本低的
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut cheap = AIndexMap::default();
        cheap.insert("iron-ore", -1.0);
        cheap.insert("iron-plate", 1.0);
        let mut expensive = AIndexMap::default();
        expensive.insert("iron-ore", -2.0);
        expensive.insert("iron-plate", 1.0);
        flows.insert("cheap", (cheap, 1.0));
        flows.insert("expensive", (expensive, 5.0));

        let solution = SolverData::new_simple(target, flows).solve();
        match solution {
            SolverSolution::Solved { prim, cost, .. } => {
                assert!(
                    (prim["cheap"] - 1.0).abs() < 1e-5,
                    "应选择低成本配方: {prim:?}"
                );
                assert!(
                    (prim["expensive"]).abs() < 1e-5,
                    "不应选择高成本配方: {prim:?}"
                );
                assert!((cost - 1.0).abs() < 1e-5, "总成本应为 1: {cost}");
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn solve_with_sources_provides_missing_input() {
        // ore 无配方生产，由外部 source 提供；source 变量计入成本
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut smelt = AIndexMap::default();
        smelt.insert("iron-ore", -1.0);
        smelt.insert("iron-plate", 1.0);
        flows.insert("smelt", (smelt, 1.0));

        let mut sources = AIndexMap::default();
        sources.insert("iron-ore", 2.0); // 每单位 ore 输入价值 2

        let solution = SolverData::new_simple(target, flows)
            .with_sources(sources)
            .solve();
        match solution {
            SolverSolution::Solved { sum, cost, .. } => {
                assert!((sum["iron-ore"] + 1.0).abs() < 1e-5, "消耗 1 ore: {sum:?}");
                assert!((cost - 3.0).abs() < 1e-5, "成本 = 1 ore * 3: {cost}");
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn solve_negative_target_means_consume() {
        // target 为负 = 必须净消耗该物品（等式约束）
        let mut target = AIndexMap::default();
        target.insert("iron-plate", -1.0);
        let mut flows = AIndexMap::default();
        let mut burn = AIndexMap::default();
        burn.insert("iron-plate", -1.0);
        flows.insert("burn", (burn, 2.0));

        let solution = SolverData::new_simple(target, flows).solve();
        match solution {
            SolverSolution::Solved { sum, cost, .. } => {
                assert!((sum["iron-plate"] + 1.0).abs() < 1e-5, "净消耗 1: {sum:?}");
                assert!((cost - 2.0).abs() < 1e-5, "成本 2: {cost}");
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn solve_unreachable_target_reports_not_solved() {
        // 目标物品没有任何配方可以生产（也无 sources），LP 不可行
        let mut target = AIndexMap::default();
        target.insert("mystery", 1.0);
        let mut flows = AIndexMap::default();
        let mut smelt = AIndexMap::default();
        smelt.insert("iron-ore", -1.0);
        smelt.insert("iron-plate", 1.0);
        flows.insert("smelt", (smelt, 1.0));
        let solution = SolverData::new_simple(target, flows).solve();
        assert!(matches!(solution, SolverSolution::NotSolved { .. }));
    }

    #[test]
    fn solve_empty_flows_reports_no_recipe() {
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);
        let flows: AIndexMap<&'static str, (AIndexMap<&'static str, f64>, f64)> =
            AIndexMap::default();
        let solution = SolverData::<&'static str, &'static str>::new_simple(target, flows).solve();
        assert!(matches!(solution, SolverSolution::NotSolved { .. }));
    }

    #[test]
    fn pruning_reports_missing_item_in_description_and_no_provider() {
        // 严格供给：配方消耗 "uranium-ore"，但没有任何配方产出它、外部
        // 也不供给 → 剪枝应移除该配方，并在失败描述里报告缺少的物品。
        let mut target = AIndexMap::default();
        target.insert("uranium-fuel-cell", 1.0);
        let mut flows = AIndexMap::default();
        let mut assemble = AIndexMap::default();
        assemble.insert("uranium-ore", -1.0);
        assemble.insert("uranium-fuel-cell", 1.0);
        flows.insert("assemble", (assemble, 1.0));
        let solution = SolverData::new_simple(target, flows)
            .with_strict_source(true)
            .solve();
        let SolverSolution::NotSolved {
            no_provider,
            description,
            ..
        } = solution
        else {
            panic!("严格供给下缺少原料应不可行");
        };
        assert!(
            no_provider.contains(&"uranium-ore"),
            "NoProvider 应包含剪枝阶段缺的物品: {no_provider:?}"
        );
        assert!(
            description.contains("uranium-ore"),
            "描述应报告剪枝阶段缺少的物品: {description}"
        );
    }

    #[test]
    fn solve_empty_target_reports_no_target() {
        let target: AIndexMap<&'static str, f64> = AIndexMap::default();
        let flows: AIndexMap<&'static str, (AIndexMap<&'static str, f64>, f64)> =
            AIndexMap::default();
        let solution = SolverData::<&'static str, &'static str>::new_simple(target, flows).solve();
        assert!(matches!(solution, SolverSolution::NotSolved { .. }));
    }

    #[test]
    fn zero_cost_conversion_products_are_not_force_balanced() {
        // 0 成本转换流的产物**不强制配平**。
        //
        // 原来这里断言"0 成本转换流产出目标物品应不可行"——那正是 `force_zero_items`
        // 的行为，而它是错的：目标的**宽键**正是由零成本子类型边产出的，强制配平会让
        // `balance(目标键) == 0` 与 `target_expr == amount` 直接冲突，任何用区间
        // 命名的流体目标都恒不可行（实测 `steam@[15,500]`）。
        //
        // 「不能凭空拿到目标」这个性质由 **strict_source** 保证，不是靠强制配平：
        // 非严格模式允许借用无来源的 `raw`，严格模式不允许。两条一起钉住。
        let build = || {
            let mut target = AIndexMap::default();
            target.insert("plate", 1.0);
            let mut flows = AIndexMap::default();
            let mut conv = AIndexMap::default();
            conv.insert("ore", -1.0);
            conv.insert("plate", 1.0);
            flows.insert("conv", (conv, 0.0)); // 0 成本转换
            let mut prod = AIndexMap::default();
            prod.insert("raw", -1.0);
            prod.insert("ore", 1.0);
            flows.insert("prod", (prod, 1.0));
            (target, flows)
        };

        let (target, flows) = build();
        assert!(
            matches!(
                SolverData::new_simple(target, flows).solve(),
                SolverSolution::Solved { .. }
            ),
            "非严格模式允许借用 raw，应当可行"
        );

        let (target, flows) = build();
        assert!(
            matches!(
                SolverData::new_simple(target, flows)
                    .with_strict_source(true)
                    .solve(),
                SolverSolution::NotSolved { .. }
            ),
            "严格模式不允许凭空拿到 raw，应当不可行——这才是「不能凭空得到目标」的守卫"
        );
    }

    #[test]
    fn strict_sink_does_not_force_the_target_itself_to_balance() {
        // 严格产出下**目标物品不参与逐项配平**——它由目标约束独占管辖。
        //
        // 否则同一个表达式会被同时要求 `== 0`（逐项配平）和 `== amount`（目标约束），
        // 任何非零目标都必然不可行。严格产出模式因此从来没有真正跑通过——这也是
        // `force_zero_items` 当初被加进来的原因：它在局部模拟「不允许剩余」，因为
        // 全局的严格产出是坏的。
        let build = || {
            let mut target = AIndexMap::default();
            target.insert("plate", 1.0);
            let mut flows = AIndexMap::default();
            let mut smelt = AIndexMap::default();
            smelt.insert("ore", -1.0);
            smelt.insert("plate", 1.0);
            flows.insert("smelt", (smelt, 1.0));
            (target, flows)
        };
        let mut sources = AIndexMap::default();
        sources.insert("ore", 1.0); // 外部供矿，严格供给下也要有来源

        let (target, flows) = build();
        assert!(
            matches!(
                SolverData::new_simple(target, flows)
                    .with_sources(sources)
                    .with_strict_source(true)
                    .with_strict_sink(true)
                    .solve(),
                SolverSolution::Solved { .. }
            ),
            "严格产出下目标物品由目标约束独占管辖，不能同时被要求配平为 0"
        );
    }

    #[test]
    fn unconstrained_items_are_never_balanced() {
        // 「指标」类坐标（污染）完全不受配平约束：一个**消耗污染**、产出目标的建筑，
        // 在没有污染来源时也必须能正常工作——吸收污染不等于「需要污染」。
        //
        // 不加 `unconstrained` 的话，严格模式下 `pollution` 会被要求 `== 0`，
        // 于是吸收建筑根本跑不起来、目标不可达。
        let build = || {
            let mut target = AIndexMap::default();
            target.insert("plate", 1.0);
            let mut flows = AIndexMap::default();
            let mut scrub = AIndexMap::default();
            scrub.insert("pollution", -1.0); // 吸收污染
            scrub.insert("plate", 1.0);
            flows.insert("scrub", (scrub, 1.0));
            (target, flows)
        };

        let (target, flows) = build();
        let mut problem = SolverData::new_simple(target, flows);
        problem.unconstrained.insert("pollution");
        assert!(matches!(
            problem
                .with_strict_sink(true)
                .with_strict_source(true)
                .solve(),
            SolverSolution::Solved { .. }
        ));
    }

    #[test]
    fn solve_two_stage_recipe_chain() {
        // 两级配方：raw → ore → plate，中间物应配平
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut mine = AIndexMap::default();
        mine.insert("raw-ore", -1.0);
        mine.insert("iron-ore", 1.0);
        flows.insert("mine", (mine, 1.0));
        let mut smelt = AIndexMap::default();
        smelt.insert("iron-ore", -1.0);
        smelt.insert("iron-plate", 1.0);
        flows.insert("smelt", (smelt, 2.0));

        let solution = SolverData::new_simple(target, flows).solve();
        match solution {
            SolverSolution::Solved { prim, sum, .. } => {
                assert!((prim["mine"] - 1.0).abs() < 1e-5, "prim: {prim:?}");
                assert!((prim["smelt"] - 1.0).abs() < 1e-5, "prim: {prim:?}");
                assert!((sum["raw-ore"] + 1.0).abs() < 1e-5, "sum: {sum:?}");
                assert!((sum["iron-ore"]).abs() < 1e-5, "中间物应配平: {sum:?}");
                assert!((sum["iron-plate"] - 1.0).abs() < 1e-5, "sum: {sum:?}");
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn solve_accessors_consistent_with_fields() {
        let solution = smelting_problem().solve();
        match &solution {
            SolverSolution::Solved {
                prim, sum, cost, ..
            } => {
                assert_eq!(solution.get_prim_of(&"smelt"), prim.get(&"smelt").copied());
                assert_eq!(
                    solution.get_sum_of(&"iron-plate"),
                    sum.get(&"iron-plate").copied()
                );
                assert_eq!(solution.get_cost(), Some(*cost));
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn trim_flows_reports_but_never_removes_unusable_recipes() {
        // strict_source：孤立配方（消耗无法获得的物品且目标不需要）**只被记录，不被删除**。
        //
        // 删除曾经在这里做，而且会级联：删掉消费流 → 那条流产出的东西也没了生产者 →
        // 再删……实测把「1W 电力」的整条链剪空。而配平约束自己就把这类流的变量压成 0
        // （`-coef·var >= 0`、变量非负），不需要提前删。
        let mut target = AIndexMap::default();
        target.insert("plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut usable = AIndexMap::default();
        usable.insert("ore", -1.0);
        usable.insert("plate", 1.0);
        flows.insert("smelt", (usable, 1.0));
        let mut miner = AIndexMap::default();
        miner.insert("rock", -1.0);
        miner.insert("ore", 1.0);
        flows.insert("miner", (miner, 1.0));
        let mut orphan = AIndexMap::default();
        orphan.insert("uranium", -1.0); // 无法获得
        orphan.insert("waste", 1.0); // 目标不需要
        flows.insert("react", (orphan, 1.0));

        let mut sources = AIndexMap::default();
        sources.insert("rock", 1.0); // 叶子输入由外部提供

        let mut data = SolverData::new_simple(target, flows)
            .with_sources(sources)
            .with_strict_source(true);
        assert!(
            !data.trim_flows(true),
            "不再有「剪枝」这个变更，返回值恒为 false"
        );
        assert!(
            data.flows.contains_key("react"),
            "不可用的配方也必须留着（删除会级联）"
        );
        assert!(data.flows.contains_key("smelt"));
        assert!(data.flows.contains_key("miner"));
        assert!(
            data.pruned_missing.contains(&"uranium"),
            "剪枝应记录缺失物品: {:?}",
            data.pruned_missing
        );
    }

    #[test]
    fn trim_flows_noop_without_strict_source() {
        // 非严格模式下 trim_flows 不剪枝
        let mut target = AIndexMap::default();
        target.insert("plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut orphan = AIndexMap::default();
        orphan.insert("uranium", -1.0);
        orphan.insert("waste", 1.0);
        flows.insert("react", (orphan, 1.0));
        let mut data = SolverData::new_simple(target, flows);
        assert!(!data.trim_flows(false));
        assert!(data.flows.contains_key("react"));
    }
}
