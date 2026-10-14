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

`float_candidate_with_observer` searches the full model in Rust with power-of-two
scaling, floating dual simplex, a two-pass ratio test and periodic structural
refactorization. Its basis and stop reason are **unverified hints**.
`refine_basis_with_observer` reconstructs that basis exactly, repairs feasibility,
and optimizes with the original costs. Auxiliary cost shifts preserve useful
dual prices when possible; zero costs provide a fallback feasibility objective.
Only the final original-model certificate establishes optimality. The optional
`refine_basis_with_rule` selects an exact repair strategy for experiments.

`float_candidate_with_options` accepts `FloatOptions` for the floating primal
stopping threshold and Harris ratio error budget. Defaults preserve the earlier
`1e-9` search. These are hint-generation parameters, not constraint tolerances
or the exact objective-gap acceptance policy. The certified application checks
the default hint first, then tighter primal/ratio settings, before permitting
full exact optimization as a final attempt. Every hint remains subject to full
original-model verification, within one shared observer budget.

Reconstruction and uncached pivots reuse an exact LU factorization for primal,
dual and direction solves. Exact active-column counts guide column ordering to
reduce fill; row and column permutations are retained in both solve directions.
Storage remains dense. `scale_to_dual_feasible` can obtain a verified lower bound
from an approximate-basis dual witness by exact contraction toward zero, for the
nonnegative-cost model class. It never changes costs and never asserts primal
feasibility or zero objective gap. Primal and dual witnesses may come from
different bases; `certify` checks both against the complete original model.

Coefficients are interpreted as the exact binary rational values of the input
`f64`s. Exact solutions are retained as rational numbers; conversion to `f64` is a
separate operation and introduces rounding.

The simplex currently supports nonnegative variables without finite upper bounds,
equality rows and `<=` rows. The certificate/reconstruction APIs additionally
support finite variable bounds. Cached exact inverse updates remain limited to
256 structural columns. Reconstruction and uncached repair/optimization support
up to 2,048 structural columns; floating search allows up to 4,096 rows.
The independent `prove_zero_propagation_infeasible` precheck uses sparse sign
propagation on homogeneous rows to establish zero columns. It returns an
original-model Farkas certificate only after exact verification; `None` means
unknown. It does not require a dense basis and can run beyond the floating row
guard. `solve_dual_with_control` adds cooperative cancellation to cold starts.
The basis inverse uses exact row/column/border/deletion updates, with
independent reconstruction tests. Pricing uses a common integer denominator.
These are prototype resource guards, not scalability guarantees. Observer time
budgets are checked between pivots. The controlled entry points additionally
check `SolveControl` deadlines or cancellation flags during uncached exact LU
and triangular solves; large scans and individual rational operations can still
overrun an observation point. Interrupted work returns an error without claiming
infeasibility or optimality. Existing APIs retain their previous observer behavior.

This core powers the default `rust-certified` application backend. Large-model
Phase I still needs better degeneracy handling and sparse factorization; broader
planet/quality coverage and performance work remain. The application bridge and
an optional hybrid exact-repair bridge live in metatorio-solver. See
`docs/lp-experiments.md` in the repository root for measured results and scope.
The opt-in `rust-verified` backend uses Rust floating search plus exact refinement
without calling HiGHS, and accepts only a complete-model exact optimum.

`certify_candidate_with_observer` combines full-model dual reconstruction with
small-support primal repair. It accepts only exact original-model feasibility
and an exact rational comparison of the objective gap against a caller-supplied
budget. The normalized gap is `(primal - dual) / max(1, abs(primal))`. This API
requires nonnegative objective costs and nonnegative unbounded-above variables.
A positive accepted gap is reported separately from exact optimality. Keep
`result.solution` when exporting witnesses: its primal and dual can originate
from different bases. Reporting conversion rounds the gap upward.

```rust
use metatorio_lp::{CertifiedResult, Model, Rational};
use std::time::{Duration, Instant};

fn solve(model: &Model) -> Result<CertifiedResult, String> {
    let start = Instant::now();
    let alive = || start.elapsed() < Duration::from_secs(120);
    let proposal = model.float_candidate_with_observer(20_000, |_| alive())?;
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    model.certify_candidate_with_observer(&proposal, 1_000, &budget, |_, _| alive())
}
// Inspect result.accepted before using values. A stopped result is not a proof
// that the model is infeasible; its certificate retains the established facts.
```

The default application backend `rust-certified` uses this budget of `1/10^12`,
requires strict original-model primal feasibility, and checks rounded writeback
values independently. It reports `certified_relative_gap` alongside the existing
exact feasibility/optimality fields. It certifies the binary f64 coefficients
passed to the LP, rather than recovering pre-rounding game data.

```powershell
cargo test -p metatorio-lp
cargo clippy -p metatorio-lp --all-targets -- -D warnings
cargo tree -p metatorio-lp
```

Floating search also accepts a warm basis through `float_candidate_with_basis`.
Its output remains an unverified hint. Exact structural blocks of dimension 32
or larger use sparsity-ordered fraction-free integer elimination, including
transpose solves; smaller blocks retain rational LU. Neither path drops small
coefficients or changes the original-model certificate requirements. Structural
blocks first undergo exact row matching and dependency decomposition; independent
and triangular blocks are solved separately in both directions. A large basis
may therefore require only small exact factorizations. Original bases are checked
before replacing them with support hints.

`solve_column_generation_with_control` solves restricted masters and prices
missing columns exactly. A restricted Farkas ray is lifted and checked against
every original column; counterexample columns are added instead of reporting
false infeasibility. Accepted primal/dual witnesses always cover the complete
original model and the requested rational gap budget.

`float_candidate_after_zero_propagation_with_control` excludes columns that
homogeneous rows force to zero, and restores all original column/row identities
in its returned hint. Original-model dual repair and certification remain
necessary: the reduced model's duals do not cover omitted columns.
