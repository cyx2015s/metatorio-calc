mod vec_ext;

// 求解器内核已拆分为独立 crate `metatorio-solver`（无 UI 依赖），
// 此处仅 re-export，保持 `crate::math::*` 的既有引用不变。
pub use metatorio_solver::{
    Compositions, FlowSpec, RuizSolution, RuizSolver, SolverData, SolverSolution, TargetSpec,
    flow_add,
};

pub use vec_ext::*;
