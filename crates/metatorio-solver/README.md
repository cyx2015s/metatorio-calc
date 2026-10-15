# Metatorio continuous LP solver

Production planning uses backend-independent variables, sparse expressions and
constraints from `lp_model`. The default build enables `rust-lp` and selects
`rust-certified`: specialized Rust floating search and exact rational
certification, without a native LP dependency. HiGHS is an optional feature.

```powershell
cargo build --release -p metatorio-headless
cargo test -p metatorio-solver --lib
cargo tree -p metatorio-headless --edges normal,build
# GUI shell supports the same backend feature selection.
cargo check -p metatorio-app
# Optional native-only build selects HiGHS by default.
cargo build --release -p metatorio-headless --no-default-features --features highs
```

With only `rust-lp` enabled, the application defaults to `rust-certified` without
an environment variable. If both `highs` and `rust-lp` are enabled, Rust remains
the default and `METATORIO_LP_BACKEND=highs` selects HiGHS. Explicitly
requesting a native backend in a Rust-only build returns an unavailable-backend
error. Unknown backend names also return an error. `lp-experiments` enables both
backends for comparison.

Workspace development builds optimize the LP core, solver adapter and num
arithmetic libraries at level 3; other application code retains its development
profile. This keeps desktop development builds usable with the Rust backend.

The Rust certified path requires nonnegative costs and nonnegative variables
without finite upper bounds, matching the current production-planning model.
It reconstructs exact rational primal and dual witnesses for the original f64
input. Every original constraint must be exactly satisfied. Acceptance compares
the normalized objective gap `(primal-dual)/max(1,abs(primal))` against the exact
rational budget `1/10^12`. A positive gap is recorded separately from exact
optimality. Values converted back to f64 must also pass application validation
and actual factory writeback checks. Numerical failure or a work limit is not
an infeasibility claim; only a complete-model proof produces that status.

These witnesses certify the binary floating coefficients received by the LP;
they do not recover the game's pre-rounding rational recipe data. The core
`metatorio-lp` supports additional exact LP operations and has its own README.

The certified/verified application search uses a shared 1,200-second cooperative
work budget, independent of the MCP response. Async auto_plan returns `running`
immediately and its background scheduler waits up to 1,800 seconds per solve by
default; ordinary synchronous recompute retains its 120-second default.
`METATORIO_SOLVE_TIMEOUT_MS` or `--solve-timeout-ms` explicitly overrides scheduler
waiting. Waiting limits are separate from LP work and do not prove infeasibility.
Uncached exact LU elimination and triangular solves check deadlines during work.
Some model scans and individual rational operations remain noninterruptible;
this is not a hard wall-clock timeout. Dense factor storage
and dimension limits remain, so passing the current corpus is not a guarantee
for every asset or arbitrary LP size.
Epic electronic-circuit targets of `1e-6` and `1` with eight shared beacons are
regular regressions. Tighter search produces exact optima on frozen former
timeout inputs, but can also produce singular hints on other models. The
certified application therefore checks the default hint, then primal/ratio
settings of `1e-12`/`1e-12` and `1e-12`/`1e-14`, before permitting full exact
optimization with the default search as a final attempt. Earlier attempts
retain full-model certification; preliminary drifted or stopped hints skip
support correction. All share the same work budget and exact acceptance policy.
The first preliminary attempt has 45 seconds, later attempts have 90 seconds,
and preliminary deadlines stop at 1,140 seconds to reserve final repair time.
Observers may veto support/full repair before construction starts. Verified infeasibility
terminates immediately. Retry hints are captured in `rust-retries/<index>/`,
and attempt witnesses in `rust-attempt-certificates/<index>/`; only the selected
attempt is published in `rust-certificates/` for independent verification.

An exact sparse zero-propagation precheck can prove missing-supply contradictions.
After the three preliminary certified hints remain incomplete, exact column generation can recover a small
master problem. Only a fully validated floating primal vector seeds it; otherwise
it starts from demand rows and exact missing-supplier rays. It uses at most 900
seconds of the shared budget, reserving final repair time. Restricted failures
are lifted and checked against every original column before reporting infeasibility.
If column recovery remains incomplete, constraint generation tries a relaxation that
starts with demand rows, then adds exactly violated original rows. Low-degree
rows are preferred to dense shared-resource rows; after 32 rounds the frontier
widens. It keeps every supplier occurring in selected rows and restores omitted
zero columns when new rows need them. Singular relaxation hints can restart from
the exact all-slack basis. Recovery validates and reuses retained exact bases;
20-second progress stops preserve completed pivots. Before exact warm repair,
floating search can continue from the retained basis; only its independently
certified result replaces the retained state. A certified primal basis may
have an independent dual, so an invalid warm hint falls back to another proposal
or cold start. A full feasible primal basis can continue exact original-cost
optimization to close an excessive gap without repeating Phase I. Recovery has
at most 256 rounds and uses the remaining shared budget minus 60 seconds reserved
for final repair, consuming time otherwise available to later hints. Each active floating model remains limited to 4,096
rows; larger original models can succeed through smaller relaxations. Accepted
primal/dual witnesses and lifted Farkas multipliers always undergo complete
original-model checks. Recovery witnesses are published directly in
`rust-certificates/`; failed relaxation snapshots, when capture is enabled, live
in `rust-row-relaxations/`.
See `docs/lp-experiments.md` and `docs/lp-specialty-smoke-fix.md` for evidence
and remaining coverage limits.

`lp_verify_witnesses capture-directory` requires every original model to have
an accepted feasible witness within the exact gap budget. The explicit
`--allow-infeasible` option additionally rechecks saved Farkas multipliers
against the original model; it is intended for mixed outcome surveys and is
not used by the successful-planning gate in `lp-experiment.ps1`.

Full actual planning regression with automatic witness verification:

```powershell
./scripts/lp-experiment.ps1 -Backend rust-certified -NativeFree -PipelineOnly -RequireSolved -UseDefaultBackend
```

This command checks that the selected dependency graph contains no HiGHS,
highs-sys or good_lp, saves each original LP and exact witnesses, and independently
recomputes their certificates after the requests complete. Use a new capture
directory for each run. `UseDefaultBackend` leaves the backend environment unset
and uses default Cargo features. Native replay remains available through
`lp-experiments`. Paired frozen-model comparison uses `lp_application_replay`,
`scripts/lp-compare-backends.ps1` and `scripts/lp-summarize-backends.py`. Summaries
audit numerical input bits and distinguish strict bounds from the application
bound tolerance; rejected solves do not count as faster successful solves.

The optional experiment `METATORIO_LP_COST_BOUNDS=1` tries an incumbent-budget
hint only after the three original hints fail and a full exact feasible incumbent
has been retained. It shares the overall deadline with a cooperative 15-second
allowance, and accepts only an original-model certificate. It is disabled by
default pending paired measurements. `lp_cost_bounds` compares floating hints
on a frozen snapshot using an independently checked incumbent; see
`docs/lp-ammonia-cost-bounds-20261015.md` for the application case and controls.
