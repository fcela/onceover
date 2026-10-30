"""Triviality demotions (issue #9): findings that restate the file's
own structure are ranked lower with the reason recorded in their
evidence — never dropped."""
import polars as pl

import onceover


def _by_claim(report, needle):
    return [f for f in report.findings if needle in f["claim"]]


def test_float_key_candidate_is_demoted():
    # An all-distinct float measurement presented as a key candidate
    # declares the continuous-measure demotion; an integer id does not.
    n = 400
    floats = [i * 0.371 + 1.0 for i in range(n)]
    report = onceover.profile(pl.DataFrame({"measure": floats}))
    pk = _by_claim(report, "primary key candidate")
    assert len(pk) == 1
    metrics = pk[0]["evidence"][0]["metrics"]
    assert metrics["triviality"] == "continuous_measure_key_candidate"
    assert metrics["triviality_factor"] == 0.6

    report = onceover.profile(pl.DataFrame({"row_id": list(range(n))}))
    pk = _by_claim(report, "primary key candidate")
    assert len(pk) == 1
    assert "triviality" not in pk[0]["evidence"][0]["metrics"]


def test_capitalization_only_format_mix_is_demoted():
    names = (["acme Corp"] * 150 + ["ACME CORP"] * 130 + ["Globex"] * 120)
    amounts = [float(i % 97) + 0.5 for i in range(400)]
    report = onceover.profile(pl.DataFrame({"vendor": names, "amount": amounts}))
    fmt = _by_claim(report, "mixes value formats")
    assert len(fmt) == 1
    metrics = fmt[0]["evidence"][0]["metrics"]
    assert metrics["triviality"] == "capitalization_only_format_mix"
    assert metrics["triviality_factor"] == 0.6


def test_shape_format_mix_is_not_demoted():
    # Date strings in two shapes ('9999-99-99' vs '99/99/9999') differ
    # by more than letter case: the mix blocks parsing, no demotion.
    dates = (["2024-01-15"] * 200 + ["01/15/2024"] * 200)
    amounts = [float(i % 89) + 0.25 for i in range(400)]
    report = onceover.profile(pl.DataFrame({"d": dates, "amount": amounts}))
    fmt = _by_claim(report, "mixes value formats")
    assert len(fmt) == 1
    assert "triviality" not in fmt[0]["evidence"][0]["metrics"]


def test_long_format_encoding_fd_is_demoted():
    # Long format: (entity) x indicator -> value, with the two
    # indicators occupying disjoint value ranges, so value ->
    # indicator holds by construction of the layout.
    entity, indicator, value = [], [], []
    for e in range(60):
        entity.append(f"E{e:02d}")
        indicator.append("gdp")
        value.append(100.0 + (e % 50))
        entity.append(f"E{e:02d}")
        indicator.append("inflation")
        value.append(2.0 + (e % 8) * 0.5)
    df = pl.DataFrame({"entity": entity, "indicator": indicator, "value": value})
    report = onceover.profile(df)
    assert any(f.get("type") == "pivot" or "long format" in f["claim"] for f in report.findings)
    fds = [
        f for f in report.findings
        if f.get("type") == "functional" and f["subject"].get("target") == "indicator"
    ]
    assert len(fds) == 1, [(f.get("type"), f["subject"]) for f in report.findings]
    metrics = fds[0]["evidence"][0]["metrics"]
    assert metrics["triviality"] == "long_format_encoding"
    assert metrics["triviality_factor"] == 0.6
