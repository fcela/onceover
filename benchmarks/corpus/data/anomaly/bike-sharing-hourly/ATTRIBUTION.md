# Bike Sharing (hourly) — attribution

- **Source**: UCI Machine Learning Repository, *Bike Sharing
  Dataset* (dataset 275),
  https://archive.ics.uci.edu/static/public/275/bike+sharing+dataset.zip
- **Creator**: Hadi Fanaee-T (2013).
- **License**: CC BY 4.0 (see the UCI dataset page).
- **Citation requested by the source**: DOI 10.24432/C5W894.
  Dataset page: https://archive.ics.uci.edu/dataset/275/bike+sharing+dataset
- **Retrieved**: 2026-11-03. The source zip's sha256,
  `b70182d0d0508e9abbb79306ce5c0cec34869000f8220175ac83d11dbe845401`,
  is pinned in `prepare.py` and verified on every run.

## Content

Hourly Capital Bikeshare (Washington, DC) rental counts for 2011
and 2012 with the hour's weather and calendar context: 17,379
rows, 17 columns. Per the dataset's own documentation on the UCI
page: `season` is coded 1:winter, 2:spring, 3:summer, 4:fall;
`yr` is 0 for 2011 and 1 for 2012; `holiday` marks DC public
holidays; `workingday` is 1 when the day is neither a weekend nor
a holiday; `weathersit` runs 1 (clear / few clouds) to 4 (heavy
rain, ice pellets, thunderstorm, snow + fog). The weather
measures are normalized as published: `temp` is
(t − t_min)/(t_max − t_min) with t_min = −8, t_max = +39 °C;
`atemp` the same for feeling temperature with −16/+50; `hum` is
humidity divided by 100; `windspeed` is wind speed divided by 67.
`casual` and `registered` are the two user-class rental counts
and `cnt` is their total. Column names are the published ones.

## Preprocessing log

None beyond extraction: `prepare.py` unpacks `hour.csv` from the
published zip and writes its bytes to `data.csv` unchanged. In
particular, the quirks below are the dataset's character and are
kept, not cleaned:

1. **Grid gaps.** (`dteday`, `hr`) is unique — 17,379 distinct
   pairs in 17,379 rows — but the span 2011-01-01 00:00 to
   2012-12-31 23:00 holds 17,544 hourly slots, so 165 hours
   (0.94%) have no row; 76 of 731 days have fewer than 24 rows.
   The deepest hole is Hurricane Sandy: 2012-10-29 has exactly
   one row and 2012-10-30 has eleven. Nothing is interpolated or
   filled. Counts measured with Polars on the prepared file,
   2026-11-03.
2. **`dteday` stays a string.** The values are `YYYY-MM-DD` text
   as published; the corpus holds the data as served rather than
   pre-parsing the column into a date.
3. **Derived calendar columns.** `yr`, `mnth`, `weekday`,
   `holiday`, `workingday`, and `season` are all functions of
   `dteday` as published. `season` is a function of the date, not
   of the month: it changes at a day boundary inside months 3, 6,
   9, and 12 (season 2 begins 2011-03-21, season 3 on 2011-06-21,
   season 4 on 2011-09-23, season 1 on 2011-12-21; the same dates
   in 2012), so `mnth` alone does not determine it.
4. **`cnt` is exactly `casual + registered`** on every one of the
   17,379 rows — the source documents `cnt` as the total count
   including both classes.
5. **`weathersit` = 4 occurs on exactly 3 rows** (the full
   distribution is 1: 11,413; 2: 4,544; 3: 1,419; 4: 3).
6. **`windspeed` is exactly 0 on 12.54% of rows** (2,179 rows),
   a floor pattern in the normalized measure, kept as served.
7. **`instant` is a row counter**, exactly 1..17,379 in row
   order, not a measurement.
