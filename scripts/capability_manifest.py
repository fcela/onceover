#!/usr/bin/env python3
"""Generate docs/capability-manifest.json from the library itself.

The manifest lists every detector the installed build exposes, the
finding categories each produces, and its requirements — so agent
consumers read capabilities from a generated file, not from prose
that drifts. Run after any detector change:

    python scripts/capability_manifest.py
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
sys.path.insert(0, str(ROOT / "python"))
import onceover  # noqa: E402


def main() -> None:
    manifest = {
        "library": "onceover",
        "version": onceover.__version__,
        "generated_by": "scripts/capability_manifest.py",
        "detectors": onceover.available_detectors(),
    }
    out = ROOT / "docs" / "capability-manifest.json"
    out.write_text(json.dumps(manifest, indent=1) + "\n")
    print(f"wrote {out} with {len(manifest['detectors'])} detectors")


if __name__ == "__main__":
    main()
