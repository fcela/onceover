"""The pointed mode (issue #310, first increment): the metadata
handover and target-designated diagnosis.

Pins: the six metadata hint keys are carried into the report's
frame account and accounted for in coverage, never silently
dropped; a designated target states the outcome's own
distribution and per-level outcome contrasts; measured absences
are stated as nulls only when metadata is declared; declared
leakage suspects are checked first and stated as declared;
declared questions admit their findings ahead of equal-score
unmatched findings.
"""

import polars as pl
import pytest

import onceover


def _cat_frame(n_per_group=200):
    """The _frame design with a categorical outcome: `strong`
    splits the event rate 70/30, `flat` holds the pooled rate."""
    rows = []
    for grp, rate in (("a", 0.7), ("b", 0.3)):
        for i in range(n_per_group):
            rows.append(
                {
                    "strong": grp,
                    "flat": "x" if i % 2 == 0 else "y",
                    "y": "yes" if i < rate * n_per_group else "no",
                }
            )
    return pl.DataFrame(rows)


def _frame(n_per_group=200):
    """Two groupings against a binary outcome: `strong` splits
    the event rate 70/30, `flat` holds it at the pooled rate."""
    rows = []
    for grp, rate in (("a", 0.7), ("b", 0.3)):
        for i in range(n_per_group):
            rows.append(
                {
                    "strong": grp,
                    "flat": "x" if i % 2 == 0 else "y",
                    "y": 1 if i < rate * n_per_group else 0,
                    "x_num": float(i),
                }
            )
    return pl.DataFrame(rows)


def _coverage(report, detector):
    return report.coverage[detector]


def test_metadata_keys_are_carried_and_accounted():
    df = _frame()
    report = onceover.profile(
        df,
        target="y",
        hints={
            "gloss": {"y": "whether the row converted"},
            "units": {"x_num": "widgets"},
            "event": 1,
            "entity": "strong",
            "recorded_after": ["x_num"],
            "purpose": "understanding conversion",
            "questions": ["what drives y?"],
        },
    )
    declarations = report.data["frame"]["declarations"]
    assert declarations["purpose"] == "understanding conversion"
    assert declarations["questions"] == ["what drives y?"]
    assert declarations["event"] == 1
    assert declarations["entity"] == "strong"
    assert declarations["recorded_after"] == ["x_num"]
    assert declarations["gloss"]["y"] == "whether the row converted"
    assert declarations["units"] == {"x_num": "widgets"}
    entry = _coverage(report, "declarations")
    assert entry["status"].startswith("ran")
    # y is numeric in this fixture, so the declared event names
    # no level: the accounting states the refusal, not a check.
    assert "event: 1 refused" in entry["detail"]
    assert "entity: 'strong' checked" in entry["detail"]
    assert "gloss: 1 column meaning(s) carried unchecked" in entry["detail"]


def test_no_metadata_means_no_declarations():
    report = onceover.profile(_frame(), target="y")
    assert "declarations" not in report.data["frame"]
    assert "declarations" not in report.coverage


def test_outcome_account_numeric_states_center_spread_range():
    df = pl.DataFrame({"g": ["a", "a", "b", "b"] * 30, "y": [1.0, 3.0, 5.0, 9.0] * 30})
    report = onceover.profile(df, target="y", hints={"units": {"y": "percent"}})
    account = report.select(type="outcome_distribution")
    assert len(account) == 1
    finding = account[0]
    assert finding["subject"]["unit"] == "percent"
    assert "median 4," in finding["claim"]
    assert "(in percent)" in finding["claim"]
    metrics = finding["evidence"][0]["metrics"]
    assert metrics["median"] == 4.0
    assert metrics["min"] == 1.0
    assert metrics["max"] == 9.0


def test_outcome_account_categorical_states_level_shares_and_event():
    df = pl.DataFrame(
        {"g": ["a", "b"] * 100, "y": ["yes", "no"] * 100}
    )
    report = onceover.profile(df, target="y", hints={"event": "yes"})
    account = report.select(type="outcome_distribution")
    assert len(account) == 1
    claim = account[0]["claim"]
    assert "'yes' 50.0% (100 rows)" in claim
    assert "The declared event is 'yes': 50.0% of all rows" in claim


def test_event_contrasts_state_differences_and_nulls_when_pointed():
    report = onceover.profile(_cat_frame(500), target="y", hints={"event": "yes"})
    assert "event: 'yes' checked" in _coverage(report, "declarations")["detail"]
    contrasts = report.select(type="outcome_contrast")
    forms = {f["specification"]["form"] for f in contrasts}
    assert "event_rate" in forms
    assert "event_rate_null" in forms
    diffs = [f for f in contrasts if f["specification"]["form"] == "event_rate"]
    assert any("strong = 'a'" in f["claim"] and "70.0%" in f["claim"] for f in diffs)
    nulls = [f for f in contrasts if f["specification"]["form"] == "event_rate_null"]
    assert any("'flat'" in f["claim"] for f in nulls)


def test_nulls_withheld_without_metadata():
    report = onceover.profile(_cat_frame(), target="y")
    contrasts = report.select(type="outcome_contrast")
    assert contrasts
    assert all(f["specification"]["form"] != "event_rate_null" for f in contrasts)
    entry = _coverage(report, "outcome_contrasts")
    assert "absences are not stated: without a declared event" in entry["detail"]


def test_declared_suspect_is_checked_first_and_stated():
    n = 120
    df = pl.DataFrame(
        {
            "y": [i % 2 for i in range(n)],
            "leak": [i % 2 for i in range(n)],
            "other": [float(i % 7) for i in range(n)],
        }
    )
    report = onceover.profile(df, target="y", hints={"recorded_after": ["leak"]})
    leaks = report.select(type="leakage_signature")
    declared = [f for f in leaks if f["evidence"][0]["metrics"].get("declared_suspect")]
    assert declared, "the declared suspect was not stated as declared"
    assert "declared by the caller" in declared[0]["claim"]
    entry = _coverage(report, "leakage")
    assert "declared suspects checked first" in entry["detail"]


def test_recorded_after_columns_leave_the_predictor_pools():
    report = onceover.profile(
        _frame(), target="y", hints={"recorded_after": ["x_num"], "event": 1}
    )
    entry = _coverage(report, "model_failures")
    assert "declared recorded-after column(s) excluded from the model inputs" in entry["detail"]


def test_declared_questions_admit_their_findings_first():
    df = _frame()
    plain = onceover.profile(df, target="y", hints={"event": 1})
    asked = onceover.profile(
        df,
        target="y",
        hints={"event": 1, "questions": ["how does strong relate to y?"]},
    )
    entry = _coverage(asked, "questions")
    assert entry["status"].startswith("ran")
    assert '"how does strong relate to y?":' in entry["detail"]
    assert "finding(s)" in entry["detail"]
    # The question names 'strong' and 'y': the strong contrasts
    # carry the marker in their evidence.
    marked = [
        f
        for f in asked.findings
        if f["evidence"][0]["metrics"].get("declared_questions")
    ]
    assert marked
    # The strong contrasts answer the question: their subject
    # names the grouping and the target the question names.
    assert any(
        f["type"] == "outcome_contrast" and f["subject"]["columns"] == ["strong", "y"]
        for f in marked
    )
    assert not [
        f
        for f in plain.findings
        if f["evidence"][0]["metrics"].get("declared_questions")
    ]


def test_unusable_declarations_are_refused_in_coverage_not_raised():
    report = onceover.profile(
        _frame(),
        target="y",
        hints={"entity": "nope", "units": {"nope": "x"}, "event": 7},
    )
    detail = _coverage(report, "declarations")["detail"]
    assert "entity: 'nope' refused" in detail
    assert "unit for 'nope' not carried" in detail
    assert "event: 7 refused" in detail


def test_gloss_is_stated_in_the_claim_of_its_column():
    report = onceover.profile(
        _frame(), target="y", hints={"gloss": {"y": "whether the row converted"}, "event": 1}
    )
    account = report.select(type="outcome_distribution")
    assert "The caller declares 'y' to mean: whether the row converted." in account[0]["claim"]


def test_outcome_contrast_reaches_the_digest_at_the_default_budget():
    # The pointed briefing's target section leads with the
    # outcome's own contrasts: on a crowded report the fail-rate
    # contrast must survive the 4,000-character digest (issue
    # #308's binding case, on a synthetic frame).
    rows = []
    for channel, rate in (("direct", 0.6), ("agent", 0.3), ("partner", 0.3)):
        for i in range(400):
            failed = i < rate * 400
            rows.append(
                {
                    "channel": channel,
                    "region": "north" if len(rows) % 2 == 0 else "south",
                    "score": 100.0 if failed else 10.0,
                    "canceled": "yes" if failed else "no",
                }
            )
    df = pl.DataFrame(rows)
    report = onceover.profile(
        df, target="canceled", hints={"event": "yes", "purpose": "cancellations"}
    )
    digest = report.to_digest(4000)
    target_section = digest.split("What relates to the target")[1].split(
        "Other relationships"
    )[0]
    assert "channel = 'direct'" in target_section
    assert "60%" in target_section
    assert "No difference in the 'yes' rate across 'region'" in target_section


@pytest.mark.parametrize("dtype", [pl.Float64, pl.Decimal(12, 2)])
def test_numeric_target_contrasts_keep_decimal_support_without_duplication(dtype):
    """Merging Decimal support must preserve pointed mode's ownership of target contrasts."""
    df = pl.DataFrame({
        "region": ["a"] * 200 + ["b"] * 200,
        "amount": pl.Series([float(i % 200 + 1000 * (i >= 200)) for i in range(400)]).cast(dtype),
    })
    unpointed = onceover.profile(df, detectors=["level_contrasts"])
    assert unpointed.select(type="level_contrast", column="amount", include_grouped=True)

    pointed = onceover.profile(
        df, target="amount", detectors=["level_contrasts", "outcome_contrasts"]
    )
    assert pointed.select(type="outcome_contrast", column="amount", include_grouped=True)
    assert not pointed.select(type="level_contrast", column="amount", include_grouped=True)
