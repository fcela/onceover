"""Regenerate data.csv for the soybean benchmark table.

Source: OpenML dataset 42 ("soybean"), the Michalski & Chilausky soybean
data (683 cases x 35 attributes + class) -- the file GritBot was
demonstrated on. OpenML records the licence as "Public" and the creators
as R.S. Michalski and R.L. Chilausky; the same dataset is UCI Soybean
(Large) (id 90, DOI 10.24432/C5JG6Z), whose current UCI data.csv carries
only 307 rows with integer-coded attributes, so the OpenML ARFF is the
bundled source: its values are the documented text labels already.

Transformations (rename/format only):
- ARFF -> CSV; '?' (missing marker) -> empty.
- Column-name hyphens become underscores; attribute order and the class
  column position (last in the ARFF) are preserved.
"""
import re
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://www.openml.org/data/v1/download/42/soybean.arff"


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "openml-42-soybean.arff"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    attrs, rows, in_data = [], [], False
    for line in raw.read_text().splitlines():
        s = line.strip()
        if not s or s.startswith("%"):
            continue
        low = s.lower()
        if low.startswith("@attribute"):
            name = s.split(None, 2)[1].strip("'\"")
            attrs.append(name.replace("-", "_"))
        elif low.startswith("@data"):
            in_data = True
        elif in_data:
            vals = [v.strip().strip("'\"") for v in s.split(",")]
            vals = ["" if v == "?" else v for v in vals]
            rows.append(vals)
    assert all(len(r) == len(attrs) for r in rows), "ragged ARFF rows"
    out = [",".join(attrs)]
    for r in rows:
        out.append(",".join(f'"{v}"' if "," in v else v for v in r))
    (HERE / "data.csv").write_text("\n".join(out) + "\n")
    print(f"wrote {len(rows)} rows, {len(attrs)} columns")


if __name__ == "__main__":
    main()
