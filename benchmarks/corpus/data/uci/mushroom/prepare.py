"""Regenerate data.csv for the Mushroom benchmark table.

Source: UCI Mushroom (id 73), fetched with ucimlrepo.
Transformations (decode/rename only):
- Column names snake_cased (cap-shape -> cap_shape, ...).
- The class is decoded from the fetched variables metadata: the
  metadata's description of the class variable carries the code
  meanings, and this script refuses to guess them -- if the
  fetched frame's class values are not the documented codes
  (e: edible, p: poisonous) it writes them unchanged and says so.
- The '?' missing symbol (documented on stalk-root) is decoded
  to null.
The per-attribute letter codes of the 22 features are kept as
fetched in this first preparation: their code tables live in the
fetched variables metadata, and decoding them is a separate,
reviewed step recorded in ATTRIBUTION.md.
"""

import hashlib
import sys
from pathlib import Path

import polars as pl

HERE = Path(__file__).parent
UCI_ID = 73
RENAME = {
    "cap-shape": "cap_shape",
    "cap-surface": "cap_surface",
    "cap-color": "cap_color",
    "bruises": "bruises",
    "odor": "odor",
    "gill-attachment": "gill_attachment",
    "gill-spacing": "gill_spacing",
    "gill-size": "gill_size",
    "gill-color": "gill_color",
    "stalk-shape": "stalk_shape",
    "stalk-root": "stalk_root",
    "stalk-surface-above-ring": "stalk_surface_above_ring",
    "stalk-surface-below-ring": "stalk_surface_below_ring",
    "stalk-color-above-ring": "stalk_color_above_ring",
    "stalk-color-below-ring": "stalk_color_below_ring",
    "veil-type": "veil_type",
    "veil-color": "veil_color",
    "ring-number": "ring_number",
    "ring-type": "ring_type",
    "spore-print-color": "spore_print_color",
    "population": "population",
    "habitat": "habitat",
    "class": "class"
}
DECODE = {
    "class": {
        "e": "edible",
        "p": "poisonous"
    }
}
QUESTION_MARK_MISSING = True
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
