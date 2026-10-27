"""Execute documentation notebooks fresh and retain output/provenance externally."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

import nbformat
from nbclient import NotebookClient
from jupyter_client import KernelManager


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--python', default=sys.executable)
    parser.add_argument('--timeout', type=int, default=600)
    parser.add_argument('notebooks', nargs='*', type=Path, help='Optional notebook subset')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output_dir.resolve()
    if output == root:
        parser.error('Choose an output directory distinct from the repository root')
    output.mkdir(parents=True, exist_ok=True)
    paths = sorted(p for p in (root / 'docs/book').rglob('*.ipynb')
                   if '_build' not in p.relative_to(root).parts)
    paths.append(root / 'notebooks/onceover_analysis.ipynb')
    if args.notebooks:
        paths = [(root / p).resolve() for p in args.notebooks]
    probe = subprocess.check_output([args.python, '-c',
        'import json,sys,onceover,importlib.metadata as m; '
        'print(json.dumps(dict(python=sys.executable,package=onceover.__file__,version=m.version("onceover"))))'], text=True)
    summary = {'started_utc': datetime.now(timezone.utc).isoformat(),
               'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
               'working_tree_modified': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip()),
               'runtime': json.loads(probe), 'notebooks': []}
    for path in paths:
        rel = path.relative_to(root)
        source_bytes = path.read_bytes()
        source_hash = hashlib.sha256(source_bytes).hexdigest()
        notebook = nbformat.reads(source_bytes.decode(), as_version=4)
        nbformat.validate(notebook)
        for cell in notebook.cells:
            if cell.cell_type == 'code':
                cell.outputs = []
                cell.execution_count = None
        target = output / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        manager = KernelManager(kernel_name='python3')
        manager.kernel_spec.argv = [args.python, '-m', 'ipykernel_launcher', '-f', '{connection_file}']
        client = NotebookClient(notebook, km=manager, timeout=args.timeout,
                                resources={'metadata': {'path': str(path.parent)}},
                                allow_errors=True, record_timing=True)
        start = time.monotonic()
        failure = None
        try:
            client.execute(cwd=str(path.parent))
        except Exception as exc:
            failure = f'{type(exc).__name__}: {exc}'
        finally:
            if manager.has_kernel:
                manager.shutdown_kernel(now=True)
            manager.cleanup_resources()
        nbformat.write(notebook, target)
        cells = []
        for index, cell in enumerate(notebook.cells):
            entry = {'index': index, 'id': cell.get('id'), 'type': cell.cell_type,
                     'source': cell.source}
            if cell.cell_type == 'code':
                entry.update(execution_count=cell.execution_count, outputs=cell.outputs)
            cells.append(entry)
        errors = [{'index': c['index'], 'id': c['id'], 'output': o}
                  for c in cells for o in c.get('outputs', []) if o.output_type == 'error']
        result = {'path': str(rel), 'source_sha256': source_hash,
                  'code_cells': sum(c['type'] == 'code' for c in cells),
                  'executed_cells': sum(c.get('execution_count') is not None for c in cells),
                  'seconds': round(time.monotonic() - start, 3),
                  'failure': failure, 'errors': errors, 'cells': cells}
        (target.with_suffix('.json')).write_text(json.dumps(result, indent=2) + '\n')
        summary['notebooks'].append({k: v for k, v in result.items() if k != 'cells'})
        (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
        print(f'{rel}: {result["executed_cells"]}/{result["code_cells"]} cells, '
              f'{len(errors)} errors, {result["seconds"]}s' + (f' {failure}' if failure else ''), flush=True)
    summary['finished_utc'] = datetime.now(timezone.utc).isoformat()
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    return int(any(n['failure'] or n['errors'] or n['executed_cells'] != n['code_cells']
                   for n in summary['notebooks']))


if __name__ == '__main__':
    raise SystemExit(main())
