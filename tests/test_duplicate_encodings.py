"""Duplicate-encoding advisory (issue #41): two label-like columns
that record the same grouping, or one that rolls up to another."""

import polars as pl

import onceover


def dupes(df, **kwargs):
    r = onceover.profile(df, budget="standard", **kwargs)
    return [f for f in r.findings if f.get("type") == "duplicate_encoding"]


def metrics(f):
    return f["evidence"][0]["metrics"]


def test_code_and_label_fire():
    n = 400
    codes = [(r % 4) + 1 for r in range(n)]
    labels = [f"group_{c}" for c in codes]
    df = pl.DataFrame({"code": codes, "label": labels})
    out = dupes(df)
    assert len(out) == 1
    m = metrics(out[0])
    assert m["form"] == "duplicate_encoding"
    assert m["exact"] is True
    assert m["levels_first"] == 4 and m["levels_second"] == 4
    assert m["exceptions"] == 0


def test_rollup_reports_as_rollup():
    cities = [f"city_{r % 9}" for r in range(450)]
    states = [f"state_{(r % 9) // 3}" for r in range(450)]
    df = pl.DataFrame({"city": cities, "state": states})
    out = dupes(df)
    assert len(out) == 1
    m = metrics(out[0])
    assert m["form"] == "rollup"
    assert m["finer"] == "city" and m["coarser"] == "state"
    assert m["levels_first"] == 9 and m["levels_second"] == 3


def test_key_determined_label_does_not_fire():
    # A genuine key and the label it determines: the key finding
    # covers this; the advisory must not fire.
    n = 300
    ids = [f"row_{r:03d}" for r in range(n)]
    labels = [f"group_{r % 4}" for r in range(n)]
    df = pl.DataFrame({"row_id": ids, "label": labels})
    assert dupes(df) == []


def test_partial_mapping_reports_exceptions():
    n_groups, n = 50, 1000
    codes = [(r % n_groups) + 1 for r in range(n)]
    labels = [f"group_{c}" for c in codes]
    done = 0
    for k in range(n):
        if codes[k] == 1 and done < 5:
            labels[k] = "group_7"
            done += 1
    df = pl.DataFrame({"code": codes, "label": labels})
    out = dupes(df)
    assert len(out) == 1
    m = metrics(out[0])
    assert m["form"] == "duplicate_encoding"
    assert m["exact"] is False
    assert m["exceptions"] == 5


def test_independent_columns_do_not_fire():
    a = [f"a_{r % 4}" for r in range(400)]
    b = [f"b_{(r // 4) % 5}" for r in range(400)]
    df = pl.DataFrame({"a": a, "b": b})
    assert dupes(df) == []


def test_quick_budget_skips_the_pass():
    n = 400
    codes = [(r % 4) + 1 for r in range(n)]
    labels = [f"group_{c}" for c in codes]
    df = pl.DataFrame({"code": codes, "label": labels})
    r = onceover.profile(df, budget="quick")
    assert [f for f in r.findings if f.get("type") == "duplicate_encoding"] == []
    assert r.coverage["duplicate_encodings"]["status"] in (
        "ran",
        "ran_exact",
        "skipped",
    )
