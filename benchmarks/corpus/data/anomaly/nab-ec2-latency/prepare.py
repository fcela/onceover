"""Regenerate data.csv + labels.json for the NAB ec2_request_latency_system_failure stream.

Source: Numenta Anomaly Benchmark (MIT),
https://raw.githubusercontent.com/numenta/NAB/master/data/realKnownCause/ec2_request_latency_system_failure.csv
Labels: labels/combined_labels.json in the same repository (the three
labeled anomaly points for this stream). The CSV is copied unchanged;
labels.json carries the points into the registry's scoring.
"""
import json
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE.parents[2] / ".raw-cache"
BASE = "https://raw.githubusercontent.com/numenta/NAB/master/"
KEY = "realKnownCause/ec2_request_latency_system_failure.csv"


def main() -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    raw = CACHE / "nab-ec2_request_latency_system_failure.csv"
    if not raw.exists():
        urllib.request.urlretrieve(BASE + "data/" + KEY, raw)
    (HERE / "data.csv").write_bytes(raw.read_bytes())
    lab = CACHE / "nab-combined_labels.json"
    if not lab.exists():
        urllib.request.urlretrieve(BASE + "labels/combined_labels.json", lab)
    points = json.loads(lab.read_text())[KEY]
    (HERE / "labels.json").write_text(json.dumps(
        {"source": "NAB labels/combined_labels.json", "points": points}, indent=1) + "\n")
    print(f"wrote data.csv + {len(points)} label points")


if __name__ == "__main__":
    main()
