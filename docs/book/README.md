# Building the onceover book

The book is a [MyST](https://mystmd.org) site. Every chapter is an
executable notebook; outputs are produced during the build and are never
stored in the sources. The Part I tutorials, the case studies, and the
code-bearing methodology and implementation chapters profile the
**installed** `onceover` package. Executing the chapters checks that the
examples run with that installed version.

## Setup

```bash
# from the repository root
python -m venv .venv
source .venv/bin/activate
pip install -r docs/book/requirements.txt maturin
maturin develop --release        # or install a wheel built for this interpreter
npm install -g mystmd            # provides the `myst` CLI
```

`jupyter-book` (installed by `requirements.txt`) is the Python wrapper
around the same build engine; either CLI works.

## Preview

```bash
cd docs/book
bash start.sh
```

The script activates the repository `.venv` (or the environment named by
`ONCEOVER_DOCS_VENV`), checks that `jupyter-server` and `ipykernel` are
available, and runs `myst start --execute`. Execution needs a Jupyter
server: without one, MyST reports `Could not load Jupyter session manager`
for every uncached notebook. Pass a different port with
`bash start.sh --port 3001`.

## HTML build

```bash
cd docs/book
bash build-html.sh               # -> _build/html/index.html
BASE_URL=/onceover bash build-html.sh   # GitHub Pages project site
```

The script runs a strict, executed build and fails if any notebook raises.
Charts are Altair and tables are Great Tables, both rendered to static
HTML assets. The deploy workflow in
`.github/workflows/pages.yml` uses the same command.

## Fresh execution

MyST can reuse notebook execution results. To run every book notebook and the
analysis notebook in new kernels, use the installed package's environment from
the repository root:

```bash
python scripts/validate_notebooks.py --output-dir /tmp/onceover-notebooks
```

The command clears outputs in memory, executes all code cells, and writes
executed copies and JSON logs to the output directory. It fails on execution
errors or unexecuted cells. The summary records the interpreter, package version,
source hashes, and cell counts. Source notebooks remain unchanged.

## Pre-publish validation

The chapters use small fixtures on purpose. Before publishing a change that
touches sampling, caps, or detector output, run the reproducible scale
benchmark — with relationships planted beyond column 64, where a screener
that only looks at the first few dozen columns would miss them:

```bash
python benchmarks/scale.py --rows 2000000 --columns 200 --threads 4 \
  --output /tmp/onceover-scale.json
```

The recorded run and its interpretation are in
[`benchmarks/README.md`](../../benchmarks/README.md). A smaller regression
subset (`tests/test_scale.py`) runs in CI on every push.

Run `python benchmarks/views.py --output /tmp/views-sensor.json` to check the
separate views sensor fixture. That benchmark records sampled spike recovery,
extra flags, and cluster diagnostics; it does not establish views discovery
beyond the detector's 64-column limit.
