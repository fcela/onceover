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


def _bike_report():
    df = pl.read_csv(
        "benchmarks/corpus/data/anomaly/bike-sharing-hourly/data.csv",
        infer_schema_length=None,
        try_parse_dates=True,
    )
    return onceover.profile(df)


def test_every_band_with_findings_gets_a_line():
    # Relations alone used to fill the budget on this table, leaving the 77
    # change statements with no line at all.
    report = _bike_report()
    digest = report.to_digest(4000)
    assert len(digest) <= 4000
    assert "[changes]" in digest
    assert "[dependencies]" in digest
    assert "[concentration]" in digest or "[formats]" in digest or "[ordering]" in digest


def test_omission_line_names_what_is_missing_and_how_to_get_it():
    digest = _bike_report().to_digest(3000)
    assert "omitted to fit 3000 chars" in digest
    omission = [ln for ln in digest.splitlines() if ln.startswith("... [")][0]
    assert "report.select(category=" in omission


def test_kept_lines_stay_in_report_order():
    digest = _bike_report().to_digest(4000)
    bands = {"[uniqueness]": 0, "[missingness]": 1, "[duplicates]": 1,
             "[dependencies]": 2, "[associations]": 2, "[formats]": 3,
             "[concentration]": 3, "[changes]": 4, "[intervals]": 4}
    seen = [bands[ln.split(" ")[0]] for ln in digest.splitlines()
            if ln.split(" ")[0] in bands]
    assert seen == sorted(seen)


def test_quality_line_states_exact_counts():
    n = 400
    frame = pl.DataFrame({
        "a": [None if i % 4 == 0 else float(i) for i in range(n)],
        "empty": pl.Series([None] * n, dtype=pl.Float64),
        "same": [1] * n,
        "b": [float(i % 7) for i in range(n)],
    })
    report = onceover.profile(frame)
    line = [ln for ln in report.to_digest(4000).splitlines()
            if ln.startswith("quality:")][0]
    assert "missing values in 1 column(s), most in a (25.0%)" in line
    assert "1 empty column(s)" in line
    assert "1 constant column(s)" in line


def test_quality_line_absent_on_a_clean_frame():
    frame = pl.DataFrame({"a": [float(i) for i in range(200)],
                          "b": [float(i % 7) for i in range(200)]})
    assert "quality:" not in onceover.profile(frame).to_digest(4000)


def test_digest_keeps_body_lines_when_the_omission_line_grows():
    # On this table the omission line for the kept set is longer than for
    # the full set; the digest must still carry statements.
    import glob
    path = glob.glob("benchmarks/corpus/data/business/absenteeism-at-work/*.csv")[0]
    report = onceover.profile(pl.read_csv(path, infer_schema_length=None))
    digest = report.to_digest(4000)
    assert len(digest) <= 4000
    assert "[dependencies]" in digest and "[changes]" in digest
