"""Token economy features: short handles, compact digests, to_brief(),
compact JSON, duplicate grain/PK suppression, and dirty-key prioritization."""
from __future__ import annotations

import datetime as dt
import json

import polars as pl

import onceover


def _sample_frame() -> pl.DataFrame:
    n = 300
    start = dt.date(2024, 1, 1)
    txn_ids = [f"TXN{i:06d}" for i in range(n)]
    # Plant 3 duplicate transaction IDs so txn_id is a dirty key (near-unique)
    txn_ids[10] = txn_ids[0]
    txn_ids[20] = txn_ids[1]
    txn_ids[30] = txn_ids[2]
    accounts = [f"A{i % 25:03d}" for i in range(n)]
    dates = [start + dt.timedelta(days=i % 30) for i in range(n)]
    regions = ["north", "south", "west"] * (n // 3)
    amount = [20.0 + (i % 15) for i in range(n)]
    amount[5] = 950.0  # global extreme
    return pl.DataFrame(
        {
            "txn_id": txn_ids,
            "account": accounts,
            "date": dates,
            "region": regions,
            "amount": amount,
        }
    )


def test_short_handles_resolve_finding_and_rows():
    df = _sample_frame()
    report = onceover.profile(df)
    assert report.findings, "expected findings on sample frame"
    f1 = report.finding("f1")
    assert f1 is not None
    assert f1["id"] == report.findings[0]["id"]
    assert report.finding("f999999") is None
    if report.grouped_findings:
        g1 = report.finding("g1")
        assert g1 is not None
        assert g1["id"] == report.grouped_findings[0]["id"]

    ex_idx = next(
        (i for i, f in enumerate(report.findings) if f.get("type") == "extreme_value"),
        None,
    )
    assert ex_idx is not None
    ex_rows = report.rows(f"f{ex_idx + 1}", df)
    assert ex_rows.height == 1
    assert ex_rows["amount"][0] == 950.0


def test_to_brief_and_compact_digest_stay_within_budget():
    df = _sample_frame()
    report = onceover.profile(df)
    brief = report.to_brief(2000)
    assert len(brief) <= 2000
    assert "Schema:" in brief
    assert "[f" in brief
    compact = report.to_digest(2000, compact=True)
    assert len(compact) <= 2000
    assert "[f" in compact


def test_dirty_key_routed_to_what_is_wrong():
    df = _sample_frame()
    report = onceover.profile(df)
    digest = report.to_digest(6000)
    wrong_idx = digest.find("What is wrong")
    assert wrong_idx != -1, digest
    next_sec = digest.find("\n\n", wrong_idx)
    wrong_section = digest[wrong_idx:next_sec] if next_sec != -1 else digest[wrong_idx:]
    assert "txn_id" in wrong_section, wrong_section


def test_no_duplicate_grain_and_pk_line():
    n = 200
    df = pl.DataFrame(
        {
            "order_id": [f"O{i:05d}" for i in range(n)],
            "region": ["a", "b", "c", "d"] * (n // 4),
            "amount": [10.0 + (i % 20) for i in range(n)],
        }
    )
    report = onceover.profile(df)
    digest = report.to_digest(6000)
    assert digest.count("uniquely identifies rows") <= 1, digest


def test_compact_json_shrinks_payload_and_adds_handles():
    df = _sample_frame()
    report = onceover.profile(df)
    full_json = report.to_json()
    compact_json = report.to_json(compact=True)
    assert len(compact_json) < len(full_json)
    parsed = json.loads(compact_json)
    assert parsed["schema_version"] == report.data["schema_version"]
    assert len(parsed["findings"]) == len(report.findings)
    assert parsed["findings"][0]["handle"] == "f1"
    assert "interpretation" not in parsed["findings"][0]
    assert "skipped_on_request" in parsed["coverage"]


def test_structured_code_entity_above_500_distinct_classified_as_identifier():
    # Regression: entity columns with >500 structured codes (like account IDs
    # A000..A519) used to fall out of the card's <=500 categorical cap into
    # free_text -> role "text", blinding panel passes to the real entity axis.
    n_entities = 520
    rows_per = 10
    start = dt.date(2024, 1, 1)
    df = pl.DataFrame(
        {
            "account": [f"A{e:03d}" for e in range(n_entities) for _ in range(rows_per)],
            "date": [
                start + dt.timedelta(days=d)
                for _ in range(n_entities)
                for d in range(rows_per)
            ],
            "amount": [
                25.0 + ((e + d) % 7)
                for e in range(n_entities)
                for d in range(rows_per)
            ],
        }
    )
    spec = onceover.infer_spec(df)
    assert spec["columns"]["account"]["role"] == "identifier"
