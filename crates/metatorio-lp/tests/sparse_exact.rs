//! Reconstruction uses a supplied basis; no floating solver participates.
use metatorio_lp::{Basis, BasisStatus, Column, Model, Rational, Row, SolveControl};
use std::time::Instant;

fn column(cost: f64) -> Column {
    Column {
        lower: 0.0,
        upper: f64::INFINITY,
        cost,
    }
}

fn chain(count: usize) -> (Model, Basis) {
    let model = Model {
        columns: vec![column(1.0); count],
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
        offset: 7.0,
    };
    let basis = Basis {
        columns: vec![BasisStatus::Basic; count],
        rows: vec![BasisStatus::Upper; count],
    };
    (model, basis)
}

#[test]
fn reconstructs_8192_structural_columns_and_certifies_nonzero_costs() {
    let (model, basis) = chain(8192);
    let solution = model.reconstruct(&basis).unwrap();
    assert_eq!(solution.basis_dimension, 8192);
    assert!(
        solution
            .values
            .iter()
            .all(|x| *x == Rational::from_integer(1.into()))
    );
    // Nonzero costs require the transpose solve, not only primal reconstruction.
    assert_eq!(solution.row_duals[0], Rational::from_integer(8192.into()));
    assert_eq!(solution.row_duals[8191], Rational::from_integer(1.into()));
    let certificate = model.certify(&solution).unwrap();
    assert!(certificate.optimal, "{certificate:?}");
    assert_eq!(certificate.objective, "8199");
    assert_eq!(certificate.objective_gap.as_deref(), Some("0"));
}

#[test]
fn mixed_slacks_bounds_duplicate_terms_and_subnormal_pivot_are_exact() {
    let tiny = f64::from_bits(1);
    let model = Model {
        columns: vec![
            column(1.0),
            column(2.0),
            Column {
                lower: 2.0,
                upper: 9.0,
                cost: 3.0,
            },
            Column {
                lower: -1.0,
                upper: 4.0,
                cost: -2.0,
            },
        ],
        rows: vec![
            Row {
                equality: true,
                rhs: 8.0,
                terms: vec![(0, 1.0), (1, 8.0), (1, -8.0), (2, 1.0), (3, 1.0)],
            },
            Row {
                equality: true,
                rhs: tiny,
                terms: vec![(0, -tiny), (1, tiny)],
            },
            Row {
                equality: false,
                rhs: 10.0,
                terms: vec![(0, 1.0), (1, 1.0)],
            },
        ],
        offset: 5.0,
    };
    let basis = Basis {
        columns: vec![
            BasisStatus::Basic,
            BasisStatus::Basic,
            BasisStatus::Lower,
            BasisStatus::Upper,
        ],
        rows: vec![BasisStatus::Upper, BasisStatus::Upper, BasisStatus::Basic],
    };
    let solution = model.reconstruct(&basis).unwrap();
    assert_eq!(solution.basis_dimension, 2);
    assert_eq!(
        solution
            .values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["2", "3", "2", "4"]
    );
    assert_eq!(solution.row_duals[2], Rational::from_integer(0.into()));
    let certificate = model.certify(&solution).unwrap();
    assert!(certificate.optimal, "{certificate:?}");
    assert_eq!(certificate.objective, "11");
    assert_eq!(certificate.objective_gap.as_deref(), Some("0"));
}

#[test]
fn exact_cancellation_makes_a_singular_basis_an_error() {
    let (mut model, basis) = chain(2);
    model.rows[1].terms = vec![(0, 2.0), (1, f64::from_bits(1)), (1, -f64::from_bits(1))];
    assert!(model.reconstruct(&basis).unwrap_err().contains("singular"));
}

#[test]
fn expired_control_returns_no_partial_solution_and_default_can_resume() {
    let (model, basis) = chain(64);
    let control = SolveControl::with_deadline(Instant::now());
    assert!(
        model
            .reconstruct_with_control(&basis, &control)
            .unwrap_err()
            .contains("cancelled")
    );
    let solution = model.reconstruct(&basis).unwrap();
    assert!(model.certify(&solution).unwrap().optimal);
}
