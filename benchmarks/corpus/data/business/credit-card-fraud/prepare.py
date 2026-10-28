"""Fetch the ULB credit-card fraud dataset for local (test-only) runs.
Requires Kaggle API credentials. Copies creditcard.csv to data.csv
unchanged (the source file is already in documented column form)."""
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).parent


def main() -> None:
    raw = HERE / "raw"
    raw.mkdir(exist_ok=True)
    r = subprocess.run(
        ["kaggle", "datasets", "download", "-d", "mlg-ulb/creditcardfraud",
         "-p", str(raw), "--unzip"],
        capture_output=True, text=True)
    if r.returncode != 0:
        print("Kaggle download failed (credentials required).\n" + r.stderr[-500:])
        sys.exit(1)
    shutil.copy(raw / "creditcard.csv", HERE / "data.csv")
    print("wrote data.csv")


if __name__ == "__main__":
    main()
