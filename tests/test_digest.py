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


def test_every_insight_class_with_findings_gets_a_line():
    # Relations alone used to fill the budget on this table, leaving the 77
    # change statements with no line at all. Under the statement-class
    # order (issue #271) the round-robin runs across the two insight
    # classes, so structure and shape both get lines; hygiene lines
    # are no longer guaranteed a line — they come last by design.
    report = _bike_report()
    digest = report.to_digest(4000)
    assert len(digest) <= 4000
    assert "[changes]" in digest
    assert "[dependencies]" in digest


def test_omission_line_names_what_is_missing_and_how_to_get_it():
    digest = _bike_report().to_digest(3000)
    assert "omitted to fit 3000 chars" in digest
    omission = [ln for ln in digest.splitlines() if ln.startswith("... [")][0]
    assert "report.select(category=" in omission


def test_kept_lines_stay_in_class_order():
    # Digest lines follow the statement-class order (issue #271):
    # account, structure, shape, hygiene — deliberately not the
    # findings array's report-band order. Class is a property of
    # the line's head finding, not of its category alone (a
    # segment-conditioned change statement is structure).
    report = _bike_report()
    digest = report.to_digest(4000)
    by_claim = {f["claim"]: f for f in report.data["findings"]}
    seen = []
    for ln in digest.splitlines():
        if not ln.startswith("["):
            continue
        text = ln.split(": ", 1)[1] if ": " in ln else ""
        head = by_claim.get(text.split(" [")[0])
        if head is not None:
            seen.append(onceover._statement_class(head))
    assert seen
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


# --- statement-class ordering (issue #271) ---


def _finding(category, typ, score, claim=None, scope="column",
             conditionality="unconditional"):
    return {
        "id": f"{category}-{typ}-{scope}-{conditionality}",
        "category": category,
        "type": typ,
        "specification": {},
        "subject": {},
        "claim": claim or f"{category} {typ} statement",
        "interestingness": score,
        "facets": {"scope": scope, "conditionality": conditionality},
        "evidence": [],
    }


def _report(findings):
    report = onceover.ProfileReport.__new__(onceover.ProfileReport)
    report._native = None
    report._data = {
        "schema_version": "0.7",
        "overview": {"n_rows": 100, "n_columns": 4},
        "findings": findings,
        "coverage": {},
        "columns": {},
        "selection": {},
    }
    return report


def test_statement_class_mirror_matches_the_ledger_rule():
    f = _finding
    assert onceover._statement_class(f("missingness", "missing_values", 0.5, scope="frame")) == 0
    assert onceover._statement_class(f("uniqueness", "unique_key", 0.5)) == 0
    assert onceover._statement_class(f("intervals", "duplicate_encoding", 0.5)) == 1
    assert onceover._statement_class(f("dependencies", "functional", 0.5)) == 1
    assert onceover._statement_class(
        f("missingness", "missing_values", 0.5, conditionality="value_conditioned")
    ) == 1
    assert onceover._statement_class(f("changes", "level_shift", 0.5)) == 2
    assert onceover._statement_class(f("intervals", "extreme_value", 0.5)) == 2
    assert onceover._statement_class(f("intervals", "value_range", 0.5)) == 3
    assert onceover._statement_class(f("missingness", "missing_values", 0.5)) == 3


def test_digest_orders_structure_then_shape_then_hygiene():
    # The hygiene statement has the highest interestingness and
    # still comes last: class order, then score within the class.
    findings = [
        _finding("missingness", "missing_values", 0.99),
        _finding("changes", "level_shift", 0.60),
        _finding("dependencies", "functional", 0.50),
    ]
    digest = _report(findings).to_digest(100_000)
    order = [
        ln.split("]")[0]
        for ln in digest.splitlines()
        if ln.startswith("[")
    ]
    assert order == ["[dependencies", "[changes", "[missingness"]


def test_digest_takes_hygiene_only_after_insight_lines():
    # Two short insight lines and one long hygiene line; the
    # budget is one character below the full digest, so exactly
    # one line must go — and the line that goes is the hygiene
    # line, even though it carries the highest score, because
    # hygiene is taken only after the insight classes.
    findings = [
        _finding("missingness", "missing_values", 0.99,
                 claim="missing values everywhere in this column " * 12),
        _finding("changes", "level_shift", 0.60),
        _finding("dependencies", "functional", 0.50),
    ]
    report = _report(findings)
    full = report.to_digest(100_000)
    budget = len(full) - 1
    digest = report.to_digest(budget)
    assert len(digest) <= budget
    assert "[dependencies]" in digest
    assert "[changes]" in digest
    assert "[missingness]" not in digest
    assert "omitted to fit" in digest
