"""Regenerate data.csv for the state unemployment benchmark table.

Source: FRED (Federal Reserve Bank of St. Louis) downloads of the BLS
Local Area Unemployment Statistics monthly state unemployment rates,
series <STATE>UR (e.g. CAUR), via
https://fred.stlouisfed.org/graph/fredgraph.csv?id=<SERIES>
51 series (50 states + District of Columbia), monthly from 1976-01,
merged wide on the observation date. Public data (US government /
FRED distribution); the CrossCat evaluations used state unemployment
panels of this form.

Transformations: merge on date; '.' (FRED's missing marker in early
fredgraph exports) -> empty. No values altered.
"""
import csv
import io
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
STATES = ["AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA",
          "HI", "ID", "IL", "IN", "IA", "KS", "KY", "LA", "ME", "MD",
          "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV", "NH", "NJ",
          "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC",
          "SD", "TN", "TX", "UT", "VT", "VA", "WA", "WV", "WI", "WY", "DC"]


def fetch_series(code: str) -> dict[str, str]:
    raw = CACHE / f"fred-{code}UR.csv"
    if not raw.exists():
        url = f"https://fred.stlouisfed.org/graph/fredgraph.csv?id={code}UR"
        urllib.request.urlretrieve(url, raw)
    rows = list(csv.reader(io.StringIO(raw.read_text())))
    return {r[0]: ("" if r[1] == "." else r[1]) for r in rows[1:] if len(r) == 2}


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    series = {}
    for st in STATES:
        series[st] = fetch_series(st)
    dates = sorted({d for s in series.values() for d in s})
    out = io.StringIO()
    w = csv.writer(out)
    w.writerow(["date"] + [f"{st.lower()}_unemployment_rate" for st in STATES])
    for d in dates:
        w.writerow([d] + [series[st].get(d, "") for st in STATES])
    (HERE / "data.csv").write_text(out.getvalue())
    print(f"wrote {len(dates)} months x {len(STATES)} states")


if __name__ == "__main__":
    main()
