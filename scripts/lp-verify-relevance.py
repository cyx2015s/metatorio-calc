"""Independently verify a relevance mapping using exact binary-rational signs.

Usage: python scripts/lp-verify-relevance.py snapshot.json proposal.json
This checks projection (with optional free supply) and zero-dual extension, not a solver's
status or a tolerance-based assertion about its returned primal vector.
"""
import json
import math
import struct
import sys
from fractions import Fraction


def value(bits):
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def row_signs(terms):
    coefficients = {}
    for column, bits in terms:
        a = value(bits)
        if column in coefficients:
            coefficients[column] = Fraction(coefficients[column]) + Fraction(a)
        else:
            coefficients[column] = a
    return ((j, (a > 0) - (a < 0)) for j, a in coefficients.items() if a != 0)


def verify(snapshot, mapping):
    columns = set(mapping["columns"])
    rows = set(mapping["rows"])
    assert len(columns) == len(mapping["columns"])
    assert len(rows) == len(mapping["rows"])
    assert all(0 <= j < len(snapshot["columns"]) for j in columns)
    assert all(0 <= i < len(snapshot["rows"]) for i in rows)
    for column in snapshot["columns"]:
        assert value(column["lower_bits"]) == 0
        assert value(column["upper_bits"]) == math.inf
    costs = [Fraction(0) for _ in snapshot["columns"]]
    for j, bits in snapshot["objective"]:
        costs[j] += Fraction(value(bits))
    assert all(c >= 0 for c in costs)
    free = set(mapping.get("free_suppliers", []))
    assert free <= columns
    assert all(costs[j] == 0 for j in free)
    free_rows = set()
    signs = [list(row_signs(row["terms"])) for row in snapshot["rows"]]
    touched_free = set()
    for i, row in enumerate(snapshot["rows"]):
        for j, sign in signs[i]:
            if j in free:
                assert sign < 0 and not row["equality"], ("not a pure free supplier", i, j)
                free_rows.add(i)
                touched_free.add(j)
    assert touched_free == free
    checked = 0
    for i, row in enumerate(snapshot["rows"]):
        rhs = value(row["rhs_bits"])
        if i not in rows:
            assert rhs == 0 if row["equality"] else rhs >= 0, ("omitted rhs", i)
        for j, sign in signs[i]:
            if i in rows and j not in columns:
                assert sign == 0 if row["equality"] else sign >= 0 or i in free_rows, ("omitted-column dual extension", i, j)
            elif i not in rows and j in columns:
                assert sign == 0 if row["equality"] else sign <= 0, ("omitted-row primal extension", i, j)
            checked += 1
    return {
        "projection_with_free_supply_preserves_feasibility": True,
        "nonnegative_cost_cannot_increase": True,
        "zero_extended_duals_cover_original_columns": True,
        "columns_before": len(snapshot["columns"]), "columns_after": len(columns),
        "rows_before": len(snapshot["rows"]), "rows_after": len(rows),
        "checked_aggregated_coefficients": checked,
        "retained_pure_free_suppliers": len(free),
    }


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    with open(sys.argv[1], encoding="utf-8") as source:
        snapshot = json.load(source)
    with open(sys.argv[2], encoding="utf-8") as source:
        proposal = json.load(source)
    print(json.dumps(verify(snapshot, proposal["relevance"])))
