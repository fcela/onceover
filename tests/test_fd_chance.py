"""Regression tests: FD chance correction (issue #128) and the
standard-tier size-3 determinant/key step (issue #132 item 2).

The planted frames are deterministic (seeded NumPy) and small.
Violation counts are recomputed with Polars group-bys, independent
of the library's own counting.
"""

import polars as pl
import pytest

import onceover

np = pytest.importorskip("numpy", reason="planted frames need numpy")

# ------------------------------------------------------------------
# Frame builders
# ------------------------------------------------------------------


def noise_frame(seed: int) -> pl.DataFrame:
    """p_zip -> p_state with ~1% violations concentrated in one zip,
    plus an unrelated 8-level column f_cat7. Splitting the ambiguous
    zip by f_cat7 cuts the violation count by chance alone."""
    rng = np.random.default_rng(seed)
    n, n_zips, n_states = 5000, 48, 8
    base, rem = divmod(n, n_zips)
    sizes = np.full(n_zips, base)
    sizes[:rem] += 1
    zip_state = rng.integers(0, n_states, size=n_zips)
    zips = np.repeat(np.arange(n_zips), sizes)
    states = zip_state[zips].astype(object)
    amb = int(rng.integers(0, n_zips))
    minority = [0.5, 0.45, 0.4, 0.5, 0.42][seed % 5]
    rows = np.where(zips == amb)[0]
    flip = rng.random(len(rows)) < minority
    states[rows[flip]] = (zip_state[amb] + 1) % n_states
    noise = rng.integers(0, 8, size=n)
    return pl.DataFrame(
        {
            "p_zip": np.array([f"z{z:02d}" for z in zips]),
            "p_state": np.array([f"s{s}" for s in states]),
            "f_cat7": np.array([f"c{c}" for c in noise]),
            "f_num": rng.normal(size=n).round(3),
        }
    )


def two_column_frame() -> pl.DataFrame:
    """y = (a + b) % 3: neither column alone determines y."""
    rng = np.random.default_rng(7)
    n = 3000
    a = rng.integers(0, 10, size=n)
    b = rng.integers(0, 10, size=n)
    return pl.DataFrame(
        {
            "a": [f"a{v}" for v in a],
            "b": [f"b{v}" for v in b],
            "y": [f"y{v}" for v in (a + b) % 3],
            "noise": rng.normal(size=n).round(3),
        }
    )


def near_unique_frame() -> pl.DataFrame:
    """An exact FD whose determinant has 4,500 distinct values over
    5,000 rows (92% of its groups are singletons): the agreement is
    what chance predicts for a determinant that fine."""
    rng = np.random.default_rng(3)
    n = 5000
    det = np.arange(n) % 4500
    mapping = rng.integers(0, 6, size=4500)
    return pl.DataFrame(
        {
            "det": [f"d{v:04d}" for v in det],
            "target": [f"t{v}" for v in mapping[det]],
            "other": rng.normal(size=n).round(3),
        }
    )


def triple_key_frame() -> pl.DataFrame:
    """Rows identified only by (a, b, c) jointly: 10 x 10 x 10 grid.
    The measure column repeats (250 distinct values), so no column
    is a key by itself."""
    i = np.arange(1000)
    return pl.DataFrame(
        {
            "a": [f"a{v}" for v in i % 10],
            "b": [f"b{v}" for v in (i // 10) % 10],
            "c": [f"c{v}" for v in (i // 100) % 10],
            "val": (i % 250) * 0.5,
        }
    )


def triple_fd_frame() -> pl.DataFrame:
    """d = (a + b + c) % 4 with 10-value columns: no pair determines
    d, the triple does, and the triple is not a key."""
    rng = np.random.default_rng(11)
    n = 5000
    a = rng.integers(0, 10, size=n)
    b = rng.integers(0, 10, size=n)
    c = rng.integers(0, 10, size=n)
    return pl.DataFrame(
        {
            "a": [f"a{v}" for v in a],
            "b": [f"b{v}" for v in b],
            "c": [f"c{v}" for v in c],
            "d": [f"d{v}" for v in (a + b + c) % 4],
        }
    )


def single_key_frame() -> pl.DataFrame:
    i = np.arange(1000)
    return pl.DataFrame(
        {
            "row_id": [f"r{v:04d}" for v in i],
            "grp": [f"g{v}" for v in i % 7],
            "val": np.random.default_rng(5).normal(size=1000).round(3),
        }
    )


# ------------------------------------------------------------------
# Helpers
# ------------------------------------------------------------------


def fd_findings(report, target):
    out = []
    for pool in [report.findings, getattr(report, "grouped_findings", [])]:
        for f in pool:
            subj = f.get("subject") or {}
            if subj.get("target") == target and "functional" in (f.get("type") or ""):
                out.append(f)
    return out


def finding_metrics(f):
    """Merged metrics over a finding's evidence blocks (and any
    top-level metrics), first writer wins."""
    merged = {}
    blocks = list(f.get("evidence") or [])
    if f.get("metrics"):
        blocks.append({"metrics": f["metrics"]})
    for block in blocks:
        for key, value in (block.get("metrics") or {}).items():
            merged.setdefault(key, value)
    return merged


def coverage_texts(report):
    cov = report.coverage
    if isinstance(cov, dict):
        return [
            str(v.get("detail", "")) if isinstance(v, dict) else str(v)
            for v in cov.values()
        ]
    return [str(entry) for entry in cov]


def polars_violations(df: pl.DataFrame, lhs: list[str], rhs: str) -> int:
    g = df.group_by(lhs + [rhs]).len()
    tot = g.group_by(lhs).agg(pl.col("len").sum().alias("tot"))
    mx = g.group_by(lhs).agg(pl.col("len").max().alias("mx"))
    j = tot.join(mx, on=lhs)
    return int((j["tot"] - j["mx"]).sum())


# ------------------------------------------------------------------
# #128: chance correction
# ------------------------------------------------------------------


def test_noise_column_never_joins_the_lhs():
    for seed in range(5):
        df = noise_frame(seed)
        report = onceover.profile(df, budget="standard")
        found = fd_findings(report, "p_state")
        assert found, f"seed {seed}: the planted FD must be reported"
        for f in found:
            assert (f.get("subject") or {}).get("inputs") == ["p_zip"], (
                f"seed {seed}: noise column joined the LHS: {f.get('claim')}"
            )
        # The claim's violation count equals an independent recount.
        metrics = finding_metrics(found[0])
        violations = metrics.get("violations", metrics.get("violation_rows"))
        assert violations == polars_violations(df, ["p_zip"], "p_state")


def test_genuine_two_column_fd_reports_both_columns():
    report = onceover.profile(two_column_frame(), budget="standard")
    found = fd_findings(report, "y")
    assert any(
        sorted((f.get("subject") or {}).get("inputs") or []) == ["a", "b"]
        for f in found
    ), "the genuine pair (a, b) must be the reported determinant"


def test_near_unique_determinant_scores_near_chance():
    report = onceover.profile(near_unique_frame(), budget="standard")
    for f in fd_findings(report, "target"):
        # Emitted or not is a selection question; if it is emitted,
        # its detector score must sit near the chance level, not at
        # the breadth score (~0.82) the old formula assigned.
        assert (f.get("detector_interestingness") or 0) < 0.15


# ------------------------------------------------------------------
# #132 item 2: standard-tier size-3 step
# ------------------------------------------------------------------


def test_triple_key_found_at_standard_budget():
    report = onceover.profile(triple_key_frame(), budget="standard")
    claims = [f.get("claim") or "" for f in report.findings]
    assert any("jointly identify rows" in c for c in claims), claims
    assert any("3-column" in c for c in coverage_texts(report)), report.coverage


def test_triple_fd_found_at_standard_budget():
    report = onceover.profile(triple_fd_frame(), budget="standard")
    found = fd_findings(report, "d")
    assert any(
        sorted((f.get("subject") or {}).get("inputs") or []) == ["a", "b", "c"]
        for f in found
    ), "the size-3 determinant must be reported at standard budget"
    assert any("size-3" in c for c in coverage_texts(report)), report.coverage


def test_single_column_key_skips_the_wider_key_search():
    # A table that already has a single-column key must not pay for
    # the size-3 key search. The size-3 determinant step is a
    # separate, per-target trigger: a unique column is never an
    # eligible determinant, so grp and val still count as
    # unexplained by the determinant pool and the step's disclosure
    # names both targets.
    report = onceover.profile(single_key_frame(), budget="standard")
    assert not any("3-column" in c for c in coverage_texts(report)), report.coverage
    assert any(
        "size-3 determinants screened for 2 of 2 targets" in c
        for c in coverage_texts(report)
    ), report.coverage
