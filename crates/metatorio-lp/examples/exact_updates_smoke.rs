//! Bounded end-to-end exact pivot measurements; timings are not assertions.
use metatorio_lp::{Basis, BasisStatus, Column, DualPivotRule, Model, Row, SolveControl};
use std::time::{Duration, Instant};

fn column(cost: f64) -> Column {
    Column {
        lower: 0.0,
        upper: f64::INFINITY,
        cost,
    }
}

fn main() {
    println!("workload,rows,pivots,milliseconds,optimal");
    for count in [128, 512] {
        let mut columns = vec![column(2.0); count];
        columns.extend(vec![column(1.0); count]);
        let model = Model {
            columns,
            rows: (0..count)
                .map(|i| Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(i, 1.0), (count + i, 1.0)],
                })
                .collect(),
            offset: 0.0,
        };
        let basis = Basis {
            columns: (0..2 * count)
                .map(|i| {
                    if i < count {
                        BasisStatus::Basic
                    } else {
                        BasisStatus::Lower
                    }
                })
                .collect(),
            rows: vec![BasisStatus::Upper; count],
        };
        let start = Instant::now();
        let control = SolveControl::with_deadline(start + Duration::from_secs(30));
        let result = model
            .optimize_without_inverse_with_control(basis, count + 1, |_| true, &control)
            .unwrap();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let certificate = model.certify(&result.solution).unwrap();
        assert!(certificate.optimal, "{certificate:?}");
        assert_eq!(certificate.objective, count.to_string());
        assert_eq!(result.pivots, count);
        println!(
            "warm-replacements,{count},{},{elapsed:.3},true",
            result.pivots
        );

        let model = Model {
            columns: vec![column(1.0); count],
            rows: (0..count)
                .map(|i| Row {
                    equality: true,
                    rhs: if i == 0 { 1.0 } else { 0.0 },
                    terms: if i == 0 {
                        vec![(i, 1.0)]
                    } else {
                        vec![(i - 1, -1.0), (i, 1.0)]
                    },
                })
                .collect(),
            offset: 0.0,
        };
        let start = Instant::now();
        let control = SolveControl::with_deadline(start + Duration::from_secs(30));
        let result = model
            .solve_dual_with_control(DualPivotRule::FirstViolation, count + 1, |_| true, &control)
            .unwrap();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let certificate = model.certify(&result.solution).unwrap();
        assert!(certificate.optimal, "{certificate:?}");
        assert_eq!(certificate.objective, count.to_string());
        assert_eq!(result.pivots, count);
        println!("dual-chain,{count},{},{elapsed:.3},true", result.pivots);
    }
}
