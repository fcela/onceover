from types import SimpleNamespace

from benchmarks import timing_gate


def test_wide_fixture_plants_affine_identity_in_final_columns():
    frame = timing_gate.synthetic(1_000, 80, late_identity=True)

    assert frame.shape == (1_000, 80)
    assert frame.columns[-2:] == ["late_x", "late_affine"]


def test_late_identity_check_matches_affine_finding():
    report = SimpleNamespace(
        findings=[
            {
                "category": "dependencies",
                "specification": {"form": "affine"},
                "subject": {"columns": ["late_x", "late_affine"]},
            }
        ]
    )

    assert timing_gate.late_identity_recovered(report)

    report.findings[0]["subject"]["columns"] = ["num_0", "num_1"]
    assert not timing_gate.late_identity_recovered(report)
    report.findings = []
    assert not timing_gate.late_identity_recovered(report)


def test_timed_passes_reports_missing_and_invalid_elapsed_without_keyerror():
    coverage = {
        "valid": {"status": "ran_exact", "elapsed_ms": 12.5},
        "missing": {"status": "ran_sampled"},
        "string": {"status": "ran_exact", "elapsed_ms": "12"},
        "nan": {"status": "ran_exact", "elapsed_ms": float("nan")},
        "infinite": {"status": "ran_exact", "elapsed_ms": float("inf")},
        "negative": {"status": "ran_exact", "elapsed_ms": -1},
        "boolean": {"status": "ran_exact", "elapsed_ms": True},
        "skipped": {"status": "skipped", "detail": "not enabled"},
    }

    valid, invalid = timing_gate.timed_passes(coverage)

    assert valid == [("valid", 12.5)]
    assert invalid == ["missing", "string", "nan", "infinite", "negative", "boolean"]
