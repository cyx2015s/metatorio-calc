use std::time::Instant;

use crate::concept::AIndexMap;

use good_lp::{
    Constraint, Expression, IntoAffineExpression, ProblemVariables, ResolutionError, Solution,
    SolverModel, Variable, VariableDefinition, microlp, solvers::microlp::MicroLpSolution,
};
use rayon::prelude::*;

pub struct RuizSolver {
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
}

pub struct RuizSolution {
    pub inner: MicroLpSolution,                // 原始结果
    pub prim_scales: AIndexMap<Variable, f64>, // 原始变量的系数分别乘了这些系数
    pub dual_scales: Vec<f64>,                 // 原始约束的系数分别乘了这些系数
    pub cost: f64,                             // 原问题的目标值，应该没有获取原始值的需求吧……
    // 实际求解的问题是原问题的 global_scale 倍，
    // 因此原始变量需要除以 global_scale 才是原问题的结果
    pub global_scale: f64,
}

pub const MAGIC: f64 = 114.0;
pub const MAGIC_INV: f64 = 1.0 / MAGIC;
impl RuizSolver {
    pub fn new(
        minimise: Expression,
        constraints: Vec<Constraint>,
        variables: ProblemVariables,
    ) -> Self {
        Self {
            minimise,
            constraints,
            variables,
        }
    }

    pub fn solve(self) -> Result<RuizSolution, ResolutionError> {
        // 每个变量，在每个约束中的系数
        let instant = Instant::now();
        let prim_coeffs = self
            .constraints
            .par_iter()
            .enumerate()
            .fold(AIndexMap::default, |mut acc, (idx, constraint)| {
                constraint
                    .expression()
                    .linear_coefficients()
                    .for_each(|(var, coeff)| {
                        acc.entry(var)
                            .or_insert_with(AIndexMap::default)
                            .insert(idx, coeff);
                    });
                acc
            })
            .reduce(AIndexMap::default, |mut map1, map2| {
                // 合并两个局部的反查表
                for (key, inner_map) in map2 {
                    map1.entry(key).or_default().extend(inner_map);
                }
                map1
            });

        let mut prim_scales = self
            .variables
            .iter_variables_with_def()
            .map(|(var, _)| (var, 1.0))
            .collect::<AIndexMap<Variable, f64>>();
        let mut dual_scales = vec![1.0; self.constraints.len()];

        // 随手计算一个停止阈值，理论上应该是 1.0，但考虑到数值误差，放宽一点
        let stop_threshold = 1.0 + 1.0 / (self.constraints.len() as f64 * 16.0 + 1.0);
        // 每次修改变量的系数和约束的系数，使得系数的均方根接近1
        for i in 0..1024 {
            // 交替修改 prim 和 dual
            // 统计每次更新的最大变化，小于一定值视为收敛

            let mut max_delta_scale = prim_scales
                .par_iter_mut()
                .map(|(var, prim_scale)| {
                    let mut sum_x2 = 0.0;
                    let mut count = 0;
                    for (&idx, &coeff) in prim_coeffs.get(var).unwrap_or(&AIndexMap::default()) {
                        if coeff == 0.0 {
                            continue;
                        }
                        let dual_scale = dual_scales[idx];
                        sum_x2 += (coeff * dual_scale * *prim_scale).powi(2);
                        count += 1;
                    }
                    let delta_scale = if count == 0 {
                        1.0
                    } else {
                        (sum_x2 / count as f64)
                            .sqrt()
                            .recip()
                            .clamp(MAGIC_INV, MAGIC)
                    };
                    *prim_scale *= delta_scale;
                    delta_scale.max(delta_scale.recip())
                })
                .reduce(|| 1.0, f64::max); // 计算最大值
            max_delta_scale = max_delta_scale.max(
                dual_scales
                    .par_iter_mut()
                    .enumerate()
                    .map(|(idx, dual_scale)| {
                        let mut sum_x2 = 0.0;
                        let mut count = 0;
                        for (var, coeff) in self.constraints[idx].expression().linear_coefficients()
                        {
                            if coeff == 0.0 {
                                continue;
                            }
                            let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                            sum_x2 += (coeff * prim_scale * *dual_scale).powi(2);
                            count += 1;
                        }

                        let delta_scale = if count == 0 {
                            1.0
                        } else {
                            (sum_x2 / count as f64)
                                .sqrt()
                                .recip()
                                .clamp(MAGIC_INV, MAGIC)
                        };
                        *dual_scale *= delta_scale;
                        delta_scale.max(delta_scale.recip())
                    })
                    .reduce(|| 1.0, f64::max),
            ); // 计算最大值
            if max_delta_scale < stop_threshold {
                log::debug!("Ruiz 预处理在第 {} 轮收敛", i);
                break;
            }
        }

        // 变换关系：x' = x * global_scale / prim_scale（由约束与目标函数的
        // 缩放公式反推；求解器内部变量 x' 的解经 value * prim_scale / global_scale
        // 恢复为原变量 x，见 solver.rs）。
        // 因此变量界的正确缩放是 界 * global_scale / prim_scale。
        // 历史 bug：此前为 界 / prim_scale（漏乘 global_scale），导致变量上界
        // 在缩放空间中放大 1/global_scale 倍，microlp 给出的解可违反原问题上界
        // （test_ruiz 曾复现：x1.max(1.0) 被解成 10/7≈1.4286）。
        // metatorio 的 SolverData 路径不使用变量上界（均为 min(0)），故未受影响。
        let mut global_scale = 0.0;
        for (idx, constraint) in self.constraints.iter().enumerate() {
            global_scale = f64::max(
                global_scale,
                constraint.expression().constant().abs() * dual_scales[idx],
            );
        }
        if global_scale == 0.0 {
            global_scale = 1.0;
        }
        global_scale = global_scale.recip();

        let new_variables_defs = self
            .variables
            .iter_variables_with_def()
            .map(|(var, def)| {
                let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                let new_min = def.get_min() * global_scale / prim_scale;
                let new_max = def.get_max() * global_scale / prim_scale;
                VariableDefinition::new().min(new_min).max(new_max)
            })
            .collect::<Vec<VariableDefinition>>();
        let mut new_variables = ProblemVariables::new();
        let _: Vec<Variable> = new_variables.add_all(new_variables_defs);
        let new_minimise = self
            .minimise
            .linear_coefficients()
            .map(|(var, coeff)| {
                let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                var * coeff * prim_scale / global_scale
            })
            .fold(Expression::from(0.0), |acc, term| acc + term);
        let new_constraints = self
            .constraints
            .par_iter()
            .enumerate()
            .map(|(idx, constraint)| {
                let is_equality = constraint.is_equality();
                let constant = constraint.expression().constant();
                let new_expr = constraint
                    .expression()
                    .linear_coefficients()
                    .map(|(var, coeff)| {
                        let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                        var * coeff * prim_scale
                    })
                    .fold(Expression::from(0.0), |acc, term| acc + term);
                let dual_scale = dual_scales[idx];
                match is_equality {
                    true => (new_expr * dual_scale).eq(-constant * dual_scale * global_scale),
                    false => (new_expr * dual_scale).leq(-constant * dual_scale * global_scale),
                }
            })
            .collect::<Vec<_>>();

        log::debug!("global_scale: {}", global_scale);
        if new_constraints.len() < 8 {
            log::debug!("prim_scales: {:?}", prim_scales);
            log::debug!("dual_scales: {:?}", dual_scales);
            log::debug!("new_minimise: {:?}", new_minimise);
            log::debug!("new_constraints:");
            for (idx, constraint) in new_constraints.iter().enumerate() {
                log::debug!("  {}: {:?}", idx, constraint);
            }
        }
        log::debug!("Ruiz 预处理耗时: {:?}", instant.elapsed());
        let solution = new_variables
            .minimise(new_minimise.clone())
            .using(microlp)
            .with_all(new_constraints)
            .solve()?;
        let cost = solution.eval(new_minimise);
        Ok(RuizSolution {
            inner: solution,
            prim_scales,
            dual_scales,
            global_scale,
            cost,
        })
    }
}

/// 定位"变量上界未生效"缺陷：直接用 microlp（不经 Ruiz 缩放）求解同一 LP。

/// 打印 RuizSolver 的缩放参数与缩放空间解，定位上界丢失点。
#[test]
fn test_ruiz() {
    use good_lp::*;
    let mut vars = ProblemVariables::new();
    let x1 = vars.add(VariableDefinition::new().max(1.0));
    let x2 = vars.add(VariableDefinition::new().min(0.0));
    let x3 = vars.add(VariableDefinition::new().min(0.0));

    let constraints = vec![
        (x1 + 2 * x2 + 3 * x3).leq(3),
        (4 * x1 + 5 * x2 + 6 * x3).leq(7),
        (7 * x1 + 8 * x2 + 9 * x3).leq(10),
    ];

    let solution = RuizSolver::new(-(x1 + x2 + x3), constraints, vars)
        .solve()
        .unwrap();

    // 注意：`solution.inner` 是 Ruiz 均衡缩放后的问题的解，
    // 原变量值必须反缩放：value * prim_scale / global_scale
    // （solver.rs 的 `SolverData::solve` 内部正是这样恢复的；
    //   历史上本测试曾直接读取 inner.value 导致误读为 0.678，
    //   实际最优为 x1=1.0, x2=0.375, x3=0，目标 1.375。）
    let g = solution.global_scale;
    let unscaled = |var: Variable| {
        let p = solution.prim_scales.get(&var).copied().unwrap_or(1.0);
        solution.inner.value(var) * p / g
    };
    let x1v = unscaled(x1);
    let x2v = unscaled(x2);
    let x3v = unscaled(x3);

    assert!((x1v - 1.0).abs() < 1e-5, "x1: {x1v}");
    assert!((x2v - 0.375).abs() < 1e-5, "x2: {x2v}");
    assert!((x3v).abs() < 1e-5, "x3: {x3v}");
    assert!(((x1v + x2v + x3v) - 1.375).abs() < 1e-5, "目标: {}", x1v + x2v + x3v);
}