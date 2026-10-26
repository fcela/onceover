"""Execute the example notebook with this Python interpreter and installed package."""

import argparse
import os
from pathlib import Path
import sys
from tempfile import TemporaryDirectory

from jupyter_client import KernelManager
import nbformat
from nbclient import NotebookClient


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="save validated outputs into the notebook")
    root = Path(__file__).resolve().parents[1]
    parser.add_argument("--notebook", type=Path, default=root / "notebooks/onceover_analysis.ipynb")
    args = parser.parse_args()
    path = args.notebook.resolve()
    notebook = nbformat.read(path, as_version=4)
    with TemporaryDirectory(prefix="onceover-notebook-") as temp:
        os.environ["IPYTHONDIR"] = str(Path(temp) / "ipython")
        os.environ["JUPYTER_RUNTIME_DIR"] = temp
        manager = KernelManager(kernel_name="python3")
        manager.kernel_spec.argv = [sys.executable, "-m", "ipykernel_launcher", "-f", "{connection_file}"]
        client = NotebookClient(notebook, km=manager, timeout=600,
                                resources={"metadata": {"path": str(path.parent)}})
        try:
            client.execute()
        finally:
            if manager.has_kernel:
                manager.shutdown_kernel(now=True)
    cells = [cell for cell in notebook.cells if cell.cell_type == "code"]
    assert all(cell.execution_count is not None for cell in cells)
    for cell in cells:
        cell.metadata.pop("execution", None)
    nbformat.validate(notebook)
    if args.write:
        nbformat.write(notebook, path)
    print(f"Notebook passed: {len(cells)} code cells executed with {sys.executable}")


if __name__ == "__main__":
    main()
