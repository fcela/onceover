"""Lazy and file inputs for profile(): LazyFrame and path inputs,
source sampling at a declared cap, and the basis of every statement
recorded in the report's `source` block, in coverage, and in each
finding's evidence scope."""
import json
from pathlib import Path

import polars as pl
import pytest

import onceover

SCHEMA = json.loads(
    (Path(__file__).resolve().parent.parent / "docs" / "report-schema.json").read_text()
)


def _frame(n=2_000):
    """Deterministic frame with an exact identity (b = 2a + 1) and an
    exact functional dependency (code -> label)."""
    return pl.DataFrame(
        {
            "a": list(range(n)),
            "b": [2 * i + 1 for i in range(n)],
            "code": [f"c{i % 7}" for i in range(n)],
            "label": [f"label-{i % 7}" for i in range(n)],
            "x": [((i * 37) % 101) / 7.0 for i in range(n)],
        }
    )


def _same_report(a, b):
    """Reports equal except for the `source` block and the `input`
    coverage entry, which only lazy/file inputs carry."""
    a = {k: v for k, v in a.items() if k != "source"}
    b = {k: v for k, v in b.items() if k != "source"}
    a["coverage"] = {k: v for k, v in a["coverage"].items() if k != "input"}
    b["coverage"] = {k: v for k, v in b["coverage"].items() if k != "input"}
    return a == b


def test_dataframe_report_has_no_source_block():
    report = onceover.profile(_frame(300))
    assert "source" not in report.data


def test_lazyframe_full_basis_matches_dataframe():
    df = _frame()
    expected = onceover.profile(df)
    report = onceover.profile(df.lazy())
    assert report.data["source"] == {
        "kind": "lazyframe",
        "rows": df.height,
        "rows_profiled": df.height,
        "basis": "full",
    }
    assert _same_report(report.data, expected.data)
    assert report.data["coverage"]["input"]["status"] == "ran_exact"


def test_parquet_path_full_basis(tmp_path):
    df = _frame()
    path = tmp_path / "frame.parquet"
    df.write_parquet(path)
    expected = onceover.profile(df)
    for given in (str(path), path):
        report = onceover.profile(given)
        assert report.data["source"]["kind"] == "parquet"
        assert report.data["source"]["basis"] == "full"
        assert report.data["source"]["rows"] == df.height
        assert _same_report(report.data, expected.data)


def test_csv_and_ipc_paths(tmp_path):
    df = _frame(500)
    csv_path = tmp_path / "frame.csv"
    ipc_path = tmp_path / "frame.ipc"
    df.write_csv(csv_path)
    df.write_ipc(ipc_path)
    csv_report = onceover.profile(str(csv_path))
    assert csv_report.data["source"]["kind"] == "csv"
    assert csv_report.data["source"]["rows"] == 500
    assert csv_report.findings
    ipc_report = onceover.profile(str(ipc_path))
    assert ipc_report.data["source"]["kind"] == "ipc"
    assert ipc_report.data["source"]["rows"] == 500
    assert ipc_report.findings


def test_sampled_basis_is_disclosed_everywhere():
    df = _frame(5_000)
    report = onceover.profile(df.lazy(), source_row_cap=500)
    data = report.data
    assert data["source"] == {
        "kind": "lazyframe",
        "rows": 5_000,
        "rows_profiled": 500,
        "basis": "sampled",
        "sample": {"method": "stratified", "seed": 0, "cap": 500},
    }
    assert data["coverage"]["input"]["status"] == "ran_sampled"
    assert "5,000" in data["coverage"]["input"]["detail"] or "5000" in data["coverage"]["input"]["detail"]
    assert "5000 source rows" in data["overview"]["headline"]
    # No finding may present sampled numbers as population numbers:
    # every evidence scope names the sample and the source population.
    seen = 0
    for finding in data["findings"] + data.get("grouped_findings", []):
        for evidence in finding["evidence"]:
            scope = evidence["scope"]
            assert scope["sampling"] == "sample", finding["claim"]
            assert scope["population_rows"] == 5_000, finding["claim"]
            seen += 1
    assert seen > 0
    assert "sample" in report.to_digest(4_000)


def test_sampled_profile_is_deterministic():
    df = _frame(5_000)
    a = onceover.profile(df.lazy(), source_row_cap=500, sample_seed=3)
    b = onceover.profile(df.lazy(), source_row_cap=500, sample_seed=3)
    assert a.data == b.data
    c = onceover.profile(df.lazy(), source_row_cap=500, sample_seed=4)
    assert c.data["source"]["sample"]["seed"] == 4


def test_sample_spans_the_whole_source():
    # The first half of the source has mean 0, the second half mean
    # 100: a sample drawn from the head alone would report a card mean
    # near 0, a stratified source sample reports one near 50.
    n = 10_000
    df = pl.DataFrame({"v": [0.0] * (n // 2) + [100.0] * (n // 2)})
    report = onceover.profile(df.lazy(), source_row_cap=200)
    mean = report.data["columns"]["v"]["numeric_stats"]["mean"]
    assert 25.0 < mean < 75.0


def test_exact_collects_the_full_source():
    df = _frame(5_000)
    report = onceover.profile(df.lazy(), source_row_cap=500, exact=True)
    assert report.data["source"]["basis"] == "full"
    assert report.data["source"]["rows_profiled"] == 5_000
    assert _same_report(report.data, onceover.profile(df).data)


def test_csv_schema_uses_the_whole_file(tmp_path):
    # "amount" is integral for the first 150 rows and fractional after:
    # a schema inferred from a head sample alone would fail or mistype
    # the column; the late values must profile as numbers.
    lines = ["amount,label"] + [f"{i},a{i % 3}" for i in range(150)]
    lines += ["12.5,a1", "7.25,a2"]
    path = tmp_path / "late.csv"
    path.write_text("\n".join(lines) + "\n")
    report = onceover.profile(str(path))
    card = report.data["columns"]["amount"]
    assert card["numeric_stats"] is not None
    assert card["numeric_stats"]["max"] == pytest.approx(149.0)


def test_input_errors_are_clear():
    with pytest.raises(TypeError):
        onceover.profile(123)
    with pytest.raises(ValueError, match="unsupported file extension"):
        onceover.profile("data.xlsx")
    with pytest.raises(ValueError, match="source_row_cap"):
        onceover.profile(_frame(100), source_row_cap=10)
    with pytest.raises(ValueError, match="source_row_cap"):
        onceover.profile(_frame(100).lazy(), source_row_cap=0)
    with pytest.raises(Exception):
        onceover.profile("/nonexistent/dir/frame.parquet")


def test_sampled_report_validates_against_the_schema():
    jsonschema = pytest.importorskip("jsonschema")
    df = _frame(5_000)
    report = onceover.profile(df.lazy(), source_row_cap=500)
    validator = jsonschema.Draft202012Validator(SCHEMA)
    errors = sorted(validator.iter_errors(report.data), key=lambda e: list(e.path))
    assert not errors, "\n".join(
        f"{'.'.join(str(p) for p in e.path)}: {e.message}" for e in errors[:10]
    )
