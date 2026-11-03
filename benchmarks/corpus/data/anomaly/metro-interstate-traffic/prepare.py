"""Regenerate data.csv for the Metro Interstate Traffic Volume table.

Source: UCI Metro Interstate Traffic Volume (id 492), file
Metro_Interstate_Traffic_Volume.csv.gz inside
https://archive.ics.uci.edu/static/public/492/metro%2Binterstate%2Btraffic%2Bvolume.zip
Transformations: none beyond decompression. The published CSV is
written to data.csv byte for byte -- column names, the string
date_time column, duplicate timestamps, and the gaps in the hourly
grid are all kept exactly as served; they are the dataset's
documented character (see ATTRIBUTION.md). The downloaded zip is
verified against the pinned sha256 below on every run, cached or
fresh.
"""
import gzip
import hashlib
import sys
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://archive.ics.uci.edu/static/public/492/metro%2Binterstate%2Btraffic%2Bvolume.zip"
# Pinned sha256 of the source zip as served at the URL above
# (verified 2026-11-03: a fresh download matched the case-study copy
# byte for byte). If UCI re-serves the archive, update this pin and
# the registry's prepared-CSV pin together, with the reason recorded.
ZIP_SHA256 = "b99aeabcbd6cc86f642da3a79d90883425798f58abd3b3302da2fa19dda73768"
INNER = "Metro_Interstate_Traffic_Volume.csv.gz"


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    zpath = CACHE / "metro+interstate+traffic+volume.zip"
    if not zpath.exists():
        urllib.request.urlretrieve(URL, zpath)
    digest = _sha256(zpath)
    if digest != ZIP_SHA256:
        sys.exit(f"source zip sha256 mismatch: got {digest}, pinned {ZIP_SHA256}")
    with zipfile.ZipFile(zpath) as z:
        payload = gzip.decompress(z.read(INNER))
    (HERE / "data.csv").write_bytes(payload)
    rows = payload.count(b"\n") - 1  # header
    print(f"wrote {rows} rows, {payload.splitlines()[0].count(b',') + 1} columns")
    print(f"data.csv sha256: {hashlib.sha256(payload).hexdigest()} "
          f"(registry pins this value; a mismatch means the preparation changed)")


if __name__ == "__main__":
    main()
