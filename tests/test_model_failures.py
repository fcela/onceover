"""Model-failure slices (issue #299): profile(df, target="y") adds
the model_failures pass, which cross-fits the house GBM to the
target and states slices where the model's out-of-fold errors are
one-sided (bias) or materially larger than its overall error
(accuracy). Without a target the profile is unchanged."""

import random
import re

import polars as pl

import onceover


def failure_findings(df, **kwargs):
    r = onceover.profile(df, budget="standard", **kwargs)
    return r, [f for f in r.findings if f["type"] == "model_failure"]


def _rng(seed):
    return random.Random(seed)


def test_accuracy_slice_fires_for_heteroscedastic_noise():
    # The model fits y = 3a well everywhere, but inside one slice
    # the noise is an order of magnitude larger: the model's errors
    # there are large in both directions (accuracy failure, no
    # bias).
    rng = _rng(29901)
    n = 4000
    a, g, y = [], [], []
    for i in range(n):
        av = rng.random()
        level = "noisy" if i % 20 < 3 else "calm"
        noise = rng.gauss(0, 2.5) if level == "noisy" else rng.gauss(0, 0.2)
        a.append(av)
        g.append(level)
        y.append(3.0 * av + noise)
    df = pl.DataFrame({"a": a, "g": g, "y": y})
    r, out = failure_findings(df, target="y")
    acc = [f for f in out if f["specification"]["form"] == "accuracy"]
    assert acc, f"expected an accuracy finding, coverage: {r.coverage['model_failures']}"
    f = acc[0]
    conds = f["specification"]["conditions"]
    assert any(
        c["column"] == "g" and c["value"] == "noisy" for c in conds
    ), f"conditions: {conds}"
    m = f["evidence"][0]["metrics"]
    assert m["slice_mae"] > 1.5 * m["overall_mae"]
    # The statement names its baseline: overall error in the claim.
    assert "against" in f["claim"]
    assert f["subject"]["target"] == "y"
    assert r.coverage["model_failures"]["status"] == "ran_exact"


def test_bias_slice_fires_for_shifted_cell():
    # A constant +4 shift inside one small conjunction (g = 'D' and
    # z in its top decile, 2.5% of rows). The model absorbs most of
    # the shift but cannot remove it at these settings, leaving
    # one-sided out-of-fold residuals in the cell; the depth-1
    # parent (g = 'D' alone) is not stated.
    rng = _rng(29902)
    n = 8000
    a, g, z, y = [], [], [], []
    for _ in range(n):
        av = rng.random()
        zv = rng.random()
        lev = rng.choice(["A", "B", "C", "D"])
        yv = 4.0 * av + rng.gauss(0, 0.3)
        if lev == "D" and zv > 0.90:
            yv += 4.0
        a.append(av)
        g.append(lev)
        z.append(zv)
        y.append(yv)
    df = pl.DataFrame({"a": a, "g": g, "z": z, "y": y})
    r, out = failure_findings(df, target="y")
    bias = [f for f in out if f["specification"]["form"] == "bias"]
    assert bias, f"expected a bias finding, coverage: {r.coverage['model_failures']}"
    named = [
        f
        for f in bias
        if any(
            c["column"] == "g" and c["value"] == "D"
            for c in f["specification"]["conditions"]
        )
        and any(c["column"] == "z" for c in f["specification"]["conditions"])
    ]
    assert named, f"no bias finding names the planted slice: {[f['claim'] for f in bias]}"
    m = named[0]["evidence"][0]["metrics"]
    assert abs(m["slice_bias"] - m["overall_bias"]) >= 0.5 * m["overall_mae"]
    assert "against" in named[0]["claim"]


def test_reversed_effect_slice_is_stated():
    # The effect of a on y reverses sign inside one conjunction
    # (g = 'D' and z high). The model partially adapts, so the
    # failure is stated in the accuracy form, naming the planted
    # conjunction with its error against the overall error.
    rng = _rng(29908)
    n = 6000
    a, g, z, y = [], [], [], []
    for _ in range(n):
        av = rng.random()
        zv = rng.random()
        lev = rng.choice(["A", "B", "C", "D"])
        if lev == "D" and zv > 0.75:
            yv = -4.0 * av + 6.0 + rng.gauss(0, 0.3)
        else:
            yv = 4.0 * av + rng.gauss(0, 0.3)
        a.append(av)
        g.append(lev)
        z.append(zv)
        y.append(yv)
    df = pl.DataFrame({"a": a, "g": g, "z": z, "y": y})
    r, out = failure_findings(df, target="y")
    named = [
        f
        for f in out
        if any(
            c["column"] == "g" and c["value"] == "D"
            for c in f["specification"]["conditions"]
        )
        and any(c["column"] == "z" for c in f["specification"]["conditions"])
    ]
    assert named, (
        f"no finding names the planted slice, coverage: {r.coverage['model_failures']}"
    )
    m = named[0]["evidence"][0]["metrics"]
    assert m["slice_mae"] >= 1.5 * m["overall_mae"]


def test_uniform_fit_is_silent():
    rng = _rng(29903)
    n = 3000
    a = [rng.random() for _ in range(n)]
    g = [rng.choice(["A", "B", "C"]) for _ in range(n)]
    y = [3.0 * av + rng.gauss(0, 0.25) for av in a]
    df = pl.DataFrame({"a": a, "g": g, "y": y})
    r, out = failure_findings(df, target="y")
    assert out == [], f"unexpected findings: {[f['claim'] for f in out]}"
    assert r.coverage["model_failures"]["status"] == "ran_exact"


def test_no_signal_target_records_chance_and_is_silent():
    rng = _rng(29904)
    n = 3000
    a = [rng.random() for _ in range(n)]
    g = [rng.choice(["A", "B", "C"]) for _ in range(n)]
    y = [rng.gauss(0, 1) for _ in range(n)]
    df = pl.DataFrame({"a": a, "g": g, "y": y})
    r, out = failure_findings(df, target="y")
    assert out == []
    detail = r.coverage["model_failures"]["detail"]
    assert "at or near chance" in detail, detail


def test_binary_target_accuracy_slice_fires():
    # Elsewhere the event is (nearly) determined by a; inside one
    # slice it is a coin flip, so the model's confident predictions
    # there are wrong half the time.
    rng = _rng(29905)
    n = 5000
    a, g, y = [], [], []
    for i in range(n):
        av = rng.random()
        level = "wild" if i % 5 == 0 else "calm"
        if level == "wild":
            event = rng.random() < 0.5
        else:
            event = (av > 0.5) != (rng.random() < 0.02)
        a.append(av)
        g.append(level)
        y.append("yes" if event else "no")
    df = pl.DataFrame({"a": a, "g": g, "y": y})
    r, out = failure_findings(df, target="y")
    acc = [f for f in out if f["specification"]["form"] == "accuracy"]
    assert acc, f"expected an accuracy finding, coverage: {r.coverage['model_failures']}"
    conds = acc[0]["specification"]["conditions"]
    assert any(
        c["column"] == "g" and c["value"] == "wild" for c in conds
    ), f"conditions: {conds}"
    m = acc[0]["evidence"][0]["metrics"]
    assert m["event_label"] == "yes"
    assert m["model_skill"] == "auc"


def test_no_target_skips_the_pass():
    rng = _rng(29906)
    n = 500
    df = pl.DataFrame(
        {
            "a": [rng.random() for _ in range(n)],
            "y": [rng.gauss(0, 1) for _ in range(n)],
        }
    )
    r = onceover.profile(df, budget="standard")
    assert r.coverage["model_failures"]["status"] == "skipped"
    assert [f for f in r.findings if f["type"] == "model_failure"] == []


def test_explicit_selection_without_target_still_skips():
    rng = _rng(29907)
    n = 500
    df = pl.DataFrame(
        {
            "a": [rng.random() for _ in range(n)],
            "y": [rng.gauss(0, 1) for _ in range(n)],
        }
    )
    r = onceover.profile(df, budget="standard", detectors=["model_failures"])
    detail = r.coverage["model_failures"]["detail"]
    assert "no target designated" in detail, detail
    assert [f for f in r.findings if f["type"] == "model_failure"] == []


def test_determining_column_excluded_and_genuine_slice_still_fires():
    # Issue #306: `failed` is derived exactly from `results`,
    # which stays in the frame — the shape of an agent workflow
    # that derives a target and screens it. Outside the wild
    # slice the event is (nearly) determined by `a`; inside wild
    # it is a coin flip, a genuine failure slice on the remaining
    # columns. The screen must exclude `results` from the fit and
    # from the slice variables, say so in coverage, and still
    # state the wild slice.
    rng = _rng(30601)
    n = 5000
    a, g, results, failed = [], [], [], []
    for i in range(n):
        av = rng.random()
        level = "wild" if i % 5 == 0 else "calm"
        if level == "wild":
            event = rng.random() < 0.5
        else:
            event = (av > 0.5) != (rng.random() < 0.02)
        a.append(av)
        g.append(level)
        results.append("Fail" if event else "Pass")
        failed.append("yes" if event else "no")
    df = pl.DataFrame({"a": a, "g": g, "results": results, "failed": failed})
    r, out = failure_findings(df, target="failed")
    detail = r.coverage["model_failures"]["detail"]
    assert "excluded from the model inputs and the slice variables" in detail, detail
    assert "'results'" in detail, detail
    # The fit is no longer the perfect fit the leaked column gave.
    m = re.search(r"out-of-fold AUC ([0-9.]+)", detail)
    assert m and 0.55 < float(m.group(1)) < 1.0, detail
    # No statement conditions on the determining column.
    assert all(
        c["column"] != "results"
        for f in out
        for c in f["specification"]["conditions"]
    ), [f["claim"] for f in out]
    # The genuine failure slice on the remaining columns fires.
    acc = [f for f in out if f["specification"]["form"] == "accuracy"]
    assert any(
        c["column"] == "g" and c["value"] == "wild"
        for f in acc
        for c in f["specification"]["conditions"]
    ), f"expected an accuracy finding on g = 'wild': {[f['claim'] for f in out]}"
    # The sibling leakage screen still states the determination.
    leak = [f for f in r.findings if f["type"] == "leakage_signature"]
    assert any(
        "'results' determines the target 'failed' exactly" in f["claim"]
        for f in leak
    ), [f["claim"] for f in leak]


def test_only_determining_column_leaves_a_chance_model_and_coverage_only():
    # Issue #306: the target is derived from `results` and every
    # other column is noise. Once the determining column is
    # excluded, the remaining model is at chance, so the pass
    # records coverage only — and the coverage names the
    # exclusion.
    rng = _rng(30602)
    n = 4000
    a, g, results, failed = [], [], [], []
    for _ in range(n):
        event = rng.random() < 0.5
        a.append(rng.random())
        g.append(rng.choice(["A", "B", "C"]))
        results.append("Fail" if event else "Pass")
        failed.append("yes" if event else "no")
    df = pl.DataFrame({"a": a, "g": g, "results": results, "failed": failed})
    r, out = failure_findings(df, target="failed")
    assert out == [], f"unexpected findings: {[f['claim'] for f in out]}"
    detail = r.coverage["model_failures"]["detail"]
    assert "excluded from the model inputs and the slice variables" in detail, detail
    assert "'results'" in detail, detail
    assert "at or near chance" in detail, detail


def test_numeric_target_categorical_determiner_is_excluded():
    # Issue #306, numeric path: `y` is constant within each of
    # the 40 levels of the categorical column `src` — an exact
    # determination by the between-group test — and takes enough
    # distinct values to be a numeric target; the only other
    # column is noise. The determiner is excluded before
    # fitting; the remaining model is at chance and the coverage
    # names the exclusion.
    rng = _rng(30603)
    n = 4000
    levels = [f"L{k:02d}" for k in range(40)]
    base = {lev: 1.0 + 0.25 * k for k, lev in enumerate(levels)}
    a, src, y = [], [], []
    for _ in range(n):
        lev = rng.choice(levels)
        src.append(lev)
        a.append(rng.random())
        y.append(base[lev])
    df = pl.DataFrame({"a": a, "src": src, "y": y})
    r, out = failure_findings(df, target="y")
    assert out == [], f"unexpected findings: {[f['claim'] for f in out]}"
    detail = r.coverage["model_failures"]["detail"]
    assert "excluded from the model inputs and the slice variables" in detail, detail
    assert "'src'" in detail, detail
    assert "at or near chance" in detail, detail
