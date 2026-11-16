"""Series past the breaks series cap are stated in coverage.

The breaks pass prescores every eligible series and scans at most
`series_cap` (16) in depth. The series past the cap are a statement
about the examination, not about the data, so they are stated in
the pass's coverage entry — which names them, the cap, and the
counts — and never as a finding: a report's findings state facts
about the table, and the coverage block is where the examination
itself is accounted for.
"""
from datetime import date, timedelta

import polars as pl

import onceover

N = 400
DAYS = [date(2021, 1, 1) + timedelta(days=i) for i in range(N)]
SHIFTED = [f"shift_{k:02d}" for k in range(16)]
QUIET = [f"quiet_{k:02d}" for k in range(4)]


class _Lcg:
    """Deterministic platform-independent noise source."""

    def __init__(self, seed: int):
        self.x = seed

    def next(self) -> float:
        self.x = (6364136223846793005 * self.x + 1442695040888963407) % (1 << 64)
        return ((self.x >> 11) / float(1 << 53)) - 0.5


def _frame(n_shifted: int, n_quiet: int) -> pl.DataFrame:
    # The shifted series step from 0 to 10 halfway (prescore close
    # to 1). The quiet series are small-amplitude noise around 1
    # (prescore close to 0), so the prescore ranking — and the set
    # the cap leaves unscanned — is known. Quiet series must carry
    # noise rather than a constant value: constant columns are
    # excluded from the pass's targets upstream by their frame
    # role, so they never reach the cap at all.
    cols: dict[str, list] = {"day": DAYS}
    for k in range(n_shifted):
        cols[f"shift_{k:02d}"] = [0.0] * (N // 2) + [10.0] * (N // 2)
    for k in range(n_quiet):
        rng = _Lcg(1000 + k)
        cols[f"quiet_{k:02d}"] = [1.0 + rng.next() * 0.1 for _ in range(N)]
    return pl.DataFrame(cols)


def test_cap_binds_states_unscanned_series_in_coverage():
    df = _frame(16, 4)
    report = onceover.profile(df, detectors=["breaks"])
    # No finding states the unscanned set.
    assert [f for f in report.findings if f.get("type") == "unscanned_series"] == []
    assert all("did not scan" not in f["claim"] for f in report.findings)
    # The coverage entry states the cap, the counts, and the names.
    detail = report.data["coverage"]["breaks"]["detail"]
    assert "series cap 16 bound: 16 of 20 eligible series scanned" in detail
    tail = detail.split("not scanned: ", 1)[1]
    assert all(name in tail for name in QUIET)


def test_cap_not_reached_states_nothing():
    df = _frame(12, 4)
    report = onceover.profile(df, detectors=["breaks"])
    assert [f for f in report.findings if f.get("type") == "unscanned_series"] == []
    detail = report.data["coverage"]["breaks"]["detail"]
    assert "series cap" not in detail
