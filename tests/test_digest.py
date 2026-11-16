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
    # The full ledger fits: no omission line, and the briefing's
    # sections carry the findings (issue #317 phase 2 replaced the
    # category header and the [category] type/form lines).
    assert "omitted" not in digest
    assert "What is wrong:" in digest
    assert "Other relationships:" in digest


def test_digest_tiny_budget_truncates_header():
    df = pl.DataFrame({"x": [float(i % 40) for i in range(200)]})
    report = onceover.profile(df)
    digest = report.to_digest(80)
    assert len(digest) <= 80
    assert digest.startswith("profile digest:")


def _digest_line_categories(report, digest):
    """The categories of the findings a digest's lines speak for,
    resolved through the trailing finding id of each line."""
    by_id = {f["id"]: f for f in report.data["findings"]}
    cats = []
    for ln in digest.splitlines():
        if ln.endswith("]") and " [" in ln:
            fid = ln.rsplit(" [", 1)[1][:-1]
            if fid in by_id:
                cats.append(by_id[fid]["category"])
    return cats


def test_digest_merges_findings_that_state_the_same_fact():
    # Two detectors state the same columns: the exact form speaks
    # for the fact and the statistical form folds into it — one
    # line, carrying the exact finding's id (issue #317 phase 2;
    # the old (xN) rollup marker is gone).
    findings = [
        _finding("dependencies", "functional", 0.50, columns=["a", "b"]),
        _finding("associations", "statistical", 0.90, columns=["a", "b"]),
        _finding("associations", "statistical", 0.80, columns=["a", "c"]),
    ]
    digest = _report(findings).to_digest(100_000)
    body = [
        ln for ln in digest.splitlines()
        if ln.endswith("]") and " [" in ln
    ]
    assert len(body) == 2
    merged = [ln for ln in body if "functional statement" in ln]
    assert len(merged) == 1
    assert merged[0].endswith("[dependencies-functional-column-unconditional]")


def _bike_report():
    df = pl.read_csv(
        "benchmarks/corpus/data/anomaly/bike-sharing-hourly/data.csv",
        infer_schema_length=None,
        try_parse_dates=True,
    )
    return onceover.profile(df)


def test_every_insight_class_with_findings_gets_a_line():
    # Relations alone used to fill the budget on this table, leaving the 77
    # change statements with no line at all. Under the briefing's
    # section allowances (issue #317 phase 2) each reader question
    # keeps its own share, so changes and dependencies both get lines.
    report = _bike_report()
    digest = report.to_digest(4000)
    assert len(digest) <= 4000
    cats = _digest_line_categories(report, digest)
    assert "changes" in cats
    assert "dependencies" in cats


def test_omission_line_names_what_is_missing_and_how_to_get_it():
    digest = _bike_report().to_digest(3000)
    assert "omitted to fit 3000 chars" in digest
    omission = [ln for ln in digest.splitlines() if ln.startswith("... [")][0]
    assert "report.select(category=" in omission


def test_kept_lines_stay_in_section_order():
    # Digest sections follow the reader-question order (issue #317
    # phase 2): what a row is, what is wrong, what is redundant,
    # what relates to the target, other relationships, anomalies,
    # not examined — replacing the statement-class line order.
    report = _bike_report()
    digest = report.to_digest(4000)
    titles = [
        ln[:-1] for ln in digest.splitlines() if ln.endswith(":")
        and ln[:-1] in onceover._DIGEST_SECTION_TITLES.values()
    ]
    order = list(onceover._DIGEST_SECTION_TITLES.values())
    positions = [order.index(t) for t in titles]
    assert positions == sorted(positions)
    assert _digest_line_categories(report, digest)


def test_quality_line_states_exact_counts():
    n = 400
    frame = pl.DataFrame({
        "a": [None if i % 4 == 0 else float(i) for i in range(n)],
        "empty": pl.Series([None] * n, dtype=pl.Float64),
        "same": [1] * n,
        "b": [float(i % 7) for i in range(n)],
    })
    report = onceover.profile(frame)
    # The exact-counts line opens the "What is wrong" section; it
    # no longer carries a "quality:" prefix (issue #317 phase 2).
    line = [ln for ln in report.to_digest(4000).splitlines()
            if ln.startswith("missing values in")][0]
    assert "missing values in 1 column(s), most in a (25.0%)" in line
    assert "1 empty column(s)" in line
    assert "1 constant column(s)" in line


def test_quality_line_absent_on_a_clean_frame():
    frame = pl.DataFrame({"a": [float(i) for i in range(200)],
                          "b": [float(i % 7) for i in range(200)]})
    report = onceover.profile(frame)
    assert onceover._quality_line(report.data) is None
    assert "missing values in" not in report.to_digest(4000)


def test_digest_keeps_body_lines_when_the_omission_line_grows():
    # On this table the omission line for the kept set is longer than for
    # the full set; the digest must still carry statements. (The table's
    # changes lines used to come from the discontinuities screen; that
    # screen is axis-gated in a default run since issue #317 phase 3 and
    # this frame has no time axis or declared axis, so the witness
    # categories are dependencies and associations.)
    import glob
    path = glob.glob("benchmarks/corpus/data/business/absenteeism-at-work/*.csv")[0]
    report = onceover.profile(pl.read_csv(path, infer_schema_length=None))
    digest = report.to_digest(4000)
    assert len(digest) <= 4000
    cats = _digest_line_categories(report, digest)
    assert "dependencies" in cats and "associations" in cats


# --- statement-class ordering (issue #271) ---


def _finding(category, typ, score, claim=None, scope="column",
             conditionality="unconditional", columns=None, target=None):
    subject = {}
    if columns is not None:
        subject["columns"] = columns
    if target is not None:
        subject["target"] = target
    return {
        "id": f"{category}-{typ}-{scope}-{conditionality}",
        "category": category,
        "type": typ,
        "specification": {},
        "subject": subject,
        "claim": claim or f"{category} {typ} statement",
        "interestingness": score,
        "facets": {"scope": scope, "conditionality": conditionality},
        "evidence": [],
    }


def _report(findings):
    report = onceover.ProfileReport.__new__(onceover.ProfileReport)
    report._native = None
    report._data = {
        "schema_version": "0.8",
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
    assert onceover._statement_class(f("anomalies", "duplicate_encoding", 0.5)) == 1
    assert onceover._statement_class(f("dependencies", "functional", 0.5)) == 1
    assert onceover._statement_class(
        f("missingness", "missing_values", 0.5, conditionality="value_conditioned")
    ) == 1
    assert onceover._statement_class(f("changes", "level_shift", 0.5)) == 2
    assert onceover._statement_class(f("anomalies", "extreme_value", 0.5)) == 2
    assert onceover._statement_class(f("anomalies", "value_range", 0.5)) == 3
    assert onceover._statement_class(f("missingness", "missing_values", 0.5)) == 3


def test_digest_orders_sections_by_reader_question():
    # The briefing's sections replace the statement-class line
    # order (issue #317 phase 2): what is wrong comes before what
    # is redundant and before other relationships, whatever the
    # findings' scores.
    findings = [
        _finding("missingness", "missing_values", 0.99),
        _finding("changes", "level_shift", 0.60),
        _finding("dependencies", "functional", 0.50),
    ]
    digest = _report(findings).to_digest(100_000)
    titles = [ln for ln in digest.splitlines() if ln.endswith(":")]
    assert titles == [
        "What is wrong:",
        "What is redundant:",
        "Other relationships:",
    ]


def _contrast(measure, grouping, score, form, claim):
    return {
        "id": f"associations-level_contrast-{measure}-{grouping}",
        "category": "associations",
        "type": "level_contrast",
        "specification": {},
        "subject": {"columns": [measure, grouping]},
        "claim": claim,
        "interestingness": score,
        "facets": {"scope": "column", "conditionality": "unconditional"},
        "evidence": [{"metrics": {
            "measure_column": measure,
            "grouping_column": grouping,
            "form": form,
        }}],
    }


def test_digest_contrast_breadth_runs_across_measures():
    # A level contrast is the per-level statement of one measure's
    # relationship with a grouping. Keyed as a single kind, the
    # contrasts of one measure (one per grouping, at near-identical
    # scores) fill the relationship section's early rounds and
    # another measure's only contrast never reaches the digest
    # (issue #317 phase 2: the bank/abalone/bike misses). The kind
    # counts once per measure it states, so the strongest contrast
    # of every measure is offered before any measure's second.
    findings = [
        _contrast("alpha", f"g{i}", 0.90 - i * 0.01, "location",
                  f"alpha contrast {i} statement " + "x" * 90)
        for i in range(6)
    ]
    findings.append(_contrast(
        "beta", "g0", 0.50, "location",
        "beta contrast statement " + "x" * 90))
    digest = _report(findings).to_digest(1500)
    assert len(digest) <= 1500
    assert "beta contrast statement" in digest


def test_digest_location_contrast_precedes_scale_only():
    # Within the level statements, one that locates the measure
    # precedes one that only spreads it: the location is the
    # relationship, the spread refines it — and on sentinel-driven
    # measures the spread statement is often a restatement of the
    # sentinel's mass (issue #317 phase 2: the bank duration
    # contrast, a location statement, was crowded out by
    # higher-scored scale statements with no location shift).
    findings = [
        _contrast("alpha", "g0", 0.90, "scale",
                  "alpha scale statement " + "x" * 90),
        _contrast("gamma", "g0", 0.88, "scale",
                  "gamma scale statement " + "x" * 90),
        _contrast("beta", "g0", 0.60, "location",
                  "beta location statement " + "x" * 90),
    ]
    digest = _report(findings).to_digest(1500)
    assert len(digest) <= 1500
    assert "beta location statement" in digest
    assert (digest.index("beta location statement")
            < digest.index("alpha scale statement"))


def test_digest_numbers_use_compact_form_below_a_thousandth():
    # Three significant figures in plain notation would print six
    # or more decimal places for a value below a thousandth; the
    # digest uses the compact scientific form there, so no
    # six-decimal number reaches the reader (issue #317 phase 2 —
    # a level contrast on orbital inclinations, median 0.000873,
    # is the case that surfaced it).
    assert onceover._digest_numbers(
        "median 0.0008726646 against 1.51"
    ) == "median 8.73e-04 against 1.51"
    assert onceover._digest_numbers(
        "median 0.00314159 against 2"
    ) == "median 0.00314 against 2"


def test_digest_keeps_hygiene_lines_under_pressure():
    # The #280 settlement (issue #317 phase 2): the "what is
    # wrong" section holds its own allowance, so hygiene lines
    # reach the digest even when relationship statements are
    # plentiful enough to fill the budget on their own. Under the
    # old taking order the hygiene lines came only after every
    # insight line had been refused, and vanished here.
    findings = [
        _finding("dependencies", "functional", 0.90 - i * 0.001,
                 claim=f"functional statement {i} " + "x" * 90,
                 columns=[f"c{i}", f"d{i}"])
        for i in range(40)
    ]
    findings.append(_finding("missingness", "missing_values", 0.10,
                             columns=["m"]))
    digest = _report(findings).to_digest(4000)
    assert len(digest) <= 4000
    assert "missingness missing_values statement" in digest
