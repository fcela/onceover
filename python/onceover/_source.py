"""LazyFrame and file source materialization with deterministic stratified sampling."""

import operator as _operator
import os

#: Default cap on rows collected from a lazy or file source.
_DEFAULT_SOURCE_ROW_CAP = 200_000

#: Row-index column used to select the source sample; a source that
#: already carries this name is rejected rather than overwritten.
_SOURCE_ROW_INDEX = "__onceover_source_row"


def _stratified_indices(n: int, cap: int, seed: int) -> list[int]:
    """The deterministic stratified source sample: one row per
    equal stratum of the source, the row within a stratum chosen by
    a splitmix64 hash of the stratum index and the seed — the design
    `onceover_core::sample_rows` uses for relationship searches,
    applied at the source, with no periodic stride to alias against
    periodic data."""
    mask = (1 << 64) - 1
    out = []
    for j in range(cap):
        lo = (j * n) // cap
        span = ((j + 1) * n) // cap - lo
        if span <= 0:
            continue
        x = (j + seed + 0x9E3779B97F4A7C15) & mask
        x = ((x ^ (x >> 30)) * 0xBF58476D1CE4E5B9) & mask
        x = ((x ^ (x >> 27)) * 0x94D049BB133111EB) & mask
        x ^= x >> 31
        out.append(lo + x % span)
    return out


def _as_int(value):
    """An integer argument as a Python int: int itself and any type
    that implements __index__ (NumPy integers), never bool. None for
    anything else (floats included)."""
    if isinstance(value, bool):
        return None
    try:
        return _operator.index(value)
    except TypeError:
        return None


def _scan_path(path):
    """A path input as (LazyFrame, kind), via the matching scan
    constructor."""
    import polars as pl

    suffix = os.path.splitext(os.fspath(path))[1].lower()
    if suffix in (".parquet", ".pq"):
        return pl.scan_parquet(path), "parquet"
    if suffix == ".csv":
        # infer_schema_length=None: the schema comes from the whole
        # file, so a type that appears only in late rows is not
        # silently read as nulls.
        return pl.scan_csv(path, infer_schema_length=None), "csv"
    if suffix in (".ipc", ".feather", ".arrow"):
        return pl.scan_ipc(path), "ipc"
    raise ValueError(
        f"unsupported file extension {suffix or '(none)'!r}: profile() "
        "reads .parquet, .pq, .csv, .ipc, .feather and .arrow files"
    )


def _materialize_source(df, *, source_row_cap, exact, sample_seed):
    """Resolve a LazyFrame or path input to (frame, source):
    collect in full when the source fits the cap (or exact=True),
    otherwise collect a stratified sample of the source at the cap.
    `source` is the report's source block, stating the basis of
    every finding the report will contain."""
    import polars as pl

    if isinstance(df, pl.LazyFrame):
        lf, kind = df, "lazyframe"
    elif isinstance(df, (str, os.PathLike)):
        lf, kind = _scan_path(df)
    else:
        raise TypeError(
            "profile() expects a polars DataFrame, a polars LazyFrame, "
            "or a path to a Parquet, CSV or IPC file; got "
            f"{type(df).__name__}"
        )
    if source_row_cap is None:
        cap = _DEFAULT_SOURCE_ROW_CAP
    else:
        cap = _as_int(source_row_cap)
        if cap is None or cap <= 0:
            raise ValueError("source_row_cap must be a positive integer")
    seed = _as_int(sample_seed)
    if seed is None or seed < 0:
        raise ValueError("sample_seed must be a nonnegative integer")
    sample_seed = seed
    try:
        n = lf.select(pl.len()).collect(engine="streaming").item()
    except Exception:
        n = lf.select(pl.len()).collect().item()
    source = {"kind": kind, "rows": n, "rows_profiled": n, "basis": "full"}
    if exact or n <= cap:
        return lf.collect(), source
    if _SOURCE_ROW_INDEX in lf.collect_schema():
        raise ValueError(
            f"source already has a column named {_SOURCE_ROW_INDEX!r}"
        )
    indices = _stratified_indices(n, cap, sample_seed)
    work = (
        lf.with_row_index(_SOURCE_ROW_INDEX)
        .filter(pl.col(_SOURCE_ROW_INDEX).is_in(indices))
        .drop(_SOURCE_ROW_INDEX)
    )
    try:
        frame = work.collect(engine="streaming")
    except Exception:
        frame = work.collect()
    source["rows_profiled"] = frame.height
    source["basis"] = "sampled"
    source["sample"] = {
        "method": "stratified",
        "seed": sample_seed,
        "cap": cap,
    }
    return frame, source
