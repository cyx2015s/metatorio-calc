use std::collections::HashMap;

use good_lp::{
    Constraint, Expression, IntoAffineExpression, ProblemVariables, ResolutionError, SolverModel,
    Variable, VariableDefinition, microlp, solvers::microlp::MicroLpSolution,
};
use rayon::prelude::*;

pub struct RuizSolver {
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
}

pub struct RuizSolution {
    pub inner: MicroLpSolution,              // 原始结果
    pub prim_scales: HashMap<Variable, f64>, // 原始变量的系数分别乘了这些系数
    pub dual_scales: Vec<f64>,               // 原始约束的系数分别乘了这些系数
    // 考察结果时建议的缩放系数
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
        let prim_coeffs = self
            .constraints
            .par_iter()
            .enumerate()
            .fold(HashMap::new, |mut acc, (idx, constraint)| {
                constraint
                    .expression()
                    .linear_coefficients()
                    .for_each(|(var, coeff)| {
                        acc.entry(var)
                            .or_insert_with(HashMap::new)
                            .insert(idx, coeff);
                    });
                acc
            })
            .reduce(HashMap::new, |mut map1, map2| {
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
            .collect::<HashMap<Variable, f64>>();
        let mut dual_scales = vec![1.0; self.constraints.len()];

        // 每次修改变量的系数和约束的系数，使得系数的均方根接近1
        for i in 0..32 {
            // 交替修改 prim 和 dual
            // 统计每次更新的最大变化，小于一定值视为收敛

            prim_scales.par_iter_mut().for_each(|(var, prim_scale)| {
                let mut sum_x2 = 0.0;
                let mut count = 0;
                for (&idx, &coeff) in prim_coeffs.get(var).unwrap_or(&HashMap::new()) {
                    let dual_scale = dual_scales[idx];
                    sum_x2 += (coeff * dual_scale * *prim_scale).powi(2);
                    count += 1;
                }
                let delta_scale = (sum_x2 / count as f64)
                    .sqrt()
                    .recip()
                    .clamp(MAGIC_INV, MAGIC);
                *prim_scale *= delta_scale;
            });

            dual_scales
                .par_iter_mut()
                .enumerate()
                .for_each(|(idx, dual_scale)| {
                    let mut sum_x2 = 0.0;
                    let mut count = 0;
                    for (var, coeff) in self.constraints[idx].expression().linear_coefficients() {
                        let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                        sum_x2 += (coeff * prim_scale * *dual_scale).powi(2);
                        count += 1;
                    }
                    let delta_scale = (sum_x2 / count as f64)
                        .sqrt()
                        .recip()
                        .clamp(MAGIC_INV, MAGIC);
                    *dual_scale *= delta_scale;
                });
        }

        let new_variables_defs = self
            .variables
            .iter_variables_with_def()
            .map(|(var, def)| {
                let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                let new_min = def.get_min() / prim_scale;
                let new_max = def.get_max() / prim_scale;
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
                var * coeff * prim_scale
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
                    true => (new_expr * dual_scale).eq(-constant * dual_scale),
                    false => (new_expr * dual_scale).leq(-constant * dual_scale),
                }
            })
            .collect::<Vec<_>>();
        let mut global_scale = 0.0;
        for constraint in &new_constraints {
            global_scale = f64::max(global_scale, constraint.expression().constant().abs());
        }
        if global_scale == 0.0 {
            global_scale = 1.0;
        }
        global_scale = global_scale.recip();
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

        Ok(RuizSolution {
            inner: new_variables
                .minimise(new_minimise)
                .using(microlp)
                .with_all(new_constraints)
                .solve()?,
            prim_scales,
            dual_scales,
            global_scale,
        })
    }
}

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

    println!("Optimal value: {}", solution.inner.eval(x1 + x2 + x3));
    println!(
        "x1: {}, x2: {}, x3: {}",
        solution.inner.value(x1),
        solution.inner.value(x2),
        solution.inner.value(x3)
    );
}
