"""Regenerate data.csv for the Air Quality benchmark table.

Source: UCI Air Quality (id 360), fetched with ucimlrepo.
Transformations (rename/format only):
- Sensor and reference columns renamed to plain snake_case names
  (C6H6(GT) -> benzene_gt, PT08.S1(CO) -> pt08_s1_co, ...).
- Date and Time rendered ISO (the source formats are DD/MM/YYYY
  and HH.MM.SS).
- The -200 values are NOT touched: -200 is the dataset's
  documented missing-value code, and stating it is the profiler's
  job, not the preparation's.
"""

import hashlib
import sys
from pathlib import Path

import polars as pl

HERE = Path(__file__).parent
UCI_ID = 360
RENAME = {
    "Date": "date",
    "Time": "time",
    "CO(GT)": "co_gt",
    "PT08.S1(CO)": "pt08_s1_co",
    "NMHC(GT)": "nmhc_gt",
    "C6H6(GT)": "benzene_gt",
    "PT08.S2(NMHC)": "pt08_s2_nmhc",
    "NOx(GT)": "nox_gt",
    "PT08.S3(NOx)": "pt08_s3_nox",
    "NO2(GT)": "no2_gt",
    "PT08.S4(NO2)": "pt08_s4_no2",
    "PT08.S5(O3)": "pt08_s5_o3",
    "T": "temperature",
    "RH": "relative_humidity",
    "AH": "absolute_humidity"
}
DECODE = {}
QUESTION_MARK_MISSING = False
OPTIONAL_RENAME = []


def _metadata_text(ds) -> str:
    md = ds.metadata
    parts = []
    for attr in ("name", "doi", "repository_url", "data_license", "license"):
        value = getattr(md, attr, None)
        if value:
            parts.append(f"{attr}: {value}")
    parts.append(str(md))
    return "\n".join(parts)


def main() -> None:
    try:
        from ucimlrepo import fetch_ucirepo
    except ImportError:
        sys.exit("ucimlrepo is not installed: pip install ucimlrepo")
    ds = fetch_ucirepo(id=UCI_ID)
    print(_metadata_text(ds))
    print(ds.variables.to_string())
    meta = _metadata_text(ds)
    if "CC BY 4.0" not in meta and "Attribution 4.0" not in meta:
        sys.exit(
            "license gate: the metadata ucimlrepo returned for this "
            "dataset does not state CC BY 4.0; refusing to write "
            "data.csv. Record the stated license in ATTRIBUTION.md "
            "and in the registry entry instead.")
    frames = []
    for part in (ds.data.ids, ds.data.features, ds.data.targets):
        if part is not None and len(part.columns):
            frames.append(part.reset_index(drop=True))
    if not frames:
        sys.exit("fetch returned no data frames")
    import pandas as pd
    df = pl.from_pandas(pd.concat(frames, axis=1))
    missing = [src for src in RENAME
               if src not in df.columns and src not in OPTIONAL_RENAME]
    if missing:
        sys.exit(f"rename sources not in the fetched frame: {missing}; "
                 f"fetched columns: {df.columns}")
    df = df.rename({k: v for k, v in RENAME.items() if k in df.columns})
    extra = [c for c in df.columns if c not in RENAME.values()]
    if extra:
        print(f"note: fetched columns kept under their own names: {extra}")
    if QUESTION_MARK_MISSING:
        df = df.with_columns([
            pl.when(pl.col(c) == "?").then(None).otherwise(pl.col(c)).alias(c)
            for c in df.columns if df[c].dtype == pl.Utf8])
    for col, fmt in (("date", "%d/%m/%Y"), ("time", "%H.%M.%S")):
        if col in df.columns and df[col].dtype == pl.Utf8:
            parsed = df[col].str.to_datetime(fmt, strict=False)
            bad = parsed.null_count()
            if bad:
                sys.exit(f"{col}: {bad} values do not match the documented format {fmt}")
            df = df.with_columns(parsed.dt.strftime("%Y-%m-%d" if col == "date" else "%H:%M:%S").alias(col))

    for col, mapping in DECODE.items():
        if col not in df.columns:
            sys.exit(f"decode column not in the prepared frame: {col}")
        df = df.with_columns(pl.col(col).replace(mapping, default=pl.col(col)))
        print(f"decode {col}: values now {df[col].unique().to_list()[:10]}")
    out = HERE / "data.csv"
    df.write_csv(out)
    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    print(f"wrote {out} rows={df.height} cols={df.width} sha256={digest}")


if __name__ == "__main__":
    main()
