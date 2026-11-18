# Defaults and framing

Issue #446, step 2.

## Changes

- **`date_gaps` is in the default set.** On metro traffic it states "column date_time is missing 218 business days at 2014-08-08..2015-06-11 (307 days elapsed)", a ten-month hole that no default pass reported.
- **Outcome contrasts state the event of a two-level outcome.** Without a declared event, a two-level categorical outcome's per-level contrasts test only its less frequent level. Before, both levels were tested and their mirror statements competed, so the majority level ("'no' is 35.3%") often took the slot.
- **"Monotone" in target shapes means monotone.** A curve is called monotone only when its binned means rise or fall throughout (Spearman |rho| >= 0.9 between bin order and bin mean, `ShapeDiagnostics::bin_trend`). Before, the label rested on the column's Spearman of 0.5 or more. Bike's hourly profile, with peaks at 08:00 and 17:00, now reads "non-monotone".
- **The FD chance bar is 10 expected violations, not 20.** Under random pairing a mapping survives with probability about exp(-expected), below 5e-5 at 10. The bar of 20 from #445 rejected genuine small-table mappings (lace-animals swims <-> water, 50 rows).
- **Missingness indicators are flags only** (booleans or 0/1 columns). A two-level category that coincides with another column's missing rows (planted_mix `source_system`, legacy/current) is a dimension with relations of its own. Before, it was excluded as an indicator and its FD on the income sentinel was lost.

## Corpus expectations

Corpus: 220/220. Five expectations moved to documented misses, each with its reason in the registry:
- thyroid `tsh_measured` FD (a missingness indicator);
- three flag concentrations (thyroid `pregnant`, south-german-credit `foreign_worker`, bike `holiday`), no longer stated since #445;
- messy_intake `fee_raw` FD. `amount_raw` is unique on the complete rows, so the FD is trivial; the 2% relation needs the text numbers parsed.

## Gate

Against main with #446 step 1 merged: no must-tier loss, permuted and must_not ceilings held, invariance improves by 1. Accepted: adventureworks' planted sentinel moves out of its capped full report (date-gap statements take slots), and digest density moves by 5-12 per mille.
