# Lace depprob harness (benchmark-only)

A standalone crate that recomputes CrossCat dependence probabilities
with the Rust `lace` crate (the engine the Python bindings wrap), for
the views-pass evaluations recorded in
`docs/benchmarks/2026-11-17-views-deep-look.md`.

This crate is deliberately detached from the workspace (empty
`[workspace]` table in its `Cargo.toml`): `lace` is a benchmark
dependency only and must never become a dependency of the shipped
library.

Modes:

- `depprob <csv> <codebook.yaml> <seed> <out.json>` — fit 16 posterior
  states (5,000 update iterations, the protocol of the 2026-10-30
  head-to-head record) and write the pairwise depprob matrix plus
  per-state view assignments.
- `kinds <csv> <kinds.json> <seed> <out.json>` — same, with a codebook
  generated from a per-column kind map (`cat2` or `cont`); the CSV
  must carry one extra identifier column beyond the codebook columns,
  and the codebook names every row.
