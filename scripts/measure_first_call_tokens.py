#!/usr/bin/env python3
"""Measure what a fresh caller reads before its first profile call.

Two routes to a first correct, well-aimed `profile` call:

- the pre-guide route: the `profile` docstring plus the
  `available_detectors()` listing;
- the packaged route: `agent_guide()` plus `capabilities()`.

Tokens are estimated as ceil(chars / 4); the raw character counts
are printed alongside, so the numbers do not depend on the rule.
Tool calls are the distinct reads a fresh session makes before
the profile call itself: the docstring read and the detector
listing on the old route (2); the guide on the new route (1 —
the manifest is needed only when selecting detectors; reading it
as well makes 2).

Measured on the current build: the docstring now carries a short
pointer to the guide, so the old route's figure slightly
understates its pre-guide cost.

Run against a built install:

    python scripts/measure_first_call_tokens.py
"""

import json
import math

import polars as pl

import onceover


def tokens(chars):
    return math.ceil(chars / 4)


def show(label, text):
    print(f"{label}: {len(text)} chars, ~{tokens(len(text))} tokens")
    return len(text)


def main():
    docstring = onceover.profile.__doc__
    detectors = json.dumps(onceover.available_detectors())
    guide = onceover.agent_guide()
    manifest = json.dumps(onceover.capabilities())

    print("old route (before the packaged guide):")
    before = show("  profile docstring", docstring)
    before += show("  available_detectors() as JSON", detectors)
    print(f"  total: {before} chars, ~{tokens(before)} tokens, 2 tool calls")

    print("packaged route:")
    after = show("  agent_guide()", guide)
    after += show("  capabilities() as JSON", manifest)
    print(f"  total: {after} chars, ~{tokens(after)} tokens, 2 tool calls")
    print(
        f"  guide alone: {len(guide)} chars, ~{tokens(len(guide))} tokens, "
        "1 tool call (sufficient for a first correct call)"
    )

    frame = pl.DataFrame({"a": [1, 2, 3], "b": ["x", "y", "x"]})
    digest = onceover.profile(frame).to_digest()
    show("digest on a trivial frame (unchanged by the guide)", digest)


if __name__ == "__main__":
    main()
