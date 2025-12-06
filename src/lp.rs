#[cfg(test)]
mod lp_tests {
    use good_lp::{Solution, SolverModel, constraint, variables, clarabel};

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
    #[test]
    fn catalyst_test() {
        variables! {
            vars:
                a >= 0;
                b >= 0;
                c >= 0;
        }
        let mut model = vars.maximise(b + c - a).using(clarabel);
        model
            .settings()
            .tol_feas(1e-12)
            .tol_gap_abs(1e-12)
            .tol_gap_rel(1e-12)
            .tol_infeas_abs(1e-10)
            .tol_infeas_rel(1e-10);
        let solution = model
            .with(constraint!(b - a >= 0))
            .with(constraint!(a - b >= 0))
            .with(constraint!(3 * a - 2 * b - 2 * c == 0))
            .with(constraint!(a + b + c <= 114))
            .solve()
            .unwrap();
        println!("a: {}", solution.value(a));
        println!("b: {}", solution.value(b));
        println!("c: {}", solution.value(c));
    }
}
