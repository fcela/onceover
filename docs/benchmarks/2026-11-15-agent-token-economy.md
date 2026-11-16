# Agent token economy: the cost of a first correct call

Issue #295. A fresh automated caller reaching its first correct,
well-aimed `profile` call used to have to read the `profile`
docstring and the `available_detectors()` listing — the only
packaged account of the call surface — and neither was written
for that reader. This change ships the account inside the
package: `onceover.agent_guide()` returns a quickstart written
for a first call (the handover first, then the call and the
budgets, then reading the report, then detector selection), and
`onceover.capabilities()` returns a compact manifest derived
from the detector registry on every call.

## Method

Characters are the measured quantity; tokens are estimated as
ceil(chars / 4) and reported alongside, so the comparison does
not depend on the rule. Serialized sizes are `json.dumps` with
default separators for both routes. Tool calls are the distinct
reads a fresh session makes before the profile call itself,
counted by scripted simulation: on the old route, reading the
docstring and calling `available_detectors()` (2); on the
packaged route, reading the guide (1 — sufficient for a first
correct call), or the guide and the manifest (2 — the manifest
is needed only when selecting detectors).

The before figures were measured on the base build this change
stacks on (the semantics application, report schema 0.8) with an
inline snippet printing the same quantities. The after figures
come from `scripts/measure_first_call_tokens.py`, run against
the build carrying this change; the script also re-measures the
old route's components on that build, where the docstring has
grown by a pointer to the guide (8,946 chars), so the old route
reads 18,669 chars there — the base-build figure below is the
honest before.

## Results

| route | reads before the first call | chars | ~tokens | tool calls |
|---|---|---:|---:|---:|
| docstring + `available_detectors()` | `profile.__doc__` 8,758; detectors JSON 9,723 | 18,481 | 4,621 | 2 |
| `agent_guide()` alone | guide 3,987 | 3,987 | 997 | 1 |
| `agent_guide()` + `capabilities()` | guide 3,987; manifest JSON 7,975 | 11,962 | 2,991 | 2 |

The guide alone cuts the first-call reading cost by 78% and one
tool call. The manifest is 18% smaller than the
`available_detectors()` listing it replaces for this purpose
while also carrying the budgets and the hint keys; it drops the
per-detector category lists, which selection does not use.

For continuity: the figures recorded when the issue was filed
(docstring ≈ 7,185 chars, detector listing ≈ 4,329 chars,
≈ 3,000 tokens for the route) were measured on the pre-semantics
tree. Both components grew with the semantics application —
longer purposes, the renamed vocabulary — which is part of the
problem the packaged guide answers: its size is capped at 4,000
characters by a regression test, so the first-call cost no
longer drifts upward as the surface grows.

The report side is unchanged: a digest on a trivial frame
(3 rows, 2 columns) measures 725 chars under both routes.

## What keeps the numbers honest

- `tests/test_agent_guide.py` fails if the guide exceeds 4,000
  characters, if the guide's named hint keys or budgets diverge
  from the implementation, if its detector counts diverge from
  the registry, or if a registered detector is missing from the
  manifest.
- The manifest is built from the registry on every call; the
  hint keys come from the `HINT_KEYS` constant in the report
  crate, which a Rust test pins to the hints struct's fields.
- The book chapter (`usage/talking_to_the_profiler.ipynb`)
  carries the guide's reference lists verbatim; the same test
  file fails if the chapter's lists or its stored
  `capabilities()` output diverge.

## Reproduce

```sh
python scripts/measure_first_call_tokens.py
```
