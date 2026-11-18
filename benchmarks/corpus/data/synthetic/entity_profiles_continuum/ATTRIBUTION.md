# Entity profiles, continuum between two profiles — attribution

- **Source**: generated for this corpus (issue #448); the
  seeded generator in `prepare.py` is the source of the data.
- **License**: generated data, no external source; redistributable
  under the package's MIT license.
- 400 customers, 35,245 rows x 6 columns; no target column.
  Each customer's mixing weight between profiles A and B is stored
  in `truth.json`.

## Preprocessing log

None. `prepare.py` writes the table exactly as it is profiled;
the generating spec is stated in its docstring.
