"""Regenerate data.csv for the congressional voting benchmark table.

Source: UCI Congressional Voting Records (id 105),
https://archive.ics.uci.edu/static/public/105/data.csv
Transformations (decode/rename only):
- '?' (the file's missing marker; these are votes not cast / not
  recorded) -> empty.
- Class -> party (values democrat / republican are the documented
  labels); vote columns' hyphens become underscores. y/n values are
  the documented labels and are kept.
"""
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/105/data.csv"


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "uci-105-voting.csv"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    lines = raw.read_text().replace("\r\n", "\n").splitlines()
    header = ["party" if h == "Class" else h.replace("-", "_")
              for h in lines[0].split(",")]
    out = [",".join(header)]
    for line in lines[1:]:
        out.append(",".join("" if v == "?" else v for v in line.split(",")))
    (HERE / "data.csv").write_text("\n".join(out) + "\n")
    print(f"wrote {len(out) - 1} rows")


if __name__ == "__main__":
    main()
