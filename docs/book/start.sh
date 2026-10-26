#!/usr/bin/env bash
# Preview the executable book with the Python environment used by its kernels.
set -euo pipefail

cd "$(dirname "$0")"
repo_root="$(cd ../.. && pwd)"

if [ -n "${ONCEOVER_DOCS_VENV:-}" ]; then
  docs_venv="$ONCEOVER_DOCS_VENV"
elif [ -f "$repo_root/.venv/bin/activate" ]; then
  docs_venv="$repo_root/.venv"
elif [ -n "${VIRTUAL_ENV:-}" ]; then
  docs_venv="$VIRTUAL_ENV"
else
  echo "No documentation Python environment found. Set ONCEOVER_DOCS_VENV to a" >&2
  echo "virtual environment with jupyter-server, ipykernel, and onceover." >&2
  exit 1
fi

if [ ! -f "$docs_venv/bin/activate" ]; then
  echo "Missing Python environment: $docs_venv/bin/activate" >&2
  exit 1
fi
# shellcheck disable=SC1090
source "$docs_venv/bin/activate"

python -c 'import jupyter_server, ipykernel, onceover' || {
  echo "Install jupyter-server, ipykernel, and the onceover wheel in $docs_venv." >&2
  exit 1
}

# Polars prints a shape caption above every table; it is visual noise in the book.
export POLARS_FMT_TABLE_HIDE_DATAFRAME_SHAPE_INFORMATION=1
export POLARS_FMT_TABLE_HIDE_COLUMN_DATA_TYPES=1

exec myst start --execute "$@"
