"""Reproducible fixtures and output formatting for onceover_analysis.ipynb.

The fixtures specify known relationships and independent controls. Formatting
functions read returned evidence; profiling calls remain in the notebook.
"""

import datetime as dt
import random

import altair as alt
from great_tables import GT, loc, style
import polars as pl


def table(data, title, subtitle=None, widths=None):
    frame = data if isinstance(data, pl.DataFrame) else pl.DataFrame(data)
    result = (GT(frame).tab_header(title=title, subtitle=subtitle)
        .tab_options(table_width="100%", table_layout="fixed", table_font_size="14px", data_row_padding="9px")
        .tab_style(style.css("white-space: normal; overflow-wrap: anywhere; line-height: 1.5;"), loc.body()))
    return result.cols_width(widths) if widths else result


def orders_fixture(n=8000, seed=2024):
    """Order snapshot: expected-unique external reference; regional lookup;
    rounded quantity × price; noisy tenure/churn relation; independent control.
    Thirty-two orders share each business date; two business dates are omitted.
    """
    if n < 1000:
        raise ValueError("Use at least 1000 rows for this fixture")
    rng = random.Random(seed)
    regions = ["emea", "amer", "apac"]
    managers = ["Ann", "Bob", "Cid"]
    region_id = [rng.randrange(3) for _ in range(n)]
    manager = [managers[r] for r in region_id]
    for i in [n // 2, 3 * n // 4]:
        manager[i] = managers[(region_id[i] + 1) % 3]
    external = [f"S{i:06}" for i in range(n)]
    for k in range(3):
        external[n - 1 - k] = external[k]
    quantity = [rng.randrange(1, 11) for _ in range(n)]
    price = [round(rng.uniform(5, 100), 2) for _ in range(n)]
    tenure = [rng.randrange(48) for _ in range(n)]
    churn = [int((t > 24) != (rng.random() < 0.06)) for t in tenure]
    business_dates = []
    day = dt.date(2024, 1, 1)
    while len(business_dates) < (n + 31) // 32 + 4:
        if day.weekday() < 5:
            business_dates.append(day)
        day += dt.timedelta(days=1)
    skip = ((n + 31) // 32) // 3
    dates = [business_dates[i // 32 + (2 if i // 32 >= skip else 0)] for i in range(n)]
    return pl.DataFrame({
        "order_id": range(n), "source_order_ref": external,
        "region_id": region_id, "region_name": [regions[r] for r in region_id], "manager": manager,
        "order_date": dates, "quantity": quantity, "unit_price": price,
        "line_total": [round(q * p, 2) for q, p in zip(quantity, price)],
        "double_quantity": [2 * q for q in quantity],
        "tenure_months": tenure, "churn": churn,
        "country": ["US" if rng.random() < 0.92 else rng.choice(["DE", "FR", "JP"]) for _ in range(n)],
        "phone": [f"({rng.randrange(100, 999)}) {rng.randrange(100, 999)}-{rng.randrange(1000, 9999)}"
                  if rng.random() < 0.55 else
                  f"{rng.randrange(100, 999)}-{rng.randrange(100, 999)}-{rng.randrange(1000, 9999)}"
                  for _ in range(n)],
        "support_hours": [None if rng.random() < 0.05 else round(rng.uniform(6, 10), 1) for _ in range(n)],
        "control": [rng.random() for _ in range(n)],
        "status_code": [rng.randrange(1, 6) for _ in range(n)],
    })


def find(report, category, *, subtype=None, form=None, column=None, inputs=None, target=None):
    return next(f for f in report.findings if f["category"]==category
                and (subtype is None or f.get("type")==subtype)
                and (form is None or f["specification"].get("form")==form)
                and (column is None or column in f["subject"]["columns"])
                and (inputs is None or set(f["subject"].get("inputs",[]))==set(inputs))
                and (target is None or f["subject"].get("target")==target))


def measured(finding, *, contains=None):
    evidence=[e for e in finding["evidence"] if contains is None or contains in e["metrics"]]
    evidence.sort(key=lambda e:(e["validation"]["method"]!="cross_validation",e["method"]["detector"]))
    return evidence[0]["metrics"]


def summary(report):
    n = report.overview["n_rows"]
    dirty = find(report, "uniqueness", column="source_order_ref")
    fd = find(report,"dependencies",subtype="functional",form="value_mapping",inputs=["region_id"],target="manager")
    affine = find(report,"dependencies",subtype="functional",form="affine",inputs=["quantity"],target="double_quantity")
    gap = find(report,"gaps",column="order_date")
    rule = measured(find(report,"dependencies",subtype="predictive",target="churn",inputs=["tenure_months"]))
    local = measured(find(report,"dependencies",subtype="predictive",form="piecewise_linear",target="line_total"))
    phone = measured(find(report,"formats",column="phone"))
    support = report.columns["support_hours"]
    country = report.columns["country"]["top_values"][0]
    rows = [
        ("Row identifier", f"order_id: {report.columns['order_id']['distinct']:,} distinct values; no missing values.",
         f"Exact counts on {n:,} rows", "Suitable as a row identifier in this extract."),
        ("External reference", f"source_order_ref: {measured(dirty)['duplicates']} repeated non-null values.",
         f"Exact counts on {n:,} rows", "The fixture expects this reference to be unique; inspect the repeated references before joining."),
        ("Regional mapping", f"region_id → manager: {measured(fd)['violations']} violations in {measured(fd)['n_used']:,} evaluated rows.",
         "Compared regional assignments in all evaluated rows", "A common mapping is present, with exceptions that require a business explanation."),
        ("Redundant measurement", f"double_quantity = {measured(affine)['a']:g} × quantity; {measured(affine)['violations']} violations.",
         f"Residual check on {measured(affine)['n']:,} rows; tolerance 1e−9 relative", "These fields encode the same quantity within the stated tolerance."),
        ("Calendar completeness", f"{measured(gap)['missing']} missing business dates between {measured(gap)['from']} and {measured(gap)['to']}.",
         "Exact date-sequence check", "A gap can affect time-based aggregation; the calendar definition must match the application."),
        ("String representation", f"phone: {phone['n_patterns']} formats among {phone['n_values']:,} values.",
         "Exact format counts", "Normalize representations before record matching or parsing."),
        ("Missing observations", f"support_hours: {support['null_count']:,} missing values ({support['null_fraction']:.2%}).",
         f"Exact counts on {n:,} rows", "Missing values should not be interpreted as observed zeros."),
        ("Concentration", f"country: {country['count']:,} observations ({country['share']:.2%}) are {country['value']}.",
         "Exact value counts", "Overall results will be dominated by this group."),
        ("Predictive relationship", f"churn from tenure_months: accuracy {rule['cv_accuracy']:.3f}; majority baseline {rule['baseline']:.3f}.",
         f"{rule['folds']}-fold diagnostic cross-validation; {rule['n']:,} rows", "Tenure contains predictive information in this fixture; the result does not identify a causal effect."),
        ("Conditional numeric structure", f"line_total: local-model R² {local['holdout_r2']:.3f}; global-linear R² {local['global_linear_holdout_r2']:.3f}.",
         f"Diagnostic holdout: {local['n_holdout']:,} rows", "One global linear relation does not describe the generated quantity–price interaction adequately."),
    ]
    return table(pl.DataFrame(rows, schema=["Diagnostic", "Result", "Basis", "Interpretation"], orient="row"),
                 "Table 1. Principal diagnostic results",
                 f"{n:,} rows, {report.overview['n_columns']} fields. Exact constraints and diagnostic model results have different meanings.",
                 {"Diagnostic":"16%", "Result":"31%", "Basis":"22%", "Interpretation":"31%"})


def columns(report):
    rows = [{"Field": name, "Type": c["dtype"], "Inferred role": c["role"],
             "Missing": f"{c['null_count']:,}", "Distinct": f"{c['distinct']:,}"}
            for name,c in report.columns.items()]
    return table(rows, "Table 9. Column summaries",
                 "Roles are heuristics. Uniqueness alone does not establish that a field is a business identifier.",
                 {"Field":"27%", "Type":"15%", "Inferred role":"25%", "Missing":"16%", "Distinct":"17%"})


def exceptions(orders, report):
    usual = orders.group_by("region_id").agg(pl.col("manager").mode().first().alias("usual_manager"))
    mismatches = (orders.join(usual,on="region_id").filter(pl.col("manager") != pl.col("usual_manager"))
                  .select("order_id", "region_name", "manager", "usual_manager").sort("order_id"))
    expected = measured(find(report,"dependencies",subtype="functional",form="value_mapping",inputs=["region_id"],target="manager"))["violations"]
    assert mismatches.height == expected == 2
    return table(mismatches.rename({"order_id":"Order ID", "region_name":"Region", "manager":"Recorded manager", "usual_manager":"Modal manager"}),
                 "Table 3. Rows inconsistent with the modal regional mapping",
                 "The modal assignment is calculated from the observations; it is not an external business rule.")


def validation(orders, report, shuffled_report):
    source = orders["source_order_ref"]
    assert orders["order_id"].n_unique() == orders.height
    assert orders.height - source.n_unique() == 3
    assert measured(find(report,"uniqueness",column="source_order_ref"))["duplicates"] == 3
    assert (orders["double_quantity"] == orders["quantity"] * 2).all()
    assert not any(f["category"]=="dependencies" and f.get("type")=="predictive" and f["subject"].get("target")=="control" for f in report.findings)
    assert not any(f["category"]=="dependencies" and f.get("type")=="predictive" and f["subject"].get("target")=="churn" for f in shuffled_report.findings)
    return table([
        {"Check":"Internal order identifier", "Expected":"Unique and complete", "Observed":"Verified directly"},
        {"Check":"External order reference", "Expected":"Three surplus repeated values", "Observed":"Direct count agrees with report"},
        {"Check":"Derived quantity", "Expected":"double_quantity = 2 × quantity", "Observed":"All rows agree"},
        {"Check":"Independent numeric control", "Expected":"No reported predictability", "Observed":"No finding reported"},
        {"Check":"Permuted churn labels", "Expected":"No reported churn prediction or decision table", "Observed":"No finding reported"},
    ], "Table 7. Verification and negative controls",
    "The permutation is one fixed control, not an estimate of a false-positive rate.",
    {"Check":"27%", "Expected":"41%", "Observed":"32%"})


def churn_plot(orders):
    data=orders.group_by("tenure_months").agg(pl.col("churn").mean().alias("Proportion"),pl.len().alias("n")).sort("tenure_months")
    return alt.Chart(data).mark_line(point=True).encode(
        x=alt.X("tenure_months:Q",title="Tenure (months)"),
        y=alt.Y("Proportion:Q",title="Observed churn proportion",scale=alt.Scale(domain=[0,1]),axis=alt.Axis(format="%")),
        tooltip=["tenure_months:Q",alt.Tooltip("Proportion:Q",format=".3f"),"n:Q"],
    ).properties(width=650,height=240)



def references(orders, report):
    rows = (orders.group_by("source_order_ref").agg(pl.col("order_id").sort().alias("Order IDs"))
            .filter(pl.col("Order IDs").list.len() > 1).sort("source_order_ref").to_dicts())
    assert sum(len(row["Order IDs"])-1 for row in rows) == measured(find(report,"uniqueness",column="source_order_ref"))["duplicates"]
    return table([{"Source reference":r["source_order_ref"], "Order IDs":", ".join(map(str,r["Order IDs"]))} for r in rows],
                 "Table 2. Repeated external references", "Each reference occurs twice. The internal order IDs remain distinct.")


def formula_check(orders, report):
    import numpy as np
    affine=measured(find(report,"dependencies",subtype="functional",form="affine",inputs=["quantity"],target="double_quantity"))
    assert (orders["double_quantity"] == 2*orders["quantity"]).all()
    total=orders["line_total"].to_numpy()
    expected=np.round(orders["quantity"].to_numpy()*orders["unit_price"].to_numpy(),2)
    matched=int(np.isclose(total,expected,rtol=0,atol=1e-9).sum())
    assert matched == orders.height
    return table([
        {"Relationship":"double_quantity = 2 × quantity", "Matching rows":f"{orders.height:,} of {orders.height:,}",
         "How established":"Profiler identity finding, checked against every row"},
        {"Relationship":"line_total = round(quantity × unit_price, 2)", "Matching rows":f"{matched:,} of {orders.height:,}",
         "How established":"Independent check of the known fixture formula; this multiplication law is not inferred by the library"},
    ],"Table 4. Exact checks of the generated quantities", "The direct formula check allows an absolute floating-point difference of 1e−9.",
    {"Relationship":"35%", "Matching rows":"20%", "How established":"45%"})


def local_models(report):
    ev=measured(find(report,"dependencies",subtype="predictive",form="piecewise_linear",target="line_total"))
    return table([
        {"Model":"One global linear model", "Holdout R²":f"{ev['global_linear_holdout_r2']:.3f}",
         "Holdout rows":f"{ev['n_holdout']:,}"},
        {"Model":f"{len(ev['leaves'])} local linear models", "Holdout R²":f"{ev['holdout_r2']:.3f}",
         "Holdout rows":f"{ev['n_holdout']:,}"},
    ],"Table 5. Approximate models of line_total",
    "Both scores are taken from the same reported diagnostic holdout. Neither score establishes an exact accounting identity.")


def total_plot(orders):
    data=orders.filter(pl.col("quantity").is_in([2,5,9])).sample(n=500,seed=7)
    return alt.Chart(data).mark_circle(size=15,opacity=0.45).encode(
        x=alt.X("unit_price:Q",title="Unit price"), y=alt.Y("line_total:Q",title="Line total"),
        color=alt.Color("quantity:N",title="Quantity"),
    ).properties(width=650,height=250)


def missing_dates(orders, report):
    observed=set(orders["order_date"].to_list())
    start,end=min(observed),max(observed)
    expected={start+dt.timedelta(days=i) for i in range((end-start).days+1) if (start+dt.timedelta(days=i)).weekday()<5}
    missing=sorted(expected-observed)
    assert len(missing) == measured(find(report,"gaps",column="order_date"))["missing"] == 2
    return table([{"Date absent from the extract":d.isoformat(),"Reference calendar":"Monday–Friday; holidays not excluded"} for d in missing],
                 "Table 6. Direct check of the date-gap finding")


def permute_churn(orders):
    labels=orders["churn"].to_list()
    random.Random(2025).shuffle(labels)
    return orders.with_columns(pl.Series("churn",labels))


def coverage(report):
    return table([{"Pass":name.replace("_"," "),"Status":v["status"],"Recorded scope":v["detail"]}
                  for name,v in report.coverage.items()],"Table 8. Scope of the diagnostic",
                  "The scope records which rows, features, and candidate relationships were examined.",
                  {"Pass":"23%","Status":"16%","Recorded scope":"61%"})
