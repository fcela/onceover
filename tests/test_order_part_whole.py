"""Part-against-whole order violations, end to end.

A part sits far from its total by construction, so the
systematic order-violation rule's comparable-scale gate cannot
pass for the pair, and the identity scan states a decomposition
only when it holds on at least 99.9% of rows. These tests pin
the part/whole form of the order_violation finding: a part
corrupted to exceed its whole in bulk is stated with its count,
a healthy pair is not, and an exact decomposition is stated by
the identity finding alone, never twice.
"""

import polars as pl

import onceover


def _decomposition(n=2000, corrupt=0):
    rest = [5.0 + ((k * 53) % 40) for k in range(n)]
    part = [10.0 + ((k * 29) % 40) for k in range(n)]
    total = [p + r for p, r in zip(part, rest)]
    for k in range(corrupt):
        part[k] = total[k] + 5.0
    return pl.DataFrame({"part": part, "rest": rest, "total": total})


def _pair_findings(report, lower, upper):
    out = []
    for finding in report.findings:
        for ev in finding.get("evidence", []):
            metrics = ev.get("metrics", {})
            if metrics.get("lower") == lower and metrics.get("upper") == upper:
                out.append((finding, metrics))
    return out


def test_corrupted_part_is_stated_with_its_count():
    report = onceover.profile(_decomposition(corrupt=480))
    hits = [
        (f, m)
        for f, m in _pair_findings(report, "part", "total")
        if f["finding_type"] == "order_violation"
    ]
    assert len(hits) == 1
    _, metrics = hits[0]
    assert metrics["violations"] == 480
    assert metrics["violation_kind"] == "part_whole"
    assert metrics["part_whole_basis"] == "identity"


def test_healthy_part_whole_pair_has_no_order_violation():
    report = onceover.profile(_decomposition())
    hits = [
        f
        for f, _ in _pair_findings(report, "part", "total")
        if f["finding_type"] == "order_violation"
    ]
    assert hits == []


def test_exact_decomposition_is_not_double_stated():
    # The decomposition holds on every row: the identity finding
    # states it, and no order_violation fires for the pair.
    report = onceover.profile(_decomposition())
    identity = [
        f
        for f in report.findings
        if f["finding_type"] == "functional"
        and "total" in f["claim"]
        and "part" in f["claim"]
    ]
    assert identity, "the exact decomposition should be stated"
    violations = [
        f
        for f, _ in _pair_findings(report, "part", "total")
        if f["finding_type"] == "order_violation"
    ]
    assert violations == []
