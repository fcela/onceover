"""Regenerate the Adult (Census Income) table in the corpus format
standard.

Source: UCI Adult (id 2),
https://archive.ics.uci.edu/static/public/2/adult.zip
The archive holds the published split: adult.data (32,561 rows)
and adult.test (16,281 rows behind a '|1x3 Cross validator'
header line). The corpus frame is the concatenation, train first:
48,842 rows.

Transformations (the full list; see ATTRIBUTION.md):
- adult.test's header line is skipped, and the trailing period
  its labels carry ('<=50K.') is stripped so both files share
  the label set.
- Every field is stripped of surrounding whitespace (the source
  format is ', '-separated).
- The source's missing-value marker '?' -- the UCI page records
  "Has Missing Values? Yes" -- is decoded to an empty field. It
  occurs in workclass (2,799 rows), occupation (2,809) and
  native-country (857), and nowhere else.
- Columns renamed to fully descriptive names (fnlwgt ->
  final_weight, education-num -> education_num,
  marital-status -> marital_status, capital-gain ->
  capital_gain, capital-loss -> capital_loss, hours-per-week ->
  hours_per_week, native-country -> native_country, the label
  -> income; hyphens become underscores throughout).
- No rows dropped, no values imputed, nothing else touched: the
  duplicate rows, the capital top-code and the education
  encoding are the dataset's character and are kept.

The downloaded zip is verified against the pinned sha256 below
on every run, cached or fresh.
"""
import csv
import hashlib
import io
import sys
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/2/adult.zip"
# Pinned sha256 of the source zip as served at the URL above
# (verified 2026-11-03 against a fresh download). If UCI re-serves
# the archive, update this pin and the registry's prepared-CSV
# pin together, with the reason recorded.
ZIP_SHA256 = "7537312dd56c2b98035880805ce99e68183a30ee468aa5329d6df0fbb3cc21bb"
OUT = "adult-census-income_uci-2.csv"

RAW_COLUMNS = [
    "age", "workclass", "fnlwgt", "education", "education-num",
    "marital-status", "occupation", "relationship", "race", "sex",
    "capital-gain", "capital-loss", "hours-per-week", "native-country",
    "label",
]
RENAMES = {
    "workclass": "work_class",
    "fnlwgt": "final_weight",
    "education-num": "education_num",
    "marital-status": "marital_status",
    "capital-gain": "capital_gain",
    "capital-loss": "capital_loss",
    "hours-per-week": "hours_per_week",
    "native-country": "native_country",
    "label": "income",
}
HEADER = [RENAMES.get(c, c) for c in RAW_COLUMNS]


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def rows_from(text: str, skip_first: bool, strip_label_period: bool):
    lines = [ln for ln in text.splitlines() if ln.strip()]
    if skip_first:
        lines = lines[1:]
    out = []
    for ln in lines:
        fields = next(csv.reader(io.StringIO(ln)))
        assert len(fields) == 15, (len(fields), ln[:60])
        fields = [f.strip() for f in fields]
        if strip_label_period:
            fields[14] = fields[14].rstrip(".")
        fields = ["" if f == "?" else f for f in fields]
        out.append(fields)
    return out


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    zpath = CACHE / "adult.zip"
    if not zpath.exists():
        urllib.request.urlretrieve(URL, zpath)
    digest = _sha256(zpath)
    if digest != ZIP_SHA256:
        sys.exit(f"source zip sha256 mismatch: got {digest}, pinned {ZIP_SHA256}")
    with zipfile.ZipFile(zpath) as z:
        data_text = z.read("adult.data").decode("utf-8")
        test_text = z.read("adult.test").decode("utf-8")
    rows = rows_from(data_text, skip_first=False, strip_label_period=False)
    n_train = len(rows)
    rows += rows_from(test_text, skip_first=True, strip_label_period=True)
    with (HERE / OUT).open("w", newline="") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(HEADER)
        w.writerows(rows)
    payload = (HERE / OUT).read_bytes()
    print(f"train rows: {n_train}, total rows: {len(rows)}")
    print(f"{OUT} sha256: {hashlib.sha256(payload).hexdigest()} "
          f"(registry pins this value; a mismatch means the preparation changed)")


if __name__ == "__main__":
    main()
