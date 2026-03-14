use std::collections::HashMap;

use good_lp::{
    Constraint, Expression, ProblemVariables, ResolutionError, SolverModel, microlp,
    solvers::microlp::MicroLpSolution,
};

pub struct RuizSolver {
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
}

pub struct RuizSolution {
    pub inner: MicroLpSolution,           // 原始结果
    pub prim_scales: HashMap<usize, f64>, // 原始变量的系数分别乘了这些系数
    pub dual_scales: HashMap<usize, f64>, // 原始约束的系数分别乘了这些系数
    pub global_scale: f64,                // 全局乘了这个系数
}

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

    pub fn solve(self) -> Result<MicroLpSolution, ResolutionError> {
        for constraint in self.constraints.iter() {
            
        }
        self.variables
            .minimise(self.minimise)
            .using(microlp)
            .with_all(self.constraints)
            .solve()
    }
}
