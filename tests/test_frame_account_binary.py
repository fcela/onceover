"""Frame account: binary outcomes as rates, duplicate encodings
collapsed, continuous drivers as binned groupings, and the peak
materiality bar.

The fixtures are miniature versions of three evaluation frames:
an Adult-shaped frame (a two-level income outcome, a sampling
weight named as a weight), a Titanic-shaped frame (a 0/1 survived
flag and the pclass/class duplicate encoding), and a
Diamonds-shaped frame (price driven by the continuous carat,
and a cut grouping whose table peak is small on the column's
own scale).
"""
import polars as pl

import onceover


def adult_df(n=3000):
    marital = ["Married-civ-spouse", "Never-married", "Divorced"]
    rel = ["Husband", "Own-child", "Not-in-family"]
    rows = {"marital_status": [], "relationship": [], "age": [],
            "final_weight": [], "income": []}
    rate = [7, 1, 3]  # on-rate per marital group, in tenths
    for i in range(n):
        g = i % 3
        rows["marital_status"].append(marital[g])
        rows["relationship"].append(rel[g])
        rows["age"].append(31 + 9 * g + (i % 13))
        rows["final_weight"].append(50000 + (i * 37) % 90000)
        rows["income"].append(">50K" if (i // 3) % 10 < rate[g] else "<=50K")
    return pl.DataFrame(rows)


def test_adult_account_states_the_income_rate():
    report = onceover.profile(adult_df())
    dominant = report.data["frame"]["dominant"]
    statements = [d["statement"] for d in dominant]
    assert any(
        "income" in s and "rate" in s and "base rate" in s for s in statements
    ), statements
    # The rate reading is in the record, with lift and support.
    rates = [r for d in dominant for r in d.get("rates") or []]
    income = [r for r in rates if r["response"] == "income"]
    assert income, rates
    r = income[0]
    assert r["on_label"] == ">50K"
    assert 0.2 < r["base_rate"] < 0.45
    assert r["peak_rate"] > r["base_rate"]
    assert r["lift"] > 1.5
    assert r["peak_support"] >= 20
    # The sampling weight is not a measure the account organises.
    assert all(
        sh["measure"] != "final_weight"
        for d in dominant
        for sh in d["shares"]
    ), dominant


def titanic_df(n=900):
    rows = {"survived": [], "pclass": [], "class": [], "sex": [],
            "fare": [], "age": []}
    label = {1: "First", 2: "Second", 3: "Third"}
    for i in range(n):
        p = 1 + (i % 3)
        female = (i // 3) % 2 == 0
        # Survival: 8/10 of women, 2/10 of men, in tenths.
        on = (i // 6) % 10 < (8 if female else 2)
        rows["survived"].append(1 if on else 0)
        rows["pclass"].append(p)
        rows["class"].append(label[p])
        rows["sex"].append("female" if female else "male")
        rows["fare"].append(80.0 / p + (i % 11))
        rows["age"].append(18.0 + (i % 41))
    return pl.DataFrame(rows)


def test_titanic_account_states_survived_without_duplicate_encodings():
    report = onceover.profile(titanic_df())
    dominant = report.data["frame"]["dominant"]
    statements = [d["statement"] for d in dominant]
    assert any(
        "survived" in s and "rate" in s and "base rate" in s for s in statements
    ), statements
    labels = [d["label"] for d in dominant]
    assert not ("pclass" in labels and "class" in labels), labels


def diamonds_df(n=5400):
    cuts = ["Fair", "Good", "Very Good", "Premium", "Ideal"]
    table_mean = {"Fair": 59.0, "Good": 58.7, "Very Good": 58.0,
                  "Premium": 58.7, "Ideal": 56.0}
    rows = {"carat": [], "cut": [], "table": [], "price": []}
    for i in range(n):
        cut = cuts[i % 5]
        carat = 0.2 + ((i * 104729) % 500) / 100.0
        table = table_mean[cut] + ((i * 7919) % 200 - 100) / 50.0
        if i % 199 == 0:
            table += 26.0 if (i // 199) % 2 == 0 else -26.0
        price = 400.0 + 3000.0 * carat * carat + (i % 97)
        rows["carat"].append(carat)
        rows["cut"].append(cut)
        rows["table"].append(table)
        rows["price"].append(price)
    return pl.DataFrame(rows)


def test_diamonds_account_states_price_by_binned_carat():
    report = onceover.profile(diamonds_df())
    dominant = report.data["frame"]["dominant"]
    binned = [d for d in dominant if d["kind"] == "binned" and d["label"] == "carat"]
    assert binned, dominant
    assert any(
        sh["measure"] == "price" for d in binned for sh in d["shares"]
    ), binned


def test_diamonds_immaterial_peak_is_not_a_headline():
    report = onceover.profile(diamonds_df())
    dominant = report.data["frame"]["dominant"]
    statements = [d["statement"] for d in dominant]
    # The cut grouping's table means span about 3 points on a
    # 43-95 scale: no "averages highest for Fair" headline.
    assert not any("averages highest for Fair" in s for s in statements), statements
    for d in dominant:
        if d["label"] == "cut":
            assert d.get("peak") is None, d
