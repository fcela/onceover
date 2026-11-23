"""Re-evaluate findings against the data through Polars.

A finding's numbers come from the Rust detectors. The functions here recount them
with Polars expressions, which share no code with those detectors, so a claim that
disagrees with the data shows up as a mismatch. They also return the rows behind a
statement and turn exact statements into constraints for new data.

Statements fitted on a sample or by a model (predictive scores, change points,
rules measured on a holdout) have no exact count to recompute, and report
`not_checkable`.

Counts go through frame expressions rather than Series methods: on Polars 1.x with
Python 3.15, Series methods such as `drop_nulls` return None.
"""
from __future__ import annotations

from typing import Callable, Iterable

import polars as pl

ROW = "__row"
FLOAT_TOLERANCE = 1e-6


def _metrics(f: dict) -> dict:
    return f["evidence"][0].get("metrics", {})


def _algorithm(f: dict) -> str:
    return f["evidence"][0]["method"]["algorithm"]


def _distinct(df: pl.DataFrame, col: str) -> int:
    return df.select(pl.col(col).n_unique()).item()


def _nulls(df: pl.DataFrame, col: str) -> int:
    return df.select(pl.col(col).null_count()).item()


def _level_expr(df: pl.DataFrame, col: str, label) -> pl.Expr:
    """Equality with a level as the report prints it."""
    dtype = df.schema[col]
    if dtype.is_numeric():
        return pl.col(col) == float(label)
    if dtype == pl.Boolean:
        return pl.col(col) == (str(label).lower() == "true")
    return pl.col(col).cast(pl.String) == str(label)


def _value_expr(df: pl.DataFrame, col: str, value) -> pl.Expr:
    if isinstance(value, str) and df.schema[col].is_numeric():
        value = float(value)
    return pl.col(col) == value


def _residual_within(m: dict, target: str, predicted: pl.Expr, residual: pl.Expr | None = None) -> pl.Expr:
    absolute = m.get("absolute_tolerance", 1e-9)
    relative = m.get("relative_tolerance", 1e-9)
    y = pl.col(target).cast(pl.Float64)
    if "tolerance_scale" in m:
        bar = pl.lit(float(absolute) + float(relative) * float(m["tolerance_scale"]))
    elif relative:
        # Legacy detectors allow tolerance * (1 + |y|).
        bar = absolute * (y.abs() + 1.0)
    else:
        bar = pl.lit(absolute) * (1.0 + 1e-9)
    diff = residual if residual is not None else (y - predicted).abs()
    return diff <= bar


def _anchored_delta_expr(col: str, origin) -> pl.Expr:
    if isinstance(origin, int) and not isinstance(origin, bool):
        return (pl.col(col) - pl.lit(origin)).cast(pl.Float64)
    return pl.col(col).cast(pl.Float64) - pl.lit(float(origin))


def _identity_parts(f: dict) -> tuple[str, list[str], pl.Expr, pl.Expr, pl.Expr | None]:
    """Target, terms, the predicted value, comparable filter, and optional anchored residual."""
    m = _metrics(f)
    target = f["subject"]["target"]
    if _algorithm(f) == "ols_residual_scan":
        terms = f["subject"]["inputs"]
        if (
            "x_origin" in m
            and "y_origin" in m
            and "x_center_offset" in m
            and "y_center_offset" in m
        ):
            dx = _anchored_delta_expr(terms[0], m["x_origin"]) - pl.lit(float(m["x_center_offset"]))
            dy = _anchored_delta_expr(target, m["y_origin"]) - pl.lit(float(m["y_center_offset"]))
            residual = (dy - pl.lit(float(m["a"])) * dx).abs()
            x = pl.col(terms[0]).cast(pl.Float64)
            return target, terms, m["a"] * x + m["b"], pl.lit(True), residual
        x = pl.col(terms[0]).cast(pl.Float64)
        return target, terms, m["a"] * x + m["b"], pl.lit(True), None
    terms = m["terms"]
    columns = [pl.col(t).cast(pl.Float64) for t in terms]
    form = m["form"]
    if form == "product":
        return target, terms, columns[0] * columns[1], pl.lit(True), None
    if form == "ratio":
        return target, terms, columns[0] / columns[1], columns[1] != 0, None
    coefficients = m["coefficients"]
    if form == "linear_combination":
        predicted = pl.lit(coefficients[0])
        weights = coefficients[1:]
    else:
        predicted = pl.lit(0.0)
        weights = coefficients
    for c, w in zip(columns, weights):
        predicted = predicted + w * c
    return target, terms, predicted, pl.lit(True), None


# Each checker returns (quantity, expected, actual) triples.
Triple = tuple[str, object, object]


def _check_unique(f, m, df) -> list[Triple]:
    cols = f["subject"]["columns"]
    tuples = df.select(cols).unique().height
    return [("distinct tuples", df.height, tuples)]


def _check_near_unique(f, m, df) -> list[Triple]:
    col = f["subject"]["columns"][0]
    values = df.select(pl.col(col).drop_nulls())
    return [("repeated values", m["duplicates"], values.height - _distinct(values, col))]


def _check_near_key(f, m, df) -> list[Triple]:
    col = f["subject"]["columns"][0]
    distinct = _distinct(df, col)
    return [
        ("distinct values", m["distinct_values"], distinct),
        ("rows repeating a value", m["duplicate_rows"], df.height - distinct),
    ]


def _check_rows_repeat(f, m, df) -> list[Triple]:
    cols = f["subject"]["columns"]
    return [("repeated rows", m["n_redundant"], df.height - df.select(cols).unique().height)]


def _violators_fd(f, df) -> pl.DataFrame:
    inputs, target = f["subject"]["inputs"], f["subject"]["target"]
    frame = df.with_row_index(ROW).drop_nulls(inputs + [target])
    modes = (
        frame.group_by(inputs + [target]).len("__n")
        .sort(["__n", target], descending=[True, False])
        .unique(subset=inputs, keep="first", maintain_order=True)
        .select(inputs + [pl.col(target).alias("__mode")])
    )
    joined = frame.join(modes, on=inputs, nulls_equal=True)
    return joined.filter(pl.col(target) != pl.col("__mode")).drop("__mode")


def _check_fd(f, m, df) -> list[Triple] | None:
    if m.get("form") == "hierarchy":
        return _check_hierarchy(f, m, df)
    if m.get("sampled"):
        return None
    return [("violations", m["violations"], _violators_fd(f, df).height)]


def _check_identity(f, m, df) -> list[Triple]:
    target, terms, predicted, comparable, residual = _identity_parts(f)
    rows = df.drop_nulls([target, *terms]).filter(comparable)
    within = rows.select(_residual_within(m, target, predicted, residual).sum()).item()
    return [
        ("comparable rows", m["n"], rows.height),
        ("rows within tolerance", m["matching_rows"], within),
    ]


def _as_cols(v) -> list[str]:
    return [v] if isinstance(v, str) else list(v)


def _split_values(df, finer: list[str], coarser: list[str]) -> int:
    """Tuples of `finer` paired with more than one tuple of `coarser`."""
    rows = df.drop_nulls(finer + coarser)
    if rows.height == 0:
        return 0
    pairs = rows.select(finer + coarser).unique()
    return pairs.height - pairs.select(finer).unique().height


def _hierarchy_levels(m) -> list[list[str]]:
    raw = m.get("hierarchy_levels") or []
    return [_as_cols(c) for c in raw]


def _check_hierarchy(f, m, df) -> list[Triple]:
    """Recount a hierarchy statement: every level determines the next.

    A mapping holds exactly when no finer tuple pairs with more than
    one coarser tuple (a "split" tuple), so splits are the recountable
    quantity, alongside each mapping's complete-row support. Columns
    sharing a level must determine each other the same way. A level
    side of a mapping may name several columns (an equivalence class);
    a declared mapping names one column per side.
    """
    triples: list[Triple] = []

    def mapping_triples(finer, coarser, stated_rows=None, stated_splits=0, stated_levels=None):
        label = f"{'+'.join(finer)} → {'+'.join(coarser)}"
        if stated_rows is not None:
            rows = df.drop_nulls(finer + coarser).height
            triples.append((f"complete rows ({label})", stated_rows, rows))
        triples.append(
            (f"split {finer[0]} values ({label})", stated_splits, _split_values(df, finer, coarser))
        )
        if stated_levels is not None and len(finer) == 1:
            actual = df.drop_nulls(finer).select(finer).unique().height
            triples.append((f"distinct {finer[0]} values", stated_levels, actual))

    mappings = m.get("hierarchy_mappings") or m.get("hierarchy_edges") or []
    for e in mappings:
        # A mapping between levels is stated on the support of its
        # widest witness pair (recorded as `witness`): a level with
        # several columns is complete on fewer rows than any one of
        # its columns, so the recount uses the witness columns.
        witness = e.get("witness") or {}
        finer = _as_cols(witness.get("finer", e["finer"]))
        coarser = _as_cols(witness.get("coarser", e["coarser"]))
        mapping_triples(
            finer,
            coarser,
            stated_rows=e["rows_evaluated"],
            stated_levels=e.get("finer_levels"),
        )
    levels = _hierarchy_levels(m)
    for level in levels:
        for a in level:
            for b in level:
                if a != b:
                    triples.append(
                        (f"split {a} values within level", 0, _split_values(df, [a], [b]))
                    )
    if not mappings and levels:
        # A deviation statement carries only its broken mappings; the
        # adjacent pairs it does not name are claimed to hold.
        broken = {(e["finer"], e["coarser"]): e for e in m.get("broken_mappings") or []}
        flat = [level[0] for level in levels if len(level) == 1]
        for finer, coarser in zip(flat, flat[1:]):
            e = broken.get((finer, coarser))
            stated = e["split_levels"] if e is not None else 0
            mapping_triples([finer], [coarser], stated_splits=stated)
    return triples


def _check_duplicate_encoding(f, m, df) -> list[Triple]:
    """Recount a duplicate-encoding or rollup statement in full.

    The correspondence is recounted, not just its size: the levels
    on each side, the distinct pairs, the rows breaking each
    direction's majority mapping, and the exceptions — the rows
    whose value pair is not the modal pair for both of its values.
    An exact duplicate encoding (a bijection) is checked in both
    directions, and its exactness is recomputed from them. A
    rollup is checked as the mapping it states, finer to coarser;
    its exceptions are by definition the rows breaking that
    mapping, so the same recount serves both.
    """
    if m.get("form") == "hierarchy":
        return _check_hierarchy(f, m, df)
    a, b = m.get("finer"), m.get("coarser")
    if a is None:
        a, b = f["subject"]["columns"]
    rows = df.drop_nulls([a, b])
    counts = rows.group_by([a, b]).len("__n")
    modal_a = counts.group_by(a).agg(pl.col("__n").max().alias("__modal"))
    modal_b = counts.group_by(b).agg(pl.col("__n").max().alias("__modal"))
    modal_rows_a = modal_a.select(pl.col("__modal").sum()).item() or 0
    modal_rows_b = modal_b.select(pl.col("__modal").sum()).item() or 0
    violations_ab = rows.height - modal_rows_a
    violations_ba = rows.height - modal_rows_b
    triples: list[Triple] = [
        ("pairs", m["pairs"], counts.height),
        ("complete rows", m["n_used"], rows.height),
    ]
    if "levels_first" in m:
        triples.append((f"levels of {a}", m["levels_first"], _distinct(rows, a)))
    if "levels_second" in m:
        triples.append((f"levels of {b}", m["levels_second"], _distinct(rows, b)))
    if "violations_first_to_second" in m:
        triples.append(
            (
                f"rows breaking the {a} → {b} mapping",
                m["violations_first_to_second"],
                violations_ab,
            )
        )
    if "violations_second_to_first" in m:
        triples.append(
            (
                f"rows breaking the {b} → {a} mapping",
                m["violations_second_to_first"],
                violations_ba,
            )
        )
    if "exceptions" in m:
        if m.get("form") == "rollup":
            exceptions = violations_ab
        else:
            joined = counts.join(modal_a, on=a).join(modal_b, on=b, suffix="_b")
            exceptions = (
                joined.filter(
                    (pl.col("__n") < pl.col("__modal"))
                    | (pl.col("__n") < pl.col("__modal_b"))
                )
                .select(pl.col("__n").sum())
                .item()
                or 0
            )
        triples.append(("exceptions", m["exceptions"], exceptions))
    if "exact" in m:
        triples.append(
            ("exact", m["exact"], violations_ab == 0 and violations_ba == 0)
        )
    return triples


def _check_missing(f, m, df) -> list[Triple]:
    col = f["subject"]["columns"][0]
    triples: list[Triple] = [("missing rows", m["missing"], _nulls(df, col))]
    triples.extend(_pattern_triples(m, df, [col]))
    return triples


def _pattern_side(m: dict, df: pl.DataFrame) -> pl.Expr | None:
    kind = m.get("pattern")
    col = m.get("pattern_column")
    if kind == "level":
        return _level_expr(df, col, m["pattern_level"])
    if kind == "threshold":
        c = pl.col(col).cast(pl.Float64)
        return c > m["pattern_cut"] if m["pattern_missing_above"] else c <= m["pattern_cut"]
    if kind == "onset":
        c = pl.col(col).cast(pl.String) if not df.schema[col].is_temporal() else pl.col(col)
        cut = pl.lit(m["pattern_cut"]).str.to_datetime(strict=False)
        if df.schema[col].is_temporal():
            c = c.cast(pl.Datetime("us"))
            return c > cut if m["pattern_missing_after"] else c <= cut
        return None
    return None


def _pattern_triples(m: dict, df: pl.DataFrame, cols: list[str]) -> list[Triple]:
    side = _pattern_side(m, df)
    if side is None:
        return []
    inside = df.filter(side)
    missing = inside.select(pl.col(cols[0]).is_null().sum()).item()
    return [
        ("rows in the pattern", m["pattern_rows"], inside.height),
        ("missing rows in the pattern", m["pattern_missing_rows"], missing),
    ]


def _check_block(f, m, df) -> list[Triple]:
    cols = f["subject"]["columns"]
    nulls = [pl.col(c).is_null() for c in cols]
    together = df.select(pl.all_horizontal(nulls).sum()).item()
    any_null = df.select(pl.any_horizontal(nulls).sum()).item()
    return [
        ("missing together", m["missing_together"], together),
        ("missing in any", m["missing_any"], any_null),
    ]


def _check_empty(f, m, df) -> list[Triple]:
    col = f["subject"]["columns"][0]
    return [("missing rows", m["n"], _nulls(df, col))]


def _check_column_statistics(f, m, df) -> list[Triple] | None:
    col = f["subject"]["columns"][0]
    if f.get("type") == "missing_values":
        return _check_empty(f, m, df)
    if "distinct" in m:
        return [("distinct values", m["distinct"], _distinct(df.drop_nulls([col]), col))]
    if "n_values" in m:
        return [("values", m["n_values"], df.height - _nulls(df, col))]
    return None


def _check_value_count(f, m, df, key: str) -> list[Triple]:
    col = f["subject"]["columns"][0]
    value = m["value"]
    return [("rows at the value", m[key], df.filter(_value_expr(df, col, value)).height)]


def _check_content(f, m, df) -> list[Triple] | None:
    col = f["subject"]["columns"][0]
    kind = m["kind"]
    present = df.select(pl.col(col)).drop_nulls()

    def count(expr: pl.Expr) -> int:
        return present.select(expr.sum()).item()

    c = pl.col(col)
    if kind == "repeated_extreme":
        return [("rows at the value", m["rows"], count(c == m["value"]))]
    if kind == "minority_sign":
        v = c.cast(pl.Float64)
        negative = m["sign"] == "negative"
        furthest = present.select(v.min() if negative else v.max()).item()
        return [("rows of that sign", m["minority_rows"], count(v < 0 if negative else v > 0)),
                ("furthest value", m["furthest"], furthest)]
    if kind == "zero_inflation":
        return [("rows at zero", m["zeros"], count(c.cast(pl.Float64) == 0))]
    if kind == "temporal_outlier":
        low, high = present.select(
            c.cast(pl.Date).min().alias("low"), c.cast(pl.Date).max().alias("high")
        ).row(0)
        extreme = str(high) if str(high) == m["extreme"] else str(low)
        return [("extreme value", m["extreme"], extreme)]
    if kind == "long_text":
        return [("longest", m["longest"], present.select(c.str.len_chars().max()).item())]
    if kind == "disguised_missing":
        listed = count(c.is_in(m["tokens"]))
        triples: list[Triple] = [("values", m["n"], present.height)]
        if m["token_rows"] <= listed or len(m["tokens"]) < 5:
            triples.append(("token rows", m["token_rows"], listed))
        return triples
    if kind in ("numbers_as_text", "dates_as_text", "booleans_as_text",
                "email", "url", "uuid", "ipv4"):
        listed = count(c.is_in(m["exception_values"]))
        triples: list[Triple] = [("values", m["n"], present.height)]
        if m["exceptions"] <= len(m["exception_values"]) or listed == m["exceptions"]:
            triples.append(("exceptions", m["exceptions"], listed))
        return triples
    if kind in ("label_variants", "possible_misspelling"):
        entries = (
            [e for group in m["groups"] for e in group]
            if kind == "label_variants"
            else m["labels"]
        )
        return [
            (f"rows of '{e['value']}'", e["rows"], count(c == e["value"]))
            for e in entries
        ]
    return None


def _conditions(f: dict) -> list[dict]:
    """The subset a statement is scoped to, as column and level pairs."""
    context = _metrics(f).get("context")
    if context:
        return context
    definition = (f.get("specification") or {}).get("definition") or {}
    if "condition_column" in definition:
        return [{"column": definition["condition_column"],
                 "value": definition["condition_value"]}]
    return []


def _context(f: dict, df: pl.DataFrame) -> pl.Expr | None:
    conditions = _conditions(f)
    if not conditions:
        return None
    exprs = [_level_expr(df, c["column"], c["value"]) for c in conditions]
    return pl.all_horizontal(exprs)


def _unconditioned_column(f: dict) -> str:
    target = _metrics(f).get("target")
    if target:
        return target
    conditioned = {c["column"] for c in _conditions(f)}
    remaining = [c for c in f["subject"]["columns"] if c not in conditioned]
    if len(remaining) != 1:
        raise ValueError(f"cannot tell the target among {remaining}")
    return remaining[0]


def _check_categorical_outlier(f, m, df) -> list[Triple]:
    target = _unconditioned_column(f)
    rows = df.with_row_index(ROW).filter(_context(f, df)).drop_nulls([target])
    breaking = rows.filter(pl.col(target).cast(pl.String) != str(m["dominant_level"]))
    return [
        ("rows in the context", m["n"], rows.height),
        ("rows breaking the pattern", len(m["outlier_rows"]), breaking.height),
    ]


def _check_extreme(f, m, df) -> list[Triple]:
    col = _unconditioned_column(f)
    actual = [df.item(int(r), col) for r in m["extreme_rows"]]
    triples: list[Triple] = [("values at the stated rows", m["extreme_values"], actual)]
    scope = df.with_row_index(ROW)
    context = _context(f, df)
    if context is not None:
        scope = scope.filter(context)
    values = scope.drop_nulls([col])
    triples.append(("rows in scope", m["n"], values.height))
    rest = values.filter(~pl.col(ROW).is_in(m["extreme_rows"]))
    v = pl.col(col).cast(pl.Float64)
    edge = rest.select(v.max() if m["side"] == "high" else v.min()).item()
    triples.append(("edge of the remaining values", m["bulk_edge"], edge))
    return triples


def _check_correlation(f, m, df) -> list[Triple] | None:
    if m.get("row_sampled"):
        return None
    a, b = m["variables"]
    rows = df.select(pl.col(a).cast(pl.Float64), pl.col(b).cast(pl.Float64)).drop_nulls()
    return [("pearson", m["pearson"], rows.select(pl.corr(a, b)).item())]


def _check_ordering(f, m, df) -> list[Triple] | None:
    if "strictly_increasing" not in m:
        return None
    col = f["subject"]["columns"][0]
    v = pl.col("v")
    up, down = df.select(pl.col(col).alias("v")).drop_nulls().select(
        (v >= v.shift(1)).all().alias("up"),
        (v <= v.shift(1)).all().alias("down"),
    ).row(0)
    return [("sorted", True, bool(up or down))]


def _check_axis_gap(f, m, df) -> list[Triple] | None:
    col = f["subject"]["columns"][0]
    if col not in df.columns or not df.schema[col].is_temporal():
        return None
    distinct = _distinct(df.drop_nulls([col]), col)
    return [("expected steps", m["expected_steps"], distinct + m["absent_steps"])]


def _check_near_duplicates(f, m, df) -> list[Triple]:
    varying = m["varying_column"]
    others = [c for c in df.columns if c != varying]
    without = df.height - df.select(others).unique().height
    exact = df.height - df.unique().height
    return [
        ("exact repeats", m["exact_repeats"], exact),
        ("rows repeating all but one column", m["near_duplicates"], without - exact),
    ]


def _line_distance(m: dict) -> pl.Expr:
    x, y = pl.col(m["x"]).cast(pl.Float64), pl.col(m["y"]).cast(pl.Float64)
    return (y - m["slope"] * x - m["intercept"]).abs()


def _check_residual_outliers(f, m, df) -> list[Triple]:
    rows = df.drop_nulls([m["x"], m["y"]])
    far = rows.filter(_line_distance(m) > m["ordinary_band"] * (1 + 1e-9)).height
    return [
        ("complete rows", m["n"], rows.height),
        ("rows beyond the band", m["outliers"], far),
    ]


def _swap_expr(f: dict, m: dict) -> pl.Expr:
    a, b = f["subject"]["columns"]
    first, second = m["range_first"], m["range_second"]
    return (
        pl.col(a).cast(pl.Float64).is_between(second[0], second[1])
        & pl.col(b).cast(pl.Float64).is_between(first[0], first[1])
    )


def _order_predicates(
    lo_dtype: pl.DataType, hi_dtype: pl.DataType, lo_col: str, hi_col: str
) -> tuple[pl.Expr, pl.Expr, pl.Expr]:
    """Exact comparison predicates (comparable, lower > upper, lower == upper).

    Preserves integer precision beyond 2^53 (signed, unsigned, and mixed
    signed/unsigned integer pairs) instead of casting integers to Float64
    (issue #466), while retaining finite filtering for floats.
    """
    lo = pl.col(lo_col)
    hi = pl.col(hi_col)
    if lo_dtype.is_integer() and hi_dtype.is_integer():
        lo_signed = lo_dtype.is_signed_integer()
        hi_signed = hi_dtype.is_signed_integer()
        if lo_signed == hi_signed:
            return pl.lit(True), lo > hi, lo == hi
        if lo_signed and not hi_signed:
            return (
                pl.lit(True),
                (lo >= 0) & (lo.cast(pl.UInt64) > hi.cast(pl.UInt64)),
                (lo >= 0) & (lo.cast(pl.UInt64) == hi.cast(pl.UInt64)),
            )
        return (
            pl.lit(True),
            (hi < 0) | (lo.cast(pl.UInt64) > hi.cast(pl.UInt64)),
            (hi >= 0) & (lo.cast(pl.UInt64) == hi.cast(pl.UInt64)),
        )
    if lo_dtype.is_numeric() and hi_dtype.is_numeric():
        lo_f = lo.cast(pl.Float64)
        hi_f = hi.cast(pl.Float64)
        return lo_f.is_finite() & hi_f.is_finite(), lo_f > hi_f, lo_f == hi_f
    return pl.lit(True), lo > hi, lo == hi


def _check_order_constraint(f, m, df) -> list[Triple] | None:
    lower = m.get("lower")
    upper = m.get("upper")
    if not lower or not upper or lower not in df.columns or upper not in df.columns:
        return None
    lo_dt, hi_dt = df.schema[lower], df.schema[upper]
    complete = df.select(pl.col(lower).alias("_lo"), pl.col(upper).alias("_hi")).drop_nulls()
    comparable, gt_expr, eq_expr = _order_predicates(lo_dt, hi_dt, "_lo", "_hi")
    complete = complete.filter(comparable)
    viol = complete.filter(gt_expr).height
    eq = complete.filter(eq_expr).height
    out: list[Triple] = [
        ("evaluated rows", m["n"], complete.height),
        ("violations", m["violations"], viol),
    ]
    if "equal_rows" in m:
        out.append(("equal rows", m["equal_rows"], eq))
    return out


def _check_swapped(f, m, df) -> list[Triple]:
    return [("swapped rows", m["swapped_rows"], df.filter(_swap_expr(f, m)).height)]


def _check_outcome_contrast(f, m, df) -> list[Triple] | None:
    defn = (f.get("specification") or {}).get("definition") or {}
    gcol = defn.get("grouping_column") or m.get("grouping_column")
    target = m.get("target") or f["subject"].get("target")
    level = defn.get("level") if "level" in defn else m.get("level")
    if not gcol or gcol not in df.columns or not target or target not in df.columns or level is None:
        return None
    sub = df.drop_nulls([gcol, target]).filter(_level_expr(df, gcol, level))
    if "level_rows" in m and "level_events" in m and "event" in m:
        ref = m.get("reference_level")
        if ref is not None:
            sub = sub.filter(
                _level_expr(df, target, m["event"]) | _level_expr(df, target, ref)
            )
        events = sub.filter(_level_expr(df, target, m["event"])).height
        return [
            ("rows in level", m["level_rows"], sub.height),
            ("event rows in level", m["level_events"], events),
        ]
    if "level_rows" in m and "level_median" in m:
        med = sub.select(pl.col(target).cast(pl.Float64).median()).item()
        return [
            ("rows in level", m["level_rows"], sub.height),
            ("level median", m["level_median"], med),
        ]
    return None


CHECKS: dict[str, Callable] = {
    "leave_one_column_out_hash": _check_near_duplicates,
    "residual_scan": _check_residual_outliers,
    "swapped_range_scan": _check_swapped,
    "uniqueness": _check_unique,
    "composite_uniqueness": _check_unique,
    "grain_factorization": _check_unique,
    "near_uniqueness": _check_near_unique,
    "near_key_axis": _check_near_key,
    "row_equality": _check_rows_repeat,
    "value_mapping": _check_fd,
    "ols_residual_scan": _check_identity,
    "multiterm_scan": _check_identity,
    "pair contingency + majority mapping, exact verification": _check_duplicate_encoding,
    "declared_hierarchy": _check_hierarchy,
    "missing_rate_scan": _check_missing,
    "co_missing_blocks": _check_block,
    "column_statistics": _check_column_statistics,
    "boundary_mass_scan": lambda f, m, df: _check_value_count(f, m, df, "count"),
    "sentinel_window_scan": lambda f, m, df: _check_value_count(f, m, df, "occurrences"),
    "column_content_scan": _check_content,
    "near-degenerate conditional distribution": _check_categorical_outlier,
    "robust isolation gap vs the subset bulk": _check_extreme,
    "pearson_spearman_chatterjee": _check_correlation,
    "adjacent_value_scan": _check_ordering,
    "pairwise_order_scan": _check_order_constraint,
    "per_level_outcome_contrast": _check_outcome_contrast,
    "axis_grid_scan": _check_axis_gap,
}


ROW_INDEXED_ALGORITHMS = (
    "robust isolation gap vs the subset bulk",
    "near-degenerate conditional distribution",
)


def _stratified_source_indices(source: dict | None) -> list[int] | None:
    if not source or source.get("basis") != "sampled":
        return None
    sample = source.get("sample") or {}
    if sample.get("method") != "stratified":
        return None
    rows = source.get("rows")
    cap = sample.get("cap")
    if rows is None or cap is None:
        return None
    from . import _source

    return _source._stratified_indices(int(rows), int(cap), int(sample.get("seed", 0)))


def _resolve_row_positions(
    f: dict,
    raw_rows: list[int],
    frame_height: int | None,
    source: dict | None,
) -> list[int]:
    """Translate sample-relative row positions to the passed frame's row basis.

    - Full-basis reports return `raw_rows` unchanged.
    - Sampled-basis reports map sample offsets to source row indices when the
      original source frame (or a LazyFrame) is passed, keep them as-is when
      the materialized sampled frame is passed, and reject frames whose height
      matches neither basis instead of silently indexing unrelated rows
      (issue #467).
    """
    m = _metrics(f)
    scope = (f.get("evidence") or [{}])[0].get("scope") or {}
    if source and source.get("basis") == "sampled":
        src_rows = source.get("rows")
        prof_rows = source.get("rows_profiled")
        if frame_height is not None and prof_rows is not None and frame_height == prof_rows:
            return [int(r) for r in raw_rows]
        if frame_height is None or (src_rows is not None and frame_height == src_rows):
            indices = _stratified_source_indices(source)
            if indices is None:
                raise ValueError(
                    f"finding row positions are sample-relative ({prof_rows} of {src_rows} "
                    "source rows) and this sample cannot be mapped back to source positions; "
                    f"pass the {prof_rows}-row sampled frame"
                )
            out: list[int] = []
            for r in raw_rows:
                idx = int(r)
                if idx < 0 or idx >= len(indices):
                    raise ValueError(
                        f"sample row position {idx} is out of range for the "
                        f"{len(indices)}-row sample"
                    )
                out.append(indices[idx])
            return out
        raise ValueError(
            f"finding row positions refer to a {prof_rows}-row sample from {src_rows} "
            f"source rows, which does not match a {frame_height}-row frame; pass either "
            f"the original {src_rows}-row source or the {prof_rows}-row sampled frame"
        )
    if m.get("row_index_basis") == "sample":
        raise ValueError(
            "finding row positions are sample-relative and no source sampling spec was "
            "supplied to translate them; call report.rows(finding_id, source_df) or pass "
            "the exact sampled frame"
        )
    return [int(r) for r in raw_rows]


def _agrees(expected, actual) -> bool:
    if isinstance(expected, list):
        return (
            isinstance(actual, list)
            and len(expected) == len(actual)
            and all(_agrees(e, a) for e, a in zip(expected, actual))
        )
    if isinstance(expected, bool) or isinstance(actual, bool):
        return expected == actual
    if isinstance(expected, int) and isinstance(actual, int):
        return expected == actual
    if isinstance(expected, (int, float)) and isinstance(actual, (int, float)):
        return abs(expected - actual) <= FLOAT_TOLERANCE * max(1.0, abs(expected))
    return str(expected) == str(actual)


def verify(
    findings: Iterable[dict],
    df: pl.DataFrame | pl.LazyFrame,
    source: dict | None = None,
) -> pl.DataFrame:
    """One row per checked quantity, and one per statement with nothing to check."""
    findings_list = list(findings)
    rows = []

    def add(f, status, quantity=None, expected=None, actual=None, note=None):
        rows.append({
            "id": f["id"],
            "type": f.get("type"),
            "status": status,
            "quantity": quantity,
            "expected": None if expected is None else str(expected),
            "actual": None if actual is None else str(actual),
            "note": note,
        })

    basis_note: str | None = None
    work_df: pl.DataFrame
    if source and source.get("basis") == "sampled":
        src_rows = source.get("rows")
        prof_rows = source.get("rows_profiled")
        indices = _stratified_source_indices(source)
        if isinstance(df, pl.LazyFrame):
            if indices is None:
                for f in findings_list:
                    add(
                        f,
                        "not_checkable",
                        note=(
                            f"report was profiled on a {prof_rows}-row sample from {src_rows} "
                            f"source rows; pass the {prof_rows}-row sampled DataFrame"
                        ),
                    )
                schema = {
                    k: pl.String
                    for k in ("id", "type", "status", "quantity", "expected", "actual", "note")
                }
                return pl.DataFrame(rows, schema=schema)
            from . import _source

            work_df = (
                df.with_row_index(_source._SOURCE_ROW_INDEX)
                .filter(pl.col(_source._SOURCE_ROW_INDEX).is_in(indices))
                .drop(_source._SOURCE_ROW_INDEX)
                .collect()
            )
            basis_note = (
                f"verified on deterministic stratified sample ({prof_rows} of {src_rows} source rows)"
            )
        elif df.height == prof_rows:
            work_df = df
        elif df.height == src_rows and indices is not None:
            work_df = df[indices]
            basis_note = (
                f"verified on deterministic stratified sample ({prof_rows} of {src_rows} source rows)"
            )
        else:
            for f in findings_list:
                add(
                    f,
                    "not_checkable",
                    note=(
                        f"frame has {df.height} rows, but report was profiled on a {prof_rows}-row "
                        f"sample from {src_rows} source rows; pass either the original "
                        f"{src_rows}-row source or the {prof_rows}-row sampled frame"
                    ),
                )
            schema = {
                k: pl.String
                for k in ("id", "type", "status", "quantity", "expected", "actual", "note")
            }
            return pl.DataFrame(rows, schema=schema)
    else:
        work_df = df.collect() if isinstance(df, pl.LazyFrame) else df

    for f in findings_list:
        m = _metrics(f)
        if m.get("source_form"):
            add(f, "not_checkable", note="stated on the pivoted wide form")
            continue
        algorithm = _algorithm(f)
        checker = CHECKS.get(algorithm)
        missing = [c for c in f["subject"].get("columns", []) if c not in work_df.columns]
        if checker is None:
            add(f, "not_checkable", note="estimated or fitted on a sample")
            continue
        if missing:
            add(f, "not_checkable", note=f"not columns of this frame: {missing}")
            continue
        if (
            algorithm in ROW_INDEXED_ALGORITHMS
            and not (source and source.get("basis") == "sampled")
            and m.get("row_index_basis") == "sample"
        ):
            add(
                f,
                "not_checkable",
                note="row-indexed finding is sample-relative and its frame basis does not match df",
            )
            continue
        try:
            triples = checker(f, m, work_df)
        except (KeyError, IndexError, ValueError, TypeError, pl.exceptions.PolarsError) as err:
            add(f, "error", note=f"{type(err).__name__}: {err}")
            continue
        if not triples:
            add(f, "not_checkable", note="no exact count for this statement")
            continue
        for quantity, expected, actual in triples:
            status = "ok" if _agrees(expected, actual) else "mismatch"
            add(f, status, quantity, expected, actual, note=basis_note)
    schema = {k: pl.String for k in ("id", "type", "status", "quantity", "expected", "actual", "note")}
    return pl.DataFrame(rows, schema=schema)


def rows_of(
    f: dict,
    df: pl.DataFrame | pl.LazyFrame,
    source: dict | None = None,
) -> pl.DataFrame:
    """The rows a statement is about, with their positions in `__row`."""
    m = _metrics(f)
    algorithm = _algorithm(f)
    cols = f["subject"].get("columns", [])

    if algorithm in ROW_INDEXED_ALGORITHMS:
        key = "extreme_rows" if algorithm == "robust isolation gap vs the subset bulk" else "outlier_rows"
        raw_rows = m[key]
        if isinstance(df, pl.LazyFrame):
            target_rows = _resolve_row_positions(f, raw_rows, None, source)
            return df.with_row_index(ROW).filter(pl.col(ROW).is_in(target_rows)).collect()
        target_rows = _resolve_row_positions(f, raw_rows, df.height, source)
        return df.with_row_index(ROW).filter(pl.col(ROW).is_in(target_rows))

    if isinstance(df, pl.LazyFrame):
        df = df.collect()

    indexed = df.with_row_index(ROW)
    if algorithm == "value_mapping":
        return _violators_fd(f, df)
    if algorithm in ("ols_residual_scan", "multiterm_scan"):
        target, terms, predicted, comparable, residual = _identity_parts(f)
        within = _residual_within(m, target, predicted, residual)
        return indexed.drop_nulls([target, *terms]).filter(comparable).filter(~within)
    if algorithm == "missing_rate_scan":
        return indexed.filter(pl.col(cols[0]).is_null())
    if algorithm == "co_missing_blocks":
        return indexed.filter(pl.any_horizontal([pl.col(c).is_null() for c in cols]))
    if algorithm == "column_statistics" and f.get("type") == "missing_values":
        return indexed.filter(pl.col(cols[0]).is_null())
    if algorithm == "row_equality":
        return indexed.filter(pl.struct(cols).is_duplicated())
    if algorithm == "near_key_axis":
        return indexed.filter(pl.col(cols[0]).is_duplicated())
    if algorithm == "leave_one_column_out_hash":
        others = [c for c in df.columns if c != m["varying_column"]]
        return indexed.filter(pl.struct(others).is_duplicated())
    if algorithm == "residual_scan":
        complete = indexed.drop_nulls([m["x"], m["y"]])
        return complete.filter(_line_distance(m) > m["ordinary_band"] * (1 + 1e-9))
    if algorithm == "swapped_range_scan":
        return indexed.filter(_swap_expr(f, m))
    if algorithm == "pairwise_order_scan":
        lower, upper = m.get("lower"), m.get("upper")
        if lower and upper and lower in df.columns and upper in df.columns:
            complete = indexed.drop_nulls([lower, upper])
            comparable, gt_expr, _ = _order_predicates(
                df.schema[lower], df.schema[upper], lower, upper
            )
            return complete.filter(comparable & gt_expr)
    if algorithm == "near_uniqueness":
        return indexed.drop_nulls(cols).filter(pl.col(cols[0]).is_duplicated())
    if algorithm in ("boundary_mass_scan", "sentinel_window_scan"):
        return indexed.filter(_value_expr(df, cols[0], m["value"]))
    if algorithm == "column_content_scan":
        kind = m["kind"]
        s = pl.col(cols[0])
        if kind == "repeated_extreme":
            return indexed.filter(s == m["value"])
        if kind == "minority_sign":
            return indexed.filter(s < 0 if m["sign"] == "negative" else s > 0)
        if kind == "zero_inflation":
            return indexed.filter(s == 0)
        if kind == "disguised_missing":
            return indexed.filter(s.is_in(m["tokens"]))
        if kind in ("numbers_as_text", "dates_as_text", "booleans_as_text",
                    "email", "url", "uuid", "ipv4"):
            return indexed.filter(s.is_in(m["exception_values"]))
        if kind == "possible_misspelling":
            return indexed.filter(s.is_in([e["value"] for e in m["labels"]]))
        if kind == "label_variants":
            return indexed.filter(
                s.is_in([e["value"] for g in m["groups"] for e in g])
            )
    if algorithm == "partitioned_scatterplot_diagnostics":
        defn = (f.get("specification") or {}).get("definition") or {}
        pair = defn.get("pair") or []
        rule = defn.get("partition") or {}
        if len(pair) == 2 and rule:
            x, y = pair
            complete = indexed.filter(
                pl.col(x).is_not_null()
                & pl.col(y).is_not_null()
                & pl.col(x).cast(pl.Float64).is_finite()
                & pl.col(y).cast(pl.Float64).is_finite()
            )
            groups = ((m.get("partition_scagnostics") or {}).get("validation") or {}).get("groups") or []
            labels = [g.get("label") or f"group_{i}" for i, g in enumerate(groups)]
            kind = rule.get("kind")
            group_expr = None
            standout_expr = None
            if kind == "levels":
                col = rule["column"]
                lvls = rule["levels"]
                col_s = pl.col(col).cast(pl.String)
                fallback = labels[len(lvls)] if len(labels) > len(lvls) else "other"
                expr = pl.lit(fallback)
                for idx in range(len(lvls) - 1, -1, -1):
                    lbl = labels[idx] if idx < len(labels) else lvls[idx]
                    expr = pl.when(col_s == lvls[idx]).then(pl.lit(lbl)).otherwise(expr)
                group_expr = expr
                standout_expr = col_s == lvls[0]
            elif kind == "threshold":
                col = rule["column"]
                val = float(rule["value"])
                col_f = pl.col(col).cast(pl.Float64)
                cond = pl.col(col).is_not_null() & col_f.is_finite() & (col_f <= val)
                l0 = labels[0] if len(labels) > 0 else f"<= {val}"
                l1 = labels[1] if len(labels) > 1 else f"> {val}"
                group_expr = pl.when(cond).then(pl.lit(l0)).otherwise(pl.lit(l1))
                standout_expr = cond
            elif kind == "cross":
                c0, c1 = rule["columns"]
                v0, v1 = rule["levels"]
                m0 = pl.col(c0).cast(pl.String) == v0
                m1 = pl.col(c1).cast(pl.String) == v1
                l0 = labels[0] if len(labels) > 0 else f"{v0} & {v1}"
                l1 = labels[1] if len(labels) > 1 else f"{v0} & other"
                l2 = labels[2] if len(labels) > 2 else f"other & {v1}"
                l3 = labels[3] if len(labels) > 3 else "other & other"
                group_expr = (
                    pl.when(m0 & m1).then(pl.lit(l0))
                    .when(m0 & ~m1).then(pl.lit(l1))
                    .when(~m0 & m1).then(pl.lit(l2))
                    .otherwise(pl.lit(l3))
                )
                standout_expr = m0 & m1
            if group_expr is not None:
                annotated = complete.with_columns(group_expr.alias("__partition_group"))
                if defn.get("objective") == "standout_subgroup" and standout_expr is not None:
                    so_idx = (m.get("partition_scagnostics") or {}).get("standout_group") or 0
                    return annotated.filter(standout_expr if so_idx == 0 else ~standout_expr)
                return annotated
    if algorithm in ("cross_fitted_gbm_residual_slice_search", "slice_search_split_sample"):
        conds = (f.get("specification") or {}).get("conditions") or m.get("conditions") or []
        exprs = []
        for p in conds:
            col = p["column"]
            op = p.get("operator") or p.get("op")
            val = p["value"]
            if op == "eq":
                exprs.append(_level_expr(df, col, val))
            elif op == "ne":
                exprs.append(~_level_expr(df, col, val))
            elif op in ("lt", "le", "gt", "ge"):
                ops = {"lt": "__lt__", "le": "__le__", "gt": "__gt__", "ge": "__ge__"}
                exprs.append(getattr(pl.col(col).cast(pl.Float64), ops[op])(float(val)))
        if exprs:
            return indexed.filter(pl.all_horizontal(exprs))
    if algorithm == "per_level_outcome_contrast":
        defn = (f.get("specification") or {}).get("definition") or {}
        gcol = defn.get("grouping_column") or m.get("grouping_column")
        level = defn.get("level") if "level" in defn else m.get("level")
        if gcol and gcol in df.columns:
            if level is not None:
                return indexed.filter(_level_expr(df, gcol, level))
            return indexed.filter(pl.col(gcol).is_not_null())
    if algorithm == "two_population_contrast":
        gcol = m.get("group_column")
        gval = m.get("group_value")
        if gcol and gcol in df.columns and gval is not None:
            return indexed.filter(_level_expr(df, gcol, gval))
    if f.get("category") == "conditional_rules":
        spec = f.get("specification") or {}
        conditions = spec.get("conditions") or []
        exprs = []
        for p in conditions:
            if p.get("transform"):
                raise ValueError("this rule's condition uses a transform")
            ops = {"eq": "__eq__", "ne": "__ne__", "lt": "__lt__", "le": "__le__",
                   "gt": "__gt__", "ge": "__ge__"}
            exprs.append(getattr(pl.col(p["column"]), ops[p["operator"]])(p["value"]))
        if exprs:
            return indexed.filter(pl.all_horizontal(exprs))
    raise ValueError(f"no row set for a {f.get('type')} statement from {algorithm}")


def constraints(findings: Iterable[dict]) -> list[dict]:
    """Exact statements as constraints to check on new data."""
    out = []
    for f in findings:
        m = _metrics(f)
        algorithm = _algorithm(f)
        subject = f["subject"]
        if algorithm in ("uniqueness", "composite_uniqueness", "grain_factorization"):
            cols = subject["columns"]
            out.append({
                "finding": f["id"], "kind": "unique", "columns": cols,
                "polars": f"pl.struct({cols!r}).is_unique().all()",
            })
        elif algorithm == "value_mapping" and m.get("violations") == 0 and not m.get("sampled"):
            out.append({
                "finding": f["id"], "kind": "functional_dependency",
                "inputs": subject["inputs"], "target": subject["target"],
                "polars": f"(pl.col({subject['target']!r}).n_unique().over({subject['inputs']!r}) == 1).all()",
            })
        elif algorithm in ("ols_residual_scan", "multiterm_scan") and m.get("violations") == 0:
            target, terms, predicted, _, _ = _identity_parts(f)
            out.append({
                "finding": f["id"], "kind": "identity",
                "target": target, "terms": terms,
                "absolute_tolerance": m.get("absolute_tolerance"),
                "relative_tolerance": m.get("relative_tolerance"),
                "claim": f["claim"],
                "polars": f"({_identity_text(f)}).all()",
            })
    return out


def _identity_text(f: dict) -> str:
    m = _metrics(f)
    target = f["subject"]["target"]
    y = f"pl.col({target!r})"
    absolute = m.get("absolute_tolerance", 1e-9)
    relative = m.get("relative_tolerance", 1e-9)
    if _algorithm(f) == "ols_residual_scan":
        x = f["subject"]["inputs"][0]
        if (
            "x_origin" in m
            and "y_origin" in m
            and "x_center_offset" in m
            and "y_center_offset" in m
            and "tolerance_scale" in m
        ):
            dx = f"((pl.col({x!r}) - {m['x_origin']!r}).cast(pl.Float64) - {m['x_center_offset']!r})"
            dy = f"((pl.col({target!r}) - {m['y_origin']!r}).cast(pl.Float64) - {m['y_center_offset']!r})"
            tol = float(absolute) + float(relative) * float(m["tolerance_scale"])
            return f"({dy} - {m['a']!r} * {dx}).abs() <= {tol!r}"
        predicted = f"{m['a']!r} * pl.col({x!r}) + {m['b']!r}"
    else:
        terms = m["terms"]
        if m["form"] == "product":
            predicted = f"pl.col({terms[0]!r}) * pl.col({terms[1]!r})"
        elif m["form"] == "ratio":
            predicted = f"pl.col({terms[0]!r}) / pl.col({terms[1]!r})"
        else:
            coefficients = m["coefficients"]
            weights = coefficients[1:] if m["form"] == "linear_combination" else coefficients
            base = repr(coefficients[0]) if m["form"] == "linear_combination" else "0.0"
            predicted = base + "".join(
                f" + {w!r} * pl.col({t!r})" for w, t in zip(weights, terms)
            )
    if "tolerance_scale" in m:
        tol = float(absolute) + float(relative) * float(m["tolerance_scale"])
        return f"({y} - ({predicted})).abs() <= {tol!r}"
    if relative:
        return f"({y} - ({predicted})).abs() <= {absolute!r} * (1 + {y}.abs())"
    return f"({y} - ({predicted})).abs() <= {absolute!r}"


def great_expectations_suite(findings: Iterable[dict], name: str) -> dict:
    """The constraints that map to built-in Great Expectations types."""
    expectations = []
    unmapped = []
    for c in constraints(findings):
        if c["kind"] == "unique" and len(c["columns"]) == 1:
            expectations.append({
                "expectation_type": "expect_column_values_to_be_unique",
                "kwargs": {"column": c["columns"][0]},
                "meta": {"finding": c["finding"]},
            })
        elif c["kind"] == "unique":
            expectations.append({
                "expectation_type": "expect_compound_columns_to_be_unique",
                "kwargs": {"column_list": c["columns"]},
                "meta": {"finding": c["finding"]},
            })
        else:
            unmapped.append(c)
    return {
        "expectation_suite_name": name,
        "expectations": expectations,
        "meta": {"unmapped_constraints": unmapped},
    }
