# Entity profiles, category and location shuffled (null control) — attribution

- **Source**: derived for this corpus (issue #448) from
  `../entity_profiles/data.csv`; the seeded generator in
  `prepare.py` is the source of the data.
- **License**: generated data, no external source; redistributable
  under the package's MIT license.
- 28,421 rows x 6 columns, same shape as `entity_profiles`; no target column.

## Preprocessing log

- Reads `../entity_profiles/data.csv` as text (no type inference),
  so the unpermuted columns are written back exactly as the source
  has them.
- Permutes the `category` column and the `location` column
  independently across all rows. The other columns are unchanged.
