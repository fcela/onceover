# Hierarchy peers in entity fingerprints

The hint discovery in `infer_spec()` proposes finest-to-coarsest
paths separately where a column has more than one kind of parent.
The proposals require complete, repeated grouping values and
exact many-to-one mappings. Only paths passed back as
`hints["hierarchies"]` are applied, and the detector checks the
mappings again against the profiled frame.

For an entity comparison, membership is constant across that
entity's rows. The detector chooses the smallest supported pool
before looking at the anomaly statistic, breaking ties in
declaration order. The subject is held out of the peer reference.
A static candidate must clear the peer range by ten percentage
points and a corrected Hoeffding probability bound. The range
omits the most extreme peer on that side when ten or more peers
are available. Peers are equally weighted in the baseline.
For weighted shares the probability bound uses effective rows,
which account for the concentration of weights within each
entity. For drift the support check applies on both sides of
each boundary, and the shared-change guard runs within the pool.

## Probes

| Case | Result |
|---|---|
| Crossed teams and sites | Hint discovery returns host → team → department and host → site → region as separate paths. The proposals are unchanged after reordering rows and columns. |
| Supported team and sparse team | A00 is compared with 11 team peers. C00 has too few team members and uses 26 regional peers. Regular hosts in a team with a different mix stay silent. |
| Multiple supported paths | H00 uses seven site peers, the smaller supported pool. |
| Identical pickup counts, different spend | H00 is flagged only in the weighted comparison. |
| Group-wide temporal change | Gallery's 24 hosts produce a shared shift. One Workshop host produces an individual finding against the other 23 Workshop hosts. |
| Broken mapping | Coverage refuses the declared path and makes no unscoped peer anomaly claim. |

The generated probes are in `tests/test_frame_spec.py` and
`tests/test_entity_fingerprints.py`. The
[library craft-kit case study](../book/case_studies/library_craft_kits.ipynb)
runs the same API on 44,403 synthetic requests. It proposes three
paths, accepts the host → program_session → program path, and
recovers all seven planted individual cases. The control flags no
individual hosts and still reports the planned Gallery schedule
change.

The full gate (seed 0) against `d0e23b3` passed with 61 scored
totals unchanged and no exceeded ceilings. Before and after,
all 228 machine-checked corpus expectations passed. The Python
suite passed all 888 tests, the Rust workspace checks passed,
and the strict executed book built successfully.
