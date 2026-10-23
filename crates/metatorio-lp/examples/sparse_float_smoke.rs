//! Run with: cargo run -p metatorio-lp --release --example sparse_float_smoke
//! Nonempty rows and one structural basic variable isolate row-count scaling.
//! Timings are descriptive, not assertions or representative factory workloads.
use metatorio_lp::{Column, Model, Row};
use std::time::Instant;

fn main() {
    println!("rows,nonzeros,pivots,structural_basis,float_ms,exact_check_ms");
    for rows in [256, 1024, 4096, 8192] {
        let model = Model {
            columns: vec![Column {
                lower: 0.0,
                upper: f64::INFINITY,
                cost: 3.0,
            }],
            rows: (0..rows)
                .map(|i| Row {
                    equality: false,
                    rhs: if i == 0 { -1.0 } else { 2.0 },
                    terms: vec![(0, if i == 0 { -1.0 } else { 1.0 })],
                })
                .collect(),
            offset: 7.0,
        };
        let started = Instant::now();
        let hint = model.float_candidate_with_observer(20, |_| true).unwrap();
        let float_ms = started.elapsed().as_secs_f64() * 1000.0;
        let checking = Instant::now();
        let solution = model.reconstruct(&hint.basis).unwrap();
        let certificate = model.certify(&solution).unwrap();
        assert!(certificate.optimal, "{certificate:?}");
        assert_eq!(certificate.objective, "10");
        let exact_ms = checking.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{rows},{rows},{},{},{float_ms:.3},{exact_ms:.3}",
            hint.pivots, solution.basis_dimension
        );
    }
}
