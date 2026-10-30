"""Per-pass timing attribution (issue #26): opt-in `timing=True`
records `elapsed_ms` on coverage entries; default reports carry
none and stay byte-reproducible."""

import polars as pl

import onceover


def _frame() -> pl.DataFrame:
    return pl.DataFrame(
        {
            "city": ["Chicago", "Austin", "Springfield"] * 40,
            "state": ["IL", "TX", "IL"] * 40,
            "amt": [10.0 + (i % 17) for i in range(120)],
        }
    )


def test_timing_attributes_passes():
    report = onceover.profile(_frame(), timing=True)
    cov = report.data["coverage"]
    ran = [c for c in cov.values() if c["status"].startswith("ran")]
    assert ran, "expected some passes to have run"
    assert all(isinstance(c.get("elapsed_ms"), (int, float)) for c in ran)
    assert all(c["elapsed_ms"] >= 0 for c in ran)
    assert sum(c["elapsed_ms"] for c in ran) > 0


def test_default_reports_have_no_timing():
    a = onceover.profile(_frame())
    b = onceover.profile(_frame())
    assert all("elapsed_ms" not in c for c in a.data["coverage"].values())
    # Determinism is the default contract.
    assert a.to_json() == b.to_json()
    # Findings are identical with timing on.
    timed = onceover.profile(_frame(), timing=True)
    assert [f["id"] for f in a.findings] == [f["id"] for f in timed.findings]
