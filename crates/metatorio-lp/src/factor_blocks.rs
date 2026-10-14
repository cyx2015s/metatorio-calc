//! Exact block triangular factorization after structural row matching.
use crate::{Rational, SolveControl, factor::Factorization};
use num_traits::Zero;

pub(crate) struct BlockFactorization {
    matched_rows: Vec<usize>,
    groups: Vec<Group>,
    lower: Vec<Vec<(usize, Rational)>>,
}
struct Group {
    indices: Vec<usize>,
    factor: Factorization,
}

fn augment(
    row: usize,
    edges: &[Vec<usize>],
    matched: &mut [Option<usize>],
    seen: &mut [bool],
) -> bool {
    for &column in &edges[row] {
        if seen[column] {
            continue;
        }
        seen[column] = true;
        if matched[column].is_none_or(|previous| augment(previous, edges, matched, seen)) {
            matched[column] = Some(row);
            return true;
        }
    }
    false
}

impl BlockFactorization {
    pub(crate) fn try_new(
        matrix: &[Vec<Rational>],
        control: &SolveControl,
    ) -> Result<Option<Self>, String> {
        let n = matrix.len();
        let edges: Vec<Vec<_>> = matrix
            .iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .filter_map(|(j, a)| (!a.is_zero()).then_some(j))
                    .collect()
            })
            .collect();
        let mut matched = vec![None; n];
        for row in 0..n {
            control.check()?;
            if !augment(row, &edges, &mut matched, &mut vec![false; n]) {
                return Err("exactly singular basis: no structural matching".into());
            }
        }
        let matched_rows: Vec<_> = matched.into_iter().map(Option::unwrap).collect();
        // Entry B[i,j] depends on x[j], where B is A with matched rows.
        let mut graph = vec![Vec::new(); n];
        let mut reverse = vec![Vec::new(); n];
        for (i, &row) in matched_rows.iter().enumerate() {
            for &j in &edges[row] {
                graph[j].push(i);
                reverse[i].push(j);
            }
        }
        let mut visited = vec![false; n];
        let mut finish = Vec::with_capacity(n);
        for root in 0..n {
            if visited[root] {
                continue;
            }
            visited[root] = true;
            let mut stack = vec![(root, 0)];
            while let Some((node, next)) = stack.pop() {
                if next == graph[node].len() {
                    finish.push(node);
                } else {
                    stack.push((node, next + 1));
                    let child = graph[node][next];
                    if !visited[child] {
                        visited[child] = true;
                        stack.push((child, 0));
                    }
                }
            }
        }
        visited.fill(false);
        let mut components = Vec::new();
        let mut component_of = vec![0; n];
        for &root in finish.iter().rev() {
            control.check()?;
            if visited[root] {
                continue;
            }
            let mut component = Vec::new();
            let mut stack = vec![root];
            visited[root] = true;
            while let Some(node) = stack.pop() {
                component_of[node] = components.len();
                component.push(node);
                for &child in &reverse[node] {
                    if !visited[child] {
                        visited[child] = true;
                        stack.push(child);
                    }
                }
            }
            components.push(component);
        }
        if components.len() <= 1 {
            return Ok(None);
        }
        let mut lower = vec![Vec::new(); n];
        let mut groups = Vec::with_capacity(components.len());
        for indices in components {
            control.check()?;
            for &i in &indices {
                for &j in &edges[matched_rows[i]] {
                    if component_of[j] != component_of[i] {
                        if component_of[j] > component_of[i] {
                            return Err("invalid exact block dependency ordering".into());
                        }
                        lower[i].push((j, matrix[matched_rows[i]][j].clone()));
                    }
                }
            }
            let diagonal = indices
                .iter()
                .map(|i| {
                    indices
                        .iter()
                        .map(|j| matrix[matched_rows[*i]][*j].clone())
                        .collect()
                })
                .collect();
            groups.push(Group {
                indices,
                factor: Factorization::new_with_control(diagonal, control)?,
            });
        }
        Ok(Some(Self {
            matched_rows,
            groups,
            lower,
        }))
    }

    pub(crate) fn solve(
        &self,
        rhs: &[Rational],
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        if rhs.len() != self.matched_rows.len() {
            return Err("exact block RHS dimension invalid".into());
        }
        let mut x = vec![Rational::zero(); rhs.len()];
        for group in &self.groups {
            control.check()?;
            let adjusted: Vec<_> = group
                .indices
                .iter()
                .map(|i| {
                    let mut value = rhs[self.matched_rows[*i]].clone();
                    for (j, a) in &self.lower[*i] {
                        if !x[*j].is_zero() {
                            value -= a * &x[*j];
                        }
                    }
                    value
                })
                .collect();
            let values = group.factor.solve_with_control(&adjusted, control)?;
            for (&i, value) in group.indices.iter().zip(values) {
                x[i] = value;
            }
        }
        Ok(x)
    }

    pub(crate) fn transpose(
        &self,
        rhs: &[Rational],
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        if rhs.len() != self.matched_rows.len() {
            return Err("exact block transpose RHS dimension invalid".into());
        }
        let mut pending = rhs.to_vec();
        let mut y = vec![Rational::zero(); rhs.len()];
        for group in self.groups.iter().rev() {
            control.check()?;
            let adjusted: Vec<_> = group.indices.iter().map(|i| pending[*i].clone()).collect();
            let values = group
                .factor
                .solve_transpose_with_control(&adjusted, control)?;
            for (&i, value) in group.indices.iter().zip(values) {
                for (j, a) in &self.lower[i] {
                    if !value.is_zero() {
                        pending[*j] -= a * &value;
                    }
                }
                y[self.matched_rows[i]] = value;
            }
        }
        Ok(y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rational;
    #[test]
    fn permuted_dependent_blocks_solve_both_directions_and_reject_numeric_singularity() {
        let n = 48;
        let mut matrix = vec![vec![Rational::zero(); n]; n];
        for i in 0..n {
            let row = (i * 17 + 3) % n;
            matrix[row][(i * 19 + 5) % n] = rational(2.0).unwrap();
            if i > 0 {
                matrix[row][((i - 1) * 19 + 5) % n] = rational(-0.5).unwrap();
            }
            if i % 4 == 0 {
                matrix[row][((i + 3) * 19 + 5) % n] = rational(0.25).unwrap();
            }
        }
        let control = SolveControl::default();
        let factor = BlockFactorization::try_new(&matrix, &control)
            .unwrap()
            .unwrap();
        assert_eq!(factor.groups.len(), 12);
        assert!(factor.groups.iter().all(|g| g.indices.len() == 4));
        let x: Vec<_> = (0..n)
            .map(|i| rational(i as f64 - 12.0).unwrap() / rational(3.0).unwrap())
            .collect();
        let rhs: Vec<_> = matrix
            .iter()
            .map(|r| r.iter().zip(&x).map(|(a, v)| a * v).sum())
            .collect();
        let rhs_t: Vec<_> = (0..n)
            .map(|j| matrix.iter().zip(&x).map(|(r, v)| &r[j] * v).sum())
            .collect();
        assert_eq!(factor.solve(&rhs, &control).unwrap(), x);
        assert_eq!(factor.transpose(&rhs_t, &control).unwrap(), x);
        matrix[3] = matrix[20].clone();
        assert!(BlockFactorization::try_new(&matrix, &control).is_err());
    }
}
