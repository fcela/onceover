"""Regenerate data.csv for the GritBot thyroid benchmark table.

Source: UCI Thyroid Disease (id 102), files allhypo.data + allhypo.test from
thyroid+disease.zip. These are the files behind RuleQuest's GritBot sample
results: 2,800 + 972 = 3,772 cases, and line 1365 of allhypo.data is
GritBot's "data case 1365" (age 455, label 861) -- verified against
https://rulequest.com/gritbot-examples.html before this mapping was adopted.

Transformations (decode/rename only):
- Column names from allhypo.names, spaces -> underscores.
- The trailing "|<id>" record suffix becomes the record_id column.
- '?' (the files' missing marker) -> empty.
- Boolean flags stored as t/f -> true/false; diagnosis values lose the
  trailing '.' used by the .names class list.
- A case_ref column (data:<line> / test:<line>, 1-indexed) preserves the
  numbering GritBot's published output uses.
"""
import io
import re
import sys
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
ZIP_URL = "https://archive.ics.uci.edu/static/public/102/thyroid+disease.zip"
FLAG_COLS = {
    "on_thyroxine", "query_on_thyroxine", "on_antithyroid_medication",
    "sick", "pregnant", "thyroid_surgery", "i131_treatment",
    "query_hypothyroid", "query_hyperthyroid", "lithium", "goitre",
    "tumor", "hypopituitary", "psych", "tsh_measured", "t3_measured",
    "tt4_measured", "t4u_measured", "fti_measured", "tbg_measured",
}


def fetch() -> tuple[str, str, str]:
    CACHE.mkdir(parents=True, exist_ok=True)
    zpath = CACHE / "thyroid+disease.zip"
    if not zpath.exists():
        urllib.request.urlretrieve(ZIP_URL, zpath)
    z = zipfile.ZipFile(zpath)
    names = z.read("allhypo.names").decode("latin-1")
    data = z.read("allhypo.data").decode("latin-1")
    test = z.read("allhypo.test").decode("latin-1")
    return names, data, test


def parse_columns(names_text: str) -> list[str]:
    cols = []
    in_attrs = False
    for line in names_text.splitlines():
        if "|  classes" in line:
            in_attrs = True
            continue
        if in_attrs:
            m = re.match(r"^([^:]+):", line.strip())
            if m:
                name = m.group(1).strip().lower().replace(" ", "_")
                cols.append(name)
    return cols + ["diagnosis"]


def rows(text: str, split: str, cols: list[str]):
    out = []
    for i, line in enumerate(text.splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        body, _, rec = line.partition("|")
        vals = [v.strip() for v in body.split(",")]
        assert len(vals) == len(cols), (split, i, len(vals), len(cols))
        row = {}
        for c, v in zip(cols, vals):
            if v == "?":
                v = ""
            if c in FLAG_COLS:
                v = {"t": "true", "f": "false"}.get(v, v)
            if c == "diagnosis":
                v = v.rstrip(".")
            row[c] = v
        row["record_id"] = rec
        row["case_ref"] = f"{split}:{i}"
        out.append(row)
    return out


def main() -> None:
    names_text, data, test = fetch()
    cols = parse_columns(names_text)
    all_rows = rows(data, "data", cols) + rows(test, "test", cols)
    fieldnames = ["case_ref", "record_id"] + cols
    out = io.StringIO()
    out.write(",".join(fieldnames) + "\n")
    for r in all_rows:
        cells = []
        for c in fieldnames:
            v = r[c]
            if "," in v or '"' in v:
                v = '"' + v.replace('"', '""') + '"'
            cells.append(v)
        out.write(",".join(cells) + "\n")
    (HERE / "data.csv").write_text(out.getvalue())
    print(f"wrote {len(all_rows)} rows, {len(fieldnames)} columns")


if __name__ == "__main__":
    sys.exit(main())
