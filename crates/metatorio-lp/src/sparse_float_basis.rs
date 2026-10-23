//! Sparse row-pivoted LU plus a bounded product-form (eta) update chain.
//! No coefficient is dropped by tolerance: numerical failures remain hints.
use std::collections::{BTreeMap, BTreeSet};

// Match the periodic refactor interval in float.rs; this bounds only the
// product-form update chain, independently of matrix dimensions and sparsity.
const MAX_ETAS: usize = 128;
const MIN_PIVOT: f64 = 1e-14;

struct Eta {
    position: usize,
    pivot: f64,
    off_diagonal: Vec<(usize, f64)>,
}

pub(crate) struct SparseFloatBasis {
    permutation: Vec<usize>,
    lower: Vec<Vec<(usize, f64)>>,
    upper: Vec<Vec<(usize, f64)>>,
    diagonal: Vec<f64>,
    etas: Vec<Eta>,
}

impl SparseFloatBasis {
    #[cfg(test)]
    pub(crate) fn factor(
        m: usize,
        n: usize,
        ids: &[usize],
        columns: &[Vec<(usize, f64)>],
    ) -> Result<Self, String> {
        Self::factor_with_observer(m, n, ids, columns, || true)
    }

    pub(crate) fn factor_with_observer(
        m: usize,
        n: usize,
        ids: &[usize],
        columns: &[Vec<(usize, f64)>],
        mut observer: impl FnMut() -> bool,
    ) -> Result<Self, String> {
        if ids.len() != m || columns.len() != n {
            return Err("nonsquare floating sparse basis".into());
        }
        if !observer() {
            return Err("floating observer requested stop during sparse factorization".into());
        }
        let mut rows = vec![BTreeMap::<usize, f64>::new(); m];
        let mut incidence = vec![BTreeSet::new(); m];
        let mut seen = BTreeSet::new();
        for (j, &id) in ids.iter().enumerate() {
            if j % 128 == 0 && !observer() {
                return Err("floating observer requested stop during sparse factorization".into());
            }
            if !seen.insert(id) {
                return Err("duplicate floating basis identity".into());
            }
            let mut insert = |i: usize, a: f64| -> Result<(), String> {
                if i >= m || !a.is_finite() {
                    return Err("invalid floating basis coefficient".into());
                }
                let previous = rows[i].get(&j).copied().unwrap_or(0.0);
                let value = previous + a;
                if !value.is_finite() {
                    return Err("nonfinite floating basis coefficient sum".into());
                }
                if value == 0.0 {
                    if rows[i].remove(&j).is_some() {
                        incidence[j].remove(&i);
                    }
                } else {
                    if previous == 0.0 {
                        incidence[j].insert(i);
                    }
                    rows[i].insert(j, value);
                }
                Ok(())
            };
            if id < n {
                for &(i, a) in &columns[id] {
                    insert(i, a)?;
                }
            } else {
                insert(id - n, 1.0)?;
            }
        }
        let mut lower = vec![Vec::new(); m];
        let mut upper = Vec::with_capacity(m);
        let mut diagonal = Vec::with_capacity(m);
        let mut permutation = Vec::with_capacity(m);
        let mut work = 0usize;
        for p in 0..m {
            if p % 128 == 0 && !observer() {
                return Err("floating observer requested stop during sparse factorization".into());
            }
            let q = incidence[p]
                .iter()
                .copied()
                .max_by(|&a, &b| {
                    rows[a][&p]
                        .abs()
                        .total_cmp(&rows[b][&p].abs())
                        .then_with(|| b.cmp(&a))
                })
                .ok_or("singular floating sparse basis")?;
            let pivot = rows[q][&p];
            if pivot.abs() < MIN_PIVOT || !pivot.is_finite() {
                return Err("unstable floating sparse refactor pivot".into());
            }
            let pivot_row = std::mem::take(&mut rows[q]);
            for &j in pivot_row.keys() {
                incidence[j].remove(&q);
            }
            let targets: Vec<_> = incidence[p].iter().copied().collect();
            for i in targets {
                let multiplier = rows[i].remove(&p).unwrap() / pivot;
                incidence[p].remove(&i);
                // The removed working entry becomes an L entry.
                if !multiplier.is_finite() {
                    return Err("nonfinite floating sparse multiplier".into());
                }
                lower[i].push((p, multiplier));
                for (&j, &a) in pivot_row.range(p + 1..) {
                    work += 1;
                    if work == 4096 {
                        work = 0;
                        if !observer() {
                            return Err(
                                "floating observer requested stop during sparse factorization"
                                    .into(),
                            );
                        }
                    }
                    let previous = rows[i].get(&j).copied().unwrap_or(0.0);
                    let value = previous - multiplier * a;
                    if !value.is_finite() {
                        return Err("nonfinite floating sparse elimination".into());
                    }
                    if value == 0.0 {
                        if rows[i].remove(&j).is_some() {
                            incidence[j].remove(&i);
                        }
                    } else {
                        if previous == 0.0 {
                            incidence[j].insert(i);
                        }
                        rows[i].insert(j, value);
                    }
                }
            }
            permutation.push(q);
            diagonal.push(pivot);
            upper.push(pivot_row.into_iter().filter(|(j, _)| *j != p).collect());
        }
        let lower = permutation
            .iter()
            .map(|&i| std::mem::take(&mut lower[i]))
            .collect();
        Ok(Self {
            permutation,
            lower,
            upper,
            diagonal,
            etas: Vec::new(),
        })
    }

    pub(crate) fn solve(&self, rhs: &[f64]) -> Result<Vec<f64>, String> {
        self.check_rhs(rhs)?;
        let m = rhs.len();
        let mut x: Vec<_> = self.permutation.iter().map(|&i| rhs[i]).collect();
        for i in 0..m {
            for &(j, a) in &self.lower[i] {
                x[i] -= a * x[j];
            }
        }
        for i in (0..m).rev() {
            for &(j, a) in &self.upper[i] {
                x[i] -= a * x[j];
            }
            x[i] /= self.diagonal[i];
        }
        for eta in &self.etas {
            let value = x[eta.position] / eta.pivot;
            for &(i, a) in &eta.off_diagonal {
                x[i] -= a * value;
            }
            x[eta.position] = value;
        }
        Self::finite(x)
    }

    pub(crate) fn solve_transpose(&self, rhs: &[f64]) -> Result<Vec<f64>, String> {
        self.transpose(rhs, false)
    }

    /// Triangle inequalities through all factors bound |B^-T| |rhs|.
    /// Test oracle only: too loose for the production ratio-test budget.
    #[cfg(test)]
    pub(crate) fn transpose_abs_bound(&self, rhs: &[f64]) -> Result<Vec<f64>, String> {
        self.transpose(rhs, true)
    }

    fn transpose(&self, rhs: &[f64], absolute: bool) -> Result<Vec<f64>, String> {
        self.check_rhs(rhs)?;
        let mut x = rhs.to_vec();
        if absolute {
            x.iter_mut().for_each(|v| *v = v.abs());
        }
        for eta in self.etas.iter().rev() {
            let mut value = x[eta.position];
            for &(i, a) in &eta.off_diagonal {
                value += if absolute { a.abs() * x[i] } else { -a * x[i] };
            }
            x[eta.position] = value / if absolute { eta.pivot.abs() } else { eta.pivot };
        }
        for i in 0..x.len() {
            x[i] /= if absolute {
                self.diagonal[i].abs()
            } else {
                self.diagonal[i]
            };
            for &(j, a) in &self.upper[i] {
                x[j] += if absolute { a.abs() * x[i] } else { -a * x[i] };
            }
        }
        for i in (0..x.len()).rev() {
            for &(j, a) in &self.lower[i] {
                x[j] += if absolute { a.abs() * x[i] } else { -a * x[i] };
            }
        }
        let mut result = vec![0.0; x.len()];
        for (i, &original) in self.permutation.iter().enumerate() {
            result[original] = x[i];
        }
        Self::finite(result)
    }

    pub(crate) fn update(&mut self, position: usize, direction: &[f64]) -> Result<(), String> {
        self.check_rhs(direction)?;
        if position >= direction.len() || direction[position].abs() < 1e-12 {
            return Err("unstable floating eta pivot".into());
        }
        if self.etas.len() >= MAX_ETAS {
            return Err("floating sparse eta chain requires refactorization".into());
        }
        self.etas.push(Eta {
            position,
            pivot: direction[position],
            off_diagonal: direction
                .iter()
                .copied()
                .enumerate()
                .filter(|(i, a)| *i != position && *a != 0.0)
                .collect(),
        });
        Ok(())
    }

    fn check_rhs(&self, rhs: &[f64]) -> Result<(), String> {
        if rhs.len() != self.diagonal.len() || rhs.iter().any(|v| !v.is_finite()) {
            return Err("invalid floating sparse solve rhs".into());
        }
        Ok(())
    }
    fn finite(x: Vec<f64>) -> Result<Vec<f64>, String> {
        if x.iter().any(|v| !v.is_finite()) {
            Err("nonfinite floating sparse solve".into())
        } else {
            Ok(x)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(m: usize, n: usize, ids: &[usize], columns: &[Vec<(usize, f64)>]) -> Vec<Vec<f64>> {
        let mut a = vec![vec![0.0; m]; m];
        for (j, &id) in ids.iter().enumerate() {
            if id < n {
                for &(i, value) in &columns[id] {
                    a[i][j] += value;
                }
            } else {
                a[id - n][j] = 1.0;
            }
        }
        a
    }

    // Independent dense Gauss-Jordan oracle used only for small test matrices.
    fn oracle(mut a: Vec<Vec<f64>>, mut rhs: Vec<f64>, transpose: bool) -> Vec<f64> {
        let m = rhs.len();
        if transpose {
            for i in 0..m {
                for j in 0..i {
                    let v = a[i][j];
                    a[i][j] = a[j][i];
                    a[j][i] = v;
                }
            }
        }
        for k in 0..m {
            let p = (k..m)
                .max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
                .unwrap();
            assert!(a[p][k].abs() > 1e-13);
            a.swap(k, p);
            rhs.swap(k, p);
            let pivot = a[k][k];
            for j in k..m {
                a[k][j] /= pivot;
            }
            rhs[k] /= pivot;
            for i in 0..m {
                if i == k {
                    continue;
                }
                let multiplier = a[i][k];
                for j in k..m {
                    a[i][j] -= multiplier * a[k][j];
                }
                rhs[i] -= multiplier * rhs[k];
            }
        }
        rhs
    }

    fn assert_close(a: &[f64], b: &[f64]) {
        for (i, (&a, &b)) in a.iter().zip(b).enumerate() {
            assert!(
                (a - b).abs() <= 2e-9 * (1.0 + b.abs()),
                "coordinate {i}: {a} != {b}"
            );
        }
    }

    fn verify(factor: &SparseFloatBasis, a: &[Vec<f64>]) {
        let m = a.len();
        let rhs: Vec<_> = (0..m).map(|i| i as f64 - 1.25).collect();
        assert_close(
            &factor.solve(&rhs).unwrap(),
            &oracle(a.to_vec(), rhs.clone(), false),
        );
        assert_close(
            &factor.solve_transpose(&rhs).unwrap(),
            &oracle(a.to_vec(), rhs.clone(), true),
        );
        let bound = factor.transpose_abs_bound(&rhs).unwrap();
        let mut inverse_bound = vec![0.0; m];
        for j in 0..m {
            let mut unit = vec![0.0; m];
            unit[j] = 1.0;
            let row = oracle(a.to_vec(), unit, true);
            for i in 0..m {
                inverse_bound[i] += row[i].abs() * rhs[j].abs();
            }
        }
        for i in 0..m {
            assert!(bound[i] + 1e-9 >= inverse_bound[i]);
        }
    }

    #[test]
    fn mixed_basis_duplicates_row_pivot_and_transpose_match_dense_oracle() {
        let columns = vec![
            vec![(0, 1.0), (0, 1.0), (1, 3.0), (2, 5.0), (3, 7.0)],
            vec![(0, 3.0), (1, 4.0), (2, 7.0), (3, 11.0)],
        ];
        for ids in [vec![0, 3, 1, 5], vec![5, 1, 3, 0], vec![3, 5, 0, 1]] {
            let factor = SparseFloatBasis::factor(4, 2, &ids, &columns).unwrap();
            verify(&factor, &matrix(4, 2, &ids, &columns));
        }
        let columns = vec![vec![(1, 2.0)], vec![(0, 3.0), (1, 4.0)]];
        let factor = SparseFloatBasis::factor(2, 2, &[0, 1], &columns).unwrap();
        assert_eq!(factor.permutation, vec![1, 0]);
        verify(&factor, &matrix(2, 2, &[0, 1], &columns));
    }

    #[test]
    fn eta_updates_slack_enter_and_leave_and_refactor_match_oracle() {
        let m = 7;
        let columns: Vec<Vec<_>> = (0..m)
            .map(|j| {
                (0..m)
                    .filter_map(|i| {
                        (i == j || (i + j) % 3 == 0).then_some((
                            i,
                            if i == j {
                                4.0
                            } else {
                                0.125 * (i as f64 - j as f64)
                            },
                        ))
                    })
                    .collect()
            })
            .collect();
        let mut ids: Vec<_> = (m..2 * m).collect();
        let mut factor = SparseFloatBasis::factor(m, m, &ids, &columns).unwrap();
        for iteration in 0..96 {
            let position = iteration % m;
            let entering = if ids[position] < m {
                m + position
            } else {
                position
            };
            let mut column = vec![0.0; m];
            if entering < m {
                for &(i, v) in &columns[entering] {
                    column[i] += v;
                }
            } else {
                column[entering - m] = 1.0;
            }
            let direction = factor.solve(&column).unwrap();
            factor.update(position, &direction).unwrap();
            ids[position] = entering;
            verify(&factor, &matrix(m, m, &ids, &columns));
            if iteration % 23 == 22 {
                factor = SparseFloatBasis::factor(m, m, &ids, &columns).unwrap();
                assert!(factor.etas.is_empty());
                verify(&factor, &matrix(m, m, &ids, &columns));
            }
        }
    }

    #[test]
    fn generated_sparse_systems_match_dense_oracle() {
        let mut state = 173u64;
        for case in 0..24 {
            let m = 3 + case % 10;
            let columns: Vec<Vec<_>> = (0..m)
                .map(|j| {
                    (0..m)
                        .filter_map(|i| {
                            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                            if i == j {
                                Some((i, 20.0))
                            } else if state % 3 == 0 {
                                Some((i, ((state >> 32) % 9) as f64 - 4.0))
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .collect();
            // Reverse order forces a noncanonical structural basis and row pivots.
            let ids: Vec<_> = (0..m).rev().collect();
            let factor = SparseFloatBasis::factor(m, m, &ids, &columns).unwrap();
            verify(&factor, &matrix(m, m, &ids, &columns));
        }
    }

    #[test]
    fn singular_unstable_nonfinite_and_cancel_fail_explicitly() {
        assert!(
            SparseFloatBasis::factor(2, 2, &[0, 1], &[vec![(0, 1.0)], vec![(0, 2.0)]]).is_err()
        );
        assert!(SparseFloatBasis::factor(1, 1, &[0], &[vec![(0, 1e-16)]]).is_err());
        assert!(SparseFloatBasis::factor(1, 1, &[0], &[vec![(0, f64::INFINITY)]]).is_err());
        assert!(
            SparseFloatBasis::factor(1, 1, &[0], &[vec![(0, f64::MAX), (0, f64::MAX)]]).is_err()
        );
        assert!(SparseFloatBasis::factor(1, 1, &[0], &[vec![(0, 1.0), (0, -1.0)]]).is_err());
        assert!(SparseFloatBasis::factor(2, 0, &[0, 0], &[]).is_err());
        let error = SparseFloatBasis::factor_with_observer(3, 0, &[0, 1, 2], &[], || false)
            .err()
            .unwrap();
        assert!(error.contains("observer requested stop"));
        let mut factor = SparseFloatBasis::factor(1, 0, &[0], &[]).unwrap();
        assert!(factor.solve(&[f64::NAN]).is_err());
        assert!(factor.solve(&[]).is_err());
        assert!(factor.update(0, &[0.0]).is_err());
        for _ in 0..MAX_ETAS {
            factor.update(0, &[1.0]).unwrap();
        }
        assert!(
            factor
                .update(0, &[1.0])
                .unwrap_err()
                .contains("requires refactorization")
        );
    }

    #[test]
    fn large_diagonal_storage_is_linear_without_a_row_limit() {
        let m = 8192;
        let ids: Vec<_> = (0..m).rev().collect();
        let factor = SparseFloatBasis::factor(m, 0, &ids, &[]).unwrap();
        let stored = factor.diagonal.len()
            + factor.lower.iter().map(Vec::len).sum::<usize>()
            + factor.upper.iter().map(Vec::len).sum::<usize>();
        assert_eq!(stored, m);
        assert!(factor.lower.iter().all(Vec::is_empty));
        assert!(factor.upper.iter().all(Vec::is_empty));
        let rhs: Vec<_> = (0..m).map(|i| i as f64).collect();
        let reversed: Vec<_> = rhs.iter().rev().copied().collect();
        assert_eq!(factor.solve(&rhs).unwrap(), reversed);
        assert_eq!(factor.solve_transpose(&rhs).unwrap(), reversed);
    }
}
