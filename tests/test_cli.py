"""The `onceover` console script: one file in, a rendering out;
several files in, a multi-table report. The CLI is a thin shell
over `profile` / `profile_many` — the JSON it prints is the API's
report, unchanged.
"""

import json

import polars as pl
import pytest

import onceover
from onceover._cli import main


def orders_frame():
    names = ["north", "south", "east", "west"]
    return pl.DataFrame(
        [
            {
                "region_id": i % 4,
                "region": names[i % 4],
                "amount": float((i * 37) % 91),
            }
            for i in range(200)
        ]
    )


def customers_frame():
    return pl.DataFrame(
        {
            "customer_id": list(range(1, 21)),
            "name": [f"customer {i}" for i in range(1, 21)],
        }
    )


def orders_with_customers():
    return pl.DataFrame(
        {
            "order_id": list(range(1, 61)),
            "customer_id": [(i % 20) + 1 for i in range(60)],
            "amount": [float((i * 29) % 83) for i in range(60)],
        }
    )


def test_cli_digest_is_the_default_rendering(tmp_path, capsys):
    csv = tmp_path / "orders.csv"
    orders_frame().write_csv(csv)
    assert main([str(csv)]) == 0
    out = capsys.readouterr().out
    assert out.startswith("profile digest: 200 rows x 3 columns")
    assert "schema 0.8" in out


def test_cli_digest_budget(tmp_path, capsys):
    csv = tmp_path / "orders.csv"
    orders_frame().write_csv(csv)
    assert main([str(csv), "--digest", "600"]) == 0
    out = capsys.readouterr().out
    assert out.startswith("profile digest:")
    # The digest itself fits the budget; stdout adds a trailing newline.
    assert len(out.rstrip("\n")) <= 600


def test_cli_json_is_the_api_report(tmp_path, capsys):
    parquet = tmp_path / "orders.parquet"
    orders_frame().write_parquet(parquet)
    assert main([str(parquet), "--format", "json"]) == 0
    printed = json.loads(capsys.readouterr().out)
    api = onceover.profile(str(parquet)).data
    assert printed["schema_version"] == api["schema_version"]
    assert printed["overview"] == api["overview"]
    assert [f["claim"] for f in printed["findings"]] == [
        f["claim"] for f in api["findings"]
    ]
    assert printed["columns"] == api["columns"]


def test_cli_output_file(tmp_path, capsys):
    csv = tmp_path / "orders.csv"
    orders_frame().write_csv(csv)
    out = tmp_path / "digest.txt"
    assert main([str(csv), "-o", str(out)]) == 0
    assert capsys.readouterr().out == ""
    assert out.read_text(encoding="utf-8").startswith("profile digest:")


def test_cli_missing_file(tmp_path, capsys):
    missing = tmp_path / "nope.parquet"
    assert main([str(missing)]) == 2
    assert "no such file" in capsys.readouterr().err


def test_cli_multiple_files_digest(tmp_path, capsys):
    customers = tmp_path / "customers.csv"
    orders = tmp_path / "orders.csv"
    customers_frame().write_csv(customers)
    orders_with_customers().write_csv(orders)
    assert main([str(orders), str(customers)]) == 0
    out = capsys.readouterr().out
    assert out.startswith("Multi-table report: 2 tables")
    assert "orders.customer_id -> customers.customer_id" in out


def test_cli_multiple_files_json(tmp_path, capsys):
    customers = tmp_path / "customers.parquet"
    orders = tmp_path / "orders.parquet"
    customers_frame().write_parquet(customers)
    orders_with_customers().write_parquet(orders)
    assert main([str(orders), str(customers), "--format", "json"]) == 0
    printed = json.loads(capsys.readouterr().out)
    strong = [c for c in printed["candidates"] if c["tier"] == "strong"]
    assert any(
        c["child_table"] == "orders"
        and c["child_column"] == "customer_id"
        and c["parent_table"] == "customers"
        and c["parent_column"] == "customer_id"
        for c in strong
    )


def test_cli_multiple_files_reject_single_table_formats(tmp_path, capsys):
    customers = tmp_path / "customers.csv"
    orders = tmp_path / "orders.csv"
    customers_frame().write_csv(customers)
    orders_with_customers().write_csv(orders)
    assert main([str(orders), str(customers), "--format", "markdown"]) == 1
    assert "digest or json" in capsys.readouterr().err


def test_cli_duplicate_table_names(tmp_path, capsys):
    first = tmp_path / "a" / "orders.csv"
    second = tmp_path / "b" / "orders.csv"
    first.parent.mkdir()
    second.parent.mkdir()
    orders_frame().write_csv(first)
    orders_frame().write_csv(second)
    assert main([str(first), str(second)]) == 1
    assert "same table name" in capsys.readouterr().err


def test_cli_version(capsys):
    with pytest.raises(SystemExit) as excinfo:
        main(["--version"])
    assert excinfo.value.code == 0
    assert onceover.__version__ in capsys.readouterr().out


def test_cli_brief_and_compact_json(tmp_path, capsys):
    csv = tmp_path / "orders.csv"
    orders_frame().write_csv(csv)
    assert main([str(csv), "--format", "brief", "--digest", "1500"]) == 0
    brief = capsys.readouterr().out
    assert "Schema:" in brief
    assert "[f" in brief
    assert len(brief.rstrip("\n")) <= 1500

    assert main([str(csv), "--format", "json", "--compact"]) == 0
    compact_doc = json.loads(capsys.readouterr().out)
    assert compact_doc["findings"][0]["handle"] == "f1"
    assert "skipped_on_request" in compact_doc["coverage"]


def test_cli_csv_infers_the_whole_file(tmp_path, capsys):
    csv = tmp_path / "late_decimal.csv"
    values = [str(i) for i in range(100)]
    values += ["100.5"]
    values += [str(i) for i in range(101, 300)]
    csv.write_text("id\n" + "\n".join(values) + "\n", encoding="utf-8")
    # Polars' 100-row inference window types this as Int64 and then
    # fails on the late decimal; the CLI reads the whole column.
    # UInt64 overflow is left to Polars: 1.28 cannot infer it at all.
    assert main([str(csv), "--format", "json"]) == 0
    report = json.loads(capsys.readouterr().out)
    assert report["overview"]["n_rows"] == 300


@pytest.mark.parametrize("flag", [
    ["--target", "v"],
    ["--budget", "quick"],
    ["--deadline-seconds", "1"],
    ["--source-row-cap", "10"],
    ["--exact"],
    ["--sample-seed", "7"],
])
def test_cli_multiple_files_reject_single_table_flags(tmp_path, capsys, flag):
    first = tmp_path / "first.csv"
    second = tmp_path / "second.csv"
    orders_frame().write_csv(first)
    orders_frame().write_csv(second)
    assert main([str(first), str(second), *flag]) == 1
    assert flag[0] in capsys.readouterr().err
