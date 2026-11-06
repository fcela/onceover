"""Multi-table profiling: candidate foreign keys across a set of tables.

`profile_many` examines every ordered pair of columns from different
tables and reports the pairs whose distinct non-null values are fully
contained in a column that is unique on the other side — an inclusion
dependency, the shape a foreign key leaves in the data.  Containment is
verified exactly; nothing is sampled into a verdict.  Candidates are
reported in two tiers and never suppressed:

- ``strong``: fully contained and the column names agree after
  normalisation (case folded, underscores and spaces removed).  A
  candidate whose names agree and whose containment chance cannot
  explain is as close to a declared foreign key as values alone get.
- ``weak``: fully contained, names differ.  Each carries a *chance
  score*: the probability that a value set of the same size, drawn from
  the same value universe, would land inside the parent's values by
  accident.  Dense integer ranges contain each other by construction —
  every ``1..k`` column sits inside every larger id column — so a weak
  integer candidate with a chance score near 1 says the values alone
  carry no evidence either way; the name agreement the strong tier
  requires is the evidence such candidates lack.

Partial containment is reported separately as value overlap: pairs into
a unique parent whose shared distinct-value share clears a threshold,
with the orphan rows (child values with no parent match) counted
exactly.  Pairs are screened with a strided probe of up to 4,096 of the
child's distinct values, tested against a cached set of the parent's:
a probe miss proves non-containment, pairs whose probe share falls
more than 0.05 under the overlap threshold are screened out, and the
survivors get an exact count, so reported shares and orphan counts are
always exact.

The chance score reuses the hypergeometric form of the functional
dependency chance correction in the native dependency pass (a ratio of
binomial coefficients, evaluated in log space), but the universe
differs because the accident differs: an FD accident is a determinant
refining by luck inside one table, while a containment accident is a
value set landing inside another column's values.  For integer columns
the universe is the child's own span — the dense-range accident is
priced against the range the child actually occupies.  For the other
families the universe is every distinct value those columns use across
the input tables.  A score of 1 means chance fully explains the
containment; a score near 0 means it cannot.  The score never removes
a candidate: it ranks the contained-only tier and warns the reader.
"""

import json
import math
from collections.abc import Mapping
from datetime import datetime, timedelta

import polars as pl

_INTEGER_TYPES = (
    pl.Int8, pl.Int16, pl.Int32, pl.Int64,
    pl.UInt8, pl.UInt16, pl.UInt32,
)
_FLOAT_TYPES = (pl.Float32, pl.Float64)
_INT64_MAX = 2**63 - 1
_PROBE_LIMIT = 4096
_PROBE_MARGIN = 0.05
_ORPHAN_EXAMPLES = 10
_REPORT_VERSION = "1"


def _family(dtype):
    """Value family for a dtype, or None when containment is not examined."""
    if dtype in _INTEGER_TYPES or dtype == pl.UInt64:
        return "integer"
    if dtype in _FLOAT_TYPES:
        return "float"
    if dtype == pl.Utf8 or isinstance(dtype, (pl.Categorical, pl.Enum)):
        return "string"
    if dtype == pl.Date or isinstance(dtype, pl.Datetime):
        return "temporal"
    return None


def _skip_reason(dtype):
    if dtype == pl.Boolean:
        return "boolean columns are not examined: a two-value domain cannot carry reference evidence"
    if isinstance(dtype, pl.Time):
        return "time-of-day columns are not examined"
    return f"columns of type {dtype} are not examined"


def _log_choose(n, k):
    if k < 0 or k > n:
        return float("-inf")
    return math.lgamma(n + 1) - math.lgamma(k + 1) - math.lgamma(n - k + 1)


def _chance_from_logs(log_num, log_den):
    if log_den == float("-inf"):
        return 1.0
    return min(1.0, math.exp(log_num - log_den))


def _names_agree(child, parent):
    def norm(name):
        return name.lower().replace("_", "").replace(" ", "")

    return bool(child) and norm(child) == norm(parent)


class _Column:
    """One examined column, canonicalised for cross-table set algebra."""

    __slots__ = (
        "table", "name", "family", "values", "distinct", "distinct_count",
        "nonnull_rows", "rows", "unique", "span_min", "span_max", "text_keys",
        "_keyset",
    )

    def __init__(self, table, name, frame, int_text_keys=False):
        self.table = table
        self.name = name
        series_dtype = frame[name].dtype
        self.family = _family(series_dtype)
        self.rows = frame.height
        values = frame.select(pl.col(name).alias("v"))
        values = values.filter(pl.col("v").is_not_null())
        if self.family == "float":
            values = values.filter(~pl.col("v").is_nan())
        # Key encoding is a family-wide decision (see profile_many):
        # if any integer column needs text keys, all of them use text
        # keys, so cross-table joins always meet the same dtype.
        self.text_keys = int_text_keys and self.family == "integer"
        if self.family == "integer":
            if values.height:
                raw_min = frame.select(pl.col(name).min()).item()
                raw_max = frame.select(pl.col(name).max()).item()
                self.span_min = int(raw_min)
                self.span_max = int(raw_max)
            else:
                self.span_min = self.span_max = None
            if self.text_keys:
                values = values.with_columns(pl.col("v").cast(pl.Utf8))
            else:
                values = values.with_columns(pl.col("v").cast(pl.Int64))
        elif self.family == "float":
            values = values.with_columns(pl.col("v").cast(pl.Float64))
            self.span_min = self.span_max = None
        elif self.family == "string":
            values = values.with_columns(pl.col("v").cast(pl.Utf8))
            self.span_min = self.span_max = None
        elif self.family == "temporal":
            expr = pl.col("v")
            if series_dtype == pl.Date:
                expr = expr.cast(pl.Datetime("us"))
            else:
                if series_dtype.time_zone is not None:
                    expr = expr.dt.convert_time_zone("UTC").dt.replace_time_zone(None)
                expr = expr.cast(pl.Datetime("us"))
            values = values.with_columns(expr.cast(pl.Int64))
            self.span_min = self.span_max = None
        self.values = values
        self.nonnull_rows = values.height
        self.distinct = values.unique().sort("v")
        self.distinct_count = self.distinct.height
        self.unique = self.distinct_count == self.nonnull_rows and self.distinct_count > 0
        self._keyset = None

    def keyset(self):
        """The distinct values as a Python set, built once per column.

        Probing candidate parents is the hot loop of the pair scan:
        a cached set makes each probe O(probe size) instead of a hash
        join against the parent's full distinct set per pair.
        """
        if self._keyset is None:
            self._keyset = set(self.distinct.get_column("v").to_list())
        return self._keyset

    def example_values(self, frame):
        """Render up to the example cap of a canonical-key frame for output."""
        keys = frame.head(_ORPHAN_EXAMPLES).get_column("v").to_list()
        if self.family == "temporal":
            epoch = datetime(1970, 1, 1)
            return [(epoch + timedelta(microseconds=k)).isoformat() for k in keys]
        if self.text_keys:
            return [int(k) for k in keys]
        return keys


def _probe(distinct, limit=_PROBE_LIMIT):
    """Strided probe of a sorted distinct frame; exact when small."""
    k = distinct.height
    if k <= limit:
        return distinct, True
    step = -(-k // limit)
    probe = distinct.gather_every(step)
    tail = distinct.tail(1)
    if probe.tail(1).get_column("v").item() != tail.get_column("v").item():
        probe = pl.concat([probe, tail]).unique().sort("v")
    return probe, False


class MultiTableReport:
    """Result of :func:`profile_many`: candidate foreign keys and overlaps.

    ``data`` is the JSON-ready dictionary.  ``candidates`` is the list of
    fully contained pairs in tier order (strong first; the contained-only
    tier by ascending chance score), ``overlaps`` the partial-containment
    pairs with their orphan counts, and ``coverage`` states what was
    examined.
    """

    def __init__(self, data):
        self.data = data

    @property
    def tables(self):
        return self.data["tables"]

    @property
    def candidates(self):
        return self.data["candidates"]

    @property
    def overlaps(self):
        return self.data["overlaps"]

    @property
    def coverage(self):
        return self.data["coverage"]

    def candidates_frame(self):
        """Candidate records as a Polars DataFrame (flat scalar fields)."""
        rows = [
            {k: v for k, v in record.items() if not isinstance(v, (list, dict))}
            for record in self.candidates
        ]
        return pl.DataFrame(rows)

    def overlaps_frame(self):
        """Overlap records as a Polars DataFrame (flat scalar fields)."""
        rows = [
            {k: v for k, v in record.items() if not isinstance(v, (list, dict))}
            for record in self.overlaps
        ]
        return pl.DataFrame(rows)

    def to_json(self, max_chars=None):
        """Return the multi-table report as JSON text."""
        text = json.dumps(
            self.data, ensure_ascii=False, indent=2, sort_keys=True, default=str
        )
        if max_chars is None or len(text) <= max_chars:
            return text
        body = dict(self.data)
        body["candidates"] = self.candidates[:10]
        body["overlaps"] = self.overlaps[:10]
        body["truncation"] = {
            "full_json_chars": len(text),
            "returned_candidate_records": len(body["candidates"]),
            "returned_overlap_records": len(body["overlaps"]),
        }
        text = json.dumps(
            body, ensure_ascii=False, indent=2, sort_keys=True, default=str
        )
        if max_chars is not None and len(text) > max_chars >= 0:
            return text[:max_chars]
        return text

    def to_digest(self, budget_chars=4000):
        """Return a compact text digest within an approximate character budget."""
        strong = [c for c in self.candidates if c["tier"] == "strong"]
        weak = [c for c in self.candidates if c["tier"] == "weak"]
        noun = "key" if len(self.candidates) == 1 else "keys"
        lines = [
            "Multi-table report: "
            f"{len(self.tables)} tables, {len(self.candidates)} candidate "
            f"foreign {noun} ({len(strong)} strong, {len(weak)} lower-tier), "
            f"{len(self.overlaps)} partial overlaps."
        ]
        for record in strong[:10]:
            lines.append(
                f"- strong: {record['child_table']}.{record['child_column']} -> "
                f"{record['parent_table']}.{record['parent_column']} "
                f"(names agree; {record['child_distinct']} distinct values)"
            )
        for record in weak[:10]:
            lines.append(
                f"- lower-tier: {record['child_table']}.{record['child_column']} -> "
                f"{record['parent_table']}.{record['parent_column']} "
                f"(chance {record['chance_score']:.3g}; "
                f"{record['child_distinct']} distinct values)"
            )
        for record in self.overlaps[:10]:
            lines.append(
                f"- overlap: {record['child_table']}.{record['child_column']} -> "
                f"{record['parent_table']}.{record['parent_column']}: "
                f"{record['containment']:.1%} of {record['child_distinct']} "
                f"distinct values match; {record['orphan_rows']} orphan rows"
            )
        lines.append(self.coverage["statement"])
        text = "\n".join(lines)
        if len(text) <= budget_chars:
            return text
        kept = []
        used = 0
        for line in lines:
            if kept and used + len(line) + 1 > budget_chars:
                break
            kept.append(line)
            used += len(line) + 1
        return "\n".join(kept)

    def __repr__(self):
        strong = sum(1 for c in self.candidates if c["tier"] == "strong")
        return (
            f"MultiTableReport(tables={len(self.tables)}, "
            f"candidates={len(self.candidates)} "
            f"({strong} strong, {len(self.candidates) - strong} lower-tier), "
            f"overlaps={len(self.overlaps)})"
        )


def profile_many(tables, *, overlap_threshold=0.9):
    """Report candidate foreign keys across a mapping of tables.

    ``tables`` maps table names to Polars DataFrames (LazyFrames are
    collected in full: containment is exact, so the values themselves
    are required).  Every ordered pair of columns from different tables
    is examined.  A pair is a *candidate foreign key* when every
    distinct non-null value of the child column appears in a parent
    column that is unique over its non-null values.  Candidates whose
    column names agree after normalisation form the strong tier; the
    rest form a lower tier ordered by ascending chance score, and each
    carries that score.  Nothing is suppressed.

    Pairs into a unique parent whose distinct-value containment share
    reaches ``overlap_threshold`` (but is not complete) are reported as
    overlaps with exact orphan counts: the distinct child values with
    no parent match, and the number of non-null child rows holding
    them.  Null child values are never orphans.

    ``profile_many`` reports cross-table structure only; it does not
    profile the individual tables — :func:`profile` does that, one
    table at a time.  The result is a :class:`MultiTableReport`,
    separate from the single-table report; the single-table report
    contract is unchanged.
    """
    if not isinstance(tables, Mapping):
        raise TypeError("profile_many expects a mapping of table name to frame")
    if not tables:
        raise ValueError("profile_many requires at least one table")
    if not (0 < overlap_threshold <= 1):
        raise ValueError("overlap_threshold must be in (0, 1]")
    frames = {}
    for name, value in tables.items():
        if not isinstance(name, str):
            raise TypeError("table names must be strings")
        if isinstance(value, pl.LazyFrame):
            value = value.collect()
        if not isinstance(value, pl.DataFrame):
            raise TypeError(
                f"table {name!r} must be a Polars DataFrame or LazyFrame, "
                f"got {type(value).__name__}"
            )
        if len(set(value.columns)) != len(value.columns):
            raise ValueError(f"table {name!r} has duplicate column names")
        frames[name] = value

    # If any unsigned 64-bit column holds a value past the signed
    # 64-bit range, integer keys cannot share one numeric dtype: the
    # whole family falls back to decimal-text keys (joins stay
    # dtype-consistent) and integer candidates use the value-pool
    # chance model instead of the span model.
    int_text_keys = False
    for frame in frames.values():
        for name in frame.columns:
            if frame[name].dtype == pl.UInt64:
                peak = frame.select(pl.col(name).max()).item()
                if peak is not None and peak > _INT64_MAX:
                    int_text_keys = True

    columns = []
    unexamined = []
    for table, frame in frames.items():
        for name in frame.columns:
            dtype = frame[name].dtype
            if _family(dtype) is None:
                unexamined.append({
                    "table": table, "column": name, "dtype": str(dtype),
                    "reason": _skip_reason(dtype),
                })
            else:
                columns.append(_Column(table, name, frame, int_text_keys))

    # Value pools per family: the universe the pool-model chance score
    # draws from.  Computed lazily, only for families that produce a
    # pool-model candidate: pooling is a concat of every distinct set
    # in the family, and most runs never need it.  Integer keys are
    # pooled as decimal text so columns past the signed 64-bit range
    # share the pool with ordinary keys.
    pools = {}

    def pool_for(fam):
        if fam not in pools:
            members = [c for c in columns if c.family == fam and c.distinct_count]
            if fam == "integer":
                parts = [c.distinct.with_columns(pl.col("v").cast(pl.Utf8))
                         for c in members]
            else:
                parts = [c.distinct for c in members]
            pools[fam] = pl.concat(parts).unique().height if parts else 0
        return pools[fam]

    funnel = {
        "ordered_cross_table_pairs": 0,
        "same_family_pairs": 0,
        "unique_parent_pairs": 0,
        "size_pruned_pairs": 0,
        "probe_checked_pairs": 0,
        "exact_checked_pairs": 0,
    }
    candidates = []
    overlaps = []
    for child in columns:
        if child.distinct_count == 0:
            continue
        for parent in columns:
            if parent.table == child.table:
                continue
            funnel["ordered_cross_table_pairs"] += 1
            if parent.family != child.family:
                continue
            funnel["same_family_pairs"] += 1
            if not parent.unique:
                continue
            funnel["unique_parent_pairs"] += 1
            # Reachability prune: containment needs child <= parent, and
            # an overlap at the threshold needs the parent to hold at
            # least threshold * child distinct values.
            if child.distinct_count * overlap_threshold > parent.distinct_count:
                funnel["size_pruned_pairs"] += 1
                continue
            funnel["probe_checked_pairs"] += 1
            probe, probe_exact = _probe(child.distinct)
            parent_keys = parent.keyset()
            probe_values = probe.get_column("v").to_list()
            probe_shared = sum(1 for v in probe_values if v in parent_keys)
            if probe_exact:
                shared = probe_shared
                missing = None  # built only if an overlap is reported
            else:
                # Screen: the exact count is worth computing only when
                # the probe share stays within the margin of the
                # overlap threshold (a contained pair probes at 1.0).
                if probe_shared / probe.height < overlap_threshold - _PROBE_MARGIN:
                    continue
                funnel["exact_checked_pairs"] += 1
                missing = child.distinct.join(parent.distinct, on="v", how="anti")
                shared = child.distinct_count - missing.height
            identity = {
                "child_table": child.table,
                "child_column": child.name,
                "parent_table": parent.table,
                "parent_column": parent.name,
                "child_distinct": child.distinct_count,
                "parent_distinct": parent.distinct_count,
                "child_rows": child.rows,
                "child_nonnull_rows": child.nonnull_rows,
                "child_null_rows": child.rows - child.nonnull_rows,
            }
            if shared == child.distinct_count:
                agree = _names_agree(child.name, parent.name)
                record = {
                    **identity,
                    "tier": "strong" if agree else "weak",
                    "name_agreement": agree,
                    "containment": 1.0,
                    "shared_distinct": child.distinct_count,
                    "orphan_rows": 0,
                }
                if child.family == "integer" and not child.text_keys and not parent.text_keys:
                    span = child.span_max - child.span_min + 1
                    inside = parent.distinct.filter(
                        (pl.col("v") >= child.span_min)
                        & (pl.col("v") <= child.span_max)
                    ).height
                    record["chance_score"] = _chance_from_logs(
                        _log_choose(inside, child.distinct_count),
                        _log_choose(span, child.distinct_count),
                    )
                    record["chance_basis"] = "integer_span"
                else:
                    record["chance_score"] = _chance_from_logs(
                        _log_choose(parent.distinct_count, child.distinct_count),
                        _log_choose(pool_for(child.family), child.distinct_count),
                    )
                    record["chance_basis"] = "value_pool"
                candidates.append(record)
            elif shared / child.distinct_count >= overlap_threshold:
                if missing is None:
                    missing = child.distinct.join(
                        parent.distinct, on="v", how="anti"
                    )
                orphan_rows = child.values.join(
                    parent.distinct, on="v", how="anti"
                ).height
                overlaps.append({
                    **identity,
                    "containment": shared / child.distinct_count,
                    "shared_distinct": shared,
                    "orphan_distinct": missing.height,
                    "orphan_rows": orphan_rows,
                    "orphan_examples": child.example_values(missing.sort("v")),
                })

    def candidate_key(record):
        return (
            0 if record["tier"] == "strong" else 1,
            record["chance_score"],
            -record["child_distinct"],
            record["child_table"], record["child_column"],
            record["parent_table"], record["parent_column"],
        )

    candidates.sort(key=candidate_key)
    overlaps.sort(key=lambda r: (
        -r["containment"], r["child_table"], r["child_column"],
        r["parent_table"], r["parent_column"],
    ))

    examined_pairs = funnel["exact_checked_pairs"]
    statement = (
        f"Considered {funnel['ordered_cross_table_pairs']} ordered column "
        f"pairs across different tables; {funnel['same_family_pairs']} pairs "
        f"shared a value family (integer, float, string, or temporal), "
        f"{funnel['unique_parent_pairs']} of those had a unique parent "
        f"candidate, {funnel['probe_checked_pairs']} passed the size prune "
        f"and were probe-screened, and {examined_pairs} pairs were large "
        f"enough to need full verification beyond the probe. Same-table "
        f"pairs were not examined."
    )
    if unexamined:
        statement += (
            f" {len(unexamined)} columns were not examined "
            f"(see coverage.unexamined_columns)."
        )
    data = {
        "kind": "onceover.profile_many",
        "version": _REPORT_VERSION,
        "tables": [
            {"name": name, "rows": frame.height, "columns": len(frame.columns)}
            for name, frame in frames.items()
        ],
        "candidates": candidates,
        "overlaps": overlaps,
        "coverage": {
            "columns_examined": len(columns),
            "unexamined_columns": unexamined,
            "pairs": dict(funnel),
            "families": sorted({c.family for c in columns}),
            "value_pools": {fam: pools[fam] for fam in sorted(pools)},
            "statement": statement,
        },
    }
    return MultiTableReport(data)
