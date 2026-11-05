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
            # Schema 0.6 selection outcomes are explicit: a strong
            # plant must be stated — selected, or grouped under a
            # verified relationship family's lead (the categorical
            # FD plant's pair is an exact bijection, so its FD is
            # the family's grouped member). A selected statement
            # must additionally hold its top-25 rank; a grouped one
            # spends no rank by design.
            assert rec["stated_recall"], (name, seed, "stated")
            assert rec["selection_outcome"] == "grouped" or rec["top25"], (
                name, seed, "top25")


def test_dependence_plants_are_stated_at_the_tested_bar():
    # The recorded battery run (see the tested-associations addendum
    # in docs/benchmarks/2026-11-04-quality-battery.md) states both
    # rho = 0.15 and rho = 0.4 at n = 4000 on all 12 seeds: the pair
    # screens now test each pair and control the false-discovery
    # rate across the screened family, so a real association of this
    # size is stated (weak tier) instead of falling under a fixed
    # effect bar. The fast subset pins seeds 0 and 1 at the measured
    # full recall; a miss here is a regression signal.
    for name in ("weak_association", "association_moderate"):
        builder = plants.CORE_PLANTS[name]
        for seed in (0, 1):
            rec = battery.run_plant(builder, seed)
            assert rec["detector_recall"], (name, seed)
            assert rec["stated_recall"], (name, seed)


def test_null_frame_dependence_discoveries_stay_at_the_tested_rate():
    # The null pin is a rate, not a zero: tested screens at a 5%
    # Benjamini-Hochberg level are expected to state a weak
    # association on a null frame occasionally, and a single seed
    # cannot pin a rate, so this runs all 12 battery seeds. The
    # recorded run measured 3 weak-tier statistical findings across
    # the 12 frames (one numeric, two mixed), no strong-tier
    # statement on any null frame, and no dependency or break
    # findings. The bounds below leave headroom over the measured
    # values while still failing on a calibration blowup, which
    # would state dozens. Effect sizes and tiers are read from the
    # findings' evidence, counted once per finding (the battery's
    # per-family counter multiplies by the number of plant specs
    # sharing the family, so this test counts findings directly).
    specs = []
    for builder in plants.CORE_PLANTS.values():
        _, spec = builder(0)
        specs.append(spec)
    statistical = 0
    for seed in range(12):
        rec = battery.run_null(seed, specs)
        assert rec["false_positives"]["dependencies"] == 0, seed
        assert rec["false_positives"]["breaks"] == 0, seed
        df = plants.null_frame(seed)
        full = battery.profile(df, None, restricted=False)
        for f in full.data["findings"]:
            if f.get("type") == "statistical":
                statistical += 1
                metrics = f["evidence"][0]["metrics"]
                assert metrics.get("tier") == "weak", (seed, f["claim"])
            assert f.get("type") != "predictive", (seed, f["claim"])
    assert statistical <= 4, statistical
