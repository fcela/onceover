import random

import polars as pl
import pytest

import onceover


def opposites():
    rng = random.Random(7)
    rows = []
    for protocol, sign in [("A", 1), ("B", -1)]:
        for _ in range(200):
            x = rng.uniform(-1, 1)
            rows.append((x, sign * x + rng.gauss(0, 0.03), protocol))
    return pl.DataFrame(rows, schema=["x", "y", "protocol"], orient="row")


@pytest.mark.parametrize("dtype", [pl.String, pl.Categorical, pl.Enum(["A", "B"])])
def test_public_call_discovers_a_split(dtype):
    df = opposites().with_columns(pl.col("protocol").cast(dtype))
    result = onceover.partition_scagnostics(
        df, pairs=[("x", "y")], partition_by=["protocol"], metrics=["monotonic"],
    )
    [finding] = result["findings"]
    assert finding["rule"] == {"kind": "levels", "column": "protocol", "levels": ["A"]}
    assert abs(finding["validation"]["pooled"]["rho"]) < 0.2
    first, second = finding["validation"]["groups"]
    assert first["shape"]["rho"] > 0.98 and second["shape"]["rho"] < -0.98
    assert result["coverage"]["validation"]["tested"] == 1


def test_default_inference_and_sampling_coverage():
    df = pl.concat([opposites()] * 15)
    result = onceover.partition_scagnostics(df)
    assert any(hit["metric"] == "monotonic" for hit in result["findings"])
    coverage = result["coverage"]
    assert coverage["input_rows"] == 6000
    assert coverage["sampled_rows"] == 4096
    assert coverage["examined_pairs"] == 1
    assert coverage["capped"] is True


def test_profile_registers_partition_search_as_opt_in():
    df = opposites()
    default = onceover.profile(df)
    assert default.coverage["partition_scagnostics"]["status"] == "skipped"
    assert not default.select(type="partitioned_shape")

    opt_in = onceover.profile(df, detectors=["partition_scagnostics"])
    assert opt_in.coverage["partition_scagnostics"]["status"] == "ran_exact"
    assert opt_in.select(type="partitioned_shape")
    for finding in opt_in.select(type="partitioned_shape"):
        evidence = finding["evidence"][0]
        assert evidence["method"]["detector"] == "partition_scagnostics"
        assert evidence["metrics"]["partition_scagnostics"] in onceover.partition_scagnostics(df)["findings"]

    deadline = onceover.profile(df, detectors=["partition_scagnostics"], deadline_seconds=0.0)
    assert deadline.coverage["partition_scagnostics"]["detail"] == "deadline reached"
    assert not deadline.select(type="partitioned_shape")


def test_numeric_threshold_and_complete_case_counts():
    df = opposites().with_columns(
        pl.when(pl.col("protocol") == "A").then(0).otherwise(1).alias("setting"),
    ).with_row_index().with_columns(
        pl.when(pl.col("index") == 3).then(None)
        .when(pl.col("index") == 17).then(float("nan"))
        .otherwise(pl.col("x")).alias("x"),
    )
    result = onceover.partition_scagnostics(
        df, pairs=[("x", "y")], partition_by=["setting"], metrics=["monotonic"],
    )
    [finding] = result["findings"]
    assert finding["rule"] == {"kind": "threshold", "column": "setting", "value": 0.0}
    assert sum(
        group["rows"] for half in ("discovery", "validation") for group in finding[half]["groups"]
    ) == df.height - 2


@pytest.mark.parametrize(
    ("options", "error"),
    [
        ({"pairs": [("x", "protocol")]}, "numeric measures"),
        ({"pairs": [("x", "y"), ("y", "x")]}, "pairs must be distinct"),
        ({"partition_by": ["missing"]}, "unknown partition column"),
        ({"metrics": ["unknown"]}, "unknown metric"),
        ({"metrics": []}, "distinct metrics"),
    ],
)
def test_validation_errors(options, error):
    with pytest.raises(ValueError, match=error):
        onceover.partition_scagnostics(opposites(), **options)


def test_binned_stringy_detects_hidden_curved_arc():
    rng = random.Random(11)
    rows = []
    for _ in range(400):
        x = rng.uniform(-1, 1)
        rows.append((x, 2.0 * x * x - 1.0 + rng.gauss(0, 0.02), "arc"))
    for _ in range(400):
        rows.append((rng.uniform(-1, 1), rng.uniform(-1, 1), "cloud"))
    df = pl.DataFrame(rows, schema=["x", "y", "mode"], orient="row")

    mono = onceover.partition_scagnostics(
        df, pairs=[("x", "y")], partition_by=["mode"], metrics=["monotonic"],
    )
    assert not mono["findings"]

    stringy = onceover.partition_scagnostics(
        df, pairs=[("x", "y")], partition_by=["mode"], metrics=["binned_stringy"],
    )
    [finding] = stringy["findings"]
    assert finding["metric"] == "binned_stringy"
    assert finding["rule"] == {"kind": "levels", "column": "mode", "levels": ["arc"]}
    assert finding["validation"]["groups"][0]["label"] == "arc"
    assert finding["validation"]["groups"][0]["shape"]["score"] > 0.7


def test_binned_clumpy_detects_hidden_clusters():
    rng = random.Random(19)
    centers = [(-0.75, -0.75), (-0.75, 0.75), (0.75, -0.75), (0.75, 0.75)]
    rows = []
    for i in range(400):
        cx, cy = centers[i % 4]
        rows.append((cx + rng.uniform(-0.07, 0.07), cy + rng.uniform(-0.07, 0.07), "clustered"))
    for _ in range(400):
        rows.append((rng.uniform(-1, 1), rng.uniform(-1, 1), "diffuse"))
    df = pl.DataFrame(rows, schema=["x", "y", "regime"], orient="row")

    res = onceover.partition_scagnostics(
        df, pairs=[("x", "y")], partition_by=["regime"], metrics=["binned_clumpy"],
    )
    [finding] = res["findings"]
    assert finding["metric"] == "binned_clumpy"
    assert finding["rule"] == {"kind": "levels", "column": "regime", "levels": ["clustered"]}
    assert finding["validation"]["groups"][0]["shape"]["score"] > 0.5


def test_high_cardinality_standout_subgroup_and_rows_extraction():
    rng = random.Random(23)
    rows = []
    for i in range(1000):
        site = f"site_{i % 10}"
        x = rng.uniform(-1, 1)
        y = x + rng.gauss(0, 0.03) if site == "site_0" else rng.uniform(-1, 1)
        rows.append((x, y, site))
    df = pl.DataFrame(rows, schema=["x", "y", "site"], orient="row")

    rep = onceover.profile(df, detectors=["partition_scagnostics"])
    hits = rep.select(type="partitioned_shape")
    assert len(hits) == 1
    hit = hits[0]
    assert hit["specification"]["definition"]["objective"] == "standout_subgroup"
    assert "site_0" in hit["claim"]
    sub_rows = rep.rows(hit["id"], df)
    assert sub_rows.height == 100
    assert set(sub_rows["site"].to_list()) == {"site_0"}
    assert "__partition_group" in sub_rows.columns


def test_range_restriction_guard_suppresses_sliced_parabola():
    rng = random.Random(31)
    rows = []
    for _ in range(600):
        x = rng.uniform(-1, 1)
        y = x * x + rng.gauss(0, 0.02)
        proxy = x + rng.gauss(0, 0.05)
        rows.append((x, y, proxy))
    df = pl.DataFrame(rows, schema=["x", "y", "proxy"], orient="row")

    res = onceover.partition_scagnostics(
        df, pairs=[("x", "y")], partition_by=["proxy"],
    )
    assert not res["findings"]
    assert res["coverage"]["validation"]["range_restriction_filtered"] >= 1


def test_flat_axis_collapse_does_not_trigger_geometric_metrics():
    rng = random.Random(37)
    rows = []
    for i in range(600):
        if i % 2 == 0:
            # Constant x=0 collapses cloud onto a vertical line in group "zero".
            rows.append((0.0, rng.uniform(-1, 1), "zero"))
        else:
            rows.append((rng.uniform(0.1, 1.0), rng.uniform(-1, 1), "spread"))
    df = pl.DataFrame(rows, schema=["x", "y", "flag"], orient="row")
    res = onceover.partition_scagnostics(
        df,
        pairs=[("x", "y")],
        partition_by=["flag"],
        metrics=["binned_striated", "binned_stringy", "binned_clumpy"],
    )
    assert not res["findings"]


def test_profile_pass_excludes_row_numbers_and_is_layout_invariant():
    rng = random.Random(41)
    rows = []
    for i in range(500):
        g = "A" if i % 2 == 0 else "B"
        bucket = f"b{i % 5}"
        x = round(rng.uniform(-1, 1), 2)
        y = round((x if g == "A" else -x) + rng.gauss(0, 0.03), 2)
        rows.append((i, x, y, g, bucket))
    df = pl.DataFrame(rows, schema=["row_no", "x", "y", "protocol", "bucket"], orient="row")
    df_alt = df.sample(fraction=1.0, shuffle=True, seed=13).select(list(reversed(df.columns)))

    r1 = onceover.profile(df, detectors=["partition_scagnostics"])
    r2 = onceover.profile(df_alt, detectors=["partition_scagnostics"])
    h1 = r1.select(type="partitioned_shape")
    h2 = r2.select(type="partitioned_shape")
    assert len(h1) == 1 and len(h2) == 1
    assert h1[0]["id"] == h2[0]["id"]
    assert h1[0]["claim"] == h2[0]["claim"]
    assert all("row_no" not in h["subject"]["columns"] for h in h1)
    all_rows = r1.rows(h1[0]["id"], df)
    assert all_rows.height == df.height
    assert set(all_rows["__partition_group"].to_list()) == {"A", "B"}


def test_functional_detects_multipeak_curve_regimes():
    import math

    rng = random.Random(53)
    rows = []
    for _ in range(500):
        h = rng.uniform(0, 24)
        # Workday twin peaks (8am & 5pm) vs weekend unimodal midday peak (1pm).
        w_val = 200 * math.exp(-((h - 8) ** 2) / 6) + 220 * math.exp(-((h - 17) ** 2) / 6) + rng.gauss(0, 25)
        rows.append((h, w_val, "workday"))
        u_val = 210 * math.exp(-((h - 13) ** 2) / 18) + rng.gauss(0, 25)
        rows.append((h, u_val, "weekend"))
    df = pl.DataFrame(rows, schema=["hour", "demand", "day_type"], orient="row")

    res = onceover.partition_scagnostics(
        df, pairs=[("hour", "demand")], partition_by=["day_type"], metrics=["functional"],
    )
    [finding] = res["findings"]
    assert finding["metric"] == "functional"
    assert finding["validation"]["gain"] > 0.15


def test_minority_sign_reversal_seven_percent_subgroup():
    rng = random.Random(61)
    rows = []
    for i in range(1000):
        minority = i < 75  # 7.5% minority (sorts second alphabetically as "True")
        x = rng.uniform(-1, 1)
        y = (x if minority else -x) + rng.gauss(0, 0.08)
        rows.append((x, y, "True" if minority else "False"))
    df = pl.DataFrame(rows, schema=["x", "y", "protocol"], orient="row")

    rep = onceover.profile(df, detectors=["partition_scagnostics"])
    hits = rep.select(type="partitioned_shape")
    assert len(hits) == 1
    hit = hits[0]
    assert hit["specification"]["definition"]["objective"] == "standout_subgroup"
    assert "True" in hit["claim"]
    sub = rep.rows(hit["id"], df)
    assert sub.height == 75
    assert set(sub["protocol"].to_list()) == {"True"}
