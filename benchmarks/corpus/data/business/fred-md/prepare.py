"""Regenerate data.csv for the FRED-MD benchmark panel.

Source: FRED-MD (McCracken & Ng, Federal Reserve Bank of St. Louis),
pinned vintage file:
https://files.stlouisfed.org/files/htdocs/fred-md/monthly/2026-09.csv
Transformations:
- The file's second row holds per-series transformation codes, not data;
  it is removed and the codes are written to transform_codes.csv beside
  data.csv (they are part of the dataset's documentation, kept visible).
- sasdate (M/D/YYYY) becomes an ISO `date` column (YYYY-MM-01).

Environment note (2026-10-28): files.stlouisfed.org returned HTTP 403 to
this project's build environment for both current.csv and the pinned
vintage; the registry entry is therefore test-only until a run from an
environment that can reach the host produces the bundle. The registry
records this; the expectations are written but unevaluated.
"""
import csv
import io
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://files.stlouisfed.org/files/htdocs/fred-md/monthly/2026-09.csv"


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "fred-md-2026-09.csv"
    if not raw.exists():
        req = urllib.request.Request(URL, headers={"User-Agent": "onceover-benchmarks"})
        with urllib.request.urlopen(req) as r:
            raw.write_bytes(r.read())
    rows = list(csv.reader(io.StringIO(raw.read_text())))
    header, tcodes, body = rows[0], rows[1], rows[2:]
    assert tcodes[0].strip() == "Transform:"
    with open(HERE / "transform_codes.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["series", "transform_code"])
        for s, c in zip(header[1:], tcodes[1:]):
            w.writerow([s, c])
    out = io.StringIO()
    w = csv.writer(out)
    w.writerow(["date"] + header[1:])
    for r in body:
        m, d, y = r[0].split("/")
        w.writerow([f"{y}-{int(m):02d}-01"] + r[1:])
    (HERE / "data.csv").write_text(out.getvalue())
    print(f"wrote {len(body)} rows, {len(header)} series")


if __name__ == "__main__":
    main()
