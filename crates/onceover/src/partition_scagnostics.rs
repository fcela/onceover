use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use onceover_core::{extract, TypedCol, TypedTable};
use onceover_mines::partition_scagnostics::{
    run_sampled_with_config, Input, Objective, Rule, ScanPlan, SearchConfig, DEFAULT_ROW_CAP,
};
use onceover_mines::ModelBasis;
use onceover_report::{
    Category, Evidence, EvidenceBasis, Finding, Method, PassInfo, Sampling, Specification, Subject,
};
use polars::prelude::*;
use rustc_hash::FxHashSet;
use serde_json::json;

pub use onceover_mines::partition_scagnostics::{default_metrics, Metric, ResultSet};

use crate::config::Budget;
use crate::pipeline::{PreparedFrame, RunState};

const SAMPLE_ORDER_SALT: u64 = 0x5c46_0001;
const MIN_AUTO_MEASURE_DISTINCT: usize = 8;

/// Pipeline pass integrated with the frame model, content-keyed sampling,
/// identity deduplication, budget tiers, and cooperative deadline checks.
pub(crate) fn profile_pass(
    state: &mut RunState<'_>,
    table: &TypedTable<'_>,
    prep: &PreparedFrame,
    model_basis: &ModelBasis,
) -> (Vec<Finding>, PassInfo) {
    let mut cfg = match state.cfg.budget {
        Budget::Quick => SearchConfig {
            row_cap: 2_048,
            pair_cap: 16,
            descriptor_cap: 64,
            rule_cap: 96,
            deadline: state.pass_deadline,
        },
        Budget::Standard => SearchConfig {
            row_cap: 4_096,
            pair_cap: 32,
            descriptor_cap: 96,
            rule_cap: 160,
            deadline: state.pass_deadline,
        },
        Budget::Deep => SearchConfig {
            row_cap: 8_192,
            pair_cap: 64,
            descriptor_cap: 128,
            rule_cap: 256,
            deadline: state.pass_deadline,
        },
    };
    if let Some(rows) = state.cfg.sample_rows {
        cfg.row_cap = rows.clamp(96, 16_384);
    }
    if let Some(pairs) = state.cfg.max_pairs {
        cfg.pair_cap = pairs.clamp(1, 128);
    }

    // Build exclusion set: scaffold exclusions + non-measure/non-descriptor roles
    // + non-representative members of exact identity groups (e.g. temp/atemp or
    // total = a + b). When `identities` was not enabled in this run (e.g.
    // `detectors=["partition_scagnostics"]`), run a lightweight exact multi-term
    // identity check so tautological arithmetic sums are still excluded.
    let mut identity_groups: Vec<Vec<usize>> = model_basis.identity_groups.clone();
    if identity_groups.is_empty() && !state.enabled("identities") {
        let (mt_f, _) = onceover_mines::multiterm_identities(table, 256, &prep.scaffold_excluded);
        for f in &mt_f {
            let exact = f.evidence.iter().all(|e| {
                e.metrics
                    .get("violations")
                    .and_then(|v| v.as_f64())
                    .is_none_or(|v| v == 0.0)
            });
            if !exact {
                continue;
            }
            let mut cols: Vec<usize> = f
                .subject
                .columns
                .iter()
                .filter_map(|c| table.names.iter().position(|n| n == c))
                .collect();
            cols.sort_unstable();
            cols.dedup();
            if cols.len() >= 2 {
                identity_groups.push(cols);
            }
        }
    }
    let mut excluded: FxHashSet<usize> = prep.scaffold_excluded.iter().copied().collect();
    for group in &identity_groups {
        if group.len() > 1 {
            // Keep canonical first column by name so column order never changes selection.
            let keep = group
                .iter()
                .copied()
                .min_by(|&a, &b| table.names[a].cmp(&table.names[b]));
            for &idx in group {
                if Some(idx) != keep {
                    excluded.insert(idx);
                }
            }
        }
    }

    let mut measures = BTreeSet::new();
    let mut ordered = BTreeSet::new();
    let mut categorical = BTreeSet::new();
    let mut col_idx: BTreeMap<String, usize> = BTreeMap::new();
    // Salience metadata per eligible measure: (is_float, distinct_count).
    let mut measure_meta: BTreeMap<String, (bool, usize)> = BTreeMap::new();

    for (idx, col) in table.cols.iter().enumerate() {
        if excluded.contains(&idx) {
            continue;
        }
        let name = &table.names[idx];
        let role = prep.frame.roles.get(idx).and_then(|r| r.as_deref());
        if matches!(
            role,
            Some("identifier" | "row_number" | "timestamp" | "timestamp_part" | "text")
        ) {
            continue;
        }
        let distinct = state.columns.get(name).map(|c| c.distinct).unwrap_or(0);
        if distinct < 2 {
            continue;
        }
        col_idx.insert(name.clone(), idx);
        if col.is_numeric() {
            let is_low_card_code =
                distinct <= 12 && matches!(role, Some("flag" | "dimension")) && !is_float_col(col);
            if is_low_card_code {
                // Integer-coded category/flag (e.g. season, workingday): use as exact Levels descriptor.
                categorical.insert(name.clone());
            } else {
                if distinct >= MIN_AUTO_MEASURE_DISTINCT {
                    measures.insert(name.clone());
                    measure_meta.insert(name.clone(), (is_float_col(col), distinct));
                }
                ordered.insert(name.clone());
            }
        } else if matches!(
            col,
            TypedCol::Str { .. } | TypedCol::StrOwned { .. } | TypedCol::Bool { .. }
        ) {
            categorical.insert(name.clone());
        }
    }

    // Build salience-prioritized, layout-invariant pair order:
    // target-involving pairs first, then continuous float pairs by cardinality.
    let target = state.cfg.target.as_deref();
    let mut ranked_pairs: Vec<([String; 2], bool, bool, usize)> = Vec::new();
    let mnames: Vec<&String> = measures.iter().collect();
    for (i, a) in mnames.iter().enumerate() {
        let (fa, da) = measure_meta[*a];
        for b in &mnames[i + 1..] {
            let (fb, db) = measure_meta[*b];
            let has_target = target.is_some_and(|t| t == a.as_str() || t == b.as_str());
            let both_float = fa && fb;
            ranked_pairs.push((
                [(*a).clone(), (*b).clone()],
                has_target,
                both_float,
                da.min(db),
            ));
        }
    }
    ranked_pairs.sort_by(|x, y| {
        y.1.cmp(&x.1)
            .then_with(|| y.2.cmp(&x.2))
            .then_with(|| Reverse(x.3).cmp(&Reverse(y.3)))
            .then_with(|| x.0.cmp(&y.0))
    });
    let preferred_pairs: Vec<[String; 2]> = ranked_pairs.into_iter().map(|(p, ..)| p).collect();

    let plan = match ScanPlan::with_config(
        &measures,
        &categorical,
        &ordered,
        None,
        Some(&preferred_pairs),
        &cfg,
    ) {
        Ok(plan) => plan,
        Err(reason) => return (Vec::new(), PassInfo::na(reason)),
    };

    let rows = content_sampled_rows(table, cfg.row_cap, 0);
    let input = build_input_from_table(table, &col_idx, &plan, &rows, default_metrics(), 0);
    let result = match run_sampled_with_config(&input, plan, table.n, &cfg) {
        Ok(result) => result,
        Err(reason) => return (Vec::new(), PassInfo::na(reason)),
    };

    let c = result.coverage;
    if c.deadline_stopped {
        state.deadline_hit = true;
    }
    let detail = format!(
        "{} of {} rows sampled (content-keyed), {} of {} numeric pairs, {} of {} descriptors, {} of {} rules, {} supported pair partitions, {} holdout candidates, {} permutations, {} findings{}",
        c.sampled_rows,
        c.input_rows,
        c.examined_pairs,
        c.pair_total,
        c.examined_descriptors,
        c.descriptors_total,
        c.examined_rules,
        c.rule_total,
        c.scored_partitions,
        c.validation_family,
        c.permutations_run,
        result.findings.len(),
        if c.deadline_stopped {
            " (stopped at deadline)"
        } else {
            ""
        },
    );
    let coverage = if c.capped {
        PassInfo::capped(detail)
    } else if c.scored_partitions == 0 {
        PassInfo::na(detail)
    } else {
        PassInfo::ran(detail)
    };

    let findings = result
        .findings
        .into_iter()
        .map(|hit| finding_from_hit(hit, c.input_rows, c.sampled_rows))
        .collect();
    (findings, coverage)
}

/// Search for scatterplot shapes that strengthen within observed groups.
pub fn partition_scagnostics(
    df: &DataFrame,
    pairs: Option<&[[String; 2]]>,
    partition_by: Option<&[String]>,
    metrics: Vec<Metric>,
    seed: u64,
) -> Result<ResultSet, String> {
    let explicit: Option<BTreeSet<&str>> =
        partition_by.map(|names| names.iter().map(String::as_str).collect());
    if let Some(names) = partition_by {
        for name in names {
            let column = df
                .column(name)
                .map_err(|_| format!("unknown partition column {name:?}"))?;
            if !column.dtype().is_numeric() && !categorical(column.dtype()) {
                return Err(format!(
                    "unsupported dtype for partition column {name:?}: {}",
                    column.dtype()
                ));
            }
        }
    }
    let table = extract(df);
    let mut measures = BTreeSet::new();
    let mut ordered = BTreeSet::new();
    let mut nominal = BTreeSet::new();
    let mut col_idx = BTreeMap::new();

    for (idx, column) in df.columns().iter().enumerate() {
        let name = column.name().as_str();
        let context = explicit.as_ref().is_none_or(|names| names.contains(name));
        col_idx.insert(name.to_owned(), idx);
        if column.dtype().is_numeric() {
            measures.insert(name.to_owned());
            if context {
                ordered.insert(name.to_owned());
            }
        } else if context && categorical(column.dtype()) {
            nominal.insert(name.to_owned());
        }
    }

    let cfg = SearchConfig {
        row_cap: DEFAULT_ROW_CAP,
        ..SearchConfig::default()
    };
    let plan = ScanPlan::with_config(&measures, &nominal, &ordered, pairs, None, &cfg)?;
    let rows = content_sampled_rows(&table, cfg.row_cap, seed);
    let input = build_input_from_table(&table, &col_idx, &plan, &rows, metrics, seed);
    run_sampled_with_config(&input, plan, table.n, &cfg)
}

fn content_sampled_rows(table: &TypedTable<'_>, row_cap: usize, seed: u64) -> Vec<u32> {
    let keys = table.row_keys();
    let sampled = keys.sample(row_cap);
    keys.order_from(&sampled, seed ^ SAMPLE_ORDER_SALT)
}

fn build_input_from_table(
    table: &TypedTable<'_>,
    col_idx: &BTreeMap<String, usize>,
    plan: &ScanPlan,
    rows: &[u32],
    metrics: Vec<Metric>,
    seed: u64,
) -> Input {
    let numeric = |name: &str| -> (String, Vec<Option<f64>>) {
        let col = &table.cols[col_idx[name]];
        let vals = rows.iter().map(|&r| col.numeric_at(r as usize)).collect();
        (name.to_owned(), vals)
    };
    let categorical = |name: &str| -> (String, Vec<Option<String>>) {
        let col = &table.cols[col_idx[name]];
        let vals = rows
            .iter()
            .map(|&r| categorical_cell(col, r as usize))
            .collect();
        (name.to_owned(), vals)
    };

    Input {
        measures: plan
            .measure_names()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(numeric)
            .collect(),
        ordered: plan.ordered_names().map(numeric).collect(),
        categorical: plan.categorical_names().map(categorical).collect(),
        metrics,
        seed,
        ..Input::default()
    }
}

fn categorical_cell(col: &TypedCol<'_>, row: usize) -> Option<String> {
    match col {
        TypedCol::Str { vals } => vals[row].map(str::to_owned),
        TypedCol::StrOwned { vals } => vals[row].clone(),
        TypedCol::Bool { vals } => vals[row].map(|v| v.to_string()),
        TypedCol::I64 { vals } => vals.at(row).map(|v| v.to_string()),
        TypedCol::U64 { vals } => vals.at(row).map(|v| v.to_string()),
        TypedCol::F64 { vals } => vals.at(row).map(onceover_mines::fmt_float),
        TypedCol::Decimal { vals, .. } => vals[row].map(|_| col.render_cell(row)),
        _ => None,
    }
}

fn is_float_col(col: &TypedCol<'_>) -> bool {
    matches!(col, TypedCol::F64 { .. } | TypedCol::Decimal { .. })
}

fn finding_from_hit(
    hit: onceover_mines::partition_scagnostics::Finding,
    input_rows: usize,
    sampled_rows: usize,
) -> Finding {
    let (context, rule_phrase) = match &hit.rule {
        Rule::Levels { column, levels } if levels.len() == 1 => (
            vec![column.clone()],
            format!("{column} = {:?} versus the remainder", levels[0]),
        ),
        Rule::Levels { column, .. } => (vec![column.clone()], format!("levels of {column}")),
        Rule::Threshold { column, value } => (
            vec![column.clone()],
            format!(
                "{column} ≤ {} versus the remainder",
                onceover_mines::fmt_float(*value)
            ),
        ),
        Rule::Cross { columns, .. } => (
            columns.to_vec(),
            format!("the {} × {} groups", columns[0], columns[1]),
        ),
    };
    let [x, y] = &hit.columns;
    let pooled = hit.validation.pooled;
    let gain = hit.validation.gain;

    let claim = match (hit.metric, hit.objective) {
        (Metric::Monotonic, Objective::PartitionGain) => {
            let parts = hit
                .validation
                .groups
                .iter()
                .filter_map(|g| {
                    let rho = g.shape.rho?;
                    Some(match &g.label {
                        Some(lbl) => format!("{lbl} {rho:+.2}"),
                        None => format!("{rho:+.2}"),
                    })
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{x} × {y} is more monotonic across {rule_phrase}. Held-out Spearman ρ is {:+.2} pooled; within groups: {parts}.",
                pooled.rho.unwrap_or_default()
            )
        }
        (Metric::Monotonic, Objective::StandoutSubgroup) => {
            let gi = hit.standout_group.unwrap_or(0);
            let g = &hit.validation.groups[gi];
            let lbl = g.label.as_deref().unwrap_or("subgroup");
            format!(
                "{x} × {y} is strongly monotonic in {lbl} ({rule_phrase}). Held-out Spearman ρ is {:+.2} in {lbl} (n={}) versus {:+.2} pooled.",
                g.shape.rho.unwrap_or_default(),
                g.rows,
                pooled.rho.unwrap_or_default()
            )
        }
        (metric, Objective::PartitionGain) => {
            let noun = metric_noun(metric);
            let parts = hit
                .validation
                .groups
                .iter()
                .map(|g| match &g.label {
                    Some(lbl) => format!("{lbl} {:.2}", g.shape.score),
                    None => format!("{:.2}", g.shape.score),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{x} × {y} has stronger {noun} across {rule_phrase}. Held-out score rises from {:.2} pooled to {:.2} weighted ({parts}).",
                pooled.score,
                pooled.score + gain
            )
        }
        (metric, Objective::StandoutSubgroup) => {
            let noun = metric_noun(metric);
            let gi = hit.standout_group.unwrap_or(0);
            let g = &hit.validation.groups[gi];
            let lbl = g.label.as_deref().unwrap_or("subgroup");
            format!(
                "{x} × {y} shows strong {noun} in {lbl} ({rule_phrase}). Held-out score is {:.2} in {lbl} (n={}) versus {:.2} pooled.",
                g.shape.score,
                g.rows,
                pooled.score
            )
        }
    };

    let train_rows: usize = hit.discovery.groups.iter().map(|g| g.rows).sum();
    let holdout_rows: usize = hit.validation.groups.iter().map(|g| g.rows).sum();
    let mut spec = Specification::default();
    spec.definition.insert("pair".into(), json!(hit.columns));
    spec.definition.insert("metric".into(), json!(hit.metric));
    spec.definition
        .insert("objective".into(), json!(hit.objective));
    spec.definition.insert("partition".into(), json!(hit.rule));

    let mut evidence = Evidence::new(
        Method::new(
            "partition_scagnostics",
            "partitioned_scatterplot_diagnostics",
            "Score maximization on content-keyed discovery rows with fixed-rule holdout validation, range-restriction check, and Bonferroni-adjusted label permutations",
        ),
        EvidenceBasis::Estimated,
        [
            ("partition_scagnostics".into(), json!(hit)),
            ("n_train".into(), json!(train_rows)),
            ("n_holdout".into(), json!(holdout_rows)),
            ("population_rows".into(), json!(input_rows)),
            ("sampled".into(), json!(sampled_rows < input_rows)),
        ]
        .into_iter()
        .collect(),
    );
    evidence.scope.missing_values =
        "Rows missing either plotted coordinate or carrying a non-finite value are excluded; missing context takes the remainder branch"
            .into();
    evidence.scope.sampling = if sampled_rows < input_rows {
        Sampling::Sample
    } else {
        Sampling::Full
    };

    let score = match hit.objective {
        Objective::PartitionGain => (0.45 + gain.min(1.0) * 0.47).clamp(0.45, 0.92),
        Objective::StandoutSubgroup => (0.42 + gain.min(1.0) * 0.44).clamp(0.42, 0.86),
    };
    Finding::from_statement(
        Category::Associations,
        Some("partitioned_shape".into()),
        Subject::columns(hit.columns.iter().cloned().chain(context)),
        spec,
        claim,
        evidence,
        score,
        None,
    )
}

fn metric_noun(metric: Metric) -> &'static str {
    match metric {
        Metric::Monotonic => "monotonicity",
        Metric::Functional => "functional curve structure (nonparametric R²)",
        Metric::BinnedStriated => "striation",
        Metric::BinnedStringy => "curved 1D structure (stringiness)",
        Metric::BinnedClumpy => "clustered structure (clumpiness)",
    }
}

fn categorical(dtype: &DataType) -> bool {
    matches!(
        dtype,
        DataType::String | DataType::Boolean | DataType::Categorical(..) | DataType::Enum(..)
    )
}
