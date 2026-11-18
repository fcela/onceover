# Entity profiles, one profile with varying supports (null control) — attribution

- **Source**: generated for this corpus (issue #448); the
  seeded generator in `prepare.py` is the source of the data.
- **License**: generated data, no external source; redistributable
  under the package's MIT license.
- 400 customers, 20,725 rows x 6 columns; no target column.
  Every customer draws from one category mix. The customer ids of
  the small (3 to 8 row) customers are listed in `truth.json`.

## Preprocessing log

None. `prepare.py` writes the table exactly as it is profiled;
the generating spec is stated in its docstring.
