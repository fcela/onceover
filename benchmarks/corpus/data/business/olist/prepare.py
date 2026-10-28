"""Fetch the Olist dataset for local (test-only) benchmark runs.

CC BY-NC-SA 4.0 data: never bundled in this repository. Requires Kaggle
API credentials (~/.kaggle/kaggle.json). Produces data.csv by joining
nothing -- the nine source files land in ./raw/ and the harness entry
for Olist profiles the orders file; multi-table expectations use the
files side by side.
"""
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).parent


def main() -> None:
    raw = HERE / "raw"
    raw.mkdir(exist_ok=True)
    cmd = [
        "kaggle", "datasets", "download", "-d", "olistbr/brazilian-ecommerce",
        "-p", str(raw), "--unzip",
    ]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        print("Kaggle download failed. This dataset is test-only "
              "(CC BY-NC-SA 4.0) and needs Kaggle API credentials.\n"
              + r.stderr[-500:])
        sys.exit(1)
    print("downloaded to", raw)


if __name__ == "__main__":
    main()
