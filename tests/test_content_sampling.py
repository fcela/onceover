"""Sampled findings follow row content, not the order rows arrive in."""

import json
import math
import re

import polars as pl

import onceover


def _frame(order):
    # 200 distinct row contents, each repeated six times: the cap binds
    # and the exact-duplicate tie-break is exercised.
    data = {name: [] for name in ("region", "x", "y", "z", "code", "label")}
    for i in order:
        k = i % 200
        x = math.sin(k / 200) * 10
        data["region"].append(["emea", "amer", "apac", "emea"][k % 4])
        data["x"].append(x)
        data["y"].append(2 * x + 1)
        data["z"].append(x * x)
        data["code"].append(k % 3)
        data["label"].append(["low", "mid", "high"][k % 3])
    return pl.DataFrame(data)


def _shuffled(n):
    order = list(range(n))
    state = 0x2545F4914F6CDD1D
    for i in range(n - 1, 0, -1):
        state ^= (state << 13) & 0xFFFFFFFFFFFFFFFF
        state ^= state >> 7
        state ^= (state << 17) & 0xFFFFFFFFFFFFFFFF
        state &= 0xFFFFFFFFFFFFFFFF
        j = state % (i + 1)
        order[i], order[j] = order[j], order[i]
    return order


def _normalize(value):
    """Drop the fields that deliberately describe original row positions."""
    if isinstance(value, list):
        return [_normalize(item) for item in value]
    if isinstance(value, dict):
        return {
            key: (
                re.sub(r"\(rows \d+-\d+\)", "", item)
                if key in ("claim", "detail") and isinstance(item, str)
                else _normalize(item)
            )
            for key, item in value.items()
            if key
            not in (
                "id",
                "grouped_members",
                "grouped_under",
                "finding_ids",
                "rows_min",
                "rows_max",
            )
        }
    return value


def _view(report):
    parsed = json.loads(report.to_json())
    # Column cards carry first-seen examples (exact positional
    # observations); groups are derived from ids that embed row ranges.
    parsed.pop("columns", None)
    parsed.pop("groups", None)
    return _normalize(parsed)


def test_sampled_profile_is_invariant_to_row_permutation():
    detectors = [
        "numeric_dependence",
        "association_rules",
        "hotspots",
        "functional_dependencies",
        "order_constraints",
        "duplicate_encodings",
        "predictability",
        "decision_tables",
        "covering_rules",
        "missingness",
        "slice_shifts",
        "row_clusters",
        "identities",
        "interactions",
        "regimes",
        "row_consistency",
    ]
    options = dict(detectors=detectors, sample_rows=400, max_findings=200)
    a = onceover.profile(_frame(list(range(1200))), **options)
    b = onceover.profile(_frame(_shuffled(1200)), **options)

    assert _view(a) == _view(b)
    assert any(
        entry["status"] in ("ran_sampled", "capped") for entry in a.coverage.values()
    )
