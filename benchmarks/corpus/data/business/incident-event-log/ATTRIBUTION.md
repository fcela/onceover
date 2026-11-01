# Incident Management Process Enriched Event Log — attribution

- **Source**: UCI Machine Learning Repository, *Incident
  Management Process Enriched Event Log* (dataset 498),
  https://archive.ics.uci.edu/static/public/498/incident+management+process+enriched+event+log.zip
- **Creators**: Claudio Amaral, Marcelo Fantinato, Sarajane
  Peres (2018, per the dataset page).
- **License**: CC BY 4.0 (per the dataset page).
- **Citation requested by the source**: DOI 10.24432/C57S4H.
  Dataset page:
  https://archive.ics.uci.edu/dataset/498/incident+management+process+enriched+event+log
- **Retrieved**: 2026-11-01. 141,712 events over 24,918 incidents
  from the ServiceNow incident management process of an IT
  company, enriched with attributes from the underlying
  relational system; the data is anonymized (dataset page).

## Preprocessing log (format-standard preparation)

1. ServiceNow-internal column names renamed to descriptive ones:
   `number` -> `incident_id`, `u_symptom` -> `symptom`,
   `cmdb_ci` -> `configuration_item`, `rfc` ->
   `request_for_change_id`, `caused_by` -> `caused_by_change_id`,
   `closed_code` -> `close_code`, `sys_mod_count` ->
   `system_modification_count`, the remaining `sys_*` and `u_*`
   prefixes spelled out.
2. The source's missing-value symbol `?` decoded to an empty
   field. It appears in 18 columns, most heavily in
   `configuration_item` (141,267 of 141,712 rows), `vendor`
   (141,468), `request_for_change_id` (140,721) and
   `caused_by_change_id` (141,689).
3. The five timestamp columns are served as day/month/year
   (`29/2/2016 01:16`) and are normalized to ISO 8601
   (`2016-02-29T01:16`); every value parses under that format.
4. `impact`, `urgency` and `priority` are served with a numeric
   prefix ("2 - Medium"); the prefix is stripped.

Measured structure of the prepared file: `priority` is a
deterministic function of `impact` and `urgency` (8 observed
combinations, one priority each). `incident_state` carries the
eight lifecycle levels plus a residual code `-100` on 5 rows.
`subcategory` does not determine `category`: 49 subcategories
appear under more than one category. No rows are dropped.
