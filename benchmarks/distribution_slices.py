"""Benchmark split criteria and compare the shipped slice detectors.

    python benchmarks/distribution_slices.py --stage criteria
    python benchmarks/distribution_slices.py --stage detectors
"""

import argparse
import time
from collections import defaultdict

import numpy as np
import onceover
import polars as pl
from scipy.stats import hypergeom


SHAPES = (
    "location", "upper_tail", "rare_tail", "spread", "bimodal", "skew",
    "tied", "interaction", "numeric_interaction", "outliers",
    "categorical", "categorical_diffuse", "categorical_rare", "categorical_rare_depleted",
)
OPS = {"eq": np.equal, "gt": np.greater, "le": np.less_equal}
BINARY_RATES = {
    "categorical": (0.55, 0.08),
    "categorical_rare": (0.07, 0.012),
    "categorical_rare_depleted": (0.0, 0.026),
    "null_categorical_rare": (0.07, 0.012),
}


def fixture(case, seed, n=3200):
    rng = np.random.default_rng(seed)
    a = rng.integers(0, 4, n).astype(str)
    b = rng.integers(0, 4, n).astype(str)
    x, z = rng.uniform(size=(2, n))
    hit = a == "1"
    if case == "upper_tail":
        hit &= b == "2"
    if case == "numeric_interaction":
        hit = (x <= 0.5) & (z > 0.5)
    y = rng.normal(size=n)
    m = int(hit.sum())
    signs = rng.choice([-1, 1], size=m)
    if case == "location":
        y[hit] += 1.8
    elif case == "upper_tail":
        y[hit & (rng.uniform(size=n) < 0.45)] += 8
    elif case == "rare_tail":
        y[hit & (rng.uniform(size=n) < 0.16)] += 16
    elif case == "spread":
        y[hit] *= 2.6
    elif case == "bimodal":
        y[hit] = signs * 1.5 + rng.normal(scale=0.18, size=m)
    elif case == "skew":
        y[hit] = rng.exponential(size=m) - np.log(2)
    elif case == "tied":
        y = np.where(np.abs(y) < 1.6, 0., np.sign(y))
        y[hit] = rng.choice([-1., 0., 1.], size=m, p=[0.3, 0.4, 0.3])
    elif case == "interaction":
        # Every level of a or b contains 25% narrow rows.
        y[a == b] *= 0.24
        hit = (a == "1") & (b == "1")
    elif case == "numeric_interaction":
        # A checkerboard: at these cuts neither axis alone changes the spread.
        y[(x <= 0.5) != (z <= 0.5)] *= 0.18
    if case == "outliers":
        y[hit] = signs * 1.5 + rng.normal(scale=0.18, size=m)
        y[rng.choice(n, size=6, replace=False)] *= 1e6
    names = {"a": a, "b": b}
    names.update({f"noise{i}": rng.integers(0, 4, n).astype(str) for i in range(4)})
    if case == "numeric_interaction":
        names.update(x=x, z=z)
    if case in BINARY_RATES:
        in_rate, out_rate = BINARY_RATES[case]
        y = np.where(rng.uniform(size=n) < np.where(hit, in_rate, out_rate), "yes", "no")
    elif case in ("categorical_diffuse", "null_categorical"):
        letters = np.array(list("abcdefgh"))
        y = rng.choice(letters, size=n)
        y[hit] = rng.choice(letters, size=m, p=[0.18] * 4 + [0.07] * 4)
    if case.startswith("null"):
        y = rng.permutation(y)
    if case == "null_wide":
        names.update({f"measure{i}": rng.normal(size=n) for i in range(6)})
    names["response"] = y
    return names, hit


def jaccard(a, b):
    return np.count_nonzero(a & b) / max(1, np.count_nonzero(a | b))


def match(mask, truth, symmetric=True):
    return max(jaccard(mask, truth), jaccard(~mask, truth) if symmetric else 0) >= 0.75


def rules(data, train):
    columns = []
    for name, x in data.items():
        if name == "response":
            continue
        if x.dtype.kind == "f":
            cuts = np.quantile(x[train], (0.125, 0.25, 0.5, 0.75, 0.875), method="lower")
            tests = [test for cut in cuts for test in (x <= cut, x > cut)]
        else:
            tests = [x == level for level in sorted(set(x[train]))]
        columns.append(tests)
    masks = [mask for col in columns for mask in col]
    masks.extend(a & b for i, col in enumerate(columns) for other in columns[i + 1:]
                 for a in col for b in other)
    masks = np.array(masks)
    counts = masks[:, train].sum(axis=1)
    return masks[(counts >= 25) & (counts <= train.sum() - 25)]


def criteria(data, truth, seed):
    all_y = data["response"]
    train = np.random.default_rng(seed + 90_201).permutation(len(all_y)) < len(all_y) // 2
    masks = rules(data, train)
    y = all_y[train]
    order = np.argsort(y, kind="stable")
    sorted_y = y[order]
    cuts = np.unique(sorted_y[np.floor((len(y) - 1) * np.r_[.01, np.arange(1, 32) / 32, .99]).astype(int)])
    cuts = cuts[cuts < sorted_y[-1]]
    left = masks[:, train]
    n = left.sum(axis=1)
    w = n / len(y)
    weight = w * (1 - w)
    start = time.perf_counter()
    cdf_total = np.searchsorted(sorted_y, cuts, side="right")
    cdf_left = np.add.reduceat(left[:, order], np.r_[0, cdf_total[:-1]], axis=1)
    cdf_left[:, -1] -= left[:, order[cdf_total[-1]:]].sum(axis=1)
    cdf_left = cdf_left.cumsum(axis=1)
    gaps = cdf_left / n[:, None] - (cdf_total - cdf_left) / (len(y) - n[:, None])
    best = np.abs(gaps).argmax(axis=1)
    eligible = np.abs(gaps[np.arange(len(n)), best]) >= .12
    grid_time = time.perf_counter() - start
    # Exact empirical CRPS improvement and an isotropic Gaussian MMD approximation
    # using the RFF construction and a median distance bandwidth.
    scores = {}
    elapsed = {}
    for method in ("grid_cdf", "exact_crps", "mmd_rff"):
        start = time.perf_counter()
        if method == "grid_cdf":
            distance = np.square(gaps).mean(axis=1)
        elif method == "exact_crps":
            cumsum = np.cumsum(left[:, order], axis=1)[:, :-1]
            delta = cumsum / n[:, None] - (
                np.arange(1, len(y)) - cumsum
            ) / (len(y) - n[:, None])
            distance = np.square(delta) @ np.diff(sorted_y)
        else:
            rng = np.random.default_rng(2707)
            bandwidth = np.median(np.abs(rng.choice(y, 4096) - rng.choice(y, 4096)))
            freq = rng.normal(size=128) / (bandwidth or np.ptp(y) or 1)
            phase = rng.uniform(0, 2 * np.pi, size=freq.size)
            features = np.cos(y[:, None] * freq + phase) * np.sqrt(2 / freq.size)
            means = left @ features
            delta = means / n[:, None] - (features.sum(axis=0) - means) / (len(y) - n[:, None])
            distance = np.square(delta).sum(axis=1)
        scores[method] = distance * weight
        elapsed[method] = time.perf_counter() - start + (grid_time if method == "grid_cdf" else 0)
    result = {}
    for method, score in scores.items():
        proposed = []
        partitions = set()
        for j in np.argsort(-score, kind="stable"):
            if not eligible[j]:
                continue
            key = min(left[j].tobytes(), (~left[j]).tobytes())
            if key in partitions:
                continue
            partitions.add(key)
            side = masks[j, ~train]
            a, b = all_y[~train][side], all_y[~train][~side]
            if min(len(a), len(b)) < 25:
                proposed.append((j, 1.))
            else:
                k = cuts[best[j]]
                i, o = (a <= k).sum(), (b <= k).sum()
                if gaps[j, best[j]] < 0:
                    i, o = len(a) - i, len(b) - o
                delta = i / len(a) - o / len(b)
                p = hypergeom.sf(i - 1, len(a) + len(b), i + o, len(a))
                proposed.append((j, p if delta >= .12 else 1.))
            if len(proposed) >= 8:
                break
        confirmed = []
        for rank, (j, p) in enumerate(sorted(proposed, key=lambda item: item[1])):
            if p > .05 / (len(proposed) - rank):
                break
            confirmed.append(j)
        result[method] = (
            int(any(match(masks[j], truth) for j, _ in proposed)),
            int(any(match(masks[j], truth) for j in confirmed)),
            len(confirmed),
            elapsed[method],
        )
    return result


def detectors(data, truth, case, ids):
    df = pl.DataFrame(data)
    result = {}
    for name in ids:
        start = time.perf_counter()
        report = onceover.profile(df, detectors=[name], target="response")
        elapsed = time.perf_counter() - start
        findings = report.select(type="slice_shift")
        hits = [f for f in findings if set(f["subject"]["columns"]) - {
            p["column"] for p in f["specification"]["conditions"]
        } == {"response"}]
        if not case.startswith("null"):
            findings = hits
        matched = False
        for hit in hits:
            if case in BINARY_RATES and not case.startswith("null"):
                m = hit["evidence"][0]["metrics"]
                if m.get("target_level") != "yes" and "yes" not in m.get("holdout", {}).get("selected_levels", []):
                    continue
            mask = np.ones(len(df), dtype=bool)
            for p in hit["specification"]["conditions"]:
                mask &= OPS[p["operator"]](data[p["column"]], p["value"])
            matched |= match(mask, truth, symmetric=not case.startswith("categorical"))
        result[name] = (int(matched), len(findings), elapsed)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", choices=["criteria", "detectors"], required=True)
    parser.add_argument("--seeds", type=int, default=16)
    parser.add_argument("--null-seeds", type=int, default=64)
    parser.add_argument("--detectors", default="slice_shifts,distribution_slices")
    args = parser.parse_args()
    for case in (*SHAPES, "null", "null_wide", "null_categorical", "null_categorical_rare"):
        if args.stage == "criteria" and ("categorical" in case or case == "null_wide"):
            continue
        values = defaultdict(list)
        for seed in range(args.null_seeds if case.startswith("null") else args.seeds):
            rows = 12_000 if "categorical_rare" in case else 600 if case == "null_wide" else 3200
            data, truth = fixture(case, seed, n=rows)
            if args.stage == "criteria":
                result = criteria(data, truth, seed)
            else:
                result = detectors(data, truth, case, args.detectors.split(","))
            for name, value in result.items():
                if case.startswith("null"):
                    value = (0,) * (len(value) - 2) + value[-2:]
                values[name].append(value)
        for name, rows in values.items():
            arr = np.asarray(rows)
            print(f"{case:20s} {name:22s} counts={arr[:, :-1].sum(axis=0).astype(int).tolist()}"
                  f" runs_with_hits={np.count_nonzero(arr[:, -2])}/{len(rows)}"
                  f" median_ms={1000 * np.median(arr[:, -1]):.2f}", flush=True)


if __name__ == "__main__":
    main()
