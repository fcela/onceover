"""Regenerate data.csv for the bank marketing benchmark table.

Source: UCI Bank Marketing (id 222),
https://archive.ics.uci.edu/static/public/222/data.csv
This file is the original bank-full variant (45,211 rows). In this
variant pdays = -1 is the documented "never previously contacted" code
(the later bank-additional files use 999 instead). Transformations:
- y -> subscribed_term_deposit (rename only).
- No value rewriting. The literal string "NaN" in categorical columns
  is a missing-value marker in this file (36,959 rows carry it in
  poutcome). Decoding it to empty was tried and reverted on measurement:
  with nulls, no current detector reports the never-contacted structure
  (pdays = -1 with poutcome missing); with the string as served, a
  conditional rule reports it exactly. The interaction is recorded in
  docs/benchmarks/2026-10-28-initial-corpus.md. 'unknown' is a genuine
  category level and is kept either way.
"""
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/222/data.csv"


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "uci-222-bank.csv"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    text = raw.read_text().replace("\r\n", "\n")
    lines = text.splitlines()
    header = lines[0].split(",")
    header = ["subscribed_term_deposit" if h == "y" else h for h in header]
    lines[0] = ",".join(header)
    (HERE / "data.csv").write_text("\n".join(lines) + "\n")
    print(f"wrote {len(lines) - 1} rows")


if __name__ == "__main__":
    main()
