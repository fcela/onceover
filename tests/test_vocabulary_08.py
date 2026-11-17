"""The schema-0.8 vocabulary, pinned at the public surface.

The renames this file pins: hint keys `ordered_axis` / `level_order`
(with `axis` / `ordinal` accepted as deprecated aliases for one
release, and every key counting in the hints emptiness check);
detector membership and trigger as two fields; the nine renamed
detectors, with the old names rejected; the card field `card_role`
and its `unique` value; the facet field `support`; the split
dependency types; and the removed examination findings, whose
statements live in coverage (pinned in test_unscanned_series.py and
test_row_clusters_cap.py).
"""

import polars as pl
import pytest

import onceover

RENAMED_DETECTORS = {
    "gbm": "predictability",
    "prism": "covering_rules",
    "changeforest": "distribution_change",
    "views": "row_clusters",
    "content": "content_anomalies",
    "boundaries": "boundary_mass",
    "surprises": "outliers",
    "digits": "digit_fingerprints",
    "dates": "date_gaps",
}


def _dose_frame() -> pl.DataFrame:
    # A non-temporal ordering column (a dose) and a response that
    # steps at dose 20: the ordered-axis battery's subject when the
    # axis is declared.
    return pl.DataFrame(
        {
            "dose": list(range(1, 41)),
            "response": [1.0] * 19 + [5.0] * 21,
            "grade": [["low", "mid", "high"][i % 3] for i in range(40)],
        }
    )


def test_ordered_axis_only_hint_is_not_no_hints():
    report = onceover.profile(_dose_frame(), hints={"ordered_axis": "dose"})
    entry = report.coverage["expectations"]
    assert entry["detail"] != "no hints provided"


def test_level_order_only_hint_is_not_no_hints():
    report = onceover.profile(
        _dose_frame(), hints={"level_order": {"grade": ["low", "mid", "high"]}}
    )
    entry = report.coverage["expectations"]
    assert entry["detail"] != "no hints provided"


def test_weights_only_hint_is_not_no_hints():
    frame = _dose_frame().with_columns(pl.Series("amount", [2.0] * 20 + [9.0] * 20))
    report = onceover.profile(frame, hints={"weights": ["amount"]})
    entry = report.coverage["expectations"]
    assert entry["detail"] != "no hints provided"


def test_deprecated_axis_alias_is_accepted_and_stated():
    report = onceover.profile(_dose_frame(), hints={"axis": "dose"})
    assert report.coverage["expectations"]["detail"] != "no hints provided"
    detail = report.coverage["hints"]["detail"]
    assert "`axis` is now `ordered_axis`" in detail
    assert "accepted for one release" in detail


def test_deprecated_ordinal_alias_is_accepted_and_stated():
    report = onceover.profile(
        _dose_frame(), hints={"ordinal": {"grade": ["low", "mid", "high"]}}
    )
    assert report.coverage["expectations"]["detail"] != "no hints provided"
    detail = report.coverage["hints"]["detail"]
    assert "`ordinal` is now `level_order`" in detail


def test_no_hints_entry_without_aliases():
    report = onceover.profile(_dose_frame(), hints={"ordered_axis": "dose"})
    assert "hints" not in report.coverage


def test_renamed_detectors_resolve_and_old_names_are_rejected():
    registry = {d["id"] for d in onceover.available_detectors()}
    df = _dose_frame()
    for old, new in RENAMED_DETECTORS.items():
        assert new in registry, new
        assert old not in registry, old
        with pytest.raises(ValueError, match="unknown detector"):
            onceover.profile(df, detectors=[old])


def test_membership_and_trigger_are_the_activation_fields():
    for d in onceover.available_detectors():
        assert "activation" not in d, d["id"]
        assert d["membership"] in ("default_set", "on_request"), d
        assert d["trigger"] in ("always", "hints", "target"), d


def test_card_role_field_and_unique_value():
    df = pl.DataFrame(
        {
            "row_id": [float(i) for i in range(200)],
            "amount": [float(i % 7) for i in range(200)],
        }
    )
    report = onceover.profile(df)
    card = report.columns["row_id"]
    assert "role" not in card
    assert card["card_role"] == "unique"


def test_support_facet_filter_and_frame_column():
    df = pl.DataFrame(
        {
            "row_id": list(range(120)),
            "amount": [float(i % 11) for i in range(120)],
        }
    )
    report = onceover.profile(df)
    exact = report.select(type="uniqueness", support="exact")
    assert exact, "the uniqueness statement must carry support=exact"
    assert not report.select(type="uniqueness", support="model_based")
    frame = report.findings_frame()
    assert "support_kind" in frame.columns
    assert "evidence_kind" not in frame.columns


def test_affine_relation_carries_the_affine_type():
    df = pl.DataFrame(
        {
            "x": [float(i) for i in range(120)],
            "y": [2.0 * i + 1.0 for i in range(120)],
        }
    )
    report = onceover.profile(df)
    types = {f["type"] for f in report.findings + report.grouped_findings}
    assert "affine" in types
    for f in report.findings + report.grouped_findings:
        assert f["category"] != "intervals"
        assert f["type"] not in (
            "residual_level",
            "distribution_shift",
            "distribution_change",
            "unexamined_columns",
            "unscanned_series",
        )


def test_schema_version_is_08():
    report = onceover.profile(_dose_frame())
    assert report.data["schema_version"] == "0.8"
