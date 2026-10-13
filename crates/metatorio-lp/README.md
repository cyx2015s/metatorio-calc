# metatorio-lp

Experimental pure Rust exact LP solver for Metatorio's continuous planning models.
This crate has no HiGHS, good_lp, native-library or C++ build dependency.

`Model::solve` starts from zero with an artificial-variable Phase I, verifies an
original-model Farkas certificate before returning infeasibility, and uses exact
primal simplex for optimization. `optimize_from_basis` accepts an existing basis.
For nonnegative objective costs, `solve_dual_with_observer` starts directly from
zero with exact dual simplex, without artificial variables or a candidate basis.
This path builds and updates its inverse from the empty structural basis.
`solve_dual_with_rule` exposes experimental leaving-row / ratio-tie strategies.
They retain exact ratio comparisons and certificates; none is assumed faster for
every model. Hitting the structural dimension cap preserves a nonterminal basis.
`repair_dual_feasible_basis` repairs primal violations in an exactly dual-feasible
basis using exact dual simplex. A rejected hint does not imply infeasibility;
ratio-test failure requires a verified Farkas certificate. Repeated bases and
execution limits return a nonterminal candidate, which may remain infeasible.
Every returned optimality claim must be confirmed by `Model::certify`; limits
preserve a nonterminal result. The observer API supports application cancellation.

Coefficients are interpreted as the exact binary rational values of the input
`f64`s. Exact solutions are retained as rational numbers; conversion to `f64` is a
separate operation and introduces rounding.

The simplex currently supports nonnegative variables without finite upper bounds,
equality rows and `<=` rows. The certificate/reconstruction APIs additionally
support finite variable bounds. Structural basis dimension is currently limited
to 256. The basis inverse uses exact row/column/border/deletion updates, with
independent reconstruction tests. Pricing uses a common integer denominator.

This is an experimental core, not the default application backend. Large-model
Phase I still needs better degeneracy handling and sparse factorization; complete
pure Rust large-model application validation remains outstanding. An opt-in
application bridge and a hybrid exact-repair bridge live in metatorio-solver. See
`docs/lp-experiments.md` in the repository root for measured results and scope.

```powershell
cargo test -p metatorio-lp
cargo clippy -p metatorio-lp --all-targets -- -D warnings
cargo tree -p metatorio-lp
```
