# Library assessment and next steps

Reviewed latest main at `613303d` on 2026-11-04, in an isolated worktree
because the original checkout contains an edited CrossCat design proposal.
Three lower-cost agents reviewed accuracy, performance, and usefulness;
their findings were checked against source and repository evidence.

## What the library should optimize for

The useful outcome is a short, correct first look that tells an analyst
what matters, what evidence supports it, what remains unexamined, and
where to investigate next. Detector count and number of emitted findings
are poor proxies for that outcome.

The library has substantial capability already. The weaknesses are the
gap between finding production and useful selection, incomplete automated
verification of claims, and search costs that remain high on particular
table shapes. Prioritize trustworthy defaults before another broad model
integration. The foundation-model appendix's reported 25-fold tabular
compute increase for one accuracy gain is a reason to require a measured
benefit before adding that path to default profiling.

## Priority and execution order

1. **Trust:** fix existing #111 claim/evidence contradictions and #110
   numeric-role eligibility; build [independent quality gates (#115)](https://github.com/fcela/onceover/issues/115)
   alongside them. These existing bugs should not get duplicate issues.
2. **Useful summaries:** [group verified encoding families and measure
   top-k recall (#116)](https://github.com/fcela/onceover/issues/116).
   Retain supporting evidence and meaningful scoped differences. Evaluate
   which known facts reach the first 10 or 25 findings.
3. **Predictable cost:** [stream wide searches and enforce work budgets
   within expensive passes (#117)](https://github.com/fcela/onceover/issues/117).
   Separate exact, behavior-preserving memory improvements from capped
   searches that change coverage and statistical correction.
4. **Measured optimization:** [reuse discontinuity axis data (#118)](https://github.com/fcela/onceover/issues/118).
   Benchmark a bounded cache against complete-case masks; do not change
   statistical behavior for speed without a separate recall evaluation.
5. **Targeted capability:** address existing #100 moderate/grouped
   missingness, then [boundary mass and suspected top-coding (#119)](https://github.com/fcela/onceover/issues/119).
   Both have concrete corpus misses. Domain claims require declared
   units or constraints; a suspicious pile-up alone does not prove an error.

## Evidence and its limits

- The [detector battery](2026-11-04-detector-battery-step2.md) found nine
  of 22 reports saturated their 100-finding caps, with 601 of 2,024
  visible statements cut. Its new discontinuity cap improves selection
  diversity, but does not reduce computation. These numbers describe
  that recorded sweep, not a fresh run of this assessment.
- The [Adult audit](2026-11-03-adult-census-income.md) independently
  checked 14 claims; 12 agreed and two contradicted evidence. The
  education encoding is restated by several detectors. The same audit
  records missed boundary masses and numeric-role problems.
- The [latest performance review](2026-11-04-code-review.md) already
  reduced discontinuity pass time from 56.3 to 42.3 seconds corpus-wide
  and hotspot time on the wide expression fixture from 11.1 to about
  1.1 seconds. Those fixes are in the reviewed base. Residual costs
  include quadratic pair-list allocation and repeated axis extraction;
  the earlier battery's 56.8% discontinuity share is not a current
  post-optimization measurement.
- In `benchmarks/corpus/run_benchmarks.py`, documented cases always
  return `ok=True`; `coverage_ok` rejects errors only; anomaly coverage
  searches timestamps throughout serialized findings. These contracts
  do not independently establish detection quality. Explicit
  `coverage_status` checks already exist and should be used where
  completion matters, rather than rejecting intentional skips globally.
- The [ADBench experiment](2026-10-30-adbench-scoring.md) found 25 of
  70,155 labeled anomalies under its strict row-evidence mapping. That
  measures a limited row-implication surface, not overall structural
  recall. A new evaluation should keep those tasks distinct.
- The published 79.67-second scale result is a historical single-run
  observation. Cross-host or changed-detector comparisons cannot
  establish a speedup. New performance work should record exact source,
  build, hardware, thread count, repeated timings, memory, and recovery.

## Improvements in this worktree

- Extreme-value claims now describe a multi-value cluster, its range,
  and its inner edge rather than claiming the largest value is alone.
  This fixes the numerical-cluster rendering portion of #111. The
  categorical exception-count issue remains open.
- The CI timing gate gains a 1,000 × 80 fixture with an affine identity
  in the final two columns and checks that it is recovered with all
  column cards. Missing or invalid timing fields now fail cleanly.
  This is a cheap continuous check, not a replacement for the full
  two-million-row benchmark.

## Validation of the changed build

Built a release wheel from this worktree (main `613303d` plus the changes
in this assessment), extracted it into an isolated import directory, and
used the existing environment only for Python dependencies. The original
checkout and its installed package were left intact.

- Python suite: **224 passed**.
- Rust detector crate: **122 passed**, including both tails, conditional
  and global cluster claims, and preserved singleton wording.
- Detector-crate Clippy with warnings denied, workspace formatting, and
  `git diff --check`: passed.
- Timing gate: all three cases passed, at 2.44 s (10k × 50), 0.11 s
  (1k × 10), and 0.42 s (1k × 80). The last case recovered the planted
  affine relationship at column positions 79 and 80 (one-based).
- Full scale: **2,000,000 × 200**, standard budget, four Rayon threads,
  **57.13 s**, **3.52 GiB peak process RSS**, **0.45 GiB increase from
  pre-profile RSS**. All eight recovery/cardinality checks passed,
  including all three relationships beyond column 64; zero independent
  noise targets were reported predictable. The [raw result](../../benchmarks/results/2m-200-assessment-2026-11-04.json)
  retains environment details and coverage.

The full-scale measurement ran after compilation and other validation
finished. It times profiling only, excludes data generation and Python
report materialization, and samples RSS every 20 ms. It is one local
observation, not a performance claim. An initial latest-main build measured
57.15 s and passed the same checks; this work changes claim rendering and
the CI harness, not the cost of discovery. The remaining categorical
exception-count problem in #111 is not fixed by this change.
