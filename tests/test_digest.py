"""Context-budgeted digest (issue #35): report.to_digest(chars)
promotes the corpus prototype (benchmarks/corpus/digest.py) to a
library surface."""
import polars as pl

import onceover


def thyroid_report():
    df = pl.read_csv(
        "benchmarks/corpus/data/gritbot/thyroid/data.csv",
        infer_schema_length=None,
    )
    return onceover.profile(df)


def test_digest_within_budget_and_names_omissions():
    report = thyroid_report()
    digest = report.to_digest(4000)
    assert len(digest) <= 4000
    assert digest.startswith("profile digest: 3772 rows x")
    small = report.to_digest(600)
    assert len(small) <= 600
    assert "omitted to fit 600 chars" in small


def test_digest_keeps_top_findings_first():
    report = thyroid_report()
    digest = report.to_digest(100_000)
    # The full ledger fits: no omission line, findings rolled up by
    # category/type/form/target, strongest group first.
    assert "omitted" not in digest
    assert "findings by category:" in digest


def test_digest_tiny_budget_truncates_header():
    df = pl.DataFrame({"x": [float(i % 40) for i in range(200)]})
    report = onceover.profile(df)
    digest = report.to_digest(80)
    assert len(digest) <= 80
    assert digest.startswith("profile digest:")


def test_digest_rollup_marker_for_repeated_group():
    # Soybean's conditional rules repeat category/type/form/target;
    # the digest rolls them into one (xN) line.
    df = pl.read_csv(
        "benchmarks/corpus/data/gritbot/soybean/data.csv",
        infer_schema_length=None,
    )
    digest = onceover.profile(df).to_digest(100_000)
    assert " (x" in digest
