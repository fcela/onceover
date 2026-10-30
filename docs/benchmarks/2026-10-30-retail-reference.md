# Online Retail II ships as a pinned reference

Date: 2026-10-30. Issue #21 recorded Online Retail II as test-only
for size, with bundling blocked on a Git LFS decision. The decision
is now made: no Git LFS. The dataset ships as a reference — the
prepared CSV stays uncommitted, and the reference is pinned and
verified at both ends.

What changed:

- `benchmarks/corpus/registry.json`: the entry now pins the
  prepared CSV's sha256
  (`f58108fb637bfae3e14a8adcd5535808478d0f03bfdd9927659044dd29718c19`)
  and row count (1,067,371), with the note recording the decision.
- `prepare.py` pins the source zip's sha256
  (`572e36277c2390fbfde10664750731e0a86f55e33470d91919085f0408e67bfb`,
  the UCI archive as served) and verifies the download against it
  on every run, cached or fresh; it prints the prepared CSV's
  sha256 at the end. Its dependency note now names fastexcel, which
  the xlsx read requires and a bare environment may lack.
- `run_benchmarks.py`: hash verification now applies to any entry
  that pins a sha256, not only to bundled entries, so a reference
  dataset present locally is verified exactly like bundled data.
- The dataset's `ATTRIBUTION.md` records the storage decision and
  both pins.

Verification (2026-10-30): a fresh download from the UCI URL passed
the zip check, and the prepared CSV was byte-identical to the
2026-10-28 preparation (same sha256). Corpus run with the file
present: `online-retail-ii ok`, 1,067,371 rows, 13 findings,
expectations 5/5, run record marks `sha256_verified`. Appending
one byte to the CSV makes the run fail with a hash mismatch.
Python suite 125 passed, 1 skipped; full corpus run exits clean.
