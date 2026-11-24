#!/usr/bin/env python3
"""Generate the capability-map block of docs/book/usage/capability_map.ipynb.

The book's capability map answers the reader's first question —
what kinds of things can this library find in my data? — grouped
by the question each kind of statement answers, not by detector
or schema category. The map's tables sit between
<!-- generated:capability-map --> markers in one markdown cell of
the notebook and are rendered here from the installed library
(`onceover.taxonomy()` and `onceover.available_detectors()`), the
way scripts/findings_doc.py renders the registry-owned blocks of
docs/findings.md.

The editorial content — which question a type answers, the
one-line statement of what it says, one example statement, and
the detector whose pass principally produces it — is declared in
ENTRIES below. The render fails unless ENTRIES names every
registered type exactly once and every detector it names exists,
so a type added, removed, or renamed in the taxonomy breaks the
render until its map entry is written. Whether a kind runs by
default or only on request is not editorial: it is read from the
named detector's registry membership and trigger at render time,
so a detector moving between the default set and the on-request
set re-renders the map.

Run after any detector, taxonomy, or map change:

    python scripts/capability_map.py

With --check, nothing is written and the exit status is non-zero
when the committed notebook differs from a fresh render; the
freshness test in tests/test_capability_map.py runs the same
comparison.
"""
import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
try:
    import onceover  # noqa: E402  the installed package, with its built extension
except ImportError:
    # Fall back to the source tree, which imports only after an
    # in-place build (maturin develop) has put the extension there.
    sys.path.insert(0, str(ROOT / "python"))
    import onceover

NOTEBOOK = ROOT / "docs" / "book" / "usage" / "capability_map.ipynb"
MARKER_START = "<!-- generated:capability-map -->"
MARKER_END = "<!-- /generated:capability-map -->"

# The questions, in the order the page presents them. The lead is
# one plain sentence per question, printed above its table.
QUESTIONS = [
    (
        "columns",
        "What's in each column?",
        "What each column holds, how its values are recorded, and what "
        "role it plays in the table.",
    ),
    (
        "duplicated",
        "What's duplicated, or saying the same thing?",
        "Rows and columns that repeat information the table already "
        "carries somewhere else.",
    ),
    (
        "goes_with",
        "What goes with what?",
        "Relationships between columns: associations, contrasts, "
        "predictions, and rules.",
    ),
    (
        "concentrates",
        "Where does it concentrate?",
        "Where rows, entities, or a declared weight pile up beyond "
        "what the marginals predict.",
    ),
    (
        "changed",
        "What changed along an order?",
        "How a series behaves along the frame's time axis, a declared "
        "ordered axis, or the row sequence itself.",
    ),
    (
        "missing",
        "What's missing that should be there?",
        "Absent values, absent steps, and absent combinations, each "
        "stated against the reference that makes it an absence.",
    ),
    (
        "breaks_pattern",
        "What breaks the pattern?",
        "Values, rows, and cells that contradict the pattern their "
        "context establishes.",
    ),
    (
        "outcome",
        "What does the designated outcome do?",
        "Statements about one column the caller designates with "
        "`target=`: its own distribution, how it differs across "
        "groups, what restates it, and where a model of it fails.",
    ),
]

# One entry per registered finding type: the question it answers,
# the detector whose pass principally produces it, what the
# statement says in one line, and one example statement in the
# library's own wording. The render checks this table against the
# taxonomy both ways, so it cannot drift silently.
ENTRIES = {
    # What's in each column?
    "categorical_role": {
        "question": "columns",
        "detector": "column_profile",
        "states": "Suggests the column is a category, from its distinct count and repetition.",
        "example": "numeric column status_code behaves like a category (2 distinct values)",
    },
    "identifier_role": {
        "question": "columns",
        "detector": "column_profile",
        "states": "Suggests the column identifies rows, from its near-uniqueness.",
        "example": "column record_id looks like a unique identifier (2500 distinct of 2500 values)",
    },
    "long_format": {
        "question": "columns",
        "detector": "pivot",
        "states": "The table is in long format: one column's levels are attributes and another's values are the measures, so pivoting gives one column per level.",
        "example": "frame is in long format: (person) × measure_name → value; pivoting on measure_name gives one column per level (age_years, height_cm, weight_kg)",
    },
    "format_variation": {
        "question": "columns",
        "detector": "column_profile",
        "states": "The same field is recorded in more than one format.",
        "example": "column job mixes value formats (8: 41% 'aaaaaaaaaa' vs 22% 'aaaa-aaaaaa')",
    },
    "content_anomaly": {
        "question": "columns",
        "detector": "content_anomalies",
        "states": "Text holds something structured or off: numbers or dates stored as text, labels differing only in case or spacing, likely misspellings, stray signs, zero inflation, separated modes, and similar kinds, each named in the evidence.",
        "example": "column days_since_previous_campaign_contact has two separated modes, near 4.14 and 179, with few values around 35",
    },
    "heaping": {
        "question": "columns",
        "detector": "digit_fingerprints",
        "states": "Values pile on round multiples beyond the local baseline, a fingerprint of rounding in recording.",
        "example": "column value heaps on multiples of 1000: 100.0% of nonzero values are divisible by 1000 against the 0.1% a smooth recording implies (z = 3395.8); zeros are 0.0% of the column and are counted separately",
    },
    "digit_preference": {
        "question": "columns",
        "detector": "digit_fingerprints",
        "states": "Terminal digits occur unevenly against the uniform baseline.",
        "example": "column casual has a terminal-digit preference: digit 0 ends 16.2% of values against 10% under uniformity (chi-square 1228.8 on 9 df, bar 44.0)",
    },
    "benford_deviation": {
        "question": "columns",
        "detector": "digit_fingerprints",
        "states": "First digits depart from Benford's distribution on a column where that distribution applies.",
        "example": "column last_contact_duration_seconds deviates from Benford's first-digit law: mean absolute deviation 0.016 (nonconformity boundary 0.012), chi-square 1075.4 on 8 df (bar 34); digit 1 leads 35.1% of values against Benford's 30.1%",
    },
    "boundary_mass": {
        "question": "columns",
        "detector": "boundary_mass",
        "states": "Values pile at the column's observed minimum or maximum beyond the local baseline — the shape of a recording cap or floor, stated as an observation, not a conclusion.",
        "example": "column capital_gain piles 244 values (0.5% of 48842 evaluated rows) at its maximum, 99999: the nearest lower value, 41310, occurs 3 times, and the median count among the 10 nearest lower values is 7 — the pile is 34.9x that local baseline; the rest of the column spans 0 to 41310 over 122 distinct values and is 0 on 91.7% of evaluated rows",
    },
    # What's duplicated, or saying the same thing?
    "duplicate_rows": {
        "question": "duplicated",
        "detector": "row_duplicates",
        "states": "Rows repeat under the stated comparison and missing-value policy.",
        "example": "52 rows repeat earlier values across the compared columns (49 groups, largest 3)",
    },
    "near_duplicate_rows": {
        "question": "duplicated",
        "detector": "row_consistency",
        "states": "Rows repeat another row on every column but one, beyond what chance coincidence on the compared columns explains.",
        "example": "95 rows repeat another row on every column except age (0.19% of 48842 rows); 2.22 expected by chance (42.8x the chance baseline)",
    },
    "uniqueness": {
        "question": "duplicated",
        "detector": "keys",
        "states": "A column, or a set of columns together, has no repeated values: it can serve as a key.",
        "example": "the grain is 51 state × 609 date: 31,059 rows, every combination exactly once — (state, date) jointly identify rows",
    },
    "duplicate_encoding": {
        "question": "duplicated",
        "detector": "duplicate_encodings",
        "states": "Two columns encode the same values under a mapping, or one is a rollup of the other (continents of countries, categories of items).",
        "example": "dteday rolls up to mnth: each of its 731 levels maps to one of 12 levels, on all 17379 rows",
    },
    "functional": {
        "question": "duplicated",
        "detector": "functional_dependencies",
        "states": "The inputs' values determine the target's value through a mapping, up to the recorded violations.",
        "example": "amount, other_installment_plans → other_debtors holds on 99.50% of rows (5 violations in 1000 rows)",
    },
    "affine": {
        "question": "duplicated",
        "detector": "identities",
        "states": "One column is a linear rescaling of another, holding within tolerance on the evaluated rows.",
        "example": "scaled = 2.500000·width + 7.000000 within tolerance on all 3000 non-missing row pairs",
    },
    "arithmetic_identity": {
        "question": "duplicated",
        "detector": "identities",
        "states": "A column equals an exact combination of others: a sum, a difference, a product, a ratio, a fitted linear combination, or a decomposition into four to eight parts.",
        "example": "cnt = casual + registered within tolerance on all 17379 non-missing rows",
    },
    # What goes with what?
    "statistical": {
        "question": "goes_with",
        "detector": "numeric_dependence",
        "states": "Two columns are associated, measured and stated without claiming a particular mapping.",
        "example": "temp and atemp show numeric association (maximum measured effect 0.990)",
    },
    "conditioned_association": {
        "question": "goes_with",
        "detector": "numeric_dependence",
        "states": "A pooled association reverses direction, or vanishes, within every usable level of a third column — a Simpson pattern.",
        "example": "The association between x and y reverses within every level of group: pooled Pearson r -0.714, within-level Pearson r from +0.946 to +0.950 (combined +0.948). The pooled direction is an artefact of the group mix.",
    },
    "partitioned_shape": {
        "question": "goes_with",
        "detector": "partition_scagnostics",
        "states": "A partition on other columns reveals stronger monotonicity or striation in a scatterplot, confirmed on held-out rows.",
        "example": "x × y is more monotonic across protocol = \"A\" versus the remainder. Held-out Spearman ρ is -0.03 pooled and +1.00, -1.00 within groups.",
    },
    "period_association": {
        "question": "goes_with",
        "detector": "period_associations",
        "states": "A pair's association is stated separately in two periods of the time axis because its strength changes between them.",
        "example": "correlation between x and y ranges from -1.00 in 2021 (n = 400) to 1.00 in 2020 (n = 400) across 2 periods; pooled correlation is -0.01",
    },
    "level_contrast": {
        "question": "goes_with",
        "detector": "level_contrasts",
        "states": "One level of a categorical column sits apart from the pooled rest on a numeric measure: its location, its scale, both, or a near-constant multiple of the rest's distribution.",
        "example": "casual for hr = '1' is less variable than a typical other level: IQR 8 against 34 (0.235294x), medians 4 against 18 pooled",
    },
    "group_contrast": {
        "question": "goes_with",
        "detector": "group_contrast",
        "states": "A subset of rows declared with the `contrast` hint differs from the rest of the table on a column.",
        "example": "score averages 11.985381 in the group (arm = 'treated'; 400 rows) against 9.936272 in the other 400 rows — a difference of 2.049109, 0.998256 pooled standard deviations (medians 12.058314 against 9.919412)",
    },
    "slice_shift": {
        "question": "goes_with",
        "detector": "slice_shifts",
        "states": "A target column shifts under a searched conjunction of conditions. The distribution_slices detector compares full numeric and categorical distributions against the remaining rows and confirms the contrast on held-out rows.",
        "example": "within education = 'Masters' (1050 rows), 100.0% of rows have education_num = '14', against 5.2% overall (19.047619x)",
    },
    "predictive": {
        "question": "goes_with",
        "detector": "predictability",
        "states": "A column is predictable from driver columns under recorded validation, with each driver's share of the model's gain. Decision-table, interaction, and target-shape forms of this kind come from on-request passes.",
        "example": "age is predictable from marital_status (49% of gain), relationship (27% of gain), hours_per_week (9% of gain) (holdout R² = 0.44 using only these drivers; 0.46 using all inputs)",
    },
    "if_then": {
        "question": "goes_with",
        "detector": "covering_rules",
        "states": "When the stated conditions hold, the outcome follows at the stated rate, against its baseline rate.",
        "example": "When relationship = \"Wife\", sex = \"Female\" holds in 159 of 159 matching diagnostic rows (100.00%; baseline 33.85%)",
    },
    "order_constraint": {
        "question": "goes_with",
        "detector": "order_constraints",
        "states": "One column is at most another on every evaluated row, or all but a counted few, with the basis for the direction recorded.",
        "example": "Perigee_km <= Apogee_km on all 1163 evaluated rows (0 violations; the two are equal on 17 of them)",
    },
    # Where does it concentrate?
    "value_concentration": {
        "question": "concentrates",
        "detector": "column_profile",
        "states": "One value, or a few values, hold a dominant share of a column — including a column that is constant.",
        "example": "column capital_gain is highly concentrated: 91.7% '0' (effective cardinality 1.82)",
    },
    "modal_share": {
        "question": "concentrates",
        "detector": "row_clusters",
        "states": "Within a cluster of the row partition, one value holds at least 90% of a column's rows.",
        "example": "41 of 41 rows in cluster 0 (rows 0-49) hold arctic at 0 (100% of the cluster) (Greedy deterministic partition; scores are marginal-likelihood values, not posterior probabilities.).",
    },
    "hotspot": {
        "question": "concentrates",
        "detector": "hotspots",
        "states": "A cell of a two- or three-column combination holds far more rows than the marginals predict under independence — or, with a declared weight, far more of the weight than its share of rows.",
        "example": "cell (job = 'retired', contact = 'telephone') holds 192 of 20000 scanned rows (1.0%) against 71.2 expected under independence of job, contact — 2.7x the baseline (z = 14.3, bar 4.5 over 1809 cells examined)",
    },
    "concentration_over_time": {
        "question": "concentrates",
        "detector": "series",
        "states": "In each period of a panel, the top group of entities holds a large share of a measure's total, or that share grows across the axis.",
        "example": "the top 1 of 24 entity (the top 1%) holds 72.9% of each month period's signups (median period; 73.8% in the first period, 71.8% in the last); an equal split over 24 entity would give the top 1 4.2%; the largest in the last period: entity_05 (71.8%), entity_16 (1.5%), entity_12 (1.4%)",
    },
    "outlier_entity": {
        "question": "concentrates",
        "detector": "series",
        "states": "An entity's total of a measure over all periods sits far in the tail of the entity totals.",
        "example": "one entity sits far outside the entity distribution of total signups: entity_05 (total 72005.5453, 59.9x the typical entity); it holds 72.9% of all signups; the typical entity totals 1201.279",
    },
    "entity_groups": {
        "question": "goes_with",
        "detector": "entity_fingerprints",
        "states": "Entities fall into groups by their share of rows on each level of one categorical column; members of one group share a mix, and groups differ from each other.",
        "example": "cust_id fall into 4 groups by their mix of category: 235 cust_id (59%) with 50% on 'fresh' (against 32% overall); 77 cust_id (19%) with 28% on 'beverages' (against 11% overall), 22% on 'snacks' (against 9% overall); 76 cust_id (19%) with 25% on 'household' (against 11% overall), 21% on 'personal_care' (against 9% overall); 12 cust_id (3%) with 47% on 'frozen' (against 9% overall), 45% on 'dairy' (against 12% overall)",
    },
    "entity_continuum": {
        "question": "goes_with",
        "detector": "entity_fingerprints",
        "states": "Entities differ in their mix over one categorical column along one continuous axis, with no separable groups.",
        "example": "cust_id differ in their mix of category along one continuous axis, not in groups: the share of 'fresh' against 'household' varies from cust_id to cust_id without clusters (excess Cramér's V 0.12)",
    },
    "entity_specialists": {
        "question": "concentrates",
        "detector": "entity_fingerprints",
        "states": "A share of entities puts essentially all of its rows on one level of a categorical column, where the overall mix spreads over several levels.",
        "example": "71% of cust_id (283 of 400 with 10 or more rows) concentrate on essentially one location level (effective number of levels 1.5 or less), against 5.0 effective levels in the overall mix",
    },
    "entity_misfits": {
        "question": "breaks_pattern",
        "detector": "entity_fingerprints",
        "states": "An entity's mix over one categorical column is rejected by the pooled profile of every group, so it matches no group.",
        "example": "12 cust_id (3%) match none of the 4 category-mix groups: each one's counts reject its nearest group's profile (chi-square, Bonferroni over 400 cust_id); together they hold 3% of rows; for example C0083, C0086, C0250, C0309, C0395",
    },
    # What changed along an order?
    "series_structure": {
        "question": "changed",
        "detector": "series",
        "states": "A whole series' shape along the axis: a trend, a cycle, persistence, levels that begin or end, shares that shift — and, on a panel grain, the same readings per entity, naming the entities that carry each pattern.",
        "example": "casual repeats every 7 days along dteday: autocorrelation 0.73 at that lag",
    },
    "level_shift": {
        "question": "changed",
        "detector": "breaks",
        "states": "The series' level moves at a located point and stays moved.",
        "example": "sessions level shifts along date between 2023-03-15 and 2023-03-15 (best estimate 2023-03-15): mean 1124.112676 before, 520.512821 after; the training-selected segmentation reduces holdout squared error by 72.5%",
    },
    "trend_change": {
        "question": "changed",
        "detector": "breaks",
        "states": "The series' trend changes at a located point.",
        "example": "sessions trend changes along date between 2023-03-15 and 2023-03-15 (best estimate 2023-03-15): slope 0.068786 before, 1.011293 after; the training-selected two-line model reduces holdout squared error by 40.8%",
    },
    "variance_change": {
        "question": "changed",
        "detector": "breaks",
        "states": "The series' variance changes at a located point.",
        "example": "sessions variance changes along date between 2023-08-11 and 2023-08-23 (best estimate 2023-08-12): variance 87142.27191 before, 22242.56634 after; the training-selected split reduces holdout squared-deviation error by 49.4%",
    },
    "transient_excursion": {
        "question": "changed",
        "detector": "breaks",
        "states": "The series leaves its local baseline for a stretch of periods and returns.",
        "example": "unemployment_rate spikes between 2020-04-01 and 2020-04-01: peak value 18.3 at 2020-04-01, 8.6 robust z from the local baseline",
    },
    "slope_break": {
        "question": "changed",
        "detector": "ordered_axis",
        "states": "The slope of a response's regression on another numeric column, or on the axis itself, changes at a located point.",
        "example": "the slope of hours_per_week on age changes at age = 43: 0.645581 per unit before, -0.602655 per unit after (located range 33 to 51)",
    },
    "regime_contrast": {
        "question": "changed",
        "detector": "ordered_axis",
        "states": "The level over a fixed window against the window before it, the windows anchored at proposed break locations or calendar boundaries and never searched; stated as an episode when the series reverts.",
        "example": "capital_loss averaged 786.826649 over the 6 points from 67, against 148.822802 over the 6 points before it — a regime contrast of +638.003847",
    },
    "spike_contrast": {
        "question": "changed",
        "detector": "ordered_axis",
        "states": "A short window's mean jumps against the same window one seasonal period earlier and reverts afterwards; without reversion the event is a level shift and the level scan owns it.",
        "example": "snow_1h spikes over 2015-12-23 12:00:00 to 2015-12-23 16:00:00: 0.383333 against a preceding-window baseline of 0 (+0.383333), reverting to 0.02 in the following window",
    },
    "step_discontinuity": {
        "question": "changed",
        "detector": "discontinuities",
        "states": "A measure's level jumps across a threshold in another numeric column — a cross-sectional jump, not a change over time.",
        "example": "atemp jumps by 0.084652 where casual crosses 17 (z +16.3 on rows not used to find it)",
    },
    "ordered_partition": {
        "question": "changed",
        "detector": "ordered_partitions",
        "states": "A model-based recursive partition of a numeric response along the axis: the splits where its level or trend changes, with each leaf's level and slope.",
        "example": "visits follows 2 level+trend regimes along date: first split break at 2024-03-01, 1 split in all, level+trend SSE reduced 14.1%",
    },
    "axis_shape": {
        "question": "changed",
        "detector": "ordered_axis",
        "states": "The shape of a numeric response along an ordered axis: monotone, monotone apart from named reversal blocks, reversing, or rising and then flattening at a located knee.",
        "example": "labor_cost rises with total_cost (Spearman rho 0.8275), with reversal blocks against the trend; the largest is at total_cost = 755, where the median moves 45 (0.5 IQR) against the trend",
    },
    "predictor_drift": {
        "question": "changed",
        "detector": "drift",
        "states": "A designated target's predictors are distributed differently in a recent window of a declared time axis than in an earlier one.",
        "example": "the predictors of atemp drift over dteday: split-routing drift score reaches 1141.7 in period 6 (2012-07-05 to 2012-10-03), against a no-drift scale of ~1",
    },
    "joint_distribution_change": {
        "question": "changed",
        "detector": "distribution_change",
        "states": "The measure columns' joint distribution changes at a located split, even when no single measure's level, trend, or variance changes — a covariance change at constant marginals is the case it exists for.",
        "example": "the distribution of unemployment_rate changes along date at 1977-09-01: a random forest distinguishes the rows before the split from the rows after it (classifier gain 81.778898; permutation p = 0.005)",
    },
    "residual_shift": {
        "question": "changed",
        "detector": "regimes",
        "states": "Residuals from a reference model shift for a group or a period, dated against the model.",
        "example": "Rows split into 8 clusters; headline column unemployment_rate shifts by 2.51 between clusters (Greedy deterministic partition; scores are marginal-likelihood values, not posterior probabilities.).",
    },
    "one_way_flag": {
        "question": "changed",
        "detector": "series",
        "states": "A boolean or 0/1 flag on a panel grain switches on within entities and never switches off; where a measure turns positive only after onset, the lag is stated.",
        "example": "yr switches on and never off within hr: 24 of 24 entities show an off-to-on onset and none shows an on-to-off transition over 731 dteday periods",
    },
    "entry_block": {
        "question": "changed",
        "detector": "series",
        "states": "A block of entities first appears in one period of the panel, far above the typical per-period entry count.",
        "example": "20 of 31 arrival_date_day_of_month entities first appear in the reservation_status_date period 2015-01-01 (64.5% of all entities), against a typical entry of 0 per period",
    },
    "exit_block": {
        "question": "changed",
        "detector": "series",
        "states": "A block of entities last appears in one period of the panel, far above the typical per-period exit count.",
        "example": "6 of 12 company entities are last seen in the period_end period 2026-06-30 (50.0% of all entities), against a typical exit of 0 per period",
    },
    "first_activity": {
        "question": "changed",
        "detector": "series",
        "states": "Entities first become active on a measure after their first observed period, with the lag stated as a median and middle half.",
        "example": "signups first becomes active after the first observed period for 6 of 24 entity entities (median lag 81 periods, middle half 79 to 82); 18 start in their first observed period and 0 never become active",
    },
    "column_ordering": {
        "question": "changed",
        "detector": "sortedness",
        "states": "A column's values follow an order within the row sequence — or hold an order except for a late block whose values return to earlier territory, the backfill shape.",
        "example": "column yr is non-decreasing with row order",
    },
    # What's missing that should be there?
    "missing_values": {
        "question": "missing",
        "detector": "missingness",
        "states": "A column is missing on at least 1% of rows, with the pattern behind it when one is measured — a grouping level, a numeric threshold, or a point on the time axis — and blocks of columns missing on the same rows.",
        "example": "previous_campaign_outcome is missing on 36,959 of 45,211 rows (82%); 100% of rows where days_since_previous_campaign_contact <= -1, against 0.1% where days_since_previous_campaign_contact > -1",
    },
    "sentinel_values": {
        "question": "missing",
        "detector": "sentinels",
        "states": "One implausible value repeats in a narrow row window, standing in for missing data; the evidence gives the column's mean with and without it.",
        "example": "column hum uses 0 as a probable missing-data sentinel: 22 occurrences (0.1% of 17379 rows), 18 of them inside rows 1551-1568 (a 18-row window, 0.1% of the column); the value sits outside the rest of the column's bulk [-0.42, 1.68]; excluding it the column mean is 0.628 (vs 0.6272 with it)",
    },
    "predictable_missingness": {
        "question": "missing",
        "detector": "missingness",
        "states": "A column's missingness is predictable from the other columns, with the driver columns and their gain shares named.",
        "example": "Whether occupation is missing is predictable from work_class (73% of gain), age (11% of gain), income (8% of gain): a model of the other columns identifies the missing rows with holdout AUC 1.00 (occupation is missing on 2,809 of 48,842 rows, 5.8%)",
    },
    "gap": {
        "question": "missing",
        "detector": "series",
        "states": "Steps are absent from the time axis, stated against the grid of its modal step.",
        "example": "the time axis date_time is missing 11976 of 52551 steps of 1 hour (22.79%); the longest gap is 7386 steps, between 2014-08-08 01:00:00 and 2015-06-11 20:00:00",
    },
    "sequence_gap": {
        "question": "missing",
        "detector": "expectations",
        "states": "Values are absent from a declared sequence, stated against its declared increment.",
        "example": "column seq is missing 4 values from its expected sequence (step 1 over 1..500)",
    },
    "missing_combinations": {
        "question": "missing",
        "detector": "expectations",
        "states": "Combinations of declared panel dimensions never occur in the table.",
        "example": "declared panel is incomplete: 2 of 48 dimension × period cells are missing (4.2%); store=\"east\" is missing 2",
    },
    "partial_period": {
        "question": "missing",
        "detector": "panel_coverage",
        "states": "One period's volume — its row count, its entity count, or a principal measure's total — falls far below the typical period's.",
        "example": "an interior period (2012-10-29) on the 'dteday' axis holds 1 row against a typical 24 rows per period (4% of typical); 1 entity present against a typical 24 (4% of typical)",
    },
    # What breaks the pattern?
    "row_inconsistency": {
        "question": "breaks_pattern",
        "detector": "row_consistency",
        "states": "Rows far from the line two tightly related columns otherwise follow, or rows where two columns with disjoint usual ranges each hold a value from the other's range.",
        "example": "24 rows sit far from the line atemp = 0.8841 * temp + 0.0372: the other 17355 rows stay within 0.1317 of it",
    },
    "interval_variation": {
        "question": "breaks_pattern",
        "detector": "date_gaps",
        "states": "The spacing between a date column's successive values is irregular.",
        "example": "column ts has irregular spacing (modal 1 days, coefficient of variation 1.29)",
    },
    "extreme_value": {
        "question": "breaks_pattern",
        "detector": "outliers",
        "states": "A value is isolated from the bulk of its column — or from the bulk within one categorical subset — by a wide empty gap, stated as a multiple of the central spread.",
        "example": "last_contact_duration_seconds reaches 4918 — isolated from the rest of the column: 100.00% of values are <= 3881, and the gap of 1037 is 2.1x the central spread (p90-p10 = 489.1) of the remainder",
    },
    "categorical_outlier": {
        "question": "breaks_pattern",
        "detector": "outliers",
        "states": "A categorical value contradicts its context's near-universal pattern, on at most a few rows, each listed.",
        "example": "within education = Some-college and relationship = Husband (3658 rows), marital_status is Married-civ-spouse in 99.9% of rows — 3 rows break the pattern: row 3787 (marital_status = Married-AF-spouse), row 39624 (marital_status = Married-AF-spouse), row 48118 (marital_status = Married-AF-spouse)",
    },
    "order_violation": {
        "question": "breaks_pattern",
        "detector": "order_constraints",
        "states": "An order between two columns fails in bulk and systematically, while the reverse direction fails at least twice as much — the shape of a corrupted part-against-whole or a swapped pair.",
        "example": "atemp <= temp is violated on 4445 of 17379 evaluated rows (25.6%), while the reverse order is violated on 12934 rows (74.4%): the worst violation is at row 531, where atemp = 0.1212 against temp = 0.02",
    },
    "spike_cells": {
        "question": "breaks_pattern",
        "detector": "row_clusters",
        "states": "Individual cells sit far below their row cluster's predictive density for the column, scored leave-one-out.",
        "example": "1 cells in signups have log predictive density below -9.2 nats under their cluster (lowest 361.0 at row 895; Greedy deterministic partition; scores are marginal-likelihood values, not posterior probabilities.).",
    },
    # What does the designated outcome do?
    "outcome_distribution": {
        "question": "outcome",
        "detector": "outcome_account",
        "states": "The designated outcome's own distribution: the share of each level for a categorical outcome, with the declared event's rate when an event is declared; the center, spread, and range for a numeric outcome.",
        "example": "The designated outcome 'cnt' is numeric: median 142, mean 189.463088, middle half 40 to 281, range 1 to 977, over 17379 values.",
    },
    "outcome_contrast": {
        "question": "outcome",
        "detector": "outcome_contrasts",
        "states": "Each grouping level's outcome against the pooled outcome — an event rate or a median — with measured absences of difference stated as nulls.",
        "example": "hr = '8': the median of 'cnt' is 385, against the pooled median 142 (727 values).",
    },
    "leakage_signature": {
        "question": "outcome",
        "detector": "leakage",
        "states": "One column nearly determines the designated target, far beyond the rest of the field — the signature of a column derived from the outcome or recorded after it. The statement names a signature to check before modelling; it does not assert leakage.",
        "example": "'reservation_status' determines the target 'is_canceled' exactly: every value of 'reservation_status' pairs with a single value of 'is_canceled' on the 119390 evaluated rows. A column that restates the target this way is usually derived from the outcome or recorded after it — check it before modelling.",
    },
    "model_failure": {
        "question": "outcome",
        "detector": "model_failures",
        "states": "Slices where a cross-fitted model of the designated target fails: the slice's residuals are biased, or its error is materially larger than the model's overall error.",
        "example": "within casual > 92 (1727 rows), the model of 'cnt' errs more: its mean absolute out-of-fold error there is 24.025331 against 8.673305 over all 17379 modelled rows (2.770032x)",
    },
}


def _runs_phrase(detector):
    """How a kind runs, from its detector's registry entry."""
    membership = detector["membership"]
    trigger = detector["trigger"]
    if membership == "default_set":
        phrase = "By default"
    else:
        phrase = "On request"
    if trigger == "hints":
        phrase += ", with declared hints"
    elif trigger == "target":
        phrase += ", with a designated target"
    elif trigger != "always":
        raise ValueError(f"detector {detector['id']}: unknown trigger {trigger!r}")
    return phrase


def _check_text(type_name, field, text):
    if "|" in text or "\n" in text:
        raise ValueError(
            f"map entry {type_name}: {field} must be one table-cell line "
            "without a '|'"
        )


def render_block(taxonomy=None, detectors=None):
    """The generated capability-map block, markers included."""
    if taxonomy is None:
        taxonomy = onceover.taxonomy()
    if detectors is None:
        detectors = onceover.available_detectors()
    registered = []
    for pattern in taxonomy["patterns"]:
        if pattern["type"] not in registered:
            registered.append(pattern["type"])
    missing = [t for t in registered if t not in ENTRIES]
    extra = [t for t in ENTRIES if t not in registered]
    if missing or extra:
        raise ValueError(
            "the capability map and the taxonomy disagree: "
            f"types with no map entry: {missing}; "
            f"map entries with no registered type: {extra}"
        )
    by_id = {d["id"]: d for d in detectors}
    for type_name, entry in ENTRIES.items():
        if entry["detector"] not in by_id:
            raise ValueError(
                f"map entry {type_name}: detector {entry['detector']!r} "
                "is not in the registry"
            )
        _check_text(type_name, "states", entry["states"])
        _check_text(type_name, "example", entry["example"])
    question_slugs = [slug for slug, _heading, _lead in QUESTIONS]
    for type_name, entry in ENTRIES.items():
        if entry["question"] not in question_slugs:
            raise ValueError(
                f"map entry {type_name}: unknown question {entry['question']!r}"
            )
    lines = [
        MARKER_START,
        f"The library registers {len(registered)} finding types. Each "
        "appears once below, under the question it answers.",
    ]
    for slug, heading, lead in QUESTIONS:
        lines += ["", f"## {heading}", "", lead, ""]
        lines += [
            "| Kind | What the statement says | Example | Runs |",
            "|---|---|---|---|",
        ]
        for type_name in registered:
            entry = ENTRIES[type_name]
            if entry["question"] != slug:
                continue
            runs = _runs_phrase(by_id[entry["detector"]])
            lines.append(
                f"| `{type_name}` | {entry['states']} | "
                f"{entry['example']} | {runs} |"
            )
    lines.append(MARKER_END)
    return "\n".join(lines)


def render(notebook, taxonomy=None, detectors=None):
    """The notebook (parsed JSON) with its map cell regenerated.

    Raises when no markdown cell carries the generated block, so a
    notebook that loses the block fails the freshness test instead
    of passing silently.
    """
    block = render_block(taxonomy, detectors)
    rendered = copy.deepcopy(notebook)
    found = False
    for cell in rendered["cells"]:
        if cell.get("cell_type") != "markdown":
            continue
        source = "".join(cell["source"])
        if MARKER_START not in source:
            continue
        if found:
            raise ValueError("more than one cell carries the capability-map block")
        start = source.index(MARKER_START)
        end = source.index(MARKER_END) + len(MARKER_END)
        cell["source"] = (source[:start] + block + source[end:]).splitlines(
            keepends=True
        )
        found = True
    if not found:
        raise ValueError("no cell carries the capability-map block")
    return rendered


def _load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def _write(path, notebook):
    import nbformat

    node = nbformat.from_dict(notebook)
    nbformat.write(node, path)
    # nbformat.write omits the trailing newline the committed
    # notebooks carry.
    with open(path, "a", encoding="utf-8") as handle:
        handle.write("\n")


def main():
    committed = _load(NOTEBOOK)
    rendered = render(committed)
    if "--check" in sys.argv:
        if rendered != committed:
            print(
                "docs/book/usage/capability_map.ipynb is stale; regenerate "
                "it with python scripts/capability_map.py"
            )
            return 1
        print("docs/book/usage/capability_map.ipynb is fresh")
        return 0
    _write(NOTEBOOK, rendered)
    print(f"wrote {NOTEBOOK}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
