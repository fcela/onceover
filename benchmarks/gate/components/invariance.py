"""Invariance: the stated facts should not depend on how the table is
laid out.

Five transforms are applied one at a time. A fact is identified by
(category, type, sorted original column names), after mapping renamed
columns back. The metric for a transform is the number of facts in the
untransformed report that are missing from the transformed one, plus
the number that appear only in the transformed one.

    reverse     the column order is reversed
    row_shuffle the rows are shuffled (skipped for tables whose frame
                model finds a time axis, where row order carries meaning)
    rename      every column name gets the prefix "r_" (a prefix, because
                the library reads the end of a name, such as "_id", to
                assign a column its role; a suffix would change those
                roles and the comparison would measure name semantics)
    noise       one independent standard-normal column is added
    scale       one numeric column is multiplied by 1000

Exclusions, applied to both reports before comparing:

    all transforms  category "column_ordering": states the order of the
                    columns, which `reverse` changes by definition.
    all transforms  facts that name the added noise column (noise only).
    row_shuffle     types "sorted_by_index" and "ordering": states the
                    order of the rows.
    scale           facts of type "arithmetic_identity", "order_constraint"
                    and "order_violation" that name the scaled column:
                    a sum or an order between columns of different units
                    changes when one column is rescaled.
    scale           facts of category "formats" that name the scaled
                    column: digit and boundary statements depend on the
                    unit the values are written in.

The scaled column is the first numeric column, by name, with at least 30
distinct values and no null.
"""
from __future__ import annotations

import numpy as np
import polars as pl

import gatelib as g

NAME = "invariance"
DESCRIPTION = "Facts lost or gained under column reorder, row shuffle, rename, an added noise column, and a rescaled column."

TRANSFORMS = ["reverse", "row_shuffle", "rename", "noise", "scale"]
METRICS = {t: {"direction": "lower", "kind": "count"} for t in TRANSFORMS}
METRICS["total"] = {"direction": "lower", "kind": "count"}

PREFIX = "r_"
NOISE = "zz_noise_column"
SCALE_FACTOR = 1000
ROW_ORDER_TYPES = {"sorted_by_index", "ordering"}
UNIT_DEPENDENT_TYPES = {"arithmetic_identity", "order_constraint", "order_violation"}


def rename_hints(obj, mapping: dict):
    """Rename every column name appearing as a string or a dict key."""
    if isinstance(obj, str):
        return mapping.get(obj, obj)
    if isinstance(obj, list):
        return [rename_hints(o, mapping) for o in obj]
    if isinstance(obj, dict):
        return {mapping.get(k, k): rename_hints(v, mapping) for k, v in obj.items()}
    return obj


def facts(prof, rename: dict | None, drop) -> set:
    out = set()
    for f in prof.findings:
        key = g.fact_key(f, rename)
        if drop(f, key):
            continue
        out.add(key)
    return out


def scale_column(df: pl.DataFrame):
    for c in sorted(df.columns):
        s = df[c]
        if s.dtype in (pl.Float32, pl.Float64, pl.Int32, pl.Int64) and not s.null_count() \
                and s.n_unique() >= 30:
            return c
    return None


def run(ctx) -> dict:
    base = ctx.profile()
    df = ctx.df
    detail = {}
    out = {t: 0 for t in TRANSFORMS}
    time_axis = ctx.has_time_axis()
    scaled = scale_column(df)

    def compare(name, frame, rename=None, extra_drop=None, hints=None, target=None):
        prof = ctx.profile(frame, hints=hints if hints is not None else ctx.hints)
        def drop(f, key):
            if f.get("category") == "column_ordering":
                return True
            return bool(extra_drop and extra_drop(f, key))
        a = facts(base, None, drop)
        b = facts(prof, rename, drop)
        lost, gained = sorted(a - b), sorted(b - a)
        out[name] = len(lost) + len(gained)
        detail[name] = {"lost": len(lost), "gained": len(gained),
                        "examples_lost": [list(k[:2]) + [list(k[2])] for k in lost[:5]],
                        "examples_gained": [list(k[:2]) + [list(k[2])] for k in gained[:5]]}

    compare("reverse", df.select(list(reversed(df.columns))))

    if time_axis:
        detail["row_shuffle"] = {"status": "skipped", "reason": "the table has a time axis"}
    else:
        rng = np.random.default_rng(ctx.seeds[0] if ctx.seeds else 0)
        perm = pl.Series(rng.permutation(df.height))
        shuffled = df.select([pl.col(c).gather(perm) for c in df.columns])
        compare("row_shuffle", shuffled,
                extra_drop=lambda f, k: f.get("type") in ROW_ORDER_TYPES)

    mapping = {c: PREFIX + c for c in df.columns}
    back = {v: k for k, v in mapping.items()}
    compare("rename", df.rename(mapping), rename=back,
            hints=rename_hints(ctx.hints, mapping) if ctx.hints else None)

    rng = np.random.default_rng(ctx.seeds[0] if ctx.seeds else 0)
    noisy = df.with_columns(pl.Series(NOISE, rng.normal(size=df.height)))
    compare("noise", noisy, extra_drop=lambda f, k: NOISE in k[2])

    if scaled is None:
        detail["scale"] = {"status": "skipped", "reason": "no suitable numeric column"}
    else:
        scaled_df = df.with_columns((pl.col(scaled) * SCALE_FACTOR).alias(scaled))
        compare("scale", scaled_df,
                extra_drop=lambda f, k: scaled in k[2] and (
                    f.get("type") in UNIT_DEPENDENT_TYPES or f.get("category") == "formats"))
        detail["scale"]["column"] = scaled

    out["total"] = sum(out[t] for t in TRANSFORMS)
    detail["time_axis"] = time_axis
    return {"metrics": out, "detail": detail}
