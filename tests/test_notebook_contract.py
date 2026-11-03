"""The example notebook's contract with the detector set.

``notebooks/onceover_analysis.ipynb`` is the README's worked example
and the only notebook CI executes against the built wheel. It reads
specific findings out of its profiles (uniqueness, functional
dependencies, the date gap, predictive rules, formats). When the
2026-11-02 detector battery made ``dates`` opt-in, the notebook's
bare ``profile()`` calls stopped producing the date-gap finding and
the wheel job's notebook step failed with ``StopIteration`` — while
every local gate stayed green, because nothing in the gate executed
the notebook or checked what it reads.

These tests pin the two halves of the repair:

- every ``onceover.profile(...)`` call in the notebook passes the
  shared selection, ``ex.notebook_detectors()`` (default set plus
  the opt-in ``dates`` pass), rather than relying on the default
  set alone;
- that selection produces every finding the notebook dereferences,
  including the date gap with the fixture's two missing business
  dates.

The second test needs the notebook's rendering dependencies
(altair, great_tables), which the wheel jobs do not install; it
skips where they are absent. The first test is stdlib-only and
runs in every gate.
"""

import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
NOTEBOOK = ROOT / "notebooks" / "onceover_analysis.ipynb"
EXAMPLE = ROOT / "notebooks" / "onceover_example.py"


def _profile_call_lines():
    # Notebook JSON is UTF-8 by spec; pin the encoding rather than
    # inheriting the locale default. On Windows that default is cp1252,
    # which cannot decode the box-drawing characters in this notebook's
    # stored table output (U+2510 encodes with byte 0x90, undefined in
    # cp1252) — read_text() raised UnicodeDecodeError there and failed
    # the windows-x86_64 wheel job's test step.
    nb = json.loads(NOTEBOOK.read_text(encoding="utf-8"))
    lines = []
    for cell in nb["cells"]:
        if cell["cell_type"] != "code":
            continue
        for line in "".join(cell["source"]).splitlines():
            if "onceover.profile(" in line:
                lines.append(line.strip())
    return lines


def test_notebook_profiles_use_the_shared_selection():
    lines = _profile_call_lines()
    assert len(lines) == 3, lines
    for line in lines:
        assert "detectors=ex.notebook_detectors()" in line, line


def _load_example():
    spec = importlib.util.spec_from_file_location("onceover_example", EXAMPLE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_notebook_selection_produces_the_findings_the_notebook_reads():
    pytest.importorskip("altair")
    pytest.importorskip("great_tables")
    import onceover

    ex = _load_example()
    detectors = ex.notebook_detectors()
    assert "dates" in detectors

    orders = ex.orders_fixture(seed=2024)
    report = onceover.profile(orders, detectors=detectors)

    # The lookups the notebook performs (notebooks/onceover_example.py).
    assert ex.measured(ex.find(report, "uniqueness", column="source_order_ref"))["duplicates"] == 3
    ex.find(report, "dependencies", subtype="functional", form="value_mapping",
            inputs=["region_id"], target="manager")
    ex.find(report, "dependencies", subtype="functional", form="affine",
            inputs=["quantity"], target="double_quantity")
    ex.find(report, "dependencies", subtype="predictive", target="churn",
            inputs=["tenure_months"])
    ex.find(report, "dependencies", subtype="predictive", form="piecewise_linear",
            target="line_total")
    ex.find(report, "formats", column="phone")

    # The regression assertion: the date-gap finding the notebook's
    # summary and section 4 read, with the fixture's two missing
    # business dates.
    gap = ex.find(report, "gaps", column="order_date")
    assert ex.measured(gap)["missing"] == 2

    # The two call sites that raised StopIteration now execute.
    ex.summary(report)
    ex.missing_dates(orders, report)
