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
