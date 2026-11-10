"""Declared hints (issue #18): the expectations pass checks structure
the user declares — gap-free columns, cadences, ordering, panel shape —
and tags its evidence reference_source="declared"."""
import datetime

import polars as pl

import onceover


def declared(report, category=None):
    out = []
    for f in report.findings:
        if category and f.get("category") != category:
            continue
        if any(
            e["metrics"].get("reference_source") == "declared"
            for e in f.get("evidence") or []
        ):
            out.append(f)
    return out


def test_integer_sequence_gaps():
    ids = [i for i in range(1, 101) if i not in (4, 7, 18)]
    df = pl.DataFrame({"id": ids, "x": [float(i % 7) for i in ids]})
    report = onceover.profile(df, hints={"no_gaps": ["id"]})
    hits = declared(report, "gaps")
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["missing"] == 3
    assert metrics["absent_values"] == [4, 7, 18]
    assert metrics["step"] == 1


def test_monthly_cadence_below_inference_floor():
    months = [
        datetime.date(2024, 1, 1),
        datetime.date(2024, 2, 1),
        datetime.date(2024, 4, 1),
        datetime.date(2024, 5, 1),
        datetime.date(2024, 6, 1),
    ]
    df = pl.DataFrame({"month": months, "v": [1.0, 2.0, 3.0, 4.0, 5.0]})
    report = onceover.profile(
        df, hints={"no_gaps": ["month"], "cadence": {"month": "monthly"}}
    )
    hits = declared(report, "gaps")
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["missing"] == 1
    assert metrics["reference"] == "months"
    # The declared check replaces inference for this column: every gap
    # finding about `month` is the declared one.
    gaps = [
        f
        for f in report.findings
        if f.get("category") == "gaps"
        and "month" in (f.get("subject") or {}).get("columns", [])
    ]
    assert len(gaps) == 1


def test_ordering_violations_and_satisfaction():
    bad = pl.DataFrame({"x": [1, 2, 3, 5, 4, 6, 7, 8, 9, 10]})
    report = onceover.profile(bad, hints={"order_by": ["x"]})
    hits = declared(report, "ordering")
    assert len(hits) == 1
    assert hits[0]["evidence"][0]["metrics"]["inversions"] == 1
    assert "not ordered" in hits[0]["claim"]

    good = pl.DataFrame({"x": list(range(10))})
    report2 = onceover.profile(good, hints={"order_by": ["x"]})
    hits2 = declared(report2, "ordering")
    assert len(hits2) == 1
    assert hits2[0]["evidence"][0]["metrics"]["inversions"] == 0


def test_panel_completeness():
    rows = []
    for ent in ("E0", "E1"):
        for year in (2020, 2021, 2022):
            if (ent, year) == ("E1", 2021):
                continue
            rows.append({"entity": ent, "year": year, "gdp": 1.0})
    df = pl.DataFrame(rows)
    hints = {"dimensions": ["entity"], "time": "year", "measures": ["gdp"]}
    report = onceover.profile(df, hints=hints)
    hits = declared(report, "gaps")
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["missing_cells"] == 1
    assert metrics["expected_cells"] == 6
    absent = str(metrics["absent_values"])
    assert "E1" in absent and "2021" in absent

    full = pl.DataFrame(rows + [{"entity": "E1", "year": 2021, "gdp": 1.0}])
    report2 = onceover.profile(full, hints=hints)
    assert declared(report2, "gaps") == []


def test_invalid_hints_are_reported_not_raised():
    df = pl.DataFrame({"id": list(range(1, 16))})
    report = onceover.profile(df, hints={"no_gaps": ["id", "dtae"], "time": "when"})
    detail = report.coverage["expectations"]["detail"]
    assert "dtae" in detail and "when" in detail


def test_no_hints_no_expectations():
    df = pl.DataFrame({"id": list(range(1, 16))})
    report = onceover.profile(df)
    assert report.coverage["expectations"]["status"] == "skipped"
    assert declared(report) == []


def _org_df(broken=False):
    rows = []
    for u in range(12):
        team = u // 3
        division = team // 2
        for r in range(50):
            rows.append(
                {
                    "unit": f"unit_{u:02d}",
                    "team": f"team_{team}",
                    "division": f"division_{division}",
                    "revenue": 10.0 + ((u * 17 + r * 7) % 31),
                }
            )
    df = pl.DataFrame(rows)
    if broken:
        # team_1's rows in unit_04 claim division_1 as well.
        df = df.with_columns(
            pl.when((pl.col("unit") == "unit_04"))
            .then(pl.lit("division_1"))
            .otherwise(pl.col("division"))
            .alias("division")
        )
    return df


def test_declared_hierarchy_holds_and_leads():
    report = onceover.profile(
        _org_df(), hints={"hierarchies": [["unit", "team", "division"]]}
    )
    hits = declared(report, "column_roles")
    assert len(hits) == 1
    lead = hits[0]
    metrics = lead["evidence"][0]["metrics"]
    assert metrics["violations"] == 0
    assert metrics["hierarchy_levels"] == ["unit", "team", "division"]
    assert [e["finer"] for e in metrics["hierarchy_edges"]] == ["unit", "team"]
    assert "holds" in lead["claim"]
    # The inferred restatements of the same chain group under the
    # declared statement instead of standing as their own findings.
    assert lead["grouped_members"], "expected grouped restatements"
    for member in report.grouped_findings:
        assert member["grouped_under"] == lead["id"]
        assert set(member["subject"]["columns"]) <= {"unit", "team", "division"}
    detail = report.coverage["expectations"]["detail"]
    assert "hierarchy on (unit, team, division)" in detail


def test_declared_hierarchy_deviation_is_one_statement():
    report = onceover.profile(
        _org_df(broken=True), hints={"hierarchies": [["unit", "team", "division"]]}
    )
    hits = declared(report, "column_roles")
    assert len(hits) == 1
    metrics = hits[0]["evidence"][0]["metrics"]
    assert metrics["violations"] == 50
    broken = metrics["broken_mappings"]
    assert len(broken) == 1
    assert broken[0]["finer"] == "team"
    assert broken[0]["coarser"] == "division"
    assert broken[0]["split_levels"] == 1
    assert "does not hold" in hits[0]["claim"]


def test_declared_hierarchy_statements_verify_against_the_data():
    hints = {"hierarchies": [["unit", "team", "division"]]}
    for broken in (False, True):
        df = _org_df(broken=broken)
        report = onceover.profile(df, hints=hints)
        hits = declared(report, "column_roles")
        assert len(hits) == 1
        result = report.verify(df)
        rows = result.filter(pl.col("id") == hits[0]["id"])
        assert rows.height > 0
        assert set(rows["status"].unique()) == {"ok"}


def test_declared_hierarchy_fails_verification_on_changed_data():
    hints = {"hierarchies": [["unit", "team", "division"]]}
    report = onceover.profile(_org_df(), hints=hints)
    hits = declared(report, "column_roles")
    result = report.verify(_org_df(broken=True))
    rows = result.filter(
        (pl.col("id") == hits[0]["id"]) & (pl.col("status") == "mismatch")
    )
    assert rows.height > 0


def test_declared_hierarchy_problems_are_noted_not_raised():
    report = onceover.profile(
        _org_df(),
        hints={"hierarchies": [["unit", "team", "region"], ["unit"]]},
    )
    detail = report.coverage["expectations"]["detail"]
    assert "region" in detail
    assert "at least two levels" in detail
    assert declared(report, "column_roles") == []


def test_exclude_columns_removes_them_everywhere():
    df = _org_df().with_columns(
        (pl.col("revenue") * 2).alias("revenue_double")
    )
    report = onceover.profile(df, hints={"exclude_columns": ["revenue_double"]})
    assert "revenue_double" not in report.data["columns"]
    assert "revenue" in report.data["columns"]
    named = [
        f
        for f in list(report.findings) + list(report.grouped_findings)
        if "revenue_double" in f["subject"]["columns"]
    ]
    assert named == []
    detail = report.coverage["column_exclusions"]["detail"]
    assert "revenue_double" in detail
    assert "hints.exclude_columns" in detail


def test_exclude_columns_unknown_names_are_stated():
    df = _org_df()
    report = onceover.profile(df, hints={"exclude_columns": ["nope"]})
    detail = report.coverage["column_exclusions"]["detail"]
    assert "nope" in detail and "ignored" in detail
    # Nothing was excluded: the table profiles in full.
    assert set(report.data["columns"]) == {"unit", "team", "division", "revenue"}


def test_bare_string_hint_values_raise():
    df = _org_df()
    for hints in ({"exclude_columns": "unit"}, {"hierarchies": "unit"}):
        try:
            onceover.profile(df, hints=hints)
        except ValueError:
            pass
        else:
            raise AssertionError(f"expected ValueError for {hints}")


def _week_df():
    rows = []
    for e in range(12):
        for w in range(1, 41):
            if (e * 7 + w) % 13 == 0:
                continue  # gaps, so week is no per-group consecutive run
            rows.append(
                {
                    "entity": f"site_{e}",
                    "week": w,
                    "value": 50.0 + ((e * 31 + w * 7) % 41)
                    + (30.0 if w > 25 else 0.0),
                }
            )
    return pl.DataFrame(rows)


def test_week_axis_is_not_a_target():
    # Issue #207: the bare week integer is the declared time axis
    # but keeps the inferred measure role. On the unmodified build
    # it stays target-eligible and draws a predictive finding; the
    # structural axis exclusion removes it from target roles, and
    # coverage says so.
    report = onceover.profile(_week_df(), hints={"time": "week"})
    assert report.data["frame"]["axis"]["source_column"] == "week"
    targeting = [
        f for f in report.findings if (f["subject"].get("target") == "week")
    ]
    assert targeting == []
    details = " ".join(
        entry["detail"] for entry in report.coverage.values()
    )
    assert "time axis" in details and "week" in details


def test_week_axis_role_declaration_overrides_the_exclusion():
    # User declaration wins over the structural default: declaring
    # the axis column's role explicitly makes it target-eligible
    # again, exactly as on the unmodified build.
    report = onceover.profile(
        _week_df(), hints={"time": "week", "roles": {"week": "measure"}}
    )
    targeting = [
        f
        for f in report.findings
        if (f["subject"].get("target") == "week") and f.get("type") == "predictive"
    ]
    assert len(targeting) == 1
