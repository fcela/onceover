"""Findings output: evidence beside each claim in Markdown, per-claim
result tables, and the findings ledger as a Polars DataFrame."""
import polars as pl

import onceover


def orders():
    names = ["north", "south", "east", "west"]
    rows = [
        {
            "region_id": i % 4,
            "region": names[i % 4],
            "amount": float((i * 37) % 91),
        }
        for i in range(400)
    ]
    return pl.DataFrame(rows)


def test_markdown_places_evidence_beside_the_claim():
    md = onceover.profile(orders(), budget="standard").to_markdown()
    claim_at = md.find("region_id → region")
    assert claim_at != -1, md[:2000]
    evidence_at = md.find("- evidence: functional_dependencies", claim_at)
    assert evidence_at != -1, md[claim_at : claim_at + 2000]
    block = md[claim_at:evidence_at] + md[evidence_at : evidence_at + 600]
    assert "violations=0" in block


def test_findings_frame_matches_the_ledger():
    report = onceover.profile(orders(), budget="standard")
    frame = report.findings_frame()
    assert frame.height == len(report.findings)
    for col in (
        "id",
        "category",
        "type",
        "columns",
        "inputs",
        "target",
        "interestingness",
        "claim",
        "n_evidence",
        "basis",
    ):
        assert col in frame.columns
    scores = frame["interestingness"].to_list()
    assert scores == sorted(scores, reverse=True)
    fd = frame.filter(
        (pl.col("category") == "dependencies") & (pl.col("target") == "region")
    )
    assert fd.height >= 1
    row = fd.row(0, named=True)
    assert row["inputs"] == ["region_id"]
    assert row["n_evidence"] >= 1
