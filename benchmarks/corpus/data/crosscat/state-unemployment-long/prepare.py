"""Reshape the bundled wide state-unemployment panel into long form.

Source data: the wide panel in ../state-unemployment (BLS LAUS
state series via FRED fredgraph, public). The reshape is a pure
unpivot: one row per (month, state) with the state code taken
from the wide column name (`al_unemployment_rate` -> `AL`).
No values are altered, filled, or dropped beyond wide cells that
are null (none exist in the bundled panel's span).
"""
import polars as pl

wide = pl.read_csv("../state-unemployment/data.csv", try_parse_dates=True)
long = wide.unpivot(
    index="date",
    variable_name="series",
    value_name="unemployment_rate",
).with_columns(
    pl.col("series")
    .str.replace("_unemployment_rate", "")
    .str.to_uppercase()
    .alias("state")
).select(["date", "state", "unemployment_rate"]).sort(["date", "state"])
long.write_csv("data.csv")
print(long.shape)
