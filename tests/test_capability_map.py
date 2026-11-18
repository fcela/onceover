"""The book's capability map is pinned to the taxonomy: the map in
docs/book/usage/capability_map.ipynb groups every registered
finding type under the question it answers, and its tables are
rendered by scripts/capability_map.py from the installed library.
These tests hold the committed page to a fresh render, so a type
added, removed, or renamed in the taxonomy — or a detector moving
between the default set and the on-request set — fails the suite
instead of leaving the map silently wrong.
"""

import importlib.util
import json
from pathlib import Path

import onceover

ROOT = Path(__file__).resolve().parent.parent
NOTEBOOK = ROOT / "docs" / "book" / "usage" / "capability_map.ipynb"


def _module():
    spec = importlib.util.spec_from_file_location(
        "capability_map", ROOT / "scripts" / "capability_map.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_capability_map_is_fresh():
    module = _module()
    committed = json.loads(NOTEBOOK.read_text(encoding="utf-8"))
    assert module.render(committed) == committed, (
        "docs/book/usage/capability_map.ipynb is stale; regenerate it "
        "with python scripts/capability_map.py"
    )


def test_capability_map_covers_every_registered_type_exactly_once():
    module = _module()
    registered = {pattern["type"] for pattern in onceover.taxonomy()["patterns"]}
    assert set(module.ENTRIES) == registered
    questions = {slug for slug, _heading, _lead in module.QUESTIONS}
    for type_name, entry in module.ENTRIES.items():
        assert entry["question"] in questions, type_name
    notebook = json.loads(NOTEBOOK.read_text(encoding="utf-8"))
    block = None
    for cell in notebook["cells"]:
        source = "".join(cell["source"])
        if module.MARKER_START in source:
            block = source
    assert block is not None
    for type_name in registered:
        assert block.count(f"`{type_name}`") == 1, type_name
