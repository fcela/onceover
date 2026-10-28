"""Long-format (pivot) detection: skinny panels in, wide frames out."""

import polars as pl

import onceover
from _findings import metrics, select


def skinny_panel() -> pl.DataFrame:
    """40 entities x 15 years x 3 indicators, melted to one value column."""
    entities = [f"E{i}" for i in range(40)]
    rows = {"entity": [], "year": [], "region": [], "indicator": [], "value": []}
    for ei, e in enumerate(entities):
        region = f"R{ei % 4}"
        for yi in range(15):
            for ii, ind in enumerate(["gdp_pc", "life_exp", "co2_pc"]):
                rows["entity"].append(e)
                rows["year"].append(2000 + yi)
                rows["region"].append(region)
                rows["indicator"].append(ind)
                # Few enough distinct values to read as a measure, not an
                # identifier: base depends on (region, year, indicator)
                # plus a five-step entity offset.
                rows["value"].append(
                    200.0 * (ei % 4) + 10.0 * yi + 30.0 * ii + 0.5 * (ei % 5)
                )
    return pl.DataFrame(rows)


def test_skinny_panel_reports_pivot_and_recipe_works():
    df = skinny_panel()
    report = onceover.profile(df, budget="standard")
    hits = select(report, "column_roles", form="pivot")
    assert len(hits) == 1
    spec = hits[0]["specification"]["definition"]
    assert spec["attribute"] == "indicator"
    assert spec["measure"] == "value"
    assert spec["index"] == ["entity", "year", "region"]
    m = metrics(hits[0], detector="pivot")
    assert m["n_rows"] == 1800
    assert m["n_index_tuples"] == 600
    assert m["n_levels"] == 3
    assert m["levels"] == ["co2_pc", "gdp_pc", "life_exp"]
    assert m["rectangularity"] == 1.0
    # The reported recipe actually pivots the frame.
    wide = df.pivot(on=spec["attribute"], index=spec["index"], values=spec["measure"])
    assert wide.height == 600
    assert set(wide.columns) == {
        "entity",
        "year",
        "region",
        "gdp_pc",
        "life_exp",
        "co2_pc",
    }


def test_wide_frame_has_no_pivot_finding():
    wide = skinny_panel().pivot(
        on="indicator", index=["entity", "year", "region"], values="value"
    )
    report = onceover.profile(wide, budget="standard")
    assert select(report, "column_roles", form="pivot") == []


def test_pivot_detector_can_be_selected_alone():
    report = onceover.profile(skinny_panel(), detectors=["pivot"])
    hits = select(report, "column_roles", form="pivot")
    assert len(hits) == 1
    assert report.coverage["pivot"]["status"] == "ran_exact"
