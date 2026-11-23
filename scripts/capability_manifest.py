#!/usr/bin/env python3
"""Generate docs/capability-manifest.json from the library itself.

The manifest lists every detector the installed build exposes, the
finding categories each produces, and its requirements — so agent
consumers read capabilities from a generated file, not from prose
that drifts. Run after any detector change:

    python scripts/capability_manifest.py
    python scripts/capability_manifest.py --check  # compare with the installed wheel
"""
import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
try:
    import onceover  # noqa: E402  the installed package, with its built extension
except ImportError:
    # Fall back to the source tree, which imports only after an
    # in-place build (maturin develop) has put the extension there.
    sys.path.insert(0, str(ROOT / "python"))
    import onceover


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if the committed manifest is stale")
    args = parser.parse_args()
    manifest = {
        "library": "onceover",
        "version": onceover.__version__,
        "generated_by": "scripts/capability_manifest.py",
        "detectors": onceover.available_detectors(),
    }
    out = ROOT / "docs" / "capability-manifest.json"
    if args.check:
        if json.loads(out.read_text(encoding="utf-8")) != manifest:
            parser.exit(
                1, "Capability manifest differs from the installed package. "
                "Regenerate it with python scripts/capability_manifest.py.\n"
            )
        print(f"{out} matches the installed package")
        return
    out.write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
    print(f"wrote {out} with {len(manifest['detectors'])} detectors")


if __name__ == "__main__":
    main()
