"""Regenerate the absenteeism table in the corpus format standard.

Source: UCI Absenteeism at Work (id 445),
https://archive.ics.uci.edu/static/public/445/data.csv
740 absence records of employees of a Brazilian courier company,
July 2007 to July 2010 (dataset page). The file as served uses short
column names and numeric codes for every categorical variable. Per
the corpus format standard this preparation:
- writes a descriptively named CSV (see the registry entry's
  `file` field);
- renames every column to a fully descriptive name;
- maps each documented code to its natural label, using the code
  lists on the dataset page (reason for absence: the 21 ICD
  categories plus the 7 non-CID reasons; day of the week 2-6;
  season 1-4; education 1-4; yes/no flags 0/1).
Codes 0 in `reason for absence` and `month of absence` fall outside
the documented lists; they are labelled "not recorded (code 0 in
source)" rather than silently dropped or guessed at. No rows are
dropped and no numeric value is altered.
"""
import csv
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/445/data.csv"
OUT = "absenteeism-at-work_uci-445.csv"

REASON = {
    "0": "not recorded (code 0 in source)",
    "1": "certain infectious and parasitic diseases",
    "2": "neoplasms",
    "3": "diseases of the blood, blood-forming organs and immune mechanism",
    "4": "endocrine, nutritional and metabolic diseases",
    "5": "mental and behavioural disorders",
    "6": "diseases of the nervous system",
    "7": "diseases of the eye and adnexa",
    "8": "diseases of the ear and mastoid process",
    "9": "diseases of the circulatory system",
    "10": "diseases of the respiratory system",
    "11": "diseases of the digestive system",
    "12": "diseases of the skin and subcutaneous tissue",
    "13": "diseases of the musculoskeletal system and connective tissue",
    "14": "diseases of the genitourinary system",
    "15": "pregnancy, childbirth and the puerperium",
    "16": "certain conditions originating in the perinatal period",
    "17": "congenital malformations, deformations and chromosomal abnormalities",
    "18": "symptoms, signs and abnormal findings not elsewhere classified",
    "19": "injury, poisoning and other consequences of external causes",
    "20": "external causes of morbidity and mortality",
    "21": "factors influencing health status and contact with health services",
    "22": "patient follow-up",
    "23": "medical consultation",
    "24": "blood donation",
    "25": "laboratory examination",
    "26": "unjustified absence",
    "27": "physiotherapy",
    "28": "dental consultation",
}
MONTH = {
    "0": "not recorded (code 0 in source)",
    "1": "january", "2": "february", "3": "march", "4": "april",
    "5": "may", "6": "june", "7": "july", "8": "august",
    "9": "september", "10": "october", "11": "november", "12": "december",
}
DAY = {"2": "monday", "3": "tuesday", "4": "wednesday",
       "5": "thursday", "6": "friday"}
SEASON = {"1": "summer", "2": "autumn", "3": "winter", "4": "spring"}
EDUCATION = {"1": "high school", "2": "graduate", "3": "postgraduate",
             "4": "master and doctor"}
YESNO = {"0": "no", "1": "yes"}

# source header -> (new name, value map or None)
COLUMNS = [
    ("ID", "employee_id", None),
    ("Reason for absence", "reason_for_absence", REASON),
    ("Month of absence", "month_of_absence", MONTH),
    ("Day of the week", "day_of_week", DAY),
    ("Seasons", "season", SEASON),
    ("Transportation expense", "transportation_expense", None),
    ("Distance from Residence to Work", "distance_from_residence_to_work_km", None),
    ("Service time", "service_time", None),
    ("Age", "age", None),
    ("Work load Average/day ", "work_load_average_per_day", None),
    ("Hit target", "hit_target", None),
    ("Disciplinary failure", "disciplinary_failure", YESNO),
    ("Education", "education", EDUCATION),
    ("Son", "number_of_children", None),
    ("Social drinker", "social_drinker", YESNO),
    ("Social smoker", "social_smoker", YESNO),
    ("Pet", "number_of_pets", None),
    ("Weight", "weight", None),
    ("Height", "height", None),
    ("Body mass index", "body_mass_index", None),
    ("Absenteeism time in hours", "absenteeism_time_hours", None),
]


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "uci-445-absenteeism.csv"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    with raw.open(newline="") as f:
        rows = list(csv.reader(f))
    header = rows[0]
    index = {h: i for i, h in enumerate(header)}
    out_rows = [[new for _, new, _ in COLUMNS]]
    for r in rows[1:]:
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
