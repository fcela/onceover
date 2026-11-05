"""Findings recounted through Polars, independently of the detectors (#135)."""
import json
from pathlib import Path

import polars as pl
import pytest

import onceover

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "benchmarks" / "corpus"
TABLES = sorted(
    p for p in CORPUS.glob("data/*/*/*.csv")
    if p.parent.name != "gse15258-expression"
)


def _load(path):
    return pl.read_csv(path, try_parse_dates=True, infer_schema_length=20000)


@pytest.mark.parametrize("path", TABLES, ids=lambda p: p.parent.name)
def test_every_checkable_statement_agrees_with_the_data(path):
    df = _load(path)
    result = onceover.profile(df).verify(df)
    bad = result.filter(pl.col("status").is_in(["mismatch", "error"]))
    assert bad.is_empty(), bad.to_dicts()[:5]


def _frame():
    n = 2000
    a = [(i * 13) % 29 for i in range(n)]
    b = [(i * 7) % 11 for i in range(n)]
    return pl.DataFrame({
        "order_id": list(range(n)),
        "a": a,
        "b": b,
        "total": [x + y for x, y in zip(a, b)],
        "gappy": [None if i % 10 == 0 else float(i % 37) for i in range(n)],
        "region": [f"R{i % 5}" for i in range(n)],
    })


def _corrupt(report, finding_id, key, value):
    for f in report.data["findings"]:
        if f["id"] == finding_id:
            f["evidence"][0]["metrics"][key] = value


def test_a_corrupted_count_is_flagged():
    df = _frame()
    report = onceover.profile(df)
    hit = next(f for f in report.findings if f["claim"].startswith("gappy is missing"))
    _corrupt(report, hit["id"], "missing", 1)
    result = report.verify(df).filter(pl.col("id") == hit["id"])
    assert "mismatch" in result["status"].to_list()


def test_a_corrupted_identity_is_flagged():
    df = _frame()
    report = onceover.profile(df)
    hit = next(f for f in report.findings if f["claim"].startswith("total = a + b"))
    _corrupt(report, hit["id"], "coefficients", [1.0, 2.0])
    result = report.verify(df).filter(pl.col("id") == hit["id"])
    assert "mismatch" in result["status"].to_list()


def test_changed_data_fails_the_statements_it_breaks():
    df = _frame()
    report = onceover.profile(df)
    newer = df.with_columns(
        pl.when(pl.col("order_id") == 5).then(pl.lit(999)).otherwise(pl.col("total")).alias("total")
    )
    result = report.verify(newer)
    identity = next(f for f in report.findings if f["claim"].startswith("total = a + b"))
    statuses = result.filter(pl.col("id") == identity["id"])["status"].to_list()
    assert "mismatch" in statuses


def test_sampled_statements_are_marked_not_checkable():
    df = _frame()
    report = onceover.profile(df)
    result = report.verify(df)
    predictive = [f["id"] for f in report.findings if f.get("type") == "predictive"]
    if predictive:
        rows = result.filter(pl.col("id").is_in(predictive))
        assert set(rows["status"].to_list()) == {"not_checkable"}


def test_rows_returns_the_rows_behind_a_statement():
    df = _frame()
    report = onceover.profile(df)
    hit = next(f for f in report.findings if f["claim"].startswith("gappy is missing"))
    rows = report.rows(hit["id"], df)
    assert rows.height == df["gappy"].null_count()
    assert rows["__row"].to_list()[:2] == [0, 10]


def test_rows_of_an_identity_are_its_violations():
    a = [float(i % 40) for i in range(2000)]
    b = [float((i * 3) % 23) for i in range(2000)]
    total = [x + y for x, y in zip(a, b)]
    total[7] = 999.0
    df = pl.DataFrame({"a": a, "b": b, "total": total})
    report = onceover.profile(df)
    hit = next(f for f in report.findings if f["claim"].startswith("total = a + b"))
    assert report.rows(hit["id"], df)["__row"].to_list() == [7]


def test_rows_refuses_a_statement_with_no_row_set():
    df = _frame()
    report = onceover.profile(df)
    predictive = next((f for f in report.findings if f.get("type") == "predictive"), None)
    if predictive:
        with pytest.raises(ValueError):
            report.rows(predictive["id"], df)
    with pytest.raises(ValueError):
        report.rows("no-such-id", df)


def test_constraints_hold_on_the_data_and_catch_a_violation():
    df = _frame()
    checks = onceover.profile(df).to_checks()
    kinds = {c["kind"] for c in checks}
    assert {"unique", "identity"} <= kinds
    for c in checks:
        expr = eval(c["polars"], {"pl": pl})  # expressions we generated above
        assert df.select(expr).item(), c
    identity = next(c for c in checks if c["kind"] == "identity")
    broken = df.with_columns((pl.col(identity["target"]) + 1).alias(identity["target"]))
    assert not broken.select(eval(identity["polars"], {"pl": pl})).item()


def test_constraints_leave_out_statements_with_exceptions():
    a = [float(i % 40) for i in range(2000)]
    b = [float((i * 3) % 23) for i in range(2000)]
    total = [x + y for x, y in zip(a, b)]
    total[7] = 999.0
    report = onceover.profile(pl.DataFrame({"a": a, "b": b, "total": total}))
    assert not [c for c in report.to_checks() if c["kind"] == "identity"]


def test_great_expectations_suite_shape():
    suite = onceover.profile(_frame()).to_checks("great_expectations", name="orders")
    assert suite["expectation_suite_name"] == "orders"
    types = {e["expectation_type"] for e in suite["expectations"]}
    assert "expect_column_values_to_be_unique" in types
    assert suite["meta"]["unmapped_constraints"]
    json.dumps(suite)


def test_unknown_format_is_refused():
    with pytest.raises(ValueError):
        onceover.profile(_frame()).to_checks("yaml")
