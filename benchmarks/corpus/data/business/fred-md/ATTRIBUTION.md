# FRED-MD — attribution (test-only in this build)

- **Source**: FRED-MD, McCracken & Ng, Federal Reserve Bank of St. Louis,
  https://www.stlouisfed.org/research/economists/mccracken/fred-databases
  Pinned vintage: monthly/2026-09.csv on files.stlouisfed.org.
- **Citation requested by the source**: McCracken, M. W., & Ng, S. (2016).
  FRED-MD: A monthly database for macroeconomic research. *Journal of
  Business & Economic Statistics*, 34(4), 574-589.
- **Status**: test-only **for access reasons, not license**: on
  2026-10-28 files.stlouisfed.org returned HTTP 403 to this project's
  build environment for both current.csv and the pinned vintage, so no
  bundle could be produced. `prepare.py` is written and will produce
  the bundle (data.csv + transform_codes.csv) from any environment
  that can reach the host; the registry records the blocker.
