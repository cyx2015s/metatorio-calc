//! Public-API regressions for sparse floating basis search. Every successful
//! search is checked by the independent exact original-model verifier.
use metatorio_lp::{Basis, BasisStatus, Column, FloatOptions, Model, Row};

fn column(cost: f64) -> Column {
    Column {
        lower: 0.0,
        upper: f64::INFINITY,
        cost,
    }
}

#[test]
fn thousands_of_nonempty_rows_keep_a_small_certifiable_structural_basis() {
    let model = Model {
        columns: vec![column(3.0)],
        rows: (0..8192)
            .map(|i| Row {
                equality: false,
                rhs: if i == 0 { -1.0 } else { 2.0 },
                terms: vec![(0, if i == 0 { -1.0 } else { 1.0 })],
            })
            .collect(),
        offset: 7.0,
    };
    let hint = model.float_candidate_with_observer(20, |_| true).unwrap();
    assert_eq!(hint.pivots, 1);
    let solution = model.reconstruct(&hint.basis).unwrap();
    assert_eq!(solution.basis_dimension, 1);
    let certificate = model.certify(&solution).unwrap();
    assert!(certificate.optimal, "{certificate:?}");
    assert_eq!(certificate.objective, "10");
    assert_eq!(solution.values[0].to_string(), "1");
}

#[test]
fn chain_crosses_periodic_refactor_and_is_exactly_certified() {
    // The unique feasible point is all ones. A zero-start search must replace
    // all 160 fixed equality slacks, crossing the 128-pivot refactor boundary.
    let count = 160;
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
        offset: 0.0,
    };
    let hint = model
        .float_candidate_with_observer(4 * count, |_| true)
        .unwrap();
    assert!(hint.pivots > 128, "{hint:?}");
    let solution = model.reconstruct(&hint.basis).unwrap();
    let certificate = model.certify(&solution).unwrap();
    assert!(certificate.optimal, "{certificate:?}");
    assert_eq!(certificate.objective, count.to_string());
    assert!(solution.values.iter().all(|v| v.to_string() == "1"));
}

#[test]
fn mixed_warm_basis_repairs_coupled_nonbasic_rows() {
    let model = Model {
        columns: vec![column(1.0), column(2.0)],
        rows: vec![
            Row {
                equality: false,
                rhs: -1.0,
                terms: vec![(0, -1.0)],
            },
            Row {
                equality: false,
                rhs: 0.0,
                terms: vec![(0, 1.0), (1, -1.0)],
            },
            Row {
                equality: false,
                rhs: 10.0,
                terms: vec![(0, 2.0), (1, 1.0)],
            },
            Row {
                equality: false,
                rhs: 10.0,
                terms: vec![(0, 1.0), (1, 3.0)],
            },
        ],
        offset: 0.0,
    };
    let warm = Basis {
        columns: vec![BasisStatus::Basic, BasisStatus::Lower],
        rows: vec![
            BasisStatus::Upper,
            BasisStatus::Basic,
            BasisStatus::Basic,
            BasisStatus::Basic,
        ],
    };
    let hint = model
        .float_candidate_with_basis(&warm, FloatOptions::default(), 20, |_| true)
        .unwrap();
    assert!(hint.pivots > 0);
    let solution = model.reconstruct(&hint.basis).unwrap();
    let certificate = model.certify(&solution).unwrap();
    assert!(certificate.optimal, "{certificate:?}");
    assert_eq!(certificate.objective, "3");
    assert!(solution.values.iter().all(|v| v.to_string() == "1"));
    // Exercise a fresh factorization containing both structural columns and
    // the coupled, nonzero inactive-row block, without any further pivot.
    let resumed = model
        .float_candidate_with_basis(&hint.basis, FloatOptions::default(), 20, |_| true)
        .unwrap();
    assert_eq!(resumed.pivots, 0);
    assert!(
        model
            .certify(&model.reconstruct(&resumed.basis).unwrap())
            .unwrap()
            .optimal
    );
}

#[test]
fn singular_hint_is_rejected_for_an_independently_feasible_model() {
    let model = Model {
        columns: vec![column(1.0), column(1.0)],
        rows: vec![
            Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1.0), (1, 1.0)],
            },
            Row {
                equality: true,
                rhs: 2.0,
                terms: vec![(0, 2.0), (1, 2.0)],
            },
        ],
        offset: 0.0,
    };
    let singular = Basis {
        columns: vec![BasisStatus::Basic; 2],
        rows: vec![BasisStatus::Upper; 2],
    };
    assert!(
        model
            .float_candidate_with_basis(&singular, FloatOptions::default(), 20, |_| true)
            .is_err()
    );
    let cold = model.float_candidate_with_observer(20, |_| true).unwrap();
    let certificate = model
        .certify(&model.reconstruct(&cold.basis).unwrap())
        .unwrap();
    assert!(certificate.optimal, "{certificate:?}");
    assert_eq!(certificate.objective, "1");
}
