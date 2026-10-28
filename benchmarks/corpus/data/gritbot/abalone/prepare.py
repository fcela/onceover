"""Regenerate data.csv for the abalone benchmark table.

Source: UCI Abalone (id 1), https://archive.ics.uci.edu/static/public/1/data.csv
Transformations (decode/rename only):
- Sex codes decoded per the UCI documentation: M -> male, F -> female,
  I -> infant.
- Column names lowercased (they arrive capitalized).
"""
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/1/data.csv"
SEX = {"M": "male", "F": "female", "I": "infant"}


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "uci-1-abalone.csv"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    lines = raw.read_text().splitlines()
    header = [h.strip().lower() for h in lines[0].split(",")]
    out = [",".join(header)]
    for line in lines[1:]:
        vals = line.split(",")
        vals[0] = SEX.get(vals[0], vals[0])
        out.append(",".join(vals))
    (HERE / "data.csv").write_text("\n".join(out) + "\n")
    print(f"wrote {len(out) - 1} rows")


if __name__ == "__main__":
    main()
