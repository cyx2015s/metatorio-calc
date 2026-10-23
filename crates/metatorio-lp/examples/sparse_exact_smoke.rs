//! Run with `cargo run -p metatorio-lp --release --example sparse_exact_smoke`.
//! Timings include public reconstruction and independent exact certification.
use metatorio_lp::{Basis, BasisStatus, Column, Model, Rational, Row};
use std::time::Instant;

fn main() {
    println!("structural_columns,reconstruct_ms,certify_ms");
    for count in [64, 256, 1024, 2048, 8192] {
        let model = Model {
            columns: vec![
                Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 1.0
                };
                count
            ],
            rows: (0..count)
                .map(|i| Row {
                    equality: true,
                    rhs: if i == 0 { 1.0 } else { 0.0 },
                    terms: if i == 0 {
                        vec![(0, 1.0)]
                    } else {
                        vec![(i - 1, -1.0), (i, 1.0)]
                    },
                })
                .collect(),
            offset: 0.0,
        };
        let basis = Basis {
            columns: vec![BasisStatus::Basic; count],
            rows: vec![BasisStatus::Upper; count],
        };
        let start = Instant::now();
        let solution = model
            .reconstruct(&basis)
            .expect("exact chain reconstruction");
        let reconstruct = start.elapsed();
        let start = Instant::now();
        let certificate = model
            .certify(&solution)
            .expect("original-model certificate");
        let certify = start.elapsed();
        assert_eq!(solution.basis_dimension, count);
        assert!(
            solution
                .values
                .iter()
                .all(|x| *x == Rational::from_integer(1.into()))
        );
        assert!(certificate.optimal, "{certificate:?}");
        assert_eq!(certificate.objective, count.to_string());
        assert_eq!(certificate.objective_gap.as_deref(), Some("0"));
        println!(
            "{count},{:.3},{:.3}",
            reconstruct.as_secs_f64() * 1000.0,
            certify.as_secs_f64() * 1000.0
        );
    }
}
