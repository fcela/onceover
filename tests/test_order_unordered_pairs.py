"""Order findings on pairs that have no order in either direction.

Independent columns drawn from the same small integer range
share a zero floor and touch their diagonal by accident, so a
containment reading of the pair used to certify a
part-against-whole relation, and the order pass then stated
violations for a direction whose reverse failed just as
often: on three such columns, both directions fail on about
40% of rows, and on 32 of them the empty statements filled
two thirds of the report. These tests pin the guard: a pair
is stated as a broken bound only when the reverse direction
fails at least twice as often, so columns with no order in
either direction earn no order finding at all.
"""

import numpy as np
import polars as pl

import onceover


def _order_findings(report):
    return [f for f in report.findings if f["type"].startswith("order")]


def test_three_independent_columns_have_no_order_findings():
    rng = np.random.default_rng(1)
    values = rng.integers(0, 5, size=(5000, 3))
    df = pl.DataFrame({name: values[:, k] for k, name in enumerate("abc")})
    report = onceover.profile(df)
    assert _order_findings(report) == []


def test_many_independent_columns_have_no_order_findings():
    rng = np.random.default_rng(7)
    values = rng.integers(0, 5, size=(5000, 32))
    df = pl.DataFrame(
        {f"c{k:02d}": values[:, k] for k in range(values.shape[1])}
    )
    report = onceover.profile(df)
    assert _order_findings(report) == []
