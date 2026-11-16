"""Plant catalogue for the planted-recovery component.

A plant takes a real table and a seeded generator and injects one
known structure. It returns `(frame, spec)`, or None when the table
has no column the plant can use. The spec follows the format of
benchmarks/quality/plants.py:

    types    finding types that can state the structure
    columns  columns a stating finding must name
    verify   callable(frame) -> bool, evaluated with Polars/NumPy on
             the generated frame before profiling; the truth never
             comes from the library under test
    target   optional: profile with this target column
    patterns optional: regular expressions that must all match one
             digest line, used when no finding in the line matches

Adding a plant is one function and one line in PLANTS.
"""
from __future__ import annotations

import re

import numpy as np
import polars as pl

NUMERIC = (pl.Int8, pl.Int16, pl.Int32, pl.Int64, pl.UInt8, pl.UInt16,
           pl.UInt32, pl.UInt64, pl.Float32, pl.Float64)


def _numeric_cols(df, min_distinct=30, allow_nulls=False):
    out = []
    for c in df.columns:
        s = df[c]
        if s.dtype not in NUMERIC:
            continue
        if s.null_count() and not allow_nulls:
            continue
        if s.drop_nulls().n_unique() < min_distinct:
            continue
        if s.drop_nulls().std() in (None, 0):
            continue
        # Skip identifiers (nearly every value distinct) and columns
        # dominated by one value (a quartile cut is then degenerate).
        if s.n_unique() > 0.95 * s.len():
            continue
        if s.value_counts()['count'].max() > 0.5 * s.len():
            continue
        out.append(c)
    return out


def _categorical_cols(df, lo=3, hi=12, allow_nulls=False):
    out = []
    for c in df.columns:
        s = df[c]
        if s.dtype not in (pl.String, pl.Categorical):
            continue
        if s.null_count() and not allow_nulls:
            continue
        if lo <= s.n_unique() <= hi:
            out.append(c)
    return out


def _pick(rng, items, k=1):
    items = sorted(items)
    if len(items) < k:
        return None
    idx = rng.choice(len(items), size=k, replace=False)
    return [items[i] for i in idx]


def _fresh(df, base):
    name = base
    while name in df.columns:
        name += "_"
    return name


def duplicate_encoding(df, rng):
    """A relabelled copy of a categorical column: same partition of the
    rows, different level names."""
    pick = _pick(rng, _categorical_cols(df))
    if not pick:
        return None
    col = pick[0]
    levels = sorted(df[col].unique().to_list())
    order = rng.permutation(len(levels))
    mapping = {lv: f"L{order[i]:02d}" for i, lv in enumerate(levels)}
    new = _fresh(df, f"{col}_code")
    out = df.with_columns(pl.col(col).replace_strict(mapping, return_dtype=pl.String).alias(new))

    def verify(d):
        pairs = d.select([col, new]).n_unique()
        return pairs == d[col].n_unique() == d[new].n_unique() > 1

    return out, {"types": {"duplicate_encoding", "functional"}, "columns": {col, new},
                 "verify": verify, "patterns": [re.escape(col), re.escape(new)]}


def sentinel_value(df, rng):
    """A share of a numeric column overwritten with an out-of-range code."""
    pick = _pick(rng, _numeric_cols(df))
    if not pick:
        return None
    col = pick[0]
    s = df[col]
    if s.min() >= 0:
        value = -999
    elif s.max() < 99_999:
        value = 99999
    else:
        return None
    n = df.height
    k = max(int(round(0.04 * n)), 10)
    idx = rng.choice(n, size=k, replace=False)
    mask = np.zeros(n, dtype=bool)
    mask[idx] = True
    cast = s.dtype
    out = df.with_columns(
        pl.when(pl.Series(mask)).then(pl.lit(value).cast(cast))
        .otherwise(pl.col(col)).alias(col))

    def verify(d):
        v = d[col].to_numpy()
        return int((v == value).sum()) == k and int((s.to_numpy() == value).sum()) == 0

    return out, {"types": {"sentinel_values", "boundary_mass"},
                 "columns": {col}, "verify": verify,
                 "patterns": [re.escape(col), r"-?999\b|99,?999"]}


def sum_identity(df, rng):
    """A new column equal to the sum of two numeric columns."""
    pick = _pick(rng, _numeric_cols(df, min_distinct=20), 2)
    if not pick:
        return None
    a, b = pick
    new = _fresh(df, f"{a}_plus_{b}")
    out = df.with_columns((pl.col(a).cast(pl.Float64) + pl.col(b).cast(pl.Float64)).alias(new))

    def verify(d):
        return bool(np.allclose(d[new].to_numpy(), d[a].to_numpy() + d[b].to_numpy()))

    return out, {"types": {"arithmetic_identity", "affine", "functional"}, "columns": {a, b, new}, "verify": verify}


def target_driver(df, rng):
    """A new binary target whose rate rises with the rank of one numeric
    column (about 0.13 in the lowest quartile, 0.87 in the highest)."""
    pick = _pick(rng, _numeric_cols(df))
    if not pick:
        return None
    col = pick[0]
    # Rank-based, so heavy tails do not flatten the relationship.
    u = (df[col].rank("average").to_numpy() - 0.5) / df.height
    p = 1.0 / (1.0 + np.exp(-5.0 * (u - 0.5)))
    y = (rng.random(df.height) < p).astype(int)
    tname = _fresh(df, "planted_y")
    out = df.with_columns(pl.Series(tname, y))

    def verify(d):
        xv = d[col].to_numpy().astype(float)
        yv = d[tname].to_numpy()
        lo, hi = np.quantile(xv, [0.25, 0.75])
        return (yv[xv >= hi].mean() - yv[xv <= lo].mean()) > 0.3

    return out, {"types": {"predictive", "statistical", "level_contrast", "piecewise_relationship",
                           "axis_shape", "if_then"},
                 "columns": {col, tname}, "target": tname, "verify": verify}


def row_anomaly(df, rng):
    """A few rows of one numeric column set far outside its range."""
    pick = _pick(rng, _numeric_cols(df))
    if not pick:
        return None
    col = pick[0]
    s = df[col].cast(pl.Float64)
    mu, sd = s.mean(), s.std()
    n = df.height
    idx = rng.choice(n, size=3, replace=False)
    vals = np.array([mu + 60 * sd, mu + 75 * sd, mu + 90 * sd])
    arr = s.to_numpy().copy()
    arr[idx] = vals
    out = df.with_columns(pl.Series(col, arr))

    def verify(d):
        v = d[col].to_numpy()
        z = np.abs(v - mu) / sd
        return int((z > 50).sum()) == 3 and int((np.abs(s.to_numpy() - mu) / sd > 50).sum()) == 0

    return out, {"types": {"extreme_value"}, "columns": {col}, "verify": verify}


def tied_missingness(df, rng):
    """One numeric column missing on 90% of the rows at one level of a
    categorical column, and nowhere else."""
    cats = _categorical_cols(df, lo=3, hi=12)
    nums = _numeric_cols(df, min_distinct=10)
    cats = [c for c in cats]
    pickc = _pick(rng, cats)
    pickn = _pick(rng, [c for c in nums if c not in (pickc or [])])
    if not pickc or not pickn:
        return None
    cat, num = pickc[0], pickn[0]
    vc = df[cat].value_counts().sort("count", descending=True)
    share = vc["count"] / df.height
    eligible = [lv for lv, sh in zip(vc[cat].to_list(), share.to_list()) if 0.05 <= sh <= 0.4]
    if not eligible:
        return None
    level = eligible[int(rng.integers(len(eligible)))]
    in_level = (df[cat] == level).to_numpy()
    drop = in_level & (rng.random(df.height) < 0.9)
    out = df.with_columns(
        pl.when(pl.Series(drop)).then(None).otherwise(pl.col(num)).alias(num))

    def verify(d):
        miss = d[num].is_null().to_numpy()
        return (miss[in_level].mean() > 0.85 and miss[~in_level].mean() == 0.0
                and not d[cat].is_null().any())

    return out, {"types": {"missing_values", "predictable_missingness"}, "columns": {cat, num},
                 "verify": verify}


# One line per plant: adding a plant is a function above and an entry here.
PLANTS = {
    "duplicate_encoding": duplicate_encoding,
    "sentinel_value": sentinel_value,
    "sum_identity": sum_identity,
    "target_driver": target_driver,
    "row_anomaly": row_anomaly,
    "tied_missingness": tied_missingness,
}
