"""Group contrast: a declared subset of the rows against the rest.

The pass is hint-gated: `hints={"contrast": {"column": ..., "value":
...}}` names the group (a boolean column needs no value), and each
other column is compared between the group and the rest — means
with Cohen's d for numeric columns, level shares for categorical
columns, the true share for boolean columns. A difference is
reported only when it clears the stated bars (docs in
crates/onceover-mines/src/contrast.rs): |d| >= 0.5 or a share
difference >= 0.10, each at least 3 times its standard error, with
at least 10 rows and 10 known values on each side.
"""

import polars as pl

import onceover


def _planted() -> pl.DataFrame:
    """2,000 rows; the 36 'north' rows average double the tokens of
    the rest and are 'pro' on plan 60% of the time against 20%."""
    region, tokens, plan = [], [], []
    for i in range(2000):
        if i < 36:
            region.append("north")
            tokens.append(400.0 + (i % 7))
            plan.append("pro" if i % 5 < 3 else "free")
        else:
            region.append(["south", "east", "west"][i % 3])
            tokens.append(200.0 + (i % 7))
            plan.append("pro" if i % 5 == 0 else "free")
    return pl.DataFrame({"region": region, "tokens": tokens, "plan": plan})


def _contrast(report) -> list[dict]:
    return [f for f in report.findings if f["type"] == "group_contrast"]


def test_planted_difference_fires_with_direction_and_size():
    report = onceover.profile(
        _planted(), hints={"contrast": {"column": "region", "value": "north"}}
    )
    hits = _contrast(report)
    assert len(hits) == 2, [f["claim"] for f in report.findings]
    for f in hits:
        assert f["category"] == "associations"
        assert f["facets"]["conditionality"] == "segment_conditioned"
        assert f["evidence"][0]["method"]["detector"] == "group_contrast"
    by_column = {f["evidence"][0]["metrics"]["column"]: f for f in hits}
    m = by_column["tokens"]["evidence"][0]["metrics"]
    assert m["group_rows"] == 36 and m["rest_rows"] == 1964
    assert abs(m["group_mean"] - 403.0) < 1.0
    assert abs(m["rest_mean"] - 203.0) < 1.0
    assert m["difference"] > 199.0
    assert m["effect_size"] > 50.0
    assert "tokens averages" in by_column["tokens"]["claim"]
    m = by_column["plan"]["evidence"][0]["metrics"]
    assert m["level"] == "pro"
    assert abs(m["group_share"] - 0.61) < 0.02
    assert abs(m["rest_share"] - 0.20) < 0.02
    assert "plan is 'pro'" in by_column["plan"]["claim"]
    entry = report.coverage["group_contrast"]
    assert entry["status"] == "ran_exact", entry
    assert "36 rows against 1964 rest rows" in entry["detail"]


def test_boolean_column_defines_the_group_without_a_value():
    flag = [i % 10 == 0 for i in range(500)]
    score = [(10.0 if f else 5.0) + (i % 3) * 0.01 for i, f in enumerate(flag)]
    df = pl.DataFrame({"flag": flag, "score": score})
    report = onceover.profile(df, hints={"contrast": {"column": "flag"}})
    hits = _contrast(report)
    assert len(hits) == 1
    m = hits[0]["evidence"][0]["metrics"]
    assert m["column"] == "score"
    assert m["group_rows"] == 50 and m["rest_rows"] == 450
    assert m["group_value"] is None


def test_random_split_reports_nothing():
    # Membership from a fixed hash-like pattern, columns cycling
    # independently of it: every apparent difference is sampling
    # noise and must stay under the bars.
    n = 2000
    member = [((i * 2654435761) % 100) < 30 for i in range(n)]
    df = pl.DataFrame(
        {
            "member": member,
            "amount": [float((i * 37) % 101) for i in range(n)],
            "kind": [["a", "b", "c"][(i * 11) % 3] for i in range(n)],
            "flag": [(i * 13) % 2 == 0 for i in range(n)],
        }
    )
    report = onceover.profile(df, hints={"contrast": {"column": "member"}})
    assert _contrast(report) == []
    entry = report.coverage["group_contrast"]
    assert entry["status"] == "ran_exact", entry
    assert "0 contrasts cleared" in entry["detail"]


def test_unusable_declarations_are_coverage_not_errors():
    df = _planted()
    report = onceover.profile(df, hints={"contrast": {"column": "nope", "value": "x"}})
    entry = report.coverage["group_contrast"]
    assert entry["status"] == "skipped"
    assert "not a column" in entry["detail"]

    report = onceover.profile(
        df, hints={"contrast": {"column": "region", "value": "arctic"}}
    )
    entry = report.coverage["group_contrast"]
    assert entry["status"] == "skipped"
    assert "names no rows" in entry["detail"]

    whole = pl.DataFrame({"cohort": ["all"] * 200, "x": [float(i % 9) for i in range(200)]})
    report = onceover.profile(whole, hints={"contrast": {"column": "cohort", "value": "all"}})
    entry = report.coverage["group_contrast"]
    assert entry["status"] == "skipped"
    assert "no rest to contrast against" in entry["detail"]

    report = onceover.profile(df, hints={"contrast": {"column": "region"}})
    entry = report.coverage["group_contrast"]
    assert entry["status"] == "skipped"
    assert "needs a value" in entry["detail"]


def test_no_declaration_no_contrast_and_no_other_change():
    df = _planted()
    plain = onceover.profile(df)
    assert _contrast(plain) == []
    entry = plain.coverage["group_contrast"]
    assert entry["status"] == "skipped"
    assert "no group contrast declared" in entry["detail"]

    hinted = onceover.profile(
        df, hints={"contrast": {"column": "region", "value": "north"}}
    )
    plain_ids = [f["id"] for f in plain.findings]
    other_ids = [f["id"] for f in hinted.findings if f["type"] != "group_contrast"]
    assert other_ids == plain_ids


def test_two_level_headline_names_the_overrepresented_level():
    # A two-level column's levels move by exactly opposite amounts;
    # the headline must name the level the group over-represents,
    # not whichever the float subtraction happens to make larger.
    region = ["north"] * 40 + ["south"] * 2000
    plan = (["pro"] * 24 + ["free"] * 16) + (["pro"] * 400 + ["free"] * 1600)
    df = pl.DataFrame({"region": region, "plan": plan})
    report = onceover.profile(
        df, hints={"contrast": {"column": "region", "value": "north"}}
    )
    hits = _contrast(report)
    assert len(hits) == 1
    m = hits[0]["evidence"][0]["metrics"]
    assert m["level"] == "pro"
    assert abs(m["group_share"] - 0.6) < 1e-9
    assert abs(m["rest_share"] - 0.2) < 1e-9


def test_group_column_itself_is_never_contrasted():
    report = onceover.profile(
        _planted(), hints={"contrast": {"column": "region", "value": "north"}}
    )
    for f in _contrast(report):
        assert f["evidence"][0]["metrics"]["column"] != "region"
