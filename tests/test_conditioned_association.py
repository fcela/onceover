"""Standalone conditioned findings (issue #276).

The Simpson check on strong association statements has two
outcomes that overturn or explain the pooled statement —
reversal within every level of a stratifier, and vanishing
within every level. Since #276 each is also emitted as its own
finding (type `conditioned_association`, form `reversal` or
`vanishing`), ranked as a headline statement rather than only
appended to the pair claim as a qualification.
"""

import polars as pl

import onceover


def _conditioned(report):
    return [f for f in report.findings if f["type"] == "conditioned_association"]


def _reversal_frame() -> pl.DataFrame:
    # Within each level of g, y falls exactly as x rises
    # (r = -1). The hi level's x range is shifted up and its y
    # baseline sits higher still, so the pooled r is strongly
    # positive: the textbook reversal.
    xs, ys, gs = [], [], []
    for g in range(2):
        for i in range(400):
            x = 100.0 * g + i / 4.0
            xs.append(x)
            ys.append(400.0 * g - x)
            gs.append("lo" if g == 0 else "hi")
    return pl.DataFrame({"x": xs, "y": ys, "g": gs})


def _vanishing_frame() -> pl.DataFrame:
    # Within each level of g, y is deterministic noise around
    # the level's own mean, independent of x. The level means
    # of x and y shift together, so the pooled correlation is
    # strong while the within-level correlation is nil.
    xs, ys, gs = [], [], []
    for g in range(2):
        for i in range(3000):
            xs.append(100.0 * g + (i % 100))
            noise = ((i * 2654435761) % 1000) / 1000.0 - 0.5
            ys.append(50.0 * g + 2.0 * noise)
            gs.append("lo" if g == 0 else "hi")
    return pl.DataFrame({"x": xs, "y": ys, "g": gs})


def _homogeneous_frame() -> pl.DataFrame:
    # y rises with x inside both levels and pooled: no
    # conditioning outcome may fire.
    xs, ys, gs = [], [], []
    for g in range(2):
        for i in range(400):
            x = i / 4.0
            xs.append(x)
            ys.append(400.0 * g + x)
            gs.append("lo" if g == 0 else "hi")
    return pl.DataFrame({"x": xs, "y": ys, "g": gs})


def test_reversal_is_a_standalone_finding():
    report = onceover.profile(_reversal_frame())
    hits = [f for f in _conditioned(report) if f["specification"]["form"] == "reversal"]
    assert len(hits) == 1, [f["claim"] for f in _conditioned(report)]
    hit = hits[0]
    assert hit["category"] == "associations"
    assert "reverses within every level of g" in hit["claim"]
    payload = hit["evidence"][0]["metrics"]["simpson"]
    assert payload["form"] == "reversal"
    assert payload["stratifier"] == "g"
    assert payload["pooled_effect"] > 0.3
    assert payload["within_effect"] < -0.9
    # The pair statement keeps its qualification clause.
    pair = [
        f
        for f in report.findings
        if f["type"] == "statistical" and "simpson" in f["evidence"][0]["metrics"]
    ]
    assert len(pair) == 1
    assert "direction reverses within every level of g" in pair[0]["claim"]


def test_vanishing_is_a_standalone_finding():
    report = onceover.profile(_vanishing_frame())
    hits = [f for f in _conditioned(report) if f["specification"]["form"] == "vanishing"]
    assert len(hits) == 1, [f["claim"] for f in _conditioned(report)]
    hit = hits[0]
    assert "vanishes within every level of g" in hit["claim"]
    payload = hit["evidence"][0]["metrics"]["simpson"]
    assert payload["form"] == "vanishing"
    assert payload["stratifier"] == "g"
    assert payload["pooled_effect"] > 0.3
    assert abs(payload["within_effect"]) < 0.1
    assert payload["within_bound"] <= 0.10
    pair = [
        f
        for f in report.findings
        if f["type"] == "statistical" and "simpson" in f["evidence"][0]["metrics"]
    ]
    assert len(pair) == 1
    assert "association vanishes within every level of g" in pair[0]["claim"]


def test_homogeneous_strata_produce_no_conditioned_finding():
    report = onceover.profile(_homogeneous_frame())
    assert _conditioned(report) == []
