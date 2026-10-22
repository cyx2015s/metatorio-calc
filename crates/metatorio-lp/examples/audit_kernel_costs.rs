//! Small, repeatable audit probe; timings are descriptive, never assertions.
use metatorio_lp::{Column, ExactSolution, Model, Rational, Row};
use std::{hint::black_box, time::Instant};

fn median_us(mut work: impl FnMut()) -> u128 {
    let mut samples = Vec::new();
    for _ in 0..7 {
        let start = Instant::now();
        for _ in 0..10 {
            work();
        }
        samples.push(start.elapsed().as_micros());
    }
    samples.sort_unstable();
    samples[3]
}

fn main() {
    let n = 16_384;
    let model = Model {
        columns: (0..n)
            .map(|_| Column {
                lower: 0.0,
                upper: f64::INFINITY,
                cost: 1.0,
            })
            .collect(),
        rows: vec![Row {
            equality: true,
            rhs: 1.0,
            terms: (0..n).map(|j| (j, 1.0)).collect(),
        }],
        offset: 0.0,
    };
    let mut values = vec![Rational::from_integer(0.into()); n];
    values[0] = Rational::from_integer(1.into());
    let solution = ExactSolution {
        values,
        row_duals: vec![Rational::from_integer(1.into())],
        basis_dimension: 1,
    };
    assert!(model.certify(&solution).unwrap().optimal);
    let certify = median_us(|| {
        black_box(model.certify(black_box(&solution)).unwrap());
    });
    let scale = median_us(|| {
        black_box(model.scale_to_dual_feasible(black_box(&solution)).unwrap());
    });
    let repeat = median_us(|| {
        let (adjusted, _) = model.scale_to_dual_feasible(black_box(&solution)).unwrap();
        black_box(model.certify(&adjusted).unwrap());
    });
    println!(
        "16,384 cols / 1 row; median of 7 x 10 calls: certify={certify}us scale={scale}us scale_then_recertify={repeat}us"
    );
    for m in [256, 1024, 4096, 4097] {
        let trivial = Model {
            columns: vec![],
            rows: (0..m)
                .map(|_| Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![],
                })
                .collect(),
            offset: 0.0,
        };
        let start = Instant::now();
        let hint = trivial.float_candidate_with_observer(1, |_| true);
        match hint {
            Ok(hint) => {
                assert_eq!(hint.pivots, 0);
                assert!(
                    trivial
                        .certify(&trivial.reconstruct(&hint.basis).unwrap())
                        .unwrap()
                        .optimal
                );
                println!(
                    "empty rows={m}: float+exact-check={}us dense-inverse-bytes={}",
                    start.elapsed().as_micros(),
                    m * m * 8
                );
            }
            Err(error) => println!("empty rows={m}: {error}"),
        }
    }
}
