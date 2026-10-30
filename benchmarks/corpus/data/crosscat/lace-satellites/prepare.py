#!/usr/bin/env python3
"""Regenerate data.csv for the Lace Satellites corpus dataset.

Bundled dataset (license gate passed: the Lace project is MIT;
see ATTRIBUTION.md). Downloads the table exactly as the Lace
project distributes it (cli/resources/datasets/satellites/data.csv
in promised-ai/lace), verifies its sha256, and installs it as
data.csv unchanged - the same file pylace's lace.examples loads.
"""
import hashlib
import pathlib
import urllib.request

URL = ("https://raw.githubusercontent.com/promised-ai/lace/master/"
       "cli/resources/datasets/satellites/data.csv")
SHA256 = "239eb10a605df2e83101b2732ebee1459c56d96d498b0327be95b5ccc73c6d17"
HERE = pathlib.Path(__file__).parent


def main() -> None:
    raw = urllib.request.urlopen(URL).read()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != SHA256:
        raise SystemExit(f"source sha256 mismatch: {digest} != {SHA256}")
    (HERE / "data.csv").write_bytes(raw)
    print(f"wrote data.csv from {URL} (sha256 verified)")


if __name__ == "__main__":
    main()
