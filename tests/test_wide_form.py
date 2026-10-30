"""Wide-form profiling of verified long-format frames (issue #5)."""

import polars as pl

import onceover


def _panel() -> pl.DataFrame:
    rows = []
    for c in range(12):
        # Three shared growth paths: the pooled measure repeats, so
        # it reads as a measure rather than an identifier.
        gdp0 = 200.0 + 90.0 * (c % 3)
        for y in range(5):
            gdp = gdp0 * (1 + 0.03 * y)
            life = 52.0 + 0.021 * gdp + ((c * 7 + y * 3) % 5) * 0.4
            for ind, v in [("gdp", gdp), ("life", life)]:
                rows.append((f"c{c:02}", 2015 + y, ind, v))
    return pl.DataFrame(
        rows, schema=["country", "year", "indicator", "value"], orient="row"
    )


def _tagged(f) -> bool:
    return any(
        (e.get("metrics") or {}).get("source_form") == "pivoted wide form"
        for e in f["evidence"]
    )


def test_wide_form_findings_are_tagged_and_present():
    report = onceover.profile(_panel())
    cov = report.data["coverage"]["pivot_profile"]
    assert cov["status"] == "ran_exact"
    # The planted gdp/life association exists only in the wide form.
    assert any(
        _tagged(f) and "gdp" in f["subject"]["columns"] and "life" in f["subject"]["columns"]
        for f in report.findings
    )
    # The long-format finding itself is not tagged.
    assert any(
        f.get("type") == "long_format" and not _tagged(f) for f in report.findings
    )


def test_quick_budget_skips_wide_profile():
    report = onceover.profile(_panel(), budget="quick")
    assert report.data["coverage"]["pivot_profile"]["status"] == "not_applicable"
    assert not any(_tagged(f) for f in report.findings)
