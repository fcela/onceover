"""Component registry.

A component is a module with:

    NAME         short id, the key in the scorecard
    DESCRIPTION  one sentence
    METRICS      {metric name: {"direction": "lower" | "higher" | "info",
                                "kind": "count" | "time",
                                "zero": optional bool}}
                 "lower"/"higher" is the better direction; "info" metrics
                 are recorded and shown but never fail the comparison.
                 "kind": "count" metrics have zero tolerance; "time"
                 metrics are compared on totals with a relative
                 tolerance. "zero": true means any positive value in the
                 head scorecard fails, whatever the base holds.
    run(ctx)     returns {"metrics": {...}, "detail": {...}} for one
                 dataset, or {"status": "not_applicable", "reason": "..."}

Adding a component is one module plus one line in COMPONENTS.
"""
from . import (cost, invariance, permuted, planted, redundancy, reference,
               truth)

COMPONENTS = [
    planted,
    permuted,
    invariance,
    reference,
    redundancy,
    truth,
    cost,
]

REGISTRY = {m.NAME: m for m in COMPONENTS}
