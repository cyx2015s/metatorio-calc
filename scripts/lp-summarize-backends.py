"""Audit frozen input identity and summarize paired application backend runs."""
import json
import statistics
import sys
from pathlib import Path

root = Path(sys.argv[1])
results = json.loads((root / "results.json").read_text(encoding="utf-8-sig"))
manifest = {c["name"]: c for c in json.loads((root / "manifest.json").read_text(encoding="utf-8-sig"))}

def numeric_model(model):
    # Labels and term iteration order do not affect the LP; coefficient bits do.
    return (
        [(c["lower_bits"], c["upper_bits"], c["integer"]) for c in model["columns"]],
        sorted(model["objective"]), model["objective_offset_bits"],
        [(r["equality"], r["rhs_bits"], sorted(r["terms"])) for r in model["rows"]],
    )

audited = []
for row in results:
    result_path = Path(row["result_path"])
    result = json.loads(result_path.read_text(encoding="utf-8-sig"))
    source = json.loads(Path(manifest[row["name"]]["snapshot"]).read_text(encoding="utf-8-sig"))
    captured = list((result_path.parent / "capture").glob("lp-*.json"))
    if len(captured) != 1:
        raise ValueError(f"Expected exactly one replayed model: {result_path}")
    replayed = json.loads(captured[0].read_text(encoding="utf-8-sig"))
    if numeric_model(source) != numeric_model(replayed):
        raise ValueError(f"Replay changed numerical input: {result_path}")
    validation = result.get("validation", {})
    accepted = (result["status"] in ("solved", "validation_failed")
                and validation.get("finite") is True
                and validation["maximum_relative_violation"] == 0
                and validation["maximum_bound_violation"] <= 1e-8)
    verification = result_path.parent / "witness-verification.json"
    if row["backend"] == "rust-certified" and accepted:
        if not verification.exists():
            raise ValueError(f"Missing independent witness verification: {result_path}")
        proof = json.loads(verification.read_text(encoding="utf-8-sig"))
        if not (proof["verified_models"] == 1 and proof["all_primal_and_dual_feasible"]
                and proof["all_rounded_checks_passed"]):
            raise ValueError(f"Incomplete independent witness verification: {result_path}")
    audited.append({**row, "application_accepted": accepted, "input_bits_identical": True,
                    "strict_bounds_met": validation.get("maximum_bound_violation") == 0})

summary = {"models": len(manifest), "runs": len(audited), "input_identity_checks": len(audited),
           "bound_tolerance": 1e-8, "backends": {}, "cases": []}
for backend in ("highs", "rust-certified"):
    rows = [r for r in audited if r["backend"] == backend]
    summary["backends"][backend] = {"runs": len(rows),
        "application_accepted": sum(r["application_accepted"] for r in rows),
        "strict_bounds_met": sum(r["strict_bounds_met"] for r in rows),
        "exact_optimality_verified": sum(bool((r.get("report") or {}).get("exact_optimality_verified")) for r in rows),
        "statuses": {s: sum(r["status"] == s for r in rows) for s in sorted({r["status"] for r in rows})}}
for name in manifest:
    case = {"name": name}
    for backend in ("highs", "rust-certified"):
        rows = [r for r in audited if r["name"] == name and r["backend"] == backend]
        times = [r["solve_seconds"] for r in rows if r["application_accepted"]]
        case[backend] = {"accepted": sum(r["application_accepted"] for r in rows),
                         "median_accepted_seconds": statistics.median(times) if times else None,
                         "min_accepted_seconds": min(times) if times else None,
                         "max_accepted_seconds": max(times) if times else None}
    summary["cases"].append(case)
paired_cost_errors = []
for name in manifest:
    for repeat in sorted({r["repeat"] for r in audited}):
        pair = [r for r in audited if r["name"] == name and r["repeat"] == repeat and r["application_accepted"]]
        if len(pair) == 2:
            paired_cost_errors.append(abs(pair[0]["cost"] - pair[1]["cost"]) / max(1, abs(pair[0]["cost"]), abs(pair[1]["cost"])))
summary["common_accepted_pairs"] = len(paired_cost_errors)
summary["maximum_common_relative_cost_difference"] = max(paired_cost_errors, default=None)
(root / "audited-results.json").write_text(json.dumps(audited, indent=2), encoding="utf-8")
(root / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
print(json.dumps(summary, indent=2))
