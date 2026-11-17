#!/usr/bin/env python3
"""Generate the registry-owned blocks of docs/findings.md.

docs/findings.md mixes hand-maintained prose with facts the
detector registry and the finding taxonomy own: which detectors
exist and how they activate, which finding types exist under
which category, the facets each type carries, and the registered
form vocabulary. Those blocks sit between <!-- generated:NAME -->
markers and are rendered here from the installed library
(`onceover.available_detectors()` and `onceover.taxonomy()`), so
they cannot drift from the registry the way hand-copied tables
did. Everything outside the markers is hand-maintained prose and
is preserved byte for byte.

Run after any detector, taxonomy, or form change:

    python scripts/findings_doc.py

With --check, nothing is written and the exit status is non-zero
when the committed file differs from a fresh render; the
freshness test in tests/test_registration.py runs the same
comparison.
"""
import re
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

MARKER = re.compile(
    r"<!-- generated:([a-z0-9-]+) -->\n(?:(.*?)\n)?<!-- /generated:\1 -->",
    re.DOTALL,
)

# The facet fields, in the order the document presents them.
FIELDS = [
    "scope",
    "object",
    "support",
    "conditionality",
    "temporality",
    "quality_dimension",
    "status",
]

# Presentation prose for the facet-fields table: a suffix for the
# field's cell, glosses for individual values, and the fields whose
# values join with "or" instead of commas. The values themselves
# come from the taxonomy; every value of a glossed field needs an
# entry here (an empty string is a deliberate no-gloss decision),
# so a new facet value fails the render until its documentation
# is decided.
FIELD_SUFFIX = {
    "scope": " — what the statement ranges over",
    "object": " — what kind of statement it is",
    "quality_dimension": ", present only on data-quality findings",
}
VALUE_GLOSS = {
    ("support", "exact"): " (no exceptions in the evaluated rows)",
    ("support", "approximate"): " (a measured rate or a tolerance)",
    ("support", "model_based"): " (a fitted model)",
    ("support", "inferred"): " (an interpretation)",
    ("conditionality", "unconditional"): "",
    ("conditionality", "value_conditioned"): " (a condition on column values)",
    ("conditionality", "segment_conditioned"): " (different behaviour in different segments)",
    ("temporality", "atemporal"): "",
    ("temporality", "ordered"): " (about an ordering)",
    ("temporality", "change_across_segments"): " (compares segments of an ordering)",
    ("status", "regularity"): " (a structure that holds)",
    ("status", "deviation"): " (a departure from an expectation)",
}
GLOSSED_FIELDS = {"support", "conditionality", "temporality", "status"}
OR_JOIN_FIELDS = {"quality_dimension", "status"}

# One deliberate preservation: an order violation is approximate
# by construction — the statement exists only where an order
# fails, so its support never reads "exact". The taxonomy's
# canonical evaluation (an empty metrics map, the convention
# `types_for` uses) reads exact for the pattern because support
# keys on the violations metric; the varied evaluation the
# taxonomy also exports shows the approximate reading every
# emitted statement of the type carries.
SUPPORT_OVERRIDES = {"order_violation": "approximate"}

# The violations merge in a type's support cell, phrased with the
# noun the document has always used for what carries the
# violations.
SUPPORT_PHRASES = {
    "functional": "exact, or approximate when the mapping has violations",
    "if_then": "exact, or approximate when the rule has violations",
    "order_constraint": "exact, or approximate when the order has violations",
}


def _bt(value):
    return f"`{value}`"


def _or_join(parts):
    if len(parts) == 1:
        return parts[0]
    if len(parts) == 2:
        return f"{parts[0]} or {parts[1]}"
    return ", ".join(parts[:-1]) + ", or " + parts[-1]


def _join(values, field=None):
    parts = [_bt(v) for v in values]
    if field in OR_JOIN_FIELDS:
        return _or_join(parts)
    return ", ".join(parts)


def render_taxonomy_index(taxonomy, _detectors):
    categories = []
    types_by_category = {}
    for pattern in taxonomy["patterns"]:
        category = pattern["category"]
        if category not in types_by_category:
            categories.append(category)
            types_by_category[category] = []
        if pattern["type"] not in types_by_category[category]:
            types_by_category[category].append(pattern["type"])
    lines = ["| Category | Types |", "|---|---|"]
    for category in categories:
        types = ", ".join(_bt(t) for t in types_by_category[category])
        lines.append(f"| {_bt(category)} | {types} |")
    return "\n".join(lines)


def render_forms(taxonomy, _detectors):
    lines = ["| Form | Type | Definition |", "|---|---|---|"]
    for form in taxonomy["forms"]:
        lines.append(f"| {_bt(form['form'])} | {_bt(form['type'])} | {form['definition']} |")
    return "\n".join(lines)


def render_facet_fields(taxonomy, _detectors):
    facet_values = taxonomy["facet_values"]
    if set(facet_values) != set(FIELDS):
        raise ValueError(
            f"facet fields changed: taxonomy has {sorted(facet_values)}, "
            f"the document presents {FIELDS}"
        )
    lines = ["| Field | Values |", "|---|---|"]
    for field in FIELDS:
        rendered = []
        for value in facet_values[field]:
            if field in GLOSSED_FIELDS and (field, value) not in VALUE_GLOSS:
                raise ValueError(f"no gloss decision for facet value {field}={value}")
            gloss = VALUE_GLOSS.get((field, value), "")
            rendered.append(_bt(value) + gloss)
        if field in OR_JOIN_FIELDS:
            cell = _or_join(rendered)
        else:
            cell = ", ".join(rendered)
        cell += FIELD_SUFFIX.get(field, "")
        lines.append(f"| {_bt(field)} | {cell} |")
    return "\n".join(lines)


def _merged_type_rows(taxonomy):
    """One rendered facet row per finding type, in taxonomy order.

    Canonical facets come from the taxonomy's canonical evaluation.
    The varied evaluations the taxonomy exports beside them (a
    violations metric present, a one-column subject, a conditioned
    specification) are merged with the phrasing the document has
    always used for them.
    """
    facet_values = taxonomy["facet_values"]
    by_type = {}
    order = []
    for pattern in taxonomy["patterns"]:
        if pattern["type"] not in by_type:
            by_type[pattern["type"]] = []
            order.append(pattern["type"])
        by_type[pattern["type"]].append(pattern)

    def field_order(field, values):
        return sorted(values, key=lambda v: facet_values[field].index(v))

    rows = {}
    for type_name in order:
        patterns = by_type[type_name]
        facets = [p["facets"] for p in patterns]
        row = {}
        # Scope: a one-column subject can narrow it (uniqueness).
        scopes = {f["scope"] for f in facets}
        for p in patterns:
            if "scope_one_column" in p:
                scopes.add(p["scope_one_column"])
        row["scope"] = " or ".join(_bt(v) for v in field_order("scope", scopes))
        row["object"] = _join(field_order("object", {f["object"] for f in facets}))
        # Support: a statement carrying violations reads approximate.
        if any("support_with_violations" in p for p in patterns):
            row["support"] = SUPPORT_PHRASES.get(
                type_name, "exact, or approximate when the statement has violations"
            )
        else:
            row["support"] = _join(field_order("support", {f["support"] for f in facets}))
        if type_name in SUPPORT_OVERRIDES:
            row["support"] = SUPPORT_OVERRIDES[type_name]
        # Conditionality: distinct canonical values across the type's
        # patterns (the predictive type's piecewise form), then a
        # conditioned specification's alternate reading.
        conditionality = []
        for p, f in zip(patterns, facets):
            if f["conditionality"] not in conditionality:
                conditionality.append((f["conditionality"], p["pattern"]))
        cell = _bt(conditionality[0][0])
        for value, pattern_name in conditionality[1:]:
            qualifier = pattern_name.removesuffix("Dependency").lower()
            cell += f"; {_bt(value)} in the {qualifier} form"
        for p in patterns:
            if "conditionality_conditioned" in p:
                cell += f"; {_bt(p['conditionality_conditioned'])} within a categorical subset"
        row["conditionality"] = cell
        row["temporality"] = _join(field_order("temporality", {f["temporality"] for f in facets}))
        quality = {f["quality_dimension"] for f in facets if "quality_dimension" in f}
        row["quality"] = _join(field_order("quality_dimension", quality)) or "—"
        row["status"] = _join(field_order("status", {f["status"] for f in facets}))
        rows[type_name] = row
    return order, rows


def render_facets_by_type(taxonomy, _detectors):
    order, rows = _merged_type_rows(taxonomy)
    grouped = []
    by_row = {}
    for type_name in order:
        row = rows[type_name]
        key = tuple(row[f] for f in ("scope", "object", "support", "conditionality", "temporality", "quality", "status"))
        if key not in by_row:
            by_row[key] = []
            grouped.append((key, by_row[key]))
        by_row[key].append(type_name)
    lines = [
        "| Type | Scope | Object | Support | Conditionality | Temporality | Quality | Status |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for key, types in grouped:
        type_cell = ", ".join(_bt(t) for t in types)
        lines.append(f"| {type_cell} | " + " | ".join(key) + " |")
    return "\n".join(lines)


def render_detectors(_taxonomy, detectors):
    default = [d for d in detectors if d["membership"] == "default_set"]
    on_request = len(detectors) - len(default)
    lines = [
        f"The default set has {len(default)} of the {len(detectors)} registered "
        f"detectors; the other {on_request} run only when named in `detectors`.",
        "",
        "| Detector | Categories | Membership | Trigger | Requires | Purpose |",
        "|---|---|---|---|---|---|",
    ]
    for d in detectors:
        for text in [d["id"], d["purpose"], *d["categories"], *d["requires"]]:
            if "|" in text:
                raise ValueError(f"detector {d['id']}: '|' in a rendered field")
        categories = ", ".join(_bt(c) for c in d["categories"])
        requires = ", ".join(_bt(r) for r in d["requires"]) or "—"
        lines.append(
            f"| {_bt(d['id'])} | {categories} | {_bt(d['membership'])} "
            f"| {_bt(d['trigger'])} | {requires} | {d['purpose']} |"
        )
    return "\n".join(lines)


RENDERERS = {
    "taxonomy-index": render_taxonomy_index,
    "forms": render_forms,
    "facet-fields": render_facet_fields,
    "facets-by-type": render_facets_by_type,
    "detectors": render_detectors,
}


def render(text, taxonomy=None, detectors=None):
    if taxonomy is None:
        taxonomy = onceover.taxonomy()
    if detectors is None:
        detectors = onceover.available_detectors()
    found = set()

    def substitute(match):
        name = match.group(1)
        if name not in RENDERERS:
            raise ValueError(f"unknown generated block: {name}")
        found.add(name)
        body = RENDERERS[name](taxonomy, detectors)
        return f"<!-- generated:{name} -->\n{body}\n<!-- /generated:{name} -->"

    rendered = MARKER.sub(substitute, text)
    missing = sorted(set(RENDERERS) - found)
    if missing:
        raise ValueError(f"generated blocks missing from the document: {missing}")
    return rendered


def main():
    path = ROOT / "docs" / "findings.md"
    committed = path.read_text(encoding="utf-8")
    rendered = render(committed)
    if "--check" in sys.argv:
        if rendered != committed:
            print("docs/findings.md is stale; regenerate it with python scripts/findings_doc.py")
            return 1
        print("docs/findings.md is fresh")
        return 0
    path.write_text(rendered, encoding="utf-8")
    print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
