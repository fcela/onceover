# Shared statistics module

Issue #374, step 5. A refactor with no change to the report.

## What changed

`crates/onceover-core/src/stats.rs` holds one implementation each of:
- the type-7 quantile, the observed-value (nearest-rank) quantile, the median and the IQR;
- 1-based midranks;
- Pearson and Spearman correlation;
- the log-gamma function.

Its module doc states the conventions, and unit tests pin each function against known values.

Removed duplicates now call it:

| quantity | removed from |
|---|---|
| log-gamma | `dep.rs` (Numerical Recipes approximation), `views.rs`, `surprises.rs` |
| midranks | `dep.rs`, `order.rs`, `axisbreaks.rs`, `shapes.rs` |
| Pearson | `dep.rs`, `order.rs`, `axisbreaks.rs`, `shapes.rs` |
| Spearman | `dep.rs` (`axisbreaks.rs` and `shapes.rs` compose the shared ranks and Pearson) |
| interpolated quantile | `card.rs`, `content.rs`, `dep.rs` |
| median | `panel.rs` (×2), `breaks.rs` |

Local wrappers keep their callers' policies: `dep::pearson` returns None below 30 pairs, and the `axisbreaks` correlations return 0 below 3. Net: 27 lines added, 232 removed, outside the new module.

## A correction to #374

Issue #374 described "about 12 quantile/median variants with different tie and interpolation rules". On inspection, the interpolated variants all computed the same type-7 quantile, and the ranking variants the same midranks. One ranks function was 0-based, which does not change a correlation. The exception is the nearest-rank quantile in `dep.rs`, which serves claims that must name an observed value. So the problem was duplicated code, not inconsistent definitions, and the consolidation changes no output.

The one numerical change is the log-gamma in `dep.rs`. It moves from the Numerical Recipes approximation (absolute error about 1e-10) to the g = 7 Lanczos approximation (relative error below 1e-15), which `views.rs` already used. It feeds the chi-square and gamma tail probabilities.

## Results

The full gate against step 4 has 0 worse, 0 improved and 61 totals unchanged. The corpus passes 220/220. `cargo test --workspace` passes, including the 4 new `stats` tests. `python -m pytest tests/ -q` has 845 passed and 1 failed (`test_breaks_segments_table`, failing on main since #373).
