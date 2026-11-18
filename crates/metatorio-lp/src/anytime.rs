//! 仅含原始可行性的随时可用（anytime）结果；最优性证明属于可选的更强证书。
use crate::{Basis, Certificate, ExactSolution};

#[derive(Clone, Debug, serde::Serialize)]
pub struct PivotCostChange {
    /// 已完成且经过认证的原始单纯形优化主元累计次数。
    pub pivot: usize,
    pub cost_before: f64,
    pub cost_after: f64,
    /// 由精确有理数差值转换得到，而非 `f64` 直接相减。
    pub decrease: f64,
}

#[derive(Debug)]
pub struct AnytimeResult {
    pub solution: ExactSolution,
    pub basis: Basis,
    /// 始终在完整原模型上认证原始可行性。
    pub certificate: Certificate,
    pub pivots: usize,
    /// 全部严格正的原目标精确下降次数（包括超出最近展示窗口或无法表示为有限 `f64` 的主元）。
    pub improving_pivots: usize,
    pub timed_out: bool,
    /// 最近 8 次可表示的原目标优化主元记录（含零下降）。每对记录属于同一段连续主问题；
    /// 新区段会另起基线，不会合成跨段差值，无法表示为有限 `f64` 的主元会省略。
    pub recent_pivot_cost_changes: Vec<PivotCostChange>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasisStatus, Column, Model, Rational, Row, SolveControl};
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Instant,
    };

    fn independent(count: usize) -> (Model, Basis) {
        let mut columns = Vec::new();
        let mut rows = Vec::new();
        let mut statuses = Vec::new();
        for i in 0..count {
            columns.extend([2.0, 1.0].map(|cost| Column {
                lower: 0.0,
                upper: f64::INFINITY,
                cost,
            }));
            statuses.extend([BasisStatus::Basic, BasisStatus::Lower]);
            rows.push(Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(2 * i, 1.0), (2 * i + 1, 1.0)],
            });
        }
        (
            Model {
                columns,
                rows,
                offset: 3.0,
            },
            Basis {
                columns: statuses,
                rows: vec![BasisStatus::Upper; count],
            },
        )
    }

    #[test]
    fn optimum_and_recent_window_are_actual_original_cost_pivots() {
        let (model, basis) = independent(12);
        let result = model
            .optimize_anytime_from_basis_with_control(&basis, 100, &SolveControl::default())
            .unwrap();
        assert!(result.certificate.optimal && !result.timed_out);
        assert_eq!(result.pivots, 12);
        assert_eq!(result.improving_pivots, 12);
        assert_eq!(result.certificate.objective, "15");
        assert_eq!(result.recent_pivot_cost_changes.len(), 8);
        for (i, change) in result.recent_pivot_cost_changes.iter().enumerate() {
            assert_eq!(change.pivot, i + 5);
            assert_eq!(change.decrease, 1.0);
            assert_eq!(change.cost_before - change.cost_after, change.decrease);
        }
        assert!(model.certify(&result.solution).unwrap().primal_feasible);
        assert!(
            model
                .certify(&model.reconstruct(&result.basis).unwrap())
                .unwrap()
                .primal_feasible
        );
    }

    #[test]
    fn expired_budget_returns_verified_initial_incumbent_but_cancel_never_does() {
        let (model, basis) = independent(2);
        let expired = SolveControl::with_deadline(Instant::now());
        let result = model
            .optimize_anytime_from_basis_with_control(&basis, 100, &expired)
            .unwrap();
        assert!(
            result.timed_out && result.certificate.primal_feasible && !result.certificate.optimal
        );
        assert_eq!(result.pivots, 0);
        assert!(result.recent_pivot_cost_changes.is_empty());
        let flag = Arc::new(AtomicBool::new(true));
        let cancelled = expired.with_cancellation(flag.clone());
        assert!(
            model
                .optimize_anytime_from_basis_with_control(&basis, 100, &cancelled)
                .unwrap_err()
                .contains("cancelled")
        );
        flag.store(false, Ordering::Relaxed);
        assert!(
            model
                .optimize_anytime_from_basis_with_control(&basis, 100, &cancelled)
                .is_ok()
        );
    }

    #[test]
    fn pivot_limit_returns_improved_feasible_incumbent_without_claiming_timeout() {
        let (model, basis) = independent(4);
        let result = model
            .optimize_anytime_from_basis_with_control(&basis, 2, &SolveControl::default())
            .unwrap();
        assert!(
            result.certificate.primal_feasible && !result.certificate.optimal && !result.timed_out
        );
        assert_eq!(result.pivots, 2);
        assert_eq!(result.certificate.objective, "9");
        assert_eq!(result.recent_pivot_cost_changes.len(), 2);
    }

    #[test]
    fn no_phase_one_pivots_are_fabricated_from_infeasible_hint() {
        let (mut model, basis) = independent(1);
        model.rows[0].rhs = -1.0;
        assert!(
            model
                .optimize_anytime_from_basis_with_control(&basis, 100, &SolveControl::default())
                .unwrap_err()
                .contains("primal feasible")
        );
    }

    #[test]
    fn zero_degenerate_pivot_is_retained() {
        let (mut model, basis) = independent(1);
        model.rows[0].rhs = 0.0;
        let result = model
            .optimize_anytime_from_basis_with_control(&basis, 10, &SolveControl::default())
            .unwrap();
        assert!(result.certificate.optimal);
        assert_eq!(result.pivots, 1);
        assert_eq!(result.improving_pivots, 0);
        let change = &result.recent_pivot_cost_changes[0];
        assert_eq!(change.decrease, 0.0);
        assert_eq!(change.cost_before, 3.0);
        assert_eq!(change.cost_after, 3.0);
        assert_eq!(result.solution.values[0], Rational::from_integer(0.into()));
    }

    #[test]
    fn master_segment_changes_never_count_as_pivots() {
        let (model, basis) = independent(40);
        let result = model
            .optimize_anytime_from_basis_with_control(&basis, 100, &SolveControl::default())
            .unwrap();
        assert!(result.certificate.optimal);
        assert_eq!(result.pivots, 40);
        assert_eq!(result.recent_pivot_cost_changes.len(), 8);
        for (i, change) in result.recent_pivot_cost_changes.iter().enumerate() {
            assert_eq!(change.pivot, i + 33);
            assert_eq!(change.decrease, 1.0);
        }
    }

    #[test]
    fn pivot_decrease_is_converted_after_exact_subtraction() {
        let (mut model, basis) = independent(1);
        model.columns[0].cost = 1.0 + f64::EPSILON;
        model.offset = 1e20;
        let result = model
            .optimize_anytime_from_basis_with_control(&basis, 10, &SolveControl::default())
            .unwrap();
        let change = &result.recent_pivot_cost_changes[0];
        assert_eq!(change.cost_before, change.cost_after);
        assert_eq!(change.decrease, f64::EPSILON);
    }
}
