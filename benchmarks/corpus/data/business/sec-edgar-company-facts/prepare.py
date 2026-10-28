"""Regenerate data.csv for the SEC EDGAR company-facts benchmark table.

Source: SEC EDGAR company facts API (public domain, US government),
https://data.sec.gov/api/xbrl/companyfacts/CIK##########.json
for a fixed set of 12 large filers (below). One row per reported fact for
six core us-gaap concepts: company, cik, concept, unit, period_end,
fiscal_period, fiscal_year, form, filed, value.

Reported facts include restatements: the same (company, concept,
period_end) can appear several times with different filed dates and
different values. Nothing is deduplicated -- that duplication is the
dataset's documented behavior and part of what the benchmark examines.
SEC fair access: a declared User-Agent, at most 10 requests/second;
this script makes 12 requests with a small delay.
"""
import json
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
COMPANIES = {
    320193: "Apple Inc.",
    789019: "MICROSOFT CORP",
    1652044: "Alphabet Inc.",
    1018724: "AMAZON.COM, INC.",
    1045810: "NVIDIA CORP",
    1326801: "Meta Platforms, Inc.",
    1067983: "BERKSHIRE HATHAWAY INC.",
    19617: "JPMORGAN CHASE & CO",
    72971: "WELLS FARGO & COMPANY/MN",
    320187: "NIKE, Inc.",
    200406: "JOHNSON & JOHNSON",
    732717: "AT&T INC.",
}
CONCEPTS = [
    "Revenues",
    "NetIncomeLoss",
    "Assets",
    "Liabilities",
    "CashAndCashEquivalentsAtCarryingValue",
    "OperatingIncomeLoss",
]
UA = {"User-Agent": "onceover-benchmarks research (contact: local@localhost)"}


def fetch(cik: int) -> dict:
    cache = CACHE / f"edgar-{cik:010d}.json"
    if not cache.exists():
        req = urllib.request.Request(
            f"https://data.sec.gov/api/xbrl/companyfacts/CIK{cik:010d}.json",
            headers=UA,
        )
        with urllib.request.urlopen(req) as r:
            cache.write_bytes(r.read())
        time.sleep(0.15)
    return json.loads(cache.read_text())


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    lines = ["company,cik,concept,unit,period_end,fiscal_period,fiscal_year,form,filed,value"]
    n = 0
    for cik, name in COMPANIES.items():
        facts = fetch(cik)["facts"].get("us-gaap", {})
        for concept in CONCEPTS:
            units = facts.get(concept, {}).get("units", {})
            for unit, entries in units.items():
                if unit not in ("USD", "USD/shares", "shares"):
                    continue
                for e in entries:
                    if "end" not in e or "val" not in e:
                        continue
                    row = [
                        f'"{name}"', str(cik), concept, unit, e["end"],
                        e.get("fp", ""), str(e.get("fy") or ""), e.get("form") or "",
                        e.get("filed") or "", str(e["val"]),
                    ]
                    lines.append(",".join("" if v is None else str(v) for v in row))
                    n += 1
    (HERE / "data.csv").write_text("\n".join(lines) + "\n")
    print(f"wrote {n} fact rows")


if __name__ == "__main__":
    main()
