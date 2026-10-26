#!/usr/bin/env bash
# Build the onceover book as a static HTML site.
#
# Every chapter is a notebook; MyST executes the code cells during the build.
# Requires: myst (npm install -g mystmd) or jupyter-book, plus the Python
# dependencies in requirements.txt and an installed onceover wheel.
#
# Usage:
#   bash build-html.sh                        # local build (root-relative paths)
#   BASE_URL=/onceover bash build-html.sh     # GitHub Pages project site
set -euo pipefail
cd "$(dirname "$0")"

repo_root="$(cd ../.. && pwd)"

# Polars prints a shape caption and a dtype row in its table reprs. Neither is
# content; hide both in the kernels the build spawns.
export POLARS_FMT_TABLE_HIDE_DATAFRAME_SHAPE_INFORMATION=1
export POLARS_FMT_TABLE_HIDE_COLUMN_DATA_TYPES=1

# Activate the environment that owns the installed onceover wheel.
if [ -n "${ONCEOVER_DOCS_VENV:-}" ]; then
  # shellcheck disable=SC1090
  source "$ONCEOVER_DOCS_VENV/bin/activate"
elif [ -f "$repo_root/.venv/bin/activate" ]; then
  # shellcheck disable=SC1091
  source "$repo_root/.venv/bin/activate"
fi

if ! command -v myst >/dev/null 2>&1 && ! command -v jupyter-book >/dev/null 2>&1; then
  echo "Install MyST first: npm install -g mystmd (or pip install jupyter-book)." >&2
  exit 1
fi

if ! python -c 'import jupyter_server, ipykernel, onceover' 2>/dev/null; then
  echo "The active Python environment needs jupyter-server, ipykernel, and onceover." >&2
  echo "Build and install the wheel for this interpreter before building the book." >&2
  exit 1
fi

# BASE_URL controls the asset prefix for a GitHub Pages project site.
export BASE_URL="${BASE_URL:-}"

# A strict, executed build: fail on broken references and on any cell that
# raises, rather than publishing a traceback.
if command -v myst >/dev/null 2>&1; then
  myst build --html --execute --strict --ci
else
  jupyter-book build --html --execute --strict --ci
fi

test -f _build/html/index.html || {
  echo "Build finished without _build/html/index.html." >&2
  exit 1
}

echo "=== Build complete (BASE_URL='${BASE_URL:-<root>}')"
echo "  preview: python -m http.server 3027 --directory _build/html --bind 127.0.0.1"
