# Gate scorecard

The gate answers one question: compared with a base build, is this
build's output better, the same, or worse? `run.py` profiles a set of
tables with the installed `onceover` and writes a scorecard (JSON).
`compare.py` takes the scorecards of two builds and prints a markdown
table with base, head and delta per metric. It exits 0 when no metric
is worse and 1 when any metric is worse beyond its tolerance.

    python benchmarks/gate/run.py --out scorecard.json --commit <sha>
    python benchmarks/gate/run.py --out s.json --only abalone --components planted,cost
    python benchmarks/gate/run.py --out s.json --fast        # three small tables
    python benchmarks/gate/compare.py base.json head.json

Run options: `--only` (comma-separated dataset ids), `--components`,
`--seeds` (comma-separated integers, default `0`), `--fast`, `--commit`.
Compare options: `--time-rel` (default 0.20), `--time-floor` in seconds
(default 1.0), `--include-held-out`, `--accepted` (default
`accepted.json`), `--out`.

The full run takes about 100 seconds on a laptop. It needs `polars` and
`numpy` (the plant generators and shuffles use numpy).

## What is measured

Every figure is a count or a time. The runs are seeded and
deterministic, so a count has zero tolerance: any change in the worse
direction fails. Time is compared on totals with a relative tolerance
(20%) and an absolute floor (1 second). Metrics marked `info` are
shown and never fail. Metrics marked `zero` (statement mismatches,
failed reference checks) fail whenever the head value is above zero.

| Component | Question | Better |
|---|---|---|
| `planted` | A known structure is injected into a real table (a relabelled copy of a categorical column, a sentinel value, a sum of two columns, a binary target driven by one column, three extreme rows, missingness tied to one level). Is it stated in the full report, and in the 4,000-character digest? Each plant's defining property is checked with Polars/NumPy on the generated frame before profiling. | `found_full`, `found_4k` higher |
| `permuted` | Every column is shuffled independently, which keeps each column's values and destroys every relationship. How many statements about two or more columns remain (full report and 4k digest, default and target mode)? Each is false by construction. | lower |
| `invariance` | Facts (category, type, sorted column names) lost plus gained when the column order is reversed, the rows are shuffled (only for tables without a time axis), the columns are renamed, an independent noise column is added, or one numeric column is multiplied by 1000. The exclusions are listed in `components/invariance.py`. | lower |
| `reference` | Hand-written statements per dataset (see below), found in the 4k digest, the 8k digest and the full report, in default and target mode. | higher |
| `redundancy` | In the 4k digest: finding lines whose column set and relation kind repeat an earlier line, characters on finding lines (per thousand), and numbers printed with six or more decimals. | `repeated`, `long_decimals` lower; `finding_permille` higher |
| `truth` | `report.verify(df)` statuses. `mismatch` and `error` must be 0. | lower |
| `cost` | Median profile time over three runs; JSON size; number of findings. | time lower |

Totals are sums over datasets that are not held out. A held-out dataset
is scored and written to the scorecard (`datasets` and
`held_out_totals`) and is reported in a separate section by
`compare.py`; it is excluded from the verdict unless
`--include-held-out` is given. A dataset or component that ran in the
base and failed or was skipped in the head counts as worse.

## Files

- `registry.json`: the datasets, and the headline metrics of the report.
- `components/`: one module per component, registered in
  `components/__init__.py`.
- `gate_plants.py`: the plant catalogue.
- `reference/<dataset id>.json`: reference statements.
- `gatelib.py`: loading (through `benchmarks/corpus/run_benchmarks.py`,
  including its sha256 check), profiling with a cache, digest parsing.
- `results/`: committed baseline scorecards.
- `accepted.json`: regressions accepted on purpose.
- `report.py`: the history of scorecards and the report rendered from it.

Nothing in `run.py`, `compare.py`, `report.py` or `gatelib.py` names a
dataset, a plant or a component.

## Add a dataset

A gate dataset refers to a corpus dataset id in
`benchmarks/corpus/registry.json`. Add one entry to
`registry.json`:

```json
{"id": "metro-interstate-traffic", "corpus": "metro-interstate-traffic",
 "target": "traffic_volume",
 "read": {"parse_dates": true}, "hints": {"time": "date_time"},
 "components": ["planted", "cost"],
 "held_out": false, "fast": false}
```

- `target`: a column name, `"@last"` for the last column, or omit it.
  With a target, the target-mode scores are recorded.
- `components`: omit to run every component.
- `held_out`: scored but excluded from the verdict.
- `fast`: member of the `--fast` subset.
- `read`: overrides of the corpus read options; `hints`, `detectors`,
  `budget`: passed to `profile`; `options`: per-component options, for
  example `{"planted": {"plants": ["sum_identity"]}}`.

A test-only corpus dataset whose data file is absent is recorded with
status `skipped` and its reason; it does not fail the run.

## Add a plant

A plant is a function in `gate_plants.py` that takes the table and a
seeded NumPy generator and returns `(frame, spec)`, or `None` when the
table has no usable column. The spec holds the finding `types` that can
state the structure, the `columns` a stating finding must name, a
`verify(frame)` function that checks the structure outside the
library, and optionally `target` and `patterns`. Example:

```python
def constant_shift(df, rng):
    """A copy of a numeric column with 100 added to every value."""
    pick = _pick(rng, _numeric_cols(df))
    if not pick:
        return None
    col = pick[0]
    new = _fresh(df, f"{col}_shifted")
    out = df.with_columns((pl.col(col).cast(pl.Float64) + 100).alias(new))
    def verify(d):
        return bool(np.allclose(d[new].to_numpy() - d[col].to_numpy(), 100))
    return out, {"types": {"affine"}, "columns": {col, new}, "verify": verify}
```

Then add one line to `PLANTS`: `"constant_shift": constant_shift,`. A
finding matches a plant when its type is in `types`, its stated columns
(subject columns, or target plus drivers for model-based findings)
include `columns`, and it names at most two further columns. A digest
line matches when any finding it stands for matches, or, failing that,
when every regular expression in `patterns` matches that line.

## Add a component

A component is a module with `NAME`, `DESCRIPTION`, `METRICS` and
`run(ctx)`, plus one line in `components/__init__.py`. `ctx` holds the
table (`ctx.df`), the target (`ctx.target`), the seeds, and
`ctx.profile(df=None, target=None)` which profiles (the dataset itself
by default, cached). `run` returns `{"metrics": {...}, "detail": {...}}`
with numeric metrics, or `{"status": "not_applicable", "reason": "..."}`.

```python
# components/columns.py
NAME = "columns"
DESCRIPTION = "Number of columns the report gives a card to."
METRICS = {"cards": {"direction": "info", "kind": "count"}}

def run(ctx):
    n = len(ctx.profile().data["columns"])
    return {"metrics": {"cards": n}, "detail": {}}
```

Add `columns` to the import and to `COMPONENTS` in
`components/__init__.py`. `direction` is `lower`, `higher` or `info`;
`kind` is `count` (zero tolerance) or `time` (compared on totals);
`"zero": true` fails the comparison whenever the head value is above
zero. A metric or component present only in the head is listed as new
and not compared.

## Reference statements

`reference/<dataset id>.json` has a `dataset` field and a list of
`statements`. Each statement:

| Field | Meaning |
|---|---|
| `id` | short unique name |
| `text` | the statement in words |
| `tier` | `must` or `should` |
| `provisional` | `true` while the statement has not been reviewed |
| `columns` | columns a matching finding must state |
| `columns_any` | optional: a matching finding states at least one of these |
| `kind` | optional relation kind: `relates`, `encoding`, `identity`, `order`, `sentinel`, `missingness`, `outlier`, `concentration`, `completeness`, `role` (table `KINDS` in `components/reference.py`) |
| `types` | optional: finding types allowed |
| `patterns` | optional list of regular expressions; the fallback match when no finding fits: all must match one statement line of the digest, or one claim of the full report |
| `modes` | optional: `["default"]` or `["target"]` to score in one mode only |
| `check` | optional Polars expression string that must evaluate to `True` on the data |

Example:

```json
{"id": "cnt-identity", "tier": "must", "provisional": true,
 "text": "cnt = casual + registered",
 "kind": "identity", "columns": ["cnt", "casual", "registered"],
 "patterns": ["\\bcnt\\b", "casual", "registered"],
 "check": "(pl.col('cnt') == pl.col('casual') + pl.col('registered')).all()"}
```

A statement whose `check` is false is wrong about the data and is
counted in `check_failed`. To add statements for a dataset, create
`reference/<id>.json`; a dataset without a file is skipped by this
component. Digest lines identify findings by exemplar id when they are
rolled up and otherwise by claim text, and the gate looks the finding up
in the report to compare its columns.

## Accepted regressions

A change sometimes makes one metric worse on purpose: removing a kind
of statement lowers the count of statements on shuffled tables and can
also change which facts survive a column reorder. `accepted.json`
records such a trade so the comparison passes:

```json
{"accepted": [
  {"metric": "invariance.reverse", "scope": "total", "limit": 280,
   "reason": "order violations between unrelated columns removed (#293); 9 facts no longer stated after reversal"}
]}
```

- `metric` is `component.metric` as printed by `compare.py`.
- `scope` is `total` (the default) or one dataset id. An accepted
  total also accepts that metric's per-dataset rows.
- `limit` is the worst head value accepted. A head value beyond it
  fails as before.
- `reason` is required and is printed in the comparison and in the
  report.

The entry is part of the pull request that needs it, so it is reviewed
with the change. Once the change is on main the base contains it, the
entry accepts nothing, and `compare.py` says so in its notes; remove it
then.

## History and report

`report.py` keeps one history entry per change and renders the history
as one markdown report:

    python benchmarks/gate/report.py add scorecard.json --history DIR \
        --commit <sha> --date <commit date, ISO 8601> --subject "<subject line>"
    python benchmarks/gate/report.py render --history DIR --out REPORT.md

An entry is the scorecard without its per-component detail, plus the
commit, its date, its subject line and the accepted regressions in
force. The report has five parts:

1. Headline metrics, one row per change, newest first. Each value is
   followed by its movement against the change before it, with
   `better` or `worse`.
2. The latest change: what got better, what got worse, what was
   accepted and why, in totals and per dataset.
3. Every total metric over the most recent changes (`--last`, default 8).
4. The latest scorecard per dataset.
5. The held-out datasets.

The headline metrics are the `headline` list in `registry.json`: each
entry names a metric, a column label and a one-line meaning. Add or
remove an entry there to change what the report shows first.

Seconds are compared only between entries produced on the same
platform.

On every push to main, the CI job `Gate history` scores the build, adds
the entry to the `history/` directory of the `gate-history` branch, and
regenerates that branch's `README.md`, which is the report. Open the
`gate-history` branch on GitHub to read it. The same report is written
to the job summary. The first run starts the history from the baselines
committed under `results/`.

## CI

The `Gate scorecard` job in `.github/workflows/ci.yml` runs on pull
requests. It builds a release wheel from the merge base and from the PR
head in one job, installs each into its own virtual environment, runs
`run.py` with each, runs `compare.py`, appends the table to the job
summary, uploads both scorecards, and fails when `compare.py` exits 1.
The scripts and corpus data always come from the PR head.
