# Interestingness taxonomy, grounded in the corpus (2026-10-28)

Issue #25 asks for a taxonomy of what makes a table region
interesting, written against measured corpus behavior rather than
in the abstract. Each family below names its corpus instance and
the profiler's measured status on it. Statuses are from the run
records in `benchmarks/corpus/runs/` and the pack write-ups.

## Families

1. **Subset-conditional surprise (case isolation).** A value that is
   ordinary globally and surprising inside a subset defined by other
   columns. Corpus: all five GritBot thyroid cases, soybean cases
   558/614, the abalone constraint violations. **Status: absent.**
   Structures are found (FDs, rules); the surprising cases inside
   them are not isolated. The corpus's single largest gap.
2. **Transient events in a series.** Hours-to-days dips/spikes
   against a stable regime. Corpus: NAB nyc_taxi (0/5 covered),
   ec2 latency (0/3). **Status: absent** — the changes machinery
   targets sustained level shifts.
3. **Near-identities with tolerance.** Relations that hold to
   rounding but not exactly. Corpus: hospital TTL_MDCR_SPND vs its
   8 parts (max abs deviation 2 over 307 rows). **Status: absent**
   — identities require near-exactness; the near miss is invisible.
4. **Exact structure with violations.** FDs and identities that hold
   except for a handful of rows. Corpus: soybean's class+halo ->
   size FD with exactly 1 violation (the GritBot case); retail's
   customer -> country at 99.95%. **Status: found** — this is the
   profiler's strength, and the violation counts are reported.
5. **Concentration and near-constancy.** Corpus: thyroid flag
   columns, retail country at 91.9% UK. **Status: found.**
6. **Missingness structure.** Missingness that is itself determined
   by other columns. Corpus: thyroid assay flags (value -> measured
   FDs), bank never-contacted (visible only while the source's
   string marker survives — see the preprocessing measurement in
   the initial corpus entry). **Status: found, fragile** — it
   depends on representation choices upstream of the profiler.
7. **Panel incompleteness.** Missing cells in a declared grid.
   Corpus: EDGAR facts (restatements and gaps), the #5 skinny-panel
   case. **Status: found when declared** (hints/expectations pass);
   undeclared panels are not yet recognized as panels.
8. **Duplicates and near-duplicates.** Corpus: retail's 34,335
   exact duplicate rows. **Status: exact found; near-duplicates
   (same entity, variant spelling/key) untested by the corpus so far.**
9. **Heaping and digit preference.** Rounded-value bunching in
   measured quantities. Corpus: no member exhibits it yet.
   **Status: untested** — a taxonomy family awaiting a dataset
   (a known gap in the corpus, recorded).
10. **Format mixing.** One column, several shapes. Corpus: retail
    stock codes, bank poutcome lengths. **Status: found.**

## The two families that become detector work

By corpus evidence — number of documented cases missed, across
independent packs — the ranking is not close:

1. **Subset-conditional surprise** (family 1): the entire GritBot
   pack's case list, across four unrelated domains, misses on this
   one capability.
2. **Transient events** (family 2): both NAB streams score 0
   coverage, and the events are the labeled ground truth of the
   anomaly pack.

Near-identities (family 3) are third: one measured instance, and a
plausible cheaper mechanism (a tolerance mode on an existing pass)
than either family above.
