"""Notebook rich display (issue #287): a report that is the result
of a notebook cell renders through `_repr_markdown_()`. That method
returned the full markdown report — tens of thousands of characters
on finding-rich tables — instead of the context-budgeted digest
built for this surface. The notebook display is now the digest plus
a one-line footer naming the full surfaces; `to_markdown()` and
plain `repr()` are unchanged.
"""
import polars as pl

import onceover


def _bike_report():
    df = pl.read_csv(
        "benchmarks/corpus/data/anomaly/bike-sharing-hourly/data.csv",
        infer_schema_length=None,
        try_parse_dates=True,
    )
    return onceover.profile(df)


def _small_report():
    df = pl.DataFrame(
        {
            "x": [float(i % 40) for i in range(200)],
            "y": [float(i % 7) for i in range(200)],
        }
    )
    return onceover.profile(df)


def test_repr_markdown_is_digest_sized_on_a_large_report():
    report = _bike_report()
    full = report.to_markdown()
    # Precondition: this fixture's full report is large, so the old
    # notebook display flooded the cell.
    assert len(full) > 50_000
    shown = report._repr_markdown_()
    assert len(shown) < 6_000
    assert shown != full
    # The content is the digest content, not a truncation of the
    # full report: the digest is a prefix of what is shown, and the
    # full report's document head never appears.
    assert shown.startswith(report.to_digest())
    assert shown.startswith("profile digest:")
    assert "# onceover report" not in shown


def test_repr_markdown_footer_names_the_full_surfaces():
    shown = _bike_report()._repr_markdown_()
    assert "report.to_markdown()" in shown
    assert "report.to_digest(n)" in shown
    assert "report.select(...)" in shown


def test_repr_markdown_on_a_small_report():
    report = _small_report()
    shown = report._repr_markdown_()
    assert len(shown) < 6_000
    assert shown.startswith(report.to_digest())
    assert shown.startswith("profile digest:")
    assert "report.to_markdown()" in shown
    # Even a one-finding report's full markdown is longer than its
    # digest display, and the display is not the full document.
    assert shown != report.to_markdown()
    assert "# onceover report" not in shown


def test_to_markdown_and_plain_repr_are_unchanged():
    report = _bike_report()
    full = report.to_markdown()
    assert full.startswith("# onceover report")
    assert len(full) > 50_000
    text = repr(report)
    assert text.startswith("<onceover.ProfileReport:")
    assert len(text) < 1_000
