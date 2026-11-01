"""Regenerate the Taiwanese bankruptcy table in the corpus format
standard.

Source: UCI Taiwanese Bankruptcy Prediction (id 572),
https://archive.ics.uci.edu/static/public/572/data.csv
6,819 companies, 95 financial-ratio features plus the bankruptcy
flag; data collected from the Taiwan Economic Journal for 1999-2009,
bankruptcy defined by Taiwan Stock Exchange regulations.
Transformations:
- The file as served separates fields with ", " and prefixes
  column names with a space; it is decoded as CSV with
  skipinitialspace and written back as a plain comma-separated
  file with trimmed column names. No values are altered.
- `Bankrupt?` -> `bankrupt` (rename only).
- Format-standard cleanup of the served names: the source typo
  `Working capitcal Turnover Rate` -> `Working capital Turnover
  Rate`, and the currency glyph in the three "(Yuan ¥)" per-share
  columns spelled out as "(Yuan)".
- Output file renamed from data.csv to the descriptive
  taiwanese-bankruptcy_uci-572.csv (corpus format standard).
"""
import csv
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/572/data.csv"
OUT = "taiwanese-bankruptcy_uci-572.csv"
RENAMES = {
    "Bankrupt?": "bankrupt",
    "Working capitcal Turnover Rate": "Working capital Turnover Rate",
    "Revenue Per Share (Yuan ¥)": "Revenue Per Share (Yuan)",
    "Operating Profit Per Share (Yuan ¥)": "Operating Profit Per Share (Yuan)",
    "Per Share Net profit before tax (Yuan ¥)":
        "Per Share Net profit before tax (Yuan)",
}


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "uci-572-taiwanese.csv"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    with raw.open(newline="") as f:
        rows = list(csv.reader(f, skipinitialspace=True))
    rows[0] = [RENAMES.get(h.strip(), h.strip()) for h in rows[0]]
    with (HERE / OUT).open("w", newline="") as f:
        csv.writer(f).writerows(rows)
    print(f"wrote {len(rows) - 1} rows")


if __name__ == "__main__":
    main()
