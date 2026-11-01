"""Regenerate the bank marketing table in the corpus format
standard.

Source: UCI Bank Marketing (id 222),
https://archive.ics.uci.edu/static/public/222/data.csv
This file is the original bank-full variant (45,211 rows). In this
variant pdays = -1 is the documented "never previously contacted"
code (the later bank-additional files use 999 instead).

Transformations:
- Columns renamed to fully descriptive names (marital ->
  marital_status, default -> credit_in_default, balance ->
  average_yearly_balance_euros, duration ->
  last_contact_duration_seconds, pdays ->
  days_since_previous_campaign_contact, poutcome ->
  previous_campaign_outcome, y -> subscribed_term_deposit, ...).
- The literal string "NaN" is this file's missing-value marker
  (36,959 rows carry it in poutcome; it also appears in contact,
  education and job) and is decoded to an empty field, per the
  corpus format standard. Recorded consequence: with the marker
  decoded, no current detector reports the never-contacted
  conditional rule (pdays = -1 with previous_campaign_outcome
  missing) that the string form made visible; the loss is recorded
  as a known detector gap in the registry entry and in
  docs/benchmarks/2026-11-01-corpus-format-standard.md. The data is
  not adjusted back.
"""
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/222/data.csv"
OUT = "bank-marketing_uci-222_bank-full.csv"
RENAMES = {
    "marital": "marital_status",
    "default": "credit_in_default",
    "balance": "average_yearly_balance_euros",
    "housing": "housing_loan",
    "loan": "personal_loan",
    "day": "last_contact_day_of_month",
    "duration": "last_contact_duration_seconds",
    "campaign": "contacts_during_this_campaign",
    "pdays": "days_since_previous_campaign_contact",
    "previous": "contacts_before_this_campaign",
    "poutcome": "previous_campaign_outcome",
    "y": "subscribed_term_deposit",
}


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "uci-222-bank.csv"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    text = raw.read_text().replace("\r\n", "\n")
    lines = text.splitlines()
    header = [RENAMES.get(h, h) for h in lines[0].split(",")]
    out = [",".join(header)]
    for ln in lines[1:]:
        out.append(",".join("" if v == "NaN" else v
                            for v in ln.split(",")))
    (HERE / OUT).write_text("\n".join(out) + "\n")
    print(f"wrote {len(out) - 1} rows")


if __name__ == "__main__":
    main()
