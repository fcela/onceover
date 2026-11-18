# Entity profiles (customer transactions) — attribution

- **Source**: generated for this corpus (issue #448); the
  seeded generator in `prepare.py` is the source of the data.
- **License**: generated data, no external source; redistributable
  under the package's MIT license.
- 28,421 rows x 6 columns; no target column. Entity
  `cust_id`, dimensions `category` and `location`, measure
  `cost`, axis `month`. The planted groups, misfits, drift and
  location specialists are listed in `truth.json`.

## Preprocessing log

None. `prepare.py` writes the table exactly as it is profiled;
the generating spec is stated in its docstring.
