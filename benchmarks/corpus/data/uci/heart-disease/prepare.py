"""Regenerate data.csv for the Heart Disease benchmark table.

Source: UCI Heart Disease (id 45), fetched with ucimlrepo (the
Cleveland database, the subset of 14 attributes all published
experiments use).
Transformations (decode/rename only):
- Columns renamed to the documentation's plain meanings
  (cp -> chest_pain_type, trestbps -> resting_blood_pressure,
  num -> diagnosis, ...).
- Coded categoricals decoded to the documentation's labels:
  sex (0 female, 1 male), chest_pain_type (1 typical angina,
  2 atypical angina, 3 non-anginal pain, 4 asymptomatic),
  resting_ecg (0 normal, 1 ST-T abnormality, 2 left ventricular
  hypertrophy), st_slope (1 upsloping, 2 flat, 3 downsloping),
  thalassemia (3 normal, 6 fixed defect, 7 reversible defect).
  The 0/1 flags (fasting_blood_sugar, exercise_induced_angina)
  and the counts/grades (major_vessels 0-3, diagnosis 0-4) stay
  numeric: their codes are quantities, not labels.
- The '?' missing symbol is decoded to null.
"""

import hashlib
import sys
from pathlib import Path

import polars as pl

HERE = Path(__file__).parent
UCI_ID = 45
RENAME = {
    "age": "age",
    "sex": "sex",
    "cp": "chest_pain_type",
    "trestbps": "resting_blood_pressure",
    "chol": "cholesterol",
    "fbs": "fasting_blood_sugar",
    "restecg": "resting_ecg",
    "thalach": "max_heart_rate",
    "exang": "exercise_induced_angina",
    "oldpeak": "st_depression",
    "slope": "st_slope",
    "ca": "major_vessels",
    "thal": "thalassemia",
    "num": "diagnosis"
}
DECODE = {
    "sex": {
        "0": "female",
        "1": "male"
    },
    "chest_pain_type": {
        "1": "typical_angina",
        "2": "atypical_angina",
        "3": "non_anginal_pain",
        "4": "asymptomatic"
    },
    "resting_ecg": {
        "0": "normal",
        "1": "st_t_abnormality",
        "2": "left_ventricular_hypertrophy"
    },
    "st_slope": {
        "1": "upsloping",
        "2": "flat",
        "3": "downsloping"
    },
    "thalassemia": {
        "3": "normal",
        "6": "fixed_defect",
        "7": "reversible_defect"
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
    for c in ("sex", "chest_pain_type", "resting_ecg", "st_slope", "thalassemia"):
        if c in df.columns and df[c].dtype != pl.Utf8:
            df = df.with_columns(pl.col(c).cast(pl.Int64, strict=False).cast(pl.Utf8))

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
