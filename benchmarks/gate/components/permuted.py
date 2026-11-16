"""Silence on permuted tables.

Every column of a real table is shuffled independently with a seeded
generator. Each column keeps its values, so the marginal distributions
are unchanged, and every relationship between columns is destroyed. A
statement about two or more columns in the profile of the shuffled
table is therefore false by construction. The metrics count those
statements in the full report and in the 4,000-character digest.

A statement counts as a relationship when the finding names two or more
columns and its category is one of gatelib.RELATION_CATEGORIES (or its
type is in gatelib.RELATION_TYPES). The count is of findings in the
report, and in the digest of the statements the digest lines stand for
(a line marked "(x5)" stands for five).

Some of these statements follow from the marginals alone, for example
"a <= b" on two columns whose value ranges do not overlap. They are
counted: they are still statements about a relationship that does not
exist.
"""
from __future__ import annotations

import numpy as np
import polars as pl

import gatelib as g

NAME = "permuted"
DESCRIPTION = "False relationship statements on tables with every column shuffled independently."
DIGEST_CHARS = 4000

METRICS = {
    "false_full": {"direction": "lower", "kind": "count"},
    "false_4k": {"direction": "lower", "kind": "count"},
    "false_full_target": {"direction": "lower", "kind": "count"},
    "false_4k_target": {"direction": "lower", "kind": "count"},
    "findings_full": {"direction": "info", "kind": "count"},
}


def shuffle_columns(df: pl.DataFrame, seed: int) -> pl.DataFrame:
    rng = np.random.default_rng(seed)
    return df.with_columns([
        pl.col(c).gather(pl.Series(rng.permutation(df.height))) for c in df.columns])


def verify_shuffle(original: pl.DataFrame, shuffled: pl.DataFrame) -> dict:
    """Checked with Polars, outside the library: every column keeps its
    multiset of values, and each column with more than one distinct
    value is, for nearly all of them, in a different order (a heavily
    imbalanced column can keep its order by chance)."""
    for c in original.columns:
        a, b = original[c], shuffled[c]
        if not a.sort(nulls_last=True).equals(b.sort(nulls_last=True)):
            raise AssertionError(f"column {c} changed its values")
    moved = 0
    for c in original.columns:
        if original[c].n_unique() > 1 and not original[c].equals(shuffled[c]):
            moved += 1
    multi = sum(1 for c in original.columns if original[c].n_unique() > 1)
    if moved < 0.9 * multi:
        raise AssertionError(f"only {moved} of {multi} columns changed their row order")
    return {"columns": original.width, "columns_moved": moved}


def count_false(prof) -> tuple[int, int, int]:
    full = sum(1 for f in prof.findings if g.is_relationship(f))
    dig = sum(l.count for l in prof.lines(DIGEST_CHARS)
              if l.is_finding and l.lead is not None and g.is_relationship(l.lead))
    return full, dig, len(prof.findings)


def run(ctx) -> dict:
    out = {k: 0 for k in METRICS}
    detail = {}
    for seed in ctx.seeds:
        shuffled = shuffle_columns(ctx.df, seed)
        check = verify_shuffle(ctx.df, shuffled)
        prof = ctx.profile(shuffled)
        full, dig, n = count_false(prof)
        out["false_full"] += full
        out["false_4k"] += dig
        out["findings_full"] += n
        rec = {**check, "false_full": full, "false_4k": dig, "findings": n}
        if ctx.target:
            tprof = ctx.profile(shuffled, target=ctx.target)
            tfull, tdig, _ = count_false(tprof)
            out["false_full_target"] += tfull
            out["false_4k_target"] += tdig
            rec.update(false_full_target=tfull, false_4k_target=tdig)
        detail[f"seed{seed}"] = rec
    return {"metrics": out, "detail": detail}
