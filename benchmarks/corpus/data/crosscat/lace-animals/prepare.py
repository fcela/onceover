#!/usr/bin/env python3
"""Regenerate data.csv for the Lace Animals corpus dataset.

Bundled dataset (license gate passed: the Lace project is MIT;
see ATTRIBUTION.md). Downloads the table exactly as the Lace
project distributes it (cli/resources/datasets/animals/data.csv
in promised-ai/lace), verifies its sha256, and installs it as
data.csv unchanged - the same file pylace's lace.examples loads.
"""
import hashlib
import pathlib
import urllib.request

URL = ("https://raw.githubusercontent.com/promised-ai/lace/master/"
       "cli/resources/datasets/animals/data.csv")
SHA256 = "09e32808c5163dc3aa34c735bf5f3f5a59d20cd688b18a28ecf19fe8c9eee4c3"
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
