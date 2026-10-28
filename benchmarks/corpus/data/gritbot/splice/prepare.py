"""Regenerate data.csv for the splice-junction benchmark table.

Source: UCI Molecular Biology (Splice-junction Gene Sequences) (id 69),
https://archive.ics.uci.edu/static/public/69/data.csv
Transformations (decode/rename only):
- Class codes decoded per the UCI documentation: EI -> exon_intron
  (donor site), IE -> intron_exon (acceptor site), N -> neither.
- Base columns renamed Base1..Base60 -> base_1..base_60; the instance
  name column becomes instance_name. Sequence letters are kept as-is.
"""
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/69/data.csv"
CLASS = {"EI": "exon_intron", "IE": "intron_exon", "N": "neither"}


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "uci-69-splice.csv"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    lines = raw.read_text().splitlines()
    header = []
    for h in lines[0].replace("\r", "").split(","):
        h = h.strip()
        if h == "instancename":
            header.append("instance_name")
        elif h.startswith("Base"):
            header.append("base_" + h[4:])
        else:
            header.append(h.lower())
    out = [",".join(header)]
    for line in lines[1:]:
        vals = line.split(",")
        vals[0] = CLASS.get(vals[0], vals[0])
        out.append(",".join(vals))
    (HERE / "data.csv").write_text("\n".join(out) + "\n")
    print(f"wrote {len(out) - 1} rows")


if __name__ == "__main__":
    main()
