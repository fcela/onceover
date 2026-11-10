"""Report diff mode and digest rollup exemplars (issue #27)."""

import polars as pl

import onceover


def _frame(dup_rows: int) -> pl.DataFrame:
    cities = ["Chicago", "Springfield", "Aurora"]
    states = ["IL", "IL", "IL"]
    rows = [(cities[i % 3], states[i % 3], 10.0 + i) for i in range(90)]
    rows += [rows[0]] * dup_rows
    return pl.DataFrame(rows, schema=["city", "state", "amt"], orient="row")


def test_diff_identical_reports_is_empty():
    report = onceover.profile(_frame(0))
    d = report.diff(report)
    assert d["added"] == [] and d["removed"] == [] and d["changed"] == []
    assert d["unchanged"] == len(report.findings)
    assert d["coverage_changes"] == {}


def test_diff_reports_added_duplicates():
    base = onceover.profile(_frame(0))
    other = onceover.profile(_frame(6))
    d = base.diff(other)
    added_cats = {e["category"] for e in d["added"]}
    assert "duplicates" in added_cats
    for e in d["added"]:
        assert other.finding(e["id"]) is not None
        assert base.finding(e["id"]) is None
    # Duplicating rows also retires the base frame's uniqueness
    # finding on `amt` (its values are no longer all distinct), so
    # `removed` is not empty here; every removed entry must resolve
    # in the base report only.
    for e in d["removed"]:
        assert base.finding(e["id"]) is not None
        assert other.finding(e["id"]) is None


def test_diff_reports_changed_claim():
    base = onceover.profile(_frame(3))
    other = onceover.profile(_frame(30))
    d = base.diff(other)
    dup_changes = [c for c in d["changed"] if c["category"] == "duplicates"]
    assert dup_changes, "growing the duplicate block should change the finding"
    entry = dup_changes[0]
    assert entry["claim"][0] != entry["claim"][1]
    # The entry carries the score pair either way; on this fixture
    # the duplicates score saturates, so the claim is what moved.
    assert len(entry["interestingness"]) == 2


def test_digest_rollup_lists_exemplars():
    df = pl.DataFrame(
        {
            "a": [1.0, None, 3.0, None, 5.0] * 20,
            "b": [None, 2.0, None, 4.0, 5.0] * 20,
        }
    )
    report = onceover.profile(df)
    # Both columns draw categorical-role findings, which the digest
    # rolls into one multi-member group.
    roles = report.select(category="column_roles", type="categorical_role")
    assert len(roles) >= 2
    digest = report.to_digest(8000)
    assert "[exemplars:" in digest
    assert roles[0]["id"] in digest or roles[1]["id"] in digest
def _card_v1() -> pl.DataFrame:
    return pl.DataFrame(
        {
            "city": ["Chicago", "Springfield", "Aurora", "Chicago"] * 10,
            "amt": [10.0, 20.0, None, 40.0] * 10,
            "code": list(range(40)),
            "gone": [1.5] * 40,
        }
    )


def _card_v2() -> pl.DataFrame:
    return pl.DataFrame(
        {
            "city": ["Chicago", "Springfield", "Aurora", "Chicago"] * 10,
            "amt": [10.0, 20.0, None, None] * 10,
            "code": [str(i) for i in range(40)],
            "fresh": [True] * 40,
        }
    )


def test_diff_columns_added_removed_retyped_changed():
    base = onceover.profile(_card_v1())
    other = onceover.profile(_card_v2())
    cols = base.diff(other)["columns"]
    assert [e["name"] for e in cols["added"]] == ["fresh"]
    assert cols["added"][0]["dtype"] == other.columns["fresh"]["dtype"]
    assert cols["added"][0]["role"] == other.columns["fresh"]["role"]
    assert [e["name"] for e in cols["removed"]] == ["gone"]
    assert cols["removed"][0]["dtype"] == base.columns["gone"]["dtype"]
    # `code` was recast from integer to string: a retype, stated
    # distinctly from a removal plus an addition.
    assert [e["name"] for e in cols["retyped"]] == ["code"]
    assert cols["retyped"][0]["dtype"] == [
        base.columns["code"]["dtype"],
        other.columns["code"]["dtype"],
    ]
    assert base.columns["code"]["dtype"] != other.columns["code"]["dtype"]
    changed = {e["name"]: e["changes"] for e in cols["changed"]}
    assert set(changed) == {"amt"}
    amt = changed["amt"]
    # Nulls doubled (10 -> 20 of 40 rows); the mean and the maximum
    # moved with the lost 40.0 values; the minimum did not move.
    assert amt["null_count"]["before"] == 10
    assert amt["null_count"]["after"] == 20
    assert amt["null_count"]["delta"] == 10
    assert amt["null_count"]["relative"] == 1.0
    assert amt["null_fraction"]["before"] == 0.25
    assert amt["null_fraction"]["after"] == 0.5
    assert amt["distinct"]["before"] == 3
    assert amt["distinct"]["after"] == 2
    assert amt["mean"]["before"] == base.columns["amt"]["numeric_stats"]["mean"]
    assert amt["mean"]["after"] == other.columns["amt"]["numeric_stats"]["mean"]
    assert amt["mean"]["delta"] == amt["mean"]["after"] - amt["mean"]["before"]
    assert amt["max"]["before"] == 40.0
    assert amt["max"]["after"] == 20.0
    assert "min" not in amt
    assert "n" not in amt  # both versions carry 40 rows
    # `city` is the one shared column whose card did not move.
    assert cols["unchanged"] == 1


def test_diff_columns_identical_data_is_empty():
    frame = _card_v1()
    cols = onceover.profile(frame).diff(onceover.profile(frame))["columns"]
    assert cols["added"] == [] and cols["removed"] == []
    assert cols["retyped"] == [] and cols["changed"] == []
    assert cols["unchanged"] == frame.width


def test_diff_columns_do_not_depend_on_budget():
    # The compared card quantities are computed by the column pass
    # on the full table, before budget gating: a quick profile and
    # a standard profile of the same data diff empty.
    frame = _card_v1()
    quick = onceover.profile(frame, budget="quick")
    standard = onceover.profile(frame, budget="standard")
    cols = quick.diff(standard)["columns"]
    assert cols["added"] == [] and cols["removed"] == []
    assert cols["retyped"] == [] and cols["changed"] == []
    assert cols["unchanged"] == frame.width
