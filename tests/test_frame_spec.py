"""The frame spec (issue #449): infer_spec returns the resolved
interpretation without profiling; profile(df, spec=...) runs on it,
reproducing the plain run when the spec is unedited and holding every
edited entry exactly."""
import datetime as dt

import polars as pl
import pytest

import onceover


def _frame():
    days = [dt.date(2024, 1, 1) + dt.timedelta(days=i // 4) for i in range(1200)]
    return pl.DataFrame({
        "row": list(range(1, 1201)),
        "day": days,
        "store": [f"S{i % 4}" for i in range(1200)],
        "promo": [i % 7 == 0 for i in range(1200)],
        "sales": [100.0 + (i % 4) * 10 + (i % 13) for i in range(1200)],
        "units": [5 + (i % 4) + (i % 3) for i in range(1200)],
    })


def test_infer_spec_states_every_column_with_source_and_evidence():
    df = _frame()
    spec = onceover.infer_spec(df, hints={"roles": {"units": "flag"}})
    assert spec["spec_version"] == 1
    assert set(spec["columns"]) == set(df.columns)
    for name, entry in spec["columns"].items():
        assert entry["role"] in onceover._SPEC_ROLES, name
        assert entry["source"] in ("declared", "inferred")
        assert entry["evidence"]
    assert spec["columns"]["units"]["source"] == "declared"
    assert spec["columns"]["sales"]["source"] == "inferred"
    assert spec["time_axis"]["column"] == "day"
    assert spec["target"] == {"column": None, "source": "none"}


def test_unedited_spec_reproduces_the_plain_run():
    df = _frame()
    plain = onceover.profile(df)
    run = onceover.profile(df, spec=onceover.infer_spec(df))
    assert [f["id"] for f in run.findings] == [f["id"] for f in plain.findings]
    assert run.data["frame"]["roles"] == plain.data["frame"]["roles"]


def test_edited_role_is_held_exactly():
    df = _frame()
    spec = onceover.infer_spec(df)
    assert spec["columns"]["units"]["role"] != "measure"
    spec["columns"]["units"]["role"] = "measure"
    run = onceover.profile(df, spec=spec)
    roles = {r["column"]: r["role"] for r in run.data["frame"]["roles"]}
    assert roles == {c: e["role"] for c, e in spec["columns"].items()}


def test_spec_target_becomes_the_run_target():
    df = _frame()
    spec = onceover.infer_spec(df)
    spec["target"] = {"column": "sales", "source": "declared"}
    run = onceover.profile(df, spec=spec)
    assert any(f["type"] == "outcome_distribution" for f in run.findings) or \
        "outcome_account" in run.coverage


@pytest.mark.parametrize("edit, message", [
    (lambda s: s["columns"].pop("units"), "columns missing from the spec"),
    (lambda s: s["columns"].__setitem__("ghost", {"role": "measure"}), "not in the frame"),
    (lambda s: s["columns"]["units"].__setitem__("role", "quantity"), "is not one of"),
    (lambda s: s.__setitem__("spec_version", 99), "spec_version"),
    (lambda s: s.__setitem__("entity", {"column": "nope"}), "is not a column of the frame"),
])
def test_a_spec_that_does_not_describe_the_frame_is_refused(edit, message):
    df = _frame()
    spec = onceover.infer_spec(df)
    edit(spec)
    with pytest.raises(ValueError, match=message):
        onceover.profile(df, spec=spec)


def test_spec_keys_are_not_also_accepted_as_hints():
    df = _frame()
    with pytest.raises(ValueError, match="carried by the spec"):
        onceover.profile(df, spec=onceover.infer_spec(df), hints={"roles": {"units": "measure"}})
    with pytest.raises(ValueError, match="differs from the spec's target"):
        spec = onceover.infer_spec(df, target="sales")
        onceover.profile(df, spec=spec, target="units")


def test_infer_spec_suggests_separate_exact_hierarchies():
    df = pl.DataFrame([
        (f"H{i:02}", f"T{i // 4}", f"D{i // 8}", f"S{i % 4}", f"R{(i % 4) // 2}",
         day, 4 * i + day)
        for i in range(32) for day in range(4)
    ], schema=["host", "team", "department", "site", "region", "period", "serial"],
       orient="row")
    hints = {"entity": "host", "ordered_axis": "period",
             "roles": {c: "dimension" for c in ["team", "department", "site", "region"]}}
    spec = onceover.infer_spec(df, hints=hints)
    paths = {tuple(h["levels"]) for h in spec["hierarchy_candidates"]}
    assert paths == {
        ("host", "team", "department"),
        ("host", "site", "region"),
    }
    assert all(h["evidence"]["rows"] == df.height for h in spec["hierarchy_candidates"])
    shuffled = df.sample(fraction=1, shuffle=True, seed=448).select(reversed(df.columns))
    assert onceover.infer_spec(shuffled, hints=hints)["hierarchy_candidates"] == spec["hierarchy_candidates"]

    # Candidates are advisory, so saving/reusing the spec doesn't
    # introduce hierarchical peer comparisons into a plain run.
    run = onceover.profile(df, spec=spec, detectors=["entity_fingerprints"])
    assert "checked peer hierarchy" not in run.coverage["entity_fingerprints"]["detail"]
