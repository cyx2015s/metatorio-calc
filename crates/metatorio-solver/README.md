# Metatorio continuous LP solver

Production planning uses backend-independent variables, sparse expressions and
constraints from `lp_model`. The default build selects HiGHS. The optional
`rust-lp` feature selects the specialized Rust floating search and exact rational
certification path, without a native LP dependency when defaults are disabled.

```powershell
cargo build --release -p metatorio-headless --no-default-features --features rust-lp
cargo test -p metatorio-solver --no-default-features --features rust-lp --lib
cargo tree -p metatorio-headless --no-default-features --features rust-lp --edges normal,build
# GUI shell supports the same backend feature selection.
cargo check -p metatorio-app --no-default-features --features rust-lp
```

With only `rust-lp` enabled, the application defaults to `rust-certified` without
an environment variable. If both `highs` and `rust-lp` are enabled, HiGHS remains
the default and `METATORIO_LP_BACKEND=rust-certified` selects Rust. Explicitly
requesting a native backend in a Rust-only build returns an unavailable-backend
error. `lp-experiments` enables both backends for comparison.

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

The application integration still uses a 120-second observer budget. Individual
exact factorizations are not interruptible mid-operation. Dense factor storage
and dimension limits remain, so passing the current corpus is not a guarantee
for every asset or arbitrary LP size.
Epic electronic-circuit targets of `1e-6` and `1` with eight shared beacons are
regular regressions. Tighter search produces exact optima on frozen former
timeout inputs, but can also produce singular hints on other models. The
certified application therefore checks the default hint, then primal/ratio
settings of `1e-12`/`1e-12` and `1e-12`/`1e-14`, before permitting full exact
optimization with the default search as a final attempt. Earlier attempts
retain support correction and full-model certification. All share the same
120-second observer budget and exact acceptance policy. Each preliminary
attempt has a 30-second observer budget to reserve time for subsequent hints;
individual factorizations can overrun an observation point. Verified infeasibility
terminates immediately. Retry hints are captured in `rust-retries/<index>/`,
and attempt witnesses in `rust-attempt-certificates/<index>/`; only the selected
attempt is published in `rust-certificates/` for independent verification.
See `docs/lp-experiments.md` for evidence and remaining coverage limits.

`lp_verify_witnesses capture-directory` requires every original model to have
an accepted feasible witness within the exact gap budget. The explicit
`--allow-infeasible` option additionally rechecks saved Farkas multipliers
against the original model; it is intended for mixed outcome surveys and is
not used by the successful-planning gate in `lp-experiment.ps1`.

Full actual planning regression with automatic witness verification:

```powershell
./scripts/lp-experiment.ps1 -Backend rust-certified -NativeFree -PipelineOnly -RequireSolved
```

This command checks that the selected dependency graph contains no HiGHS,
highs-sys or good_lp, saves each original LP and exact witnesses, and independently
recomputes their certificates after the requests complete. Use a new capture
directory for each run. Native replay remains available through `lp-experiments`.
