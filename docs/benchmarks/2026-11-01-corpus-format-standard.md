# Corpus format standard applied: three datasets added, two restandardized

Date: 2026-11-01. Branch `feat/corpus-realistic-tier` (extends the
realistic-tier record in `2026-11-01-corpus-realistic-tier.md`).

## The standard

A bundled dataset ships as a CSV whose file name describes its
content and provenance, whose columns are fully descriptive, and
whose coded categorical variables are mapped to the natural labels
given in the dataset's own documentation. Every transformation is
logged in the dataset's `prepare.py` and `ATTRIBUTION.md`. The
runner now reads an optional per-entry `file` field (default
`data.csv`) so bundled files can carry descriptive names.

## Added

**Absenteeism at Work** (UCI 445, CC BY 4.0) —
`absenteeism-at-work_uci-445.csv`, 740 absence records over 36
employees. Reason codes mapped to the page's ICD category names,
weekdays/seasons/education mapped from their documented codes.
Expectations 4/4: `employee_id` determines the person's stable
attributes — the functional pass reports employee_id -> education,
-> social_drinker and -> distance_from_residence_to_work_km, each
holding on 99.86% of rows (exactly 1 violation in 740; the same
rate holds for age, body_mass_index, height, number_of_children,
service_time and weight). Profile time 2.3 s.

**South German Credit** (UCI 522, CC BY 4.0) —
`south-german-credit_uci-522.csv`, 1,000 credits from 1973–1975.
The served `.asc` file (abbreviated German column names) is
converted using the page's variable names and the dataset's own
codetable labels. Expectations 3/3: `foreign_worker` value
concentration (96.3% "no"), plus documented cases for the
stratified 700 good / 300 bad sample composition and the
`personal_status_sex` conflation (code 2 covers female non-single
and male single; sex is not recoverable). Profile time 0.5 s.

**Incident Management Process Enriched Event Log** (UCI 498,
CC BY 4.0) — `servicenow-incident-event-log_uci-498.csv`,
141,712 events over 24,918 incidents; at 44.6 MB it would be the
corpus's largest bundled file. It enters as test-only for one
mechanical reason: the prepared file exceeds the blob size the
GitHub API accepts, so it cannot be published from this
environment; the registry entry carries its sha256 and
expectations, the local corpus run verifies it (below), and
committing the file from any local clone turns the entry bundled
with no other change. ServiceNow-internal names made descriptive,
the source's `?` missing symbol decoded (18 columns, up to 99.98%
of rows), timestamps normalized to ISO 8601, rank prefixes
stripped from impact/urgency/priority. Expectations 3/3: the
functional pass recovers `impact, urgency -> priority` with
0 violations in 141,712 rows — the priority matrix the dataset
page describes as calculated. Also measured: `subcategory` does
not determine `category` (49 subcategories span multiple
categories), and `incident_state` carries a residual `-100` code
on 5 rows. Profile time 16.5 s.

## Restandardized

**Taiwanese Bankruptcy** (UCI 572): file renamed to
`taiwanese-bankruptcy_uci-572.csv`; the source typo `Working
capitcal Turnover Rate` corrected; the currency glyph in the
three "(Yuan ¥)" column names spelled out as "(Yuan)".
Expectations unchanged in substance, 4/4 on the renamed file.

**Bank Marketing** (UCI 222): file renamed to
`bank-marketing_uci-222_bank-full.csv`; columns renamed to fully
descriptive names (`pdays` -> `days_since_previous_campaign_contact`,
`poutcome` -> `previous_campaign_outcome`, `balance` ->
`average_yearly_balance_euros`, ...); the file's missing-value
marker — the literal string `"NaN"` — decoded to null.

The decoding has a measured detector cost, recorded rather than
worked around. On the string form, two expectations held that
were artifacts of the marker being visible as a value: the
`month = jun -> contact = "NaN"` conditional rule and the
matching (contact, month) hotspot. Both are retired. The
never-contacted structure itself — `days_since_previous_campaign_contact`
= -1 on 36,954 rows, `previous_campaign_outcome` missing on
36,959, the sets differing on 5 rows — was reported exactly as a
conditional rule on the string form and is reported by no
detector on the decoded file. It is now pinned as a known-miss
tripwire: if a missingness-aware pass starts reporting it, the
expectation fails and the case is promoted. The registry entry
carries the measurements. Expectations 3/3.

## Verification

Per-dataset runs on this branch, then a full corpus run with the
incident file present locally: **20 ok, 0 failures, 10 skipped**
(test-only datasets without local files), 30 registry entries,
zero expectation failures. In a fresh clone the incident entry
skips until its file is committed, so the run reads 19 ok /
11 skipped. Run record: `benchmarks/corpus/runs/` (2026-11-01).

## Remaining audit backlog

Bundled datasets that predate the standard still ship as
`data.csv` with source column names — the sample databases,
soybean, thyroid, abalone and the other classic-tier sets. Their
registry expectations and the published case analyses reference
the source names, so restandardizing them is a per-dataset change
with a rename mapping, not a sweep; it is deliberately not part of
this change.
