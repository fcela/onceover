"""Regenerate data.csv for the hospital (DHA) benchmark table.

Source: the CrossCat project's distributed hospital table,
https://raw.githubusercontent.com/probcomp/crosscat/master/www/data/dha.csv
(Apache-2.0 repository; underlying measures are Medicare spending and
outcome measures for 306 hospitals). This is the hospital table the
CrossCat paper's evaluation used (defense/medicare hospital measures).

Transformations: none beyond a byte-identical copy. Column names are
the source's measure codes (TTL_MDCR_SPND, MDCR_SPND_INP, ...) and are
kept: renaming ~100 measure codes without the source codebook would
invent information. NAME is the facility name.
"""
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://raw.githubusercontent.com/probcomp/crosscat/master/www/data/dha.csv"


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "crosscat-dha.csv"
    if not raw.exists():
        urllib.request.urlretrieve(URL, raw)
    (HERE / "data.csv").write_bytes(raw.read_bytes())
    print("copied", raw.stat().st_size, "bytes")


if __name__ == "__main__":
    main()
