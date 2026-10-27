"""Standalone example for the `views` detector.

Two small demos, both runnable against the installed package:

1. A shared partition: two columns have the same two operating states.
   They merge into one view; an independently generated noise column
   stays in a separate view.
2. Spikes: a single column with three planted spikes at 999.9. The
   pass reports the planted spikes and may report other low-density cells.

Run: python notebooks/views_example.py
"""

import random

import polars as pl

import onceover


def show(report):
    for finding in report.findings:
        print(finding["category"], finding["subject"]["columns"], "::", finding["claim"])


def shared_partition():
    print("== shared partition ==")
    rng = random.Random(3)
    temperature = [20.0] * 400 + [32.0] * 400
    pressure = [1010.0] * 400 + [990.0] * 400
    humidity = [50.0 + rng.gauss(0, 10) for _ in range(800)]
    df = pl.DataFrame(
        {"temperature": temperature, "pressure": pressure, "humidity": humidity}
    )
    report = onceover.profile(df, budget="standard", detectors=["views"])
    shared = [set(f["subject"]["columns"]) for f in report.findings
              if f["category"] == "associations"]
    assert {"temperature", "pressure"} in shared
    assert all("humidity" not in columns for columns in shared)
    show(report)


def spikes():
    print("== spikes ==")
    rng = random.Random(5)
    series = [20.0 + rng.gauss(0, 1) for _ in range(200)]
    for i in (30, 100, 170):
        series[i] = 999.9
    df = pl.DataFrame({"temperature": series})
    report = onceover.profile(df, budget="standard", detectors=["views"])
    rows = {cell["row"] for f in report.findings if f.get("type") == "spike_cells"
            for e in f["evidence"] for cell in e["metrics"].get("cells", [])}
    planted = {30, 100, 170}
    assert planted <= rows
    show(report)
    print(f"Planted extreme values found: {len(rows & planted)}/{len(planted)}")
    print(f"Other reported cells: {len(rows - planted)}")


def main():
    shared_partition()
    spikes()


if __name__ == "__main__":
    main()
