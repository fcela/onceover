"""Regenerate the incident event log in the corpus format standard.

Source: UCI Incident Management Process Enriched Event Log
(id 498):
https://archive.ics.uci.edu/static/public/498/incident+management+process+enriched+event+log.zip
An event log of 141,712 events over 24,918 incidents from a
ServiceNow incident management process, enriched with attributes
from the underlying relational database (dataset page).

Per the corpus format standard this preparation:
- renames the ServiceNow-internal columns to descriptive names
  (number -> incident_id, u_symptom -> symptom, cmdb_ci ->
  configuration_item, rfc -> request_for_change_id, sys_* ->
  system_*, ...);
- decodes the source's missing-value symbol "?" to an empty field
  (the dataset page states that unknown information is missing;
  "?" appears in 18 columns, up to 99.98% of rows);
- normalizes the five timestamp columns from the served
  day/month/year format to ISO 8601 (values unchanged in meaning);
- strips the numeric prefix from impact, urgency and priority
  ("2 - Medium" -> "Medium"), which the served values carry.
No rows are dropped and no other value is altered.
"""
import csv
import re
import urllib.request
import zipfile
from datetime import datetime
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = ("https://archive.ics.uci.edu/static/public/498/"
       "incident+management+process+enriched+event+log.zip")
OUT = "servicenow-incident-event-log_uci-498.csv"

RENAMES = {
    "number": "incident_id",
    "sys_mod_count": "system_modification_count",
    "sys_created_by": "system_created_by",
    "sys_created_at": "system_created_at",
    "sys_updated_by": "system_updated_by",
    "sys_updated_at": "system_updated_at",
    "u_symptom": "symptom",
    "cmdb_ci": "configuration_item",
    "u_priority_confirmation": "priority_confirmation",
    "rfc": "request_for_change_id",
    "caused_by": "caused_by_change_id",
    "closed_code": "close_code",
}
TIMESTAMPS = {"opened_at", "system_created_at", "system_updated_at",
              "resolved_at", "closed_at"}
PREFIXED = {"impact", "urgency", "priority"}
PREFIX = re.compile(r"^\d+ - ")


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    blob = CACHE / "uci-498-incident-log.zip"
    if not blob.exists():
        urllib.request.urlretrieve(URL, blob)
    with zipfile.ZipFile(blob) as z:
        raw = z.read("incident_event_log.csv").decode("utf-8")
    rows = list(csv.reader(raw.splitlines()))
    header = [RENAMES.get(h, h) for h in rows[0]]
    ts_idx = [i for i, h in enumerate(header) if h in TIMESTAMPS]
    px_idx = [i for i, h in enumerate(header) if h in PREFIXED]
    bad = []
    out_rows = [header]
    for r in rows[1:]:
        out = []
        for i, v in enumerate(r):
            if v == "?":
                v = ""
            elif i in ts_idx and v:
                try:
                    v = datetime.strptime(v, "%d/%m/%Y %H:%M").strftime(
                        "%Y-%m-%dT%H:%M")
                except ValueError:
                    bad.append(v)
            elif i in px_idx:
                v = PREFIX.sub("", v)
            out.append(v)
        out_rows.append(out)
    if bad:
        raise SystemExit(f"unparsed timestamps: {bad[:5]} ({len(bad)} total)")
    with (HERE / OUT).open("w", newline="") as f:
        csv.writer(f).writerows(out_rows)
    print(f"wrote {len(out_rows) - 1} rows")


if __name__ == "__main__":
    main()
