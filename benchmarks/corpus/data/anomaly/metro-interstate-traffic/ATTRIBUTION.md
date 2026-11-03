# Metro Interstate Traffic Volume — attribution

- **Source**: UCI Machine Learning Repository, *Metro Interstate
  Traffic Volume* (dataset 492),
  https://archive.ics.uci.edu/static/public/492/metro%2Binterstate%2Btraffic%2Bvolume.zip
- **Creator**: John Hogue.
- **License**: CC BY 4.0 (see the UCI dataset page).
- **Citation requested by the source**: DOI 10.24432/C5X60B.
  Dataset page: https://archive.ics.uci.edu/dataset/492/metro+interstate+traffic+volume
- **Retrieved**: 2026-11-03. The source zip's sha256,
  `b99aeabcbd6cc86f642da3a79d90883425798f58abd3b3302da2fa19dda73768`,
  is pinned in `prepare.py` and verified on every run.

## Content

Hourly westbound I-94 traffic volume for MN DoT ATR station 301
(between Minneapolis and St Paul), 2012-10-02 09:00 to 2018-09-30
23:00 local time, with the hour's weather and holiday label: 48,204
rows, 9 columns (`holiday`, `temp`, `rain_1h`, `snow_1h`,
`clouds_all`, `weather_main`, `weather_description`, `date_time`,
`traffic_volume`). `temp` is in Kelvin, `rain_1h`/`snow_1h` in mm,
`clouds_all` is percent cloud cover — the units in the dataset's own
documentation, and the column names are the published ones.

## Preprocessing log

None beyond decompression: `prepare.py` unpacks the published
`.csv.gz` and writes its bytes to `data.csv` unchanged. In
particular, the quirks below are the dataset's character and are
kept, not cleaned:

1. **Duplicate timestamps.** `date_time` is the natural key but does
   not identify rows: 40,575 distinct values in 48,204 rows; 5,445
   timestamps appear more than once, and 7,629 rows are extra copies
   of an hour already present. Counts measured with Polars on the
   prepared file, 2026-11-03.
2. **Grid gaps.** The span holds 52,551 hourly slots; 11,976 of them
   (22.8%) have no row. Nothing is interpolated or filled.
3. **`holiday` encoding.** Non-holidays are the literal string
   `None`, per the source file; there are no nulls anywhere in the
   table (UCI documents "Has Missing Values? No", and the prepared
   file has zero nulls per column).
4. **`date_time` stays a string.** The values are
   `YYYY-MM-DD HH:MM:SS` text as published; the corpus holds the
   data as served rather than pre-parsing the column into a
   datetime.
5. **`temp` includes ten 0.0 K readings** (2014-01-31 to 2014-02-02),
   physically impossible and evidently a sensor/recording artifact
   in the source. Kept as served.
