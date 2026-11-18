"""Reproduce structural detection probes; run against a freshly built package.

Usage: python scripts/audit_structure.py > /tmp/onceover-structure-audit.json
This records observations, not assertions about capabilities.
"""
import json
import random
import time
from decimal import Decimal

import polars as pl
import onceover


def probe(name, df, **kwargs):
    started = time.perf_counter()
    report = onceover.profile(df, max_findings=1000, **kwargs)
    return {
        "case": name,
        "seconds": time.perf_counter() - started,
        "shape": df.shape,
        "options": kwargs,
        "coverage": report.coverage,
        "findings": report.findings,
        "columns": report.columns,
        "grouped_findings": json.loads(report.to_json()).get("grouped_findings", []),
    }


def main():
    rng = random.Random(1729)
    n = 6000
    x = [rng.gauss(0, 1) for _ in range(n)]
    cases = []
    for budget in ("standard", "deep"):
        for name, df in [
            ("identical_gaussian", pl.DataFrame({"x": x, "y": x})),
            ("affine_gaussian", pl.DataFrame({"x": x, "y": [3*v+7 for v in x]})),
            ("identical_binary", pl.DataFrame({"x": [i % 2 for i in range(n)], "y": [i % 2 for i in range(n)]})),
            ("independent_gaussian", pl.DataFrame({"x": x, "y": [rng.gauss(0, 1) for _ in range(n)]})),
            ("independent_binary", pl.DataFrame({"x": [i % 2 for i in range(n)], "y": [(i // 2) % 2 for i in range(n)]})),
        ]:
            cases.append(probe(name, df, budget=budget, detectors=["row_clusters"]))
    for detectors in (None, ["row_clusters"], ["functional_dependencies"], ["identities"]):
        cases.append(probe("identical_gaussian_detector_comparison", pl.DataFrame({"x": x, "y": x}), detectors=detectors))
    for size in (50, 6000):
        cases.append(probe("search_row_override", pl.DataFrame({"x": x, "y": x}), sample_rows=size, detectors=["row_clusters"]))
    # Only the spelling of categories changes; the partition is identical.
    for labels in (["a", "b", "c", "d"], ["a", "c", "b", "d"]):
        cases.append(probe("category_relabeling", pl.DataFrame({"x": [labels[i % 4] for i in range(n)], "y": ["lo" if i % 4 < 2 else "hi" for i in range(n)]}), detectors=["row_clusters"]))
    for offset in (0, 2**60):
        cases.append(probe("integer_precision", pl.DataFrame({"x": [offset + i % 4 for i in range(n)], "y": [offset + i % 4 for i in range(n)]}), detectors=["row_clusters"]))
    # Independent copies of the complete Cartesian product are an exact
    # negative control, with zero empirical mutual information.
    for kind in ("string", "integer"):
        data = {f"c{k}": [(i // 2**k) % 2 for i in range(4096)] for k in range(6)}
        if kind == "string":
            data = {k: [str(v) for v in values] for k, values in data.items()}
        cases.append(probe("independent_cartesian_" + kind, pl.DataFrame(data), detectors=["row_clusters"]))
    for budget in ("standard", "deep"):
        cases.append(probe("default_trivial_structure", pl.DataFrame({
            "x": x, "copy": x, "affine": [3*v+7 for v in x],
            "sum": [4*v+7 for v in x], "constant": [42] * n,
            "binary": [i % 2 for i in range(n)],
            "label": ["yes" if i % 2 else "no" for i in range(n)],
        }), budget=budget))
    late = [None] * 1000 + x[1000:]
    for wide in (False, True):
        data = {f"constant_{i}": [float(i)] * n for i in range(64)} if wide else {}
        data.update({"late_x": late, "late_y": late})
        cases.append(probe("leading_null_dependent_pair_wide_" + str(wide), pl.DataFrame(data), detectors=["row_clusters"]))
    cases.append(probe("decimal_exact_integrity", pl.DataFrame({"x": [Decimal(2**60 + i % 4) for i in range(100)]}), budget="quick"))
    cases.append(probe("merge_attempt_cap", pl.DataFrame({f"x{k}": [rng.gauss(0, 1) for _ in range(600)] for k in range(30)}), detectors=["row_clusters"]))
    cases.append(probe("pooled_category_not_real_value", pl.DataFrame({"x": [f"level_{i % 64:02}" for i in range(6400)]}), detectors=["row_clusters"]))
    cases.append(probe("modal_share_missing_denominator", pl.DataFrame({"x": [1] * 100 + [None] * 900}), detectors=["row_clusters"]))
    sparse = pl.DataFrame({"key": [0, 2**60]})
    many = onceover.profile_many({"child": sparse, "parent": sparse.rename({"key": "parent_key"})})
    decimal_frame = pl.DataFrame({"x": [Decimal(2**60 + i % 4) for i in range(100)]})
    decimal_report = onceover.profile(decimal_frame, budget="quick")
    print(json.dumps({"package": onceover.__file__, "version": onceover.__version__, "cases": cases,
                      "sparse_integer_foreign_key": json.loads(many.to_json()),
                      "decimal_verification": decimal_report.verify(decimal_frame).to_dicts(),
                      "decimal_ground_truth": {
                          "distinct": decimal_frame["x"].n_unique(),
                          "redundant_rows": decimal_frame.height - decimal_frame.unique().height,
                          "non_decreasing": decimal_frame["x"].is_sorted(),
                      },
                      "expected_sparse_chance": 2 / ((2**60 + 1) * 2**60)}, indent=2))


if __name__ == "__main__":
    main()
