"""Regenerate data.csv for the ULB credit-card fraud benchmark table.

Source: OpenML dataset 43627 ("credit-card-fraud-detection"), the
ARFF distribution of the ULB / Worldline transaction data originally
released on Kaggle (mlg-ulb/creditcardfraud). OpenML is used because
it serves the file without account credentials; its record publishes
an md5 checksum for the file, which is verified here alongside a
pinned sha256.

Transformation (format only; no rows dropped, no values altered):
the ARFF data section is plain CSV under a 31-attribute header; this
script copies those rows verbatim under snake_case column names
(Time -> time, V1..V28 -> v1..v28, Amount -> amount, Class -> class).

The OpenML record gives the licence as ODbL (database) / DbCL
(contents), which fails this repository's bundle gate: the dataset
is test-only and data.csv is never committed. The registry pins the
prepared CSV's sha256, and the corpus runner verifies it whenever
the file is present locally.
"""
import hashlib
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
URL = "https://openml.org/data/v1/download/22102452/credit-card-fraud-detection.arff"
# Integrity pins for the source file: the md5 is the checksum OpenML
# itself publishes in the dataset record (verified 2026-10-30); the
# sha256 was computed from the same download.
ARFF_MD5 = "d7d4434708894b1d786706505defd2fe"
ARFF_SHA256 = "f9b190614995080ee8ba205a75b538704a55f093b8c1991de10de4f23f6461de"
HEADER = ["time"] + [f"v{i}" for i in range(1, 29)] + ["amount", "class"]


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    arff = CACHE / "credit-card-fraud-detection.arff"
    if not arff.exists():
        urllib.request.urlretrieve(URL, arff)
    blob = arff.read_bytes()
    md5 = hashlib.md5(blob).hexdigest()
    if md5 != ARFF_MD5:
        sys.exit(f"source arff md5 mismatch: got {md5}, OpenML publishes {ARFF_MD5}")
    sha = hashlib.sha256(blob).hexdigest()
    if sha != ARFF_SHA256:
        sys.exit(f"source arff sha256 mismatch: got {sha}, pinned {ARFF_SHA256}")
    lines = blob.decode("utf-8").splitlines()
    start = next(i for i, ln in enumerate(lines) if ln.strip().lower() == "@data")
    rows = [ln for ln in lines[start + 1:] if ln.strip()]
    out = HERE / "data.csv"
    payload = (",".join(HEADER) + "\n" + "\n".join(rows) + "\n").encode()
    out.write_bytes(payload)
    # Class is served as 1.0/0.0 text in the ARFF data section.
    fraud = sum(1 for ln in rows if ln.rsplit(",", 1)[1] in ("1", "1.0"))
    print(f"wrote {len(rows)} rows ({fraud} fraud), {len(HEADER)} columns")
    print(f"data.csv sha256: {hashlib.sha256(payload).hexdigest()} "
          f"(registry pins this value; a mismatch means the preparation changed)")


if __name__ == "__main__":
    main()
