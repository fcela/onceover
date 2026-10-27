"""Standalone example for the `views` detector.

Two small demos, both runnable against the installed package:

1. A shared regime: two continuous columns shift at the same row, plus
   an independent noise control. The shifting pair merges into one
   view; the control stays singleton.
2. Spikes: a single column with three planted spikes at 999.9. The
   spike cells rank first by log predictive density.

Run: python notebooks/views_example.py
"""

import random

import polars as pl

import onceover


def show(report):
    for finding in report.findings:
        print(finding["category"], finding["subject"]["columns"], "::", finding["claim"])


def shared_regime():
    print("== shared regime ==")
    rng = random.Random(3)
    temperature = [20.0 + rng.gauss(0, 1) for _ in range(400)]
    temperature += [32.0 + rng.gauss(0, 1) for _ in range(400)]
    pressure = [1010.0 + rng.gauss(0, 5) for _ in range(400)]
    pressure += [990.0 + rng.gauss(0, 5) for _ in range(400)]
    humidity = [50.0 + rng.gauss(0, 10) for _ in range(800)]
    df = pl.DataFrame(
        {"temperature": temperature, "pressure": pressure, "humidity": humidity}
    )
    show(onceover.profile(df, budget="standard", detectors=["views"]))


def spikes():
    print("== spikes ==")
    rng = random.Random(5)
    series = [20.0 + rng.gauss(0, 1) for _ in range(200)]
    for i in (30, 100, 170):
        series[i] = 999.9
    df = pl.DataFrame({"temperature": series})
    show(onceover.profile(df, budget="standard", detectors=["views"]))


def main():
    shared_regime()
    spikes()


if __name__ == "__main__":
    main()
