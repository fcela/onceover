"""Fast CI subset of the planted-signal battery (issue #115).

Runs two seeds of the battery's strongest plants and asserts the
recalls measured in the full battery run recorded in
docs/benchmarks/2026-11-04-quality-battery.md. The floors are the
measured values: these plants were found on both seeds, by the
family detectors and after selection, in the recorded run, so any
miss here is a regression signal, not a tolerance question. The
full battery (12 seeds, all plants, perturbation variants) runs
from benchmarks/quality/battery.py; it is a script, not a test,
because it takes minutes.

The battery's truth generation needs numpy, which is not a library
dependency (the wheel does not ship it), so this module skips where
numpy is unavailable — the wheel-test environment installs only the
wheel and pytest. Environments that install scipy run it in full.
"""

import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
QDIR = ROOT / "benchmarks" / "quality"

# plants.py and battery.py import numpy at module level; skip the
# whole module where it is absent instead of failing collection.
pytest.importorskip(
    "numpy",
    reason="planted-signal battery needs numpy (not a library dependency)")


def _load(name):
    spec = importlib.util.spec_from_file_location(name, QDIR / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


plants = _load("plants")
battery = _load("battery")

# Plants asserted at full recall: measured detector and
# post-selection recall 1.00 on seeds 0 and 1 in the recorded run.
STRONG_PLANTS = ["affine_identity", "categorical_fd", "nonlinear_only",
                 "interaction_only", "level_shift"]


def test_strong_plants_are_found_on_both_seeds():
    for name in STRONG_PLANTS:
        builder = plants.CORE_PLANTS[name]
        for seed in (0, 1):
            rec = battery.run_plant(builder, seed)
            assert rec["detector_recall"], (name, seed, "detector")
            assert rec["selected_recall"], (name, seed, "selected")
            assert rec["top25"], (name, seed, "top25")


def test_dependence_reporting_bar_is_where_the_battery_measured_it():
    # The recorded battery run found neither rho = 0.15 nor rho = 0.4
    # reported at n = 4000 (dependence coverage reads "0 strong
    # pairwise"; the predictability screen reads "0 predictable").
    # This pins the measured bar: if a change lowers it, the failure
    # here is the signal to update the battery doc and this pin in
    # the same change — the same promotion rule as the corpus
    # known-miss tripwires.
    for name in ("weak_association", "association_moderate"):
        builder = plants.CORE_PLANTS[name]
        for seed in (0, 1):
            rec = battery.run_plant(builder, seed)
            assert not rec["detector_recall"], (name, seed)
            assert not rec["selected_recall"], (name, seed)


def test_null_frame_has_no_dependence_or_dependency_false_positives():
    specs = []
    for builder in plants.CORE_PLANTS.values():
        _, spec = builder(0)
        specs.append(spec)
    rec = battery.run_null(0, specs)
    assert rec["false_positives"]["dependencies"] == 0
    assert rec["false_positives"]["dependence"] == 0
    assert rec["false_positives"]["breaks"] == 0
