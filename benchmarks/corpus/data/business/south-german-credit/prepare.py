"""Regenerate the South German credit table in the corpus format
standard.

Source: UCI South German Credit (id 522), the corrected successor
to the Statlog German credit data:
https://archive.ics.uci.edu/static/public/522/south+german+credit.zip
The download holds SouthGermanCredit.asc (a whitespace-separated
file with abbreviated German column names), the dataset's own
codetable.txt, and an R reader. 1,000 credits from 1973 to 1975,
700 good and 300 bad (bad credits heavily oversampled).

Per the corpus format standard this preparation:
- converts the .asc file to a descriptively named CSV;
- renames every column from its German abbreviation to the
  descriptive variable name the dataset page gives for it
  (laufkont -> status, moral -> credit_history, ...);
- maps every categorical code to the label in the dataset's own
  codetable.txt, lightly regularized (ranges spelled out).
Numeric values pass through untouched; no rows are dropped.
"""
import csv
import io
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/522/south+german+credit.zip"
OUT = "south-german-credit_uci-522.csv"

STATUS = {"1": "no checking account", "2": "checking account below 0 DM",
          "3": "checking account 0 to below 200 DM",
          "4": "checking account 200 DM or more, or salary for at least 1 year"}
CREDIT_HISTORY = {"0": "delay in paying off in the past",
                  "1": "critical account or other credits elsewhere",
                  "2": "no credits taken or all credits paid back duly",
                  "3": "existing credits paid back duly till now",
                  "4": "all credits at this bank paid back duly"}
PURPOSE = {"0": "other", "1": "new car", "2": "used car",
           "3": "furniture and equipment", "4": "radio and television",
           "5": "domestic appliances", "6": "repairs", "7": "education",
           "8": "vacation", "9": "retraining", "10": "business"}
SAVINGS = {"1": "unknown or no savings account", "2": "below 100 DM",
           "3": "100 to below 500 DM", "4": "500 to below 1000 DM",
           "5": "1000 DM or more"}
EMPLOYMENT = {"1": "unemployed", "2": "under 1 year",
              "3": "1 to under 4 years", "4": "4 to under 7 years",
              "5": "7 years or more"}
INSTALLMENT = {"1": "35% or more", "2": "25% to under 35%",
               "3": "20% to under 25%", "4": "under 20%"}
PERSONAL = {"1": "male, divorced or separated",
            "2": "female non-single or male single",
            "3": "male, married or widowed", "4": "female, single"}
OTHER_DEBTORS = {"1": "none", "2": "co-applicant", "3": "guarantor"}
RESIDENCE = {"1": "under 1 year", "2": "1 to under 4 years",
             "3": "4 to under 7 years", "4": "7 years or more"}
PROPERTY = {"1": "unknown or no property", "2": "car or other property",
            "3": "building society savings agreement or life insurance",
            "4": "real estate"}
OTHER_PLANS = {"1": "bank", "2": "stores", "3": "none"}
HOUSING = {"1": "for free", "2": "rent", "3": "own"}
NUMBER_CREDITS = {"1": "1", "2": "2 to 3", "3": "4 to 5", "4": "6 or more"}
JOB = {"1": "unemployed or unskilled, non-resident",
       "2": "unskilled, resident", "3": "skilled employee or official",
       "4": "manager, self-employed, or highly qualified employee"}
PEOPLE_LIABLE = {"1": "3 or more", "2": "0 to 2"}
TELEPHONE = {"1": "no", "2": "yes"}
FOREIGN_WORKER = {"1": "yes", "2": "no"}
CREDIT_RISK = {"0": "bad", "1": "good"}

# source column -> (new name, value map or None)
COLUMNS = [
    ("laufkont", "status", STATUS),
    ("laufzeit", "duration", None),
    ("moral", "credit_history", CREDIT_HISTORY),
    ("verw", "purpose", PURPOSE),
    ("hoehe", "amount", None),
    ("sparkont", "savings", SAVINGS),
    ("beszeit", "employment_duration", EMPLOYMENT),
    ("rate", "installment_rate", INSTALLMENT),
    ("famges", "personal_status_sex", PERSONAL),
    ("buerge", "other_debtors", OTHER_DEBTORS),
    ("wohnzeit", "present_residence", RESIDENCE),
    ("verm", "property", PROPERTY),
    ("alter", "age", None),
    ("weitkred", "other_installment_plans", OTHER_PLANS),
    ("wohn", "housing", HOUSING),
    ("bishkred", "number_credits", NUMBER_CREDITS),
    ("beruf", "job", JOB),
    ("pers", "people_liable", PEOPLE_LIABLE),
    ("telef", "telephone", TELEPHONE),
    ("gastarb", "foreign_worker", FOREIGN_WORKER),
    ("kredit", "credit_risk", CREDIT_RISK),
]


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    blob = CACHE / "uci-522-southgerman.zip"
    if not blob.exists():
        urllib.request.urlretrieve(URL, blob)
    with zipfile.ZipFile(blob) as z:
        text = z.read("SouthGermanCredit.asc").decode("utf-8")
    lines = [ln.split() for ln in text.strip().splitlines()]
    header, data = lines[0], lines[1:]
    index = {h: i for i, h in enumerate(header)}
    out_rows = [[new for _, new, _ in COLUMNS]]
    for r in data:
        out = []
        for src, _new, mapping in COLUMNS:
            v = r[index[src]]
            out.append(mapping[v] if mapping else v)
        out_rows.append(out)
    with (HERE / OUT).open("w", newline="") as f:
        csv.writer(f).writerows(out_rows)
    print(f"wrote {len(out_rows) - 1} rows")


if __name__ == "__main__":
    main()
