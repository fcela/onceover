# Foundation-model evaluation: Kumo and Timestral probes (2026-11-03)

Protocol: five experiments in two rounds, evaluating pretrained
foundation models as candidate second-pass screeners (relational
prediction, single-table prediction, forecasting, anomaly detection).
All runs executed on CPU on one machine (4 cores, 7,934 MiB RAM, no
GPU), against baselines computed on the same data: seasonal-naive
forecasts for the forecasting legs, base-rate AUROC for the relational
probes, and a defaults HistGradientBoosting model for the tabular leg.
Round 1: Kumo Relational (structured-data-models) and Timestral
(28M and 5M checkpoints) on a planted-signal synthetic, an
AdventureWorks derivation, the state-unemployment panel, and Beijing
PM2.5 at Dongsi. Round 2: Kumo Forecast on the identical forecasting
protocol, Kumo Anomaly on Dongsi's 11 channels, and Kumo Tabular
against gradient boosting on Taiwan credit default (fixed split, seed
20261103, 4,000 context / 2,000 query).

The full experimental record — environment and stacks, per-experiment
protocols and deviations, results tables, what the evaluation does and
does not establish, and the reproducibility index — is the book
appendix "Foundation-model evaluation"
(`docs/book/appendix/foundation_models.ipynb`). This entry is the
pointer; the appendix is the record, and the tables are not duplicated
here.

Headline results: Kumo Relational runs on CPU (planted-signal AUROC
0.628 against the 0.500 base rate through the direct path, 0.572
through the sampling path; AdventureWorks smoke test 1.000 with a
separability caveat). Timestral beats the lag-12 seasonal naive on
Michigan, California, and South Dakota on the monthly panel and loses
to the weekly naive on hourly Beijing PM2.5, with intervals
over-covering on the panel and under-covering on Beijing. Kumo
Forecast, on the identical protocol, splits the panel (wins Michigan
and California, loses Nevada and South Dakota) and loses to both
naives on Beijing; it emits point forecasts only. Kumo Anomaly
completed no run on CPU in three attempts (about two hours of CPU,
zero scored rows). Kumo Tabular beat the gradient-boosting baseline on
the fixed Taiwan split, AUROC 0.787 against 0.759, at about 25 times
the fit-and-predict compute.

Provenance: working files (scripts, logs, raw result JSONs) are kept
outside the repository in `~/workspace/research_notes/kumo-probe-20261103/`
(round 1, relational), `~/workspace/research_notes/timestral-probe-20261103/`
(round 1, Timestral), and `~/workspace/research_notes/kumo-ts-probe-20261103/`
(round 2). The design note the evaluation feeds is
`~/workspace/research_notes/onceover-foundation-screeners-20261103/design.md`.
