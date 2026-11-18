"""Tier wiring for the screen's model-row cap (issue #363).

Measured in docs/benchmarks/2026-11-17-sample-caps.md: at 5,000
fitting rows the screen reaches only about half the attainable
holdout R2 for a driver confined to 1% of rows, and nearly none
of it at 0.5%; 50,000 fitting rows reach the knee of the curve.
Standard fits on up to 50,000 rows per target and deep on up to
200,000. The screen does not run at the quick tier at all.
"""

import polars as pl

import onceover


def _frame(n=400):
    xs = [float(i % 37) for i in range(n)]
    return pl.DataFrame(
        {"x": xs, "y": [2.0 * v + float(i % 3) for i, v in enumerate(xs)]}
    )


def _screen_entry(budget):
    report = onceover.profile(_frame(), budget=budget)
    return report.coverage["predictability_screen"]


def test_screen_fitting_rows_by_tier():
    assert _screen_entry("quick")["status"] == "skipped"
    assert "predictors / 50000 fitting rows" in _screen_entry("standard")["detail"]
    assert "predictors / 200000 fitting rows" in _screen_entry("deep")["detail"]
