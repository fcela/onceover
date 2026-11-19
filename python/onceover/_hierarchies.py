"""Advisory hierarchy discovery for the resolved frame spec."""

import polars as pl


_MAX_COLUMNS = 16
_MAX_CHAINS = 32


def candidates(df, roles, entity, excluded):
    """Find exact, nontrivial many-to-one edges on the inspected frame.

    Use repeated grouping columns with complete values. The two-sided
    support rule avoids key-to-anything and single-exception rollups.
    """
    supported_types = (
        pl.String, pl.Boolean, pl.Categorical, pl.Enum, pl.Date, pl.Datetime,
        pl.Int8, pl.Int16, pl.Int32, pl.Int64, pl.UInt8, pl.UInt16, pl.UInt32, pl.UInt64,
    )
    schema = df.schema
    eligible = sorted(
        (c for c, entry in roles.items()
         if c not in excluded and schema[c].base_type() in supported_types and (
             entry["role"] == "dimension" or c == entity or entry["role"] == "identifier"
         )),
        key=lambda c: (c != entity, roles[c]["role"] != "dimension", c),
    )
    if not eligible or not df.height:
        return []
    summary = df.select(
        [pl.col(c).n_unique().alias(f"n_{i}") for i, c in enumerate(eligible)]
        + [pl.col(c).null_count().alias(f"null_{i}") for i, c in enumerate(eligible)]
    ).row(0)
    width = len(eligible)
    cardinality = {
        name: summary[i]
        for i, name in enumerate(eligible)
        if 2 <= summary[i] <= df.height // 2 and summary[i + width] == 0
    }
    columns = list(cardinality)[:_MAX_COLUMNS]
    if len(columns) < 2:
        return []

    # Different events with the same grouping signature add no
    # evidence about whether a mapping is functional.
    signatures = df.select(columns).unique()
    edges = {c: set() for c in columns}
    for fine in columns:
        coarser = [c for c in columns if cardinality[c] < cardinality[fine]]
        if not coarser:
            continue
        cardinalities = signatures.group_by(fine).agg(pl.col(coarser).n_unique())
        maxima = dict(zip(coarser, cardinalities.select(coarser).max().row(0)))
        exact = [c for c in coarser if maxima[c] == 1]
        if not exact:
            continue
        first = signatures.group_by(fine).agg(pl.col(exact).first())
        for coarse in exact:
            sizes = first[coarse].alias("parent").value_counts().get_column("count")
            if (sizes > 1).sum() >= 2:
                edges[fine].add(coarse)

    # Keep paths through immediate ancestors. A branching hierarchy
    # yields separate paths rather than forcing one parent per level.
    immediate = {
        c: sorted(parents - {ancestor for p in parents for ancestor in edges[p]})
        for c, parents in edges.items()
    }
    children = {p for ps in immediate.values() for p in ps}
    stack = [(c,) for c in reversed(columns) if immediate[c] and c not in children]
    out = []
    while stack and len(out) < _MAX_CHAINS:
        chain = stack.pop()
        if immediate[chain[-1]]:
            stack.extend((*chain, parent) for parent in reversed(immediate[chain[-1]]))
            continue
        out.append({
            "levels": list(chain),
            "source": "inferred",
            "evidence": {
                "rows": df.height,
                "distinct_levels": [cardinality[c] for c in chain],
            },
        })
    return out
