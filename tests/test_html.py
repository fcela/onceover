"""The HTML report: a self-contained rendering of the same report
object the JSON contract defines. Every claim in the JSON must
appear in the HTML, the file must make no external requests, and
the rendering must say what to install when the html extra is
missing.
"""

import html as html_lib
import re
import sys

import polars as pl
import pytest

import onceover
from onceover._cli import main as cli_main

_EXTERNAL_REF = re.compile(r"""(?:src|href)\s*=\s*["']https?:""")


def orders():
    names = ["north", "south", "east", "west"]
    rows = [
        {
            "region_id": i % 4,
            "region": names[i % 4],
            "amount": float((i * 37) % 91),
            "quantity": (i * 13) % 40,
        }
        for i in range(400)
    ]
    return pl.DataFrame(rows)


def render(report):
    pytest.importorskip("altair")
    pytest.importorskip("vl_convert")
    return report.to_html()


def test_html_carries_every_claim_and_column():
    report = onceover.profile(orders(), budget="standard")
    text = render(report)
    data = report.data
    assert html_lib.escape(data["overview"]["headline"]) in text
    for finding in data["findings"]:
        assert html_lib.escape(finding["claim"]) in text
    for finding in data.get("grouped_findings") or []:
        assert html_lib.escape(finding["claim"]) in text
    for name in data["columns"]:
        assert html_lib.escape(name) in text
    assert str(data["overview"]["n_rows"]) in text
    assert str(data["overview"]["n_columns"]) in text


def test_html_is_self_contained():
    report = onceover.profile(orders(), budget="standard")
    text = render(report)
    assert text.startswith("<!DOCTYPE html>")
    # No external requests: no remote src/href, no CDN, no linked assets.
    assert not _EXTERNAL_REF.search(text)
    assert "<script src" not in text
    assert "<link" not in text
    assert "cdn." not in text
    # The Vega bundle is inlined once, however many charts follow:
    # per-chart bundles would put a multi-chart report over 2 MB.
    assert text.count("vegaEmbed('#vis-") >= 3
    assert len(text) < 2_000_000


def test_html_charts_agree_with_the_report():
    report = onceover.profile(orders(), budget="standard")
    text = render(report)
    findings = report.data["findings"]
    categories = {f["category"] for f in findings}
    # One findings-table row per selected finding, plus header rows.
    assert text.count("<tr>") >= len(findings)
    for category in categories:
        assert category in text
    # The category chart's spec is embedded as Vega-Lite.
    assert '"vega-lite"' in text or "vega-lite" in text


def test_html_rendering_is_deterministic():
    report = onceover.profile(orders(), budget="standard")
    assert render(report) == render(report)


def test_save_html_matches_to_html(tmp_path):
    pytest.importorskip("altair")
    pytest.importorskip("vl_convert")
    report = onceover.profile(orders(), budget="standard")
    path = str(tmp_path / "report.html")
    assert report.save(path, fmt="html") == path
    with open(path, encoding="utf-8") as fh:
        assert fh.read() == report.to_html()


def test_html_without_the_extra_names_the_extra(monkeypatch):
    report = onceover.profile(orders(), budget="quick")
    monkeypatch.setitem(sys.modules, "altair", None)
    monkeypatch.setitem(sys.modules, "vl_convert", None)
    with pytest.raises(ImportError, match=r"onceover\[html\]"):
        report.to_html()


def test_cli_html_output_file(tmp_path, capsys):
    pytest.importorskip("altair")
    pytest.importorskip("vl_convert")
    csv = tmp_path / "orders.csv"
    orders().write_csv(csv)
    out = tmp_path / "report.html"
    assert cli_main([str(csv), "--format", "html", "-o", str(out)]) == 0
    text = out.read_text(encoding="utf-8")
    assert text.startswith("<!DOCTYPE html>")
    assert not _EXTERNAL_REF.search(text)
