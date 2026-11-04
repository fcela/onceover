"""Planted-signal generators for the discovery-quality battery (issue #115).

Each builder takes a seed and returns (frame, spec). The frame is
synthetic: every column is generated here, so the ground truth is
known by construction and is verified independently of the library
under test (the battery re-checks each plant's defining property
with NumPy before profiling). The spec carries what a matching
finding looks like: the detector family, the finding types that can
report the signal, and the signal columns a matching finding must
involve.

Every frame also carries independent noise columns (prefix `n_`), so
selection pressure and false-positive opportunities exist on every
run, not only on the null controls.
"""

import numpy as np
import polars as pl

N_NOISE = 4


def _noise(rng: np.random.Generator, n: int, start: int = 1) -> dict:
    cols = {}
    for i in range(start, start + N_NOISE):
        if i % 2:
            cols[f"n_{i}"] = rng.normal(size=n)
        else:
            cols[f"n_{i}"] = rng.choice(["u", "v", "w"], size=n)
    return cols


def _frame(cols: dict) -> pl.DataFrame:
    return pl.DataFrame(cols)


def affine_identity(seed: int):
    """y = 3x + 2 exactly. Family: dependencies."""
    rng = np.random.default_rng(seed)
    n = 4000
    x = rng.normal(size=n)
    cols = {"x": x, "y": 3.0 * x + 2.0, **_noise(rng, n)}
    spec = {"family": "dependencies", "detectors": ["functional_dependencies", "identities"],
            "types": {"functional", "arithmetic_identity"},
            "columns": {"x", "y"}, "response": "y",
            "verify": lambda d: bool(np.allclose(d["y"], 3.0 * d["x"] + 2.0))}
    return _frame(cols), spec


def categorical_fd(seed: int):
    """label is an exact function of code. Family: dependencies."""
    rng = np.random.default_rng(seed)
    n = 4000
    codes = [f"C{i}" for i in range(8)]
    mapping = {c: f"L{(i * 3) % 8}" for i, c in enumerate(codes)}
    code = rng.choice(codes, size=n)
    cols = {"code": code, "label": np.array([mapping[c] for c in code]),
            **_noise(rng, n)}
    spec = {"family": "dependencies", "detectors": ["functional_dependencies"],
            "types": {"functional"},
            "columns": {"code", "label"}, "response": "label",
            "verify": lambda d: all(mapping[c] == lab
                                    for c, lab in zip(d["code"], d["label"]))}
    return _frame(cols), spec


def weak_association(seed: int):
    """Bivariate normal pair with rho = 0.15. Family: dependence."""
    rng = np.random.default_rng(seed)
    n = 4000
    rho = 0.15
    x = rng.normal(size=n)
    y = rho * x + np.sqrt(1 - rho ** 2) * rng.normal(size=n)
    cols = {"x": x, "y": y, **_noise(rng, n)}
    spec = {"family": "dependence", "detectors": ["numeric_dependence"],
            "types": {"statistical", "predictive"},
            "columns": {"x", "y"}, "response": "y",
            "verify": lambda d: abs(np.corrcoef(d["x"], d["y"])[0, 1] - rho) < 0.05}
    return _frame(cols), spec


def association_moderate(seed: int):
    """Bivariate normal pair with rho = 0.4: brackets the dependence
    screen's reporting bar from above. Family: dependence."""
    rng = np.random.default_rng(seed)
    n = 4000
    rho = 0.4
    x = rng.normal(size=n)
    y = rho * x + np.sqrt(1 - rho ** 2) * rng.normal(size=n)
    cols = {"x": x, "y": y, **_noise(rng, n)}
    spec = {"family": "dependence", "detectors": ["numeric_dependence"],
            "types": {"statistical", "predictive"},
            "columns": {"x", "y"}, "response": "y",
            "verify": lambda d: abs(np.corrcoef(d["x"], d["y"])[0, 1] - rho) < 0.05}
    return _frame(cols), spec


def nonlinear_only(seed: int):
    """y = x^2 + noise on symmetric x: no linear signal. Family: dependence."""
    rng = np.random.default_rng(seed)
    n = 4000
    x = rng.uniform(-2, 2, size=n)
    y = x ** 2 + rng.normal(scale=0.05, size=n)
    cols = {"x": x, "y": y, **_noise(rng, n)}
    spec = {"family": "dependence", "detectors": ["numeric_dependence"],
            "types": {"statistical", "predictive", "piecewise_relationship"},
            "columns": {"x", "y"}, "response": "y",
            "verify": lambda d: abs(np.corrcoef(d["x"], d["y"])[0, 1]) < 0.05
            and np.corrcoef(d["x"] ** 2, d["y"])[0, 1] > 0.95}
    return _frame(cols), spec


def interaction_only(seed: int):
    """y marks the sign of x1*x2: each marginal is uninformative.
    Family: interactions."""
    rng = np.random.default_rng(seed)
    n = 6000
    x1 = rng.normal(size=n)
    x2 = rng.normal(size=n)
    y = np.where(x1 * x2 > 0, "P", "Q")
    cols = {"x1": x1, "x2": x2, "y": y, **_noise(rng, n)}
    spec = {"family": "interactions", "detectors": ["interactions"],
            "types": {"interaction_h", "predictive", "conditional_rules", "if_then"},
            "columns": {"x1", "x2", "y"}, "response": "y",
            "verify": lambda d: (
                abs(np.mean(d["y"][d["x1"] > 0] == "P") - 0.5) < 0.03
                and abs(np.mean(d["y"][d["x2"] > 0] == "P") - 0.5) < 0.03
                and abs(np.mean(d["y"][(d["x1"] > 0) & (d["x2"] > 0)] == "P")
                        - 1.0) < 0.03)}
    return _frame(cols), spec


def conditional_extremes(seed: int):
    """Six extreme values planted inside one slice of v.
    Family: surprises (numeric)."""
    rng = np.random.default_rng(seed)
    n = 4000
    g = rng.choice(["P", "Q"], size=n)
    v = rng.normal(size=n)
    idx = np.where(g == "Q")[0][:6]
    v[idx] = 24.0 + rng.random(6) * 5.0
    cols = {"g": g, "v": v, **_noise(rng, n)}
    spec = {"family": "surprises", "detectors": ["surprises"],
            "types": {"extreme_value"},
            "columns": {"g", "v"}, "response": "v",
            "verify": lambda d: int(np.sum(d["v"] > 20)) == 6}
    return _frame(cols), spec


def rare_slice_outlier(seed: int):
    """Three exception rows inside one slice of a categorical column.
    Family: surprises (categorical)."""
    rng = np.random.default_rng(seed)
    n = 5000
    g = rng.choice(["A", "B"], size=n, p=[0.9, 0.1])
    c = np.full(n, "common", dtype=object)
    idx = np.where(g == "B")[0][:3]
    c[idx] = "rare"
    cols = {"g": g, "c": c, **_noise(rng, n)}
    spec = {"family": "surprises", "detectors": ["surprises"],
            "types": {"categorical_outlier"},
            "columns": {"g", "c"}, "response": "c",
            "verify": lambda d: int(np.sum(d["c"] == "rare")) == 3}
    return _frame(cols), spec


def level_shift(seed: int):
    """A +2.5 sigma level shift halfway through a daily series.
    Family: breaks."""
    rng = np.random.default_rng(seed)
    n = 1200
    s = rng.normal(size=n)
    s[n // 2:] += 2.5
    t = np.arange("2023-01-01", n, dtype="datetime64[D]")
    cols = {"t": t, "s": s, **_noise(rng, n)}
    spec = {"family": "breaks", "detectors": ["breaks"],
            "types": {"level_shift"},
            "columns": {"s"}, "response": "s", "hints": {"time": "t"},
            "verify": lambda d: abs(np.mean(d["s"][n // 2:])
                                    - np.mean(d["s"][:n // 2]) - 2.5) < 0.3}
    return _frame(cols), spec


def null_frame(seed: int):
    """Independent columns only: every family match is a false
    positive by construction."""
    rng = np.random.default_rng(seed)
    n = 4000
    cols = _noise(rng, n, start=1)
    cols.update(_noise(rng, n, start=11))
    return _frame(cols)


CORE_PLANTS = {
    "affine_identity": affine_identity,
    "categorical_fd": categorical_fd,
    "weak_association": weak_association,
    "association_moderate": association_moderate,
    "nonlinear_only": nonlinear_only,
    "interaction_only": interaction_only,
    "conditional_extremes": conditional_extremes,
    "rare_slice_outlier": rare_slice_outlier,
    "level_shift": level_shift,
}

# Plants whose response can be permuted for the destroyed-signal
# control (level_shift is excluded: permuting destroys the time
# order itself, not just the association).
PERMUTABLE = ["affine_identity", "categorical_fd", "weak_association",
              "association_moderate", "nonlinear_only", "interaction_only",
              "conditional_extremes", "rare_slice_outlier"]

# Plants that get the 5% MCAR missingness variant.
MISSINGNESS = ["affine_identity", "weak_association", "conditional_extremes"]

# Plants planted beyond column 64 in the wide variant.
WIDE = ["affine_identity", "categorical_fd"]
