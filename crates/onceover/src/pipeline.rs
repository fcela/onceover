//! The pipeline: run passes in cost order under the budget, assemble the
//! findings ledger, overview and coverage block. Deterministic end to end
//! (unless an opt-in wall-clock deadline fires, which is reported honestly).

use std::collections::BTreeMap;
use std::time::Instant;

use onceover_core::{extract, find_duplicate_rows, CardOut, TypedTable, VKey};
use onceover_deps::{analyze_keys, discover, sortedness, ColInfo, DepsOutcome};
use onceover_mines::{
    analyze_temporal, association_rules, decision_tables, encode, identities, prism_rules,
    profile_regimes, screen_dependence, screen_predictability,
};
use onceover_report::{
    finalize_ledger, ColumnCard, Finding, Overview, PassInfo, ProfileReport, SCHEMA_VERSION,
};
use polars::prelude::DataFrame;
use serde_json::json;

use crate::config::{Budget, ProfileConfig, TierConfig};

/// Profile a DataFrame. Returns the complete, deterministic report.
pub fn profile(df: &DataFrame, cfg: &ProfileConfig) -> ProfileReport {
    let start = Instant::now();
    let tier = TierConfig::for_budget(cfg.budget);
    let mut coverage: BTreeMap<String, PassInfo> = BTreeMap::new();

    let mut deadline_hit = false;
    let check_deadline = |coverage: &mut BTreeMap<String, PassInfo>, name: &str| -> bool {
        if let Some(d) = cfg.deadline {
            if start.elapsed() > d {
                coverage.insert(name.to_string(), PassInfo::skipped("deadline reached"));
                return true;
            }
        }
        false
    };

    // ---------- extraction ----------
    let table = extract(df);
    let unsupported = table
        .dtypes
        .iter()
        .filter(|d| d.as_str() == "other")
        .count();
    coverage.insert(
        "extraction".into(),
        PassInfo::ran(format!(
            "{} rows × {} columns{}",
            table.n,
            table.cols.len(),
            if unsupported > 0 {
                format!(" ({unsupported} unsupported column type(s) opaque to the profiler)")
            } else {
                String::new()
            }
        )),
    );

    // ---------- column cards ----------
    use rayon::prelude::*;
    let cards: Vec<CardOut> = table
        .cols
        .par_iter()
        .enumerate()
        .map(|(i, col)| onceover_core::column_card(&table.names[i], &table.dtypes[i], col))
        .collect();
    coverage.insert(
        "column_cards".into(),
        PassInfo::ran(format!("{} cards (exact)", cards.len())),
    );

    // ColInfo for downstream passes.
    let info: Vec<ColInfo> = cards
        .iter()
        .enumerate()
        .map(|(i, c)| ColInfo {
            name: c.card.name.clone(),
            dtype: table.dtypes[i].clone(),
            role: c.card.role.clone(),
            n: c.card.n,
            null_count: c.card.null_count,
            distinct: c.card.distinct,
            dup_examples: dup_examples(&table, i, c),
        })
        .collect();

    let mut findings: Vec<Finding> = Vec::new();
    let mut columns: BTreeMap<String, ColumnCard> = BTreeMap::new();
    for c in cards {
        columns.insert(c.card.name.clone(), c.card.clone());
        findings.extend(c.findings);
    }

    // ---------- duplicate rows ----------
    if check_deadline(&mut coverage, "row_duplicates") {
        deadline_hit = true;
    }
    if !deadline_hit {
        match find_duplicate_rows(&table) {
            Some(dup) => {
                coverage.insert(
                    "row_duplicates".into(),
                    PassInfo::ran(format!(
                        "{} groups, {} redundant rows (exact)",
                        dup.n_groups, dup.n_redundant
                    )),
                );
                let mut ev = BTreeMap::new();
                ev.insert("n_redundant".to_string(), json!(dup.n_redundant));
                ev.insert("n_groups".to_string(), json!(dup.n_groups));
                ev.insert("max_group_size".to_string(), json!(dup.max_group_size));
                ev.insert("examples".to_string(), json!(dup.examples));
                findings.push(Finding::new(
                    "duplicate_rows",
                    BTreeMap::new(),
                    format!(
                        "{} rows are exact duplicates of earlier rows ({} groups, largest {})",
                        dup.n_redundant, dup.n_groups, dup.max_group_size
                    ),
                    onceover_report::Confidence::Exact,
                    0.70,
                    ev,
                    Some("deduplicate before joins or model training".into()),
                    "exact row hashing + verification (onceover v0.1)",
                ));
            }
            None => {
                coverage.insert(
                    "row_duplicates".into(),
                    PassInfo::ran("no exact duplicate rows (exact)"),
                );
            }
        }
    }

    // ---------- keys & dependencies ----------
    let (deps, key_findings): (DepsOutcome, Vec<Finding>) = if cfg.budget == Budget::Quick {
        let d = quick_deps(&info, table.n);
        coverage.insert(
            "functional_dependencies".into(),
            PassInfo::skipped("quick tier"),
        );
        let k = analyze_keys(&info, &d);
        (d, k.findings)
    } else {
        if check_deadline(&mut coverage, "functional_dependencies") {
            deadline_hit = true;
        }
        if deadline_hit {
            (DepsOutcome::default(), Vec::new())
        } else {
            let d = discover(&table, &info, &tier.fd);
            coverage.insert(
                "functional_dependencies".into(),
                PassInfo::capped(d.coverage.clone()),
            );
            let k = analyze_keys(&info, &d);
            let mut d = d;
            findings.extend(std::mem::take(&mut d.findings));
            (d, k.findings)
        }
    };
    if !key_findings.is_empty() {
        coverage.insert(
            "keys".into(),
            PassInfo::ran(format!(
                "{} single-column unique, {} composite",
                deps.single_unique.len(),
                deps.composite_uccs.len()
            )),
        );
        findings.extend(key_findings);
    } else {
        coverage.insert(
            "keys".into(),
            PassInfo::na(format!(
                "{} single-column unique, {} composite",
                deps.single_unique.len(),
                deps.composite_uccs.len()
            )),
        );
    }

    // ---------- sortedness ----------
    if check_deadline(&mut coverage, "sortedness") {
        deadline_hit = true;
    }
    if !deadline_hit {
        let sorted = sortedness(&table);
        coverage.insert(
            "sortedness".into(),
            PassInfo::ran(format!("{} columns sorted by row order", sorted.len())),
        );
        findings.extend(sorted);
    }

    // ---------- dates ----------
    if check_deadline(&mut coverage, "dates") {
        deadline_hit = true;
    }
    if !deadline_hit {
        let d = analyze_temporal(&table);
        coverage.insert("dates".into(), PassInfo::ran(d.coverage.clone()));
        // Backfill modal intervals into cards.
        for (name, modal) in d.modal_intervals {
            if let Some(card) = columns.get_mut(&name) {
                if let Some(ts) = card.temporal_stats.as_mut() {
                    ts.modal_interval_days = Some(modal);
                }
            }
        }
        findings.extend(d.findings);
    }

    // ---------- model-based passes (standard / deep) ----------
    if cfg.budget != Budget::Quick {
        if check_deadline(&mut coverage, "predictability_screen") {
            deadline_hit = true;
        }
        if !deadline_hit {
            let (enc, enc_cov) = encode(&table, &tier.encode);
            coverage.insert("encoding".into(), PassInfo::sampled(enc_cov));

            let screen = screen_predictability(&table, &enc, &tier.screen);
            coverage.insert(
                "predictability_screen".into(),
                PassInfo::sampled(screen.coverage.clone()),
            );
            findings.extend(screen.findings.clone());

            // Spend target budgets on columns with a predictive signal,
            // rather than letting near-majority baselines crowd them out.
            let mut cat_targets: Vec<&onceover_mines::TargetResult> = screen
                .results
                .iter()
                .filter(|r| {
                    r.is_categorical && r.metric >= r.baseline + tier.dtable.min_baseline_gain
                })
                .collect();
            cat_targets.sort_by(|a, b| b.metric.total_cmp(&a.metric).then(a.name.cmp(&b.name)));
            let dtable_eligible = cat_targets.len();
            cat_targets.truncate(tier.dtable_top_targets);
            if !cat_targets.is_empty() {
                let (f, cov) = decision_tables(&enc, &cat_targets, &tier.dtable);
                let detail = format!(
                    "{cov}; {} of {dtable_eligible} eligible targets selected (cap {})",
                    cat_targets.len(),
                    tier.dtable_top_targets
                );
                coverage.insert(
                    "decision_tables".into(),
                    if cat_targets.len() < dtable_eligible {
                        PassInfo::capped(detail)
                    } else {
                        PassInfo::sampled(detail)
                    },
                );
                findings.extend(f);
            } else {
                coverage.insert(
                    "decision_tables".into(),
                    PassInfo::na("no categorical targets with predictive signal above baseline"),
                );
            }

            // PRISM rules: top targets by accuracy.
            let mut prism_targets: Vec<&onceover_mines::TargetResult> = screen
                .results
                .iter()
                .filter(|r| {
                    r.is_categorical
                        && r.metric >= tier.prism.min_target_accuracy
                        && r.baseline < 0.9
                        && r.cat_ids
                            .as_ref()
                            .is_some_and(|(_, labels)| labels.len() <= tier.prism.max_classes)
                })
                .collect();
            prism_targets.sort_by(|a, b| b.metric.total_cmp(&a.metric).then(a.name.cmp(&b.name)));
            let prism_eligible = prism_targets.len();
            prism_targets.truncate(tier.prism_top_targets);
            let (f, cov) = prism_rules(&enc, &prism_targets, &tier.prism);
            let detail = format!(
                "{cov}; {} of {prism_eligible} eligible targets selected (cap {})",
                prism_targets.len(),
                tier.prism_top_targets
            );
            coverage.insert(
                "prism_rules".into(),
                if prism_targets.len() < prism_eligible {
                    PassInfo::capped(detail)
                } else {
                    PassInfo::sampled(detail)
                },
            );
            findings.extend(f);

            if check_deadline(&mut coverage, "regimes") {
                deadline_hit = true;
            }
            if !deadline_hit {
                let excluded: Vec<usize> = info
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| {
                        let name = c.name.to_ascii_lowercase();
                        let id_name =
                            name == "id" || name.ends_with("_id") || name.ends_with("_key");
                        // Cardinality alone labels unique real-valued measurements
                        // as identifiers in v0.1 cards. Keep those eligible here.
                        (matches!(c.role.as_str(), "constant" | "all_null")
                            || (c.role == "identifier"
                                && (matches!(table.cols[i], onceover_core::TypedCol::I64 { .. })
                                    || id_name)))
                            .then_some(i)
                    })
                    .collect();
                let (f, cov) =
                    profile_regimes(&table, &enc, &screen.results, &excluded, &tier.regimes);
                coverage.insert("regimes".into(), PassInfo::sampled(cov));
                findings.extend(f);
            }
            if deadline_hit || check_deadline(&mut coverage, "association_rules") {
                deadline_hit = true;
                coverage.insert(
                    "association_rules".into(),
                    PassInfo::skipped("deadline reached"),
                );
            } else {
                let (f, cov) = association_rules(&enc, &tier.association);
                coverage.insert("association_rules".into(), PassInfo::sampled(cov));
                findings.extend(f);
            }

            // Pairwise dependence (supporting role) + identities.
            if check_deadline(&mut coverage, "dependence_screen") {
                deadline_hit = true;
            }
            if !deadline_hit && tier.dep_pair_cap > 0 {
                let (deps_pair, dep_findings, dep_cov) =
                    screen_dependence(&table, tier.dep_pair_cap);
                let numeric = table
                    .cols
                    .iter()
                    .filter(|col| {
                        matches!(
                            col,
                            onceover_core::TypedCol::I64 { .. }
                                | onceover_core::TypedCol::F64 { .. }
                        )
                    })
                    .count();
                let possible_pairs = numeric * numeric.saturating_sub(1) / 2;
                coverage.insert(
                    "dependence_screen".into(),
                    if deps_pair.len() < possible_pairs {
                        PassInfo::capped(dep_cov)
                    } else {
                        PassInfo::ran(dep_cov)
                    },
                );
                findings.extend(dep_findings);

                if tier.ident_pair_cap > 0 {
                    let (id_f, id_cov) = identities(&table, &deps_pair, tier.ident_pair_cap);
                    coverage.insert("arithmetic_identities".into(), PassInfo::ran(id_cov));
                    findings.extend(id_f);
                }
            } else if deadline_hit {
                coverage.insert(
                    "dependence_screen".into(),
                    PassInfo::skipped("deadline reached"),
                );
            }
        } else {
            coverage.insert(
                "predictability_screen".into(),
                PassInfo::skipped("deadline reached"),
            );
        }
    } else {
        for pass in [
            "encoding",
            "predictability_screen",
            "decision_tables",
            "prism_rules",
            "regimes",
            "association_rules",
            "dependence_screen",
            "arithmetic_identities",
        ] {
            coverage.insert(pass.into(), PassInfo::skipped("quick tier"));
        }
    }

    for pass in ["regimes", "association_rules"] {
        coverage.entry(pass.into()).or_insert_with(|| {
            PassInfo::skipped(if deadline_hit {
                "deadline reached"
            } else {
                "model screening unavailable"
            })
        });
    }

    // ---------- suppress identifier_like for the chosen primary key ----------
    if let Some(pk) = keys_pk(&findings) {
        findings.retain(|f| {
            !(f.kind == "identifier_like" && f.subject.get("column") == Some(&json!(pk)))
        });
    }

    // ---------- assemble ----------
    let findings = finalize_ledger(findings);
    let overview = build_overview(&table, &columns, &findings, cfg);
    if deadline_hit {
        coverage.insert(
            "deadline".into(),
            PassInfo::skipped(format!(
                "wall-clock deadline {:.1}s reached; later passes skipped",
                start.elapsed().as_secs_f64()
            )),
        );
    }

    ProfileReport {
        schema_version: SCHEMA_VERSION.into(),
        overview,
        columns,
        findings,
        coverage,
    }
}

fn quick_deps(info: &[ColInfo], n: usize) -> DepsOutcome {
    DepsOutcome {
        single_unique: (0..info.len())
            .filter(|&i| info[i].null_count == 0 && info[i].distinct == n && n > 0)
            .collect(),
        coverage: "quick tier: single-column uniqueness only".into(),
        ..Default::default()
    }
}

fn keys_pk(findings: &[Finding]) -> Option<String> {
    findings
        .iter()
        .find(|f| f.kind == "likely_primary_key")
        .and_then(|f| {
            f.subject
                .get("column")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
}

/// Duplicate value examples for dirty-key findings (dtype-aware display).
fn dup_examples(table: &TypedTable, i: usize, c: &CardOut) -> Vec<String> {
    c.counts
        .top
        .iter()
        .filter(|(_, count)| *count >= 2)
        .take(3)
        .map(|(k, _)| display_key(k, &table.dtypes[i]))
        .collect()
}

fn display_key(k: &VKey, dtype: &str) -> String {
    match (k, dtype) {
        (VKey::I(v), "date") => onceover_core::dateutil::iso_date(*v),
        (VKey::I(v), "datetime") => onceover_core::dateutil::iso_datetime(*v),
        _ => k.display(),
    }
}

fn build_overview(
    table: &TypedTable,
    columns: &BTreeMap<String, ColumnCard>,
    findings: &[Finding],
    cfg: &ProfileConfig,
) -> Overview {
    let mut dtype_counts: BTreeMap<String, usize> = BTreeMap::new();
    for d in &table.dtypes {
        *dtype_counts.entry(d.clone()).or_insert(0) += 1;
    }
    let mut role_counts: BTreeMap<String, usize> = BTreeMap::new();
    for c in columns.values() {
        *role_counts.entry(c.role.clone()).or_insert(0) += 1;
    }

    let count = |kind: &str| findings.iter().filter(|f| f.kind == kind).count();
    let n_fd = count("functional_dependency");
    let n_afd = count("approximate_fd");
    let n_pred = count("predictable_from");
    let n_gaps = count("date_gap");
    let n_dup = count("duplicate_rows");

    let pk = keys_pk(findings)
        .map(|k| format!("key: {k}; "))
        .unwrap_or_default();

    let headline = format!(
        "{} rows × {} columns ({} budget); {}{} exact FD{}{}{}{}{}",
        table.n,
        table.cols.len(),
        cfg.budget.as_str(),
        pk,
        n_fd,
        if n_fd == 1 { "" } else { "s" },
        if n_afd > 0 {
            format!(", {n_afd} near-exact")
        } else {
            String::new()
        },
        if n_pred > 0 {
            format!(
                ", {n_pred} predictable column{}",
                if n_pred == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        },
        if n_gaps > 0 {
            format!(", {n_gaps} date gap{}", if n_gaps == 1 { "" } else { "s" })
        } else {
            String::new()
        },
        if n_dup > 0 {
            format!(
                ", {n_dup} duplicate-row group{}",
                if n_dup == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        },
    );

    Overview {
        n_rows: table.n,
        n_columns: table.cols.len(),
        headline,
        dtype_counts,
        role_counts,
        top_findings: findings.iter().take(5).map(|f| f.claim.clone()).collect(),
    }
}
