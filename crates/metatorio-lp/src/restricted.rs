//! Sparse extraction of a restricted master, preserving original term order.
use crate::{Model, Row, SolveControl};

/// Compressed column incidence. Store references into the original rows rather
/// than another copy of coefficients. The index is reused across CG rounds.
pub(crate) struct RestrictedIndex<'a> {
    model: &'a Model,
    offsets: Vec<usize>,
    entries: Vec<(usize, usize)>,
}

impl<'a> RestrictedIndex<'a> {
    /// Caller validates the model before constructing the index.
    pub(crate) fn new(model: &'a Model, control: &SolveControl) -> Result<Self, String> {
        control.check()?;
        let mut offsets = vec![0; model.columns.len() + 1];
        for row in &model.rows {
            control.check()?;
            for (j, _) in &row.terms {
                offsets[j + 1] += 1;
            }
        }
        for j in 1..offsets.len() {
            offsets[j] += offsets[j - 1];
        }
        let mut entries = vec![(0, 0); *offsets.last().unwrap()];
        let mut next = offsets[..model.columns.len()].to_vec();
        for (i, row) in model.rows.iter().enumerate() {
            control.check()?;
            for (p, (j, _)) in row.terms.iter().enumerate() {
                entries[next[*j]] = (i, p);
                next[*j] += 1;
            }
        }
        Ok(Self {
            model,
            offsets,
            entries,
        })
    }

    pub(crate) fn build(
        &self,
        selected: &[usize],
        control: &SolveControl,
    ) -> Result<(Model, Vec<usize>), String> {
        control.check()?;
        let terms: usize = selected
            .iter()
            .map(|j| self.offsets[j + 1] - self.offsets[*j])
            .sum();
        // At high density a sequential row scan avoids per-row sorting and
        // indirect access. This changes the extraction strategy, never its set.
        if terms >= self.entries.len() / 4 {
            return Self::by_rows(self.model, selected, control);
        }
        let mut per_row = vec![Vec::new(); self.model.rows.len()];
        for (new, &old) in selected.iter().enumerate() {
            control.check()?;
            for &(row, term) in &self.entries[self.offsets[old]..self.offsets[old + 1]] {
                per_row[row].push((term, new));
            }
        }
        let mut rows = Vec::new();
        let mut originals = Vec::new();
        for (i, mut terms) in per_row.into_iter().enumerate() {
            control.check()?;
            let original = &self.model.rows[i];
            if original.rhs == 0.0 && !terms.iter().any(|(p, _)| original.terms[*p].1 != 0.0) {
                continue;
            }
            terms.sort_unstable_by_key(|(p, _)| *p);
            rows.push(Row {
                equality: original.equality,
                rhs: original.rhs,
                terms: terms
                    .into_iter()
                    .map(|(p, j)| (j, original.terms[p].1))
                    .collect(),
            });
            originals.push(i);
        }
        Ok((
            Model {
                columns: selected
                    .iter()
                    .map(|j| self.model.columns[*j].clone())
                    .collect(),
                rows,
                offset: self.model.offset,
            },
            originals,
        ))
    }

    pub(crate) fn by_rows(
        model: &Model,
        selected: &[usize],
        control: &SolveControl,
    ) -> Result<(Model, Vec<usize>), String> {
        control.check()?;
        let mut positions = vec![usize::MAX; model.columns.len()];
        for (new, &old) in selected.iter().enumerate() {
            positions[old] = new;
        }
        let mut rows = Vec::new();
        let mut originals = Vec::new();
        for (i, original) in model.rows.iter().enumerate() {
            control.check()?;
            if original.rhs == 0.0
                && (selected.is_empty()
                    || !original
                        .terms
                        .iter()
                        .any(|(j, a)| positions[*j] != usize::MAX && *a != 0.0))
            {
                continue;
            }
            rows.push(Row {
                equality: original.equality,
                rhs: original.rhs,
                terms: original
                    .terms
                    .iter()
                    .filter_map(|(j, a)| {
                        (positions[*j] != usize::MAX).then_some((positions[*j], *a))
                    })
                    .collect(),
            });
            originals.push(i);
        }
        Ok((
            Model {
                columns: selected.iter().map(|j| model.columns[*j].clone()).collect(),
                rows,
                offset: model.offset,
            },
            originals,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Column;
    use std::{hint::black_box, time::Instant};

    fn model(n: usize, m: usize) -> Model {
        let mut rows: Vec<_> = (0..m)
            .map(|i| Row {
                equality: i % 2 == 0,
                rhs: if i == 0 { 1.0 } else { 0.0 },
                terms: Vec::new(),
            })
            .collect();
        for j in (0..n).rev() {
            for k in 0..4 {
                rows[(j * 13 + k * 37) % m]
                    .terms
                    .push((j, (k as f64) - 1.0));
            }
        }
        Model {
            columns: vec![
                Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 1.0
                };
                n
            ],
            rows,
            offset: 3.0,
        }
    }

    #[test]
    fn sparse_extraction_preserves_duplicate_order_signed_zero_and_forced_rows() {
        let mut model = model(128, 16);
        model.rows[0]
            .terms
            .extend([(0, 1e200), (0, -1e200), (0, -0.0)]);
        model.rows.push(Row {
            equality: true,
            rhs: 2.0,
            terms: vec![],
        });
        model.rows.push(Row {
            equality: true,
            rhs: 0.0,
            terms: vec![(0, -0.0)],
        });
        let control = SolveControl::default();
        let index = RestrictedIndex::new(&model, &control).unwrap();
        let mut selections: Vec<Vec<_>> = (0..32)
            .map(|mask| (0..5).rev().filter(|j| mask & (1 << j) != 0).collect())
            .collect();
        selections.push((0..128).collect()); // Exercise dense fallback as well.
        for selected in selections {
            let (sparse, rows) = index.build(&selected, &control).unwrap();
            let (reference, expected_rows) =
                RestrictedIndex::by_rows(&model, &selected, &control).unwrap();
            assert_eq!(rows, expected_rows);
            assert_eq!(sparse.columns.len(), selected.len());
            assert_eq!(sparse.offset.to_bits(), reference.offset.to_bits());
            for (a, b) in sparse.rows.iter().zip(&reference.rows) {
                assert_eq!(a.equality, b.equality);
                assert_eq!(a.rhs.to_bits(), b.rhs.to_bits());
                assert_eq!(a.terms.len(), b.terms.len());
                for ((aj, av), (bj, bv)) in a.terms.iter().zip(&b.terms) {
                    assert_eq!(aj, bj);
                    assert_eq!(av.to_bits(), bv.to_bits());
                }
            }
        }
        let cancelled = SolveControl::with_deadline(Instant::now());
        assert!(RestrictedIndex::new(&model, &cancelled).is_err());
        assert!(index.build(&[0], &cancelled).is_err());
        assert!(RestrictedIndex::by_rows(&model, &[], &cancelled).is_err());
    }

    #[test]
    #[ignore = "small release microbenchmark; run explicitly, not a performance assertion"]
    fn small_sparse_extraction_benchmark() {
        let model = model(16_384, 512);
        let control = SolveControl::default();
        let began = Instant::now();
        let index = RestrictedIndex::new(&model, &control).unwrap();
        let setup = began.elapsed();
        for count in [32, 256, 8192] {
            let selected: Vec<_> = (0..count)
                .map(|j| (j * 7919) % model.columns.len())
                .collect();
            let mut sparse = Vec::new();
            let mut scan = Vec::new();
            for run in 0..7 {
                for method in [run % 2, 1 - run % 2] {
                    let began = Instant::now();
                    for _ in 0..20 {
                        black_box(if method == 0 {
                            index.build(black_box(&selected), &control).unwrap()
                        } else {
                            RestrictedIndex::by_rows(&model, black_box(&selected), &control)
                                .unwrap()
                        });
                    }
                    if method == 0 {
                        sparse.push(began.elapsed());
                    } else {
                        scan.push(began.elapsed());
                    }
                }
            }
            sparse.sort();
            scan.sort();
            eprintln!(
                "[SPARSE-EXTRACT-BENCH] n=16384 m=512 nnz=65536 selected={count} iterations=20 index_setup_us={} indexed_median_us={} scan_median_us={}",
                setup.as_micros(),
                sparse[3].as_micros(),
                scan[3].as_micros()
            );
        }
    }
}
