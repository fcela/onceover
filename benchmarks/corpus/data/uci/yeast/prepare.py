"""Regenerate data.csv for the Yeast benchmark table.

Source: UCI Yeast (id 110), fetched with ucimlrepo.
Transformations (rename only):
- The eight score columns are renamed from the documentation's
  abbreviations to what each score is (mcg -> signal_mcgeoch,
  gvh -> signal_von_heijne, alm -> membrane_spanning_score,
  mit -> mitochondrial_discriminant, erl -> er_motif_score,
  pox -> peroxisomal_discriminant, vac -> vacuole_discriminant,
  nuc -> nuclear_signal_score). sequence_name (the SWISS-PROT
  accession, an identifier) and class (the documentation's
  localization-site codes: CYT, NUC, MIT, ME1, ME2, ME3, EXC,
  VAC, POX, ERL) keep their names and values.
"""

import hashlib
import sys
from pathlib import Path

import polars as pl

HERE = Path(__file__).parent
UCI_ID = 110
RENAME = {
    "mcg": "signal_mcgeoch",
    "gvh": "signal_von_heijne",
    "alm": "membrane_spanning_score",
    "mit": "mitochondrial_discriminant",
    "erl": "er_motif_score",
    "pox": "peroxisomal_discriminant",
    "vac": "vacuole_discriminant",
    "nuc": "nuclear_signal_score",
    "Sequence_Name": "sequence_name",
    "sequence_name": "sequence_name",
    "class": "class"
}
DECODE = {}
QUESTION_MARK_MISSING = False
OPTIONAL_RENAME = ["Sequence_Name", "sequence_name"]


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
