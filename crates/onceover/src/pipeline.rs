//! The pipeline: run passes in cost order under the budget, assemble the
//! findings ledger, overview and coverage block. Deterministic end to end
//! (unless an opt-in wall-clock deadline fires, which is reported honestly).

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;
use std::time::Instant;

use onceover_core::{extract, find_duplicate_rows, CardOut, TypedTable, VKey};
use onceover_deps::{analyze_keys, discover, sortedness, ColInfo, DepsOutcome};
use onceover_mines::{
    analyze_temporal, association_rules, decision_tables, encode, identities, prism_rules,
    profile_breaks, profile_discontinuities, profile_drift, profile_interactions, profile_regimes,
    profile_surprises, profile_target_shapes, screen_predictability,
};
use onceover_report::{
    group_findings, select_findings, Category, ColumnCard, Finding, Overview, PassInfo,
    ProfileReport, Sampling, SCHEMA_VERSION,
};
use polars::prelude::DataFrame;
use serde_json::json;

use crate::config::{Budget, ProfileConfig, TierConfig};

/// Profile a DataFrame. Returns the complete, deterministic report.
pub fn profile(df: &DataFrame, cfg: &ProfileConfig) -> ProfileReport {
    profile_with_detectors(df, cfg, &[])
}

pub fn profile_with_detectors(
    df: &DataFrame,
    cfg: &ProfileConfig,
    extensions: &[&dyn crate::Detector],
) -> ProfileReport {
    let mut requested = cfg.detectors.clone();
    if let Some(ids) = &mut requested {
        loop {
            let n = ids.len();
            for detector in extensions {
                let d = detector.descriptor();
                if ids.contains(&d.id) {
                    ids.extend(d.requires);
                }
            }
            if ids.len() == n {
                break;
            }
        }
    }
    let mut active = crate::detectors::active_detectors(requested.as_ref());
    // Designating a target activates the target-aware pass when the
    // caller did not explicitly restrict the detector set.
    if cfg.target.is_some() && cfg.detectors.is_none() {
        active.insert("target_shapes".into());
    }
    let enabled = |id: &str| active.contains(id);
    let selected = |id: &str| cfg.detectors.as_ref().is_none_or(|ids| ids.contains(id));
    let mut shared_encoding = None;
    let start = Instant::now();
    let mut tier = TierConfig::for_budget(cfg.budget);
    if let Some(rows) = cfg.sample_rows {
        tier.encode.row_cap = rows.max(1);
        tier.fd.row_cap = rows.max(1);
        tier.screen.max_model_rows = rows.max(1);
        tier.regimes.row_cap = rows.max(1);
    }
    tier.views.row_cap = tier.encode.row_cap;
    if let Some(predictors) = cfg.max_predictors {
        tier.screen.max_predictors = predictors.max(1);
        tier.encode.feature_cap = predictors.max(1);
    }
    if let Some(pairs) = cfg.max_pairs {
        tier.dep_pair_cap = pairs;
    }
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
        if enabled("column_profile") {
            findings.extend(c.findings);
        }
    }

    // ---------- duplicate rows ----------
    if check_deadline(&mut coverage, "row_duplicates") {
        deadline_hit = true;
    }
    if !deadline_hit
        && enabled("row_duplicates")
        && table
            .cols
            .iter()
            .any(|c| !matches!(c, onceover_core::TypedCol::Other { .. }))
    {
        match if info
            .iter()
            .any(|c| c.null_count == 0 && c.distinct == table.n)
        {
            None
        } else {
            find_duplicate_rows(&table)
        } {
            Some(dup) => {
                coverage.insert(
                    "row_duplicates".into(),
                    PassInfo::ran(format!(
                        "{} groups, {} redundant rows (exact)",
                        dup.n_groups, dup.n_redundant
                    )),
                );
                let mut ev = BTreeMap::new();
                ev.insert("n".into(), json!(table.n));
                ev.insert("n_redundant".to_string(), json!(dup.n_redundant));
                ev.insert("n_groups".to_string(), json!(dup.n_groups));
                ev.insert("max_group_size".to_string(), json!(dup.max_group_size));
                ev.insert("examples".to_string(), json!(dup.examples));
                findings.push(Finding::new(
                    Pattern::Duplicates,
                    BTreeMap::from([("columns".into(),json!(table.names.iter().zip(&table.cols).filter(|(_,col)|!matches!(col,onceover_core::TypedCol::Other{..})).map(|(name,_)|name).collect::<Vec<_>>()))]),
                    format!(
                        "{} rows repeat earlier values across the compared columns ({} groups, largest {})",
                        dup.n_redundant, dup.n_groups, dup.max_group_size
                    ),
                    onceover_report::EvidenceBasis::Measured,
                    0.70,
                    ev,
                    Some("deduplicate before joins or model training".into()),
                    Method::new(
                        "row_duplicates",
                        "row_equality",
                        "exact row hashing + verification (onceover v0.1)",
                    ),
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

    if enabled("row_duplicates")
        && table
            .cols
            .iter()
            .all(|c| matches!(c, onceover_core::TypedCol::Other { .. }))
    {
        coverage.insert(
            "row_duplicates".into(),
            PassInfo::na("no comparable columns"),
        );
    }

    // ---------- keys & dependencies ----------
    let (deps, key_findings): (DepsOutcome, Vec<Finding>) =
        if cfg.budget == Budget::Quick || !enabled("functional_dependencies") {
            let d = if cfg.budget == Budget::Quick {
                quick_deps(&info, table.n)
            } else if enabled("keys") {
                onceover_deps::fd::discover_keys(&table, &info, &tier.fd)
            } else {
                DepsOutcome::default()
            };
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
    let key_findings = if enabled("keys") {
        key_findings
    } else {
        Vec::new()
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

    // ---------- duplicate encodings ----------
    if check_deadline(&mut coverage, "duplicate_encodings") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("duplicate_encodings") {
        let (f, cov) = onceover_deps::profile_duplicate_encodings(&table, &info, &tier.dupe);
        coverage.insert("duplicate_encodings".into(), PassInfo::ran(cov));
        findings.extend(f);
    }

    // ---------- pivot (long format) ----------
    if enabled("pivot") {
        let p = onceover_deps::analyze_pivot(
            &table,
            &info,
            &onceover_deps::PivotConfig {
                max_levels: tier.pivot_max_levels,
                max_findings: tier.pivot_max_findings,
                ..Default::default()
            },
        );
        if p.checked == 0 {
            coverage.insert(
                "pivot".into(),
                PassInfo::na(
                    "needs exactly one continuous measure column and a categorical attribute candidate"
                        .to_string(),
                ),
            );
        } else {
            coverage.insert(
                "pivot".into(),
                PassInfo::ran(format!(
                    "{} attribute candidates checked, {} verified jointly unique",
                    p.checked, p.verified
                )),
            );
        }
        findings.extend(p.findings);
    }

    // ---------- sortedness ----------
    if check_deadline(&mut coverage, "sortedness") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("sortedness") {
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
    if !deadline_hit && enabled("dates") {
        // Columns with a declared gap check are skipped here: the
        // expectations pass reports their gaps against the declared
        // reference instead of the inferred one.
        let declared_skip = cfg
            .hints
            .as_ref()
            .map(|h| onceover_mines::expectations::declared_temporal_columns(&table, h))
            .unwrap_or_default();
        let d = analyze_temporal(&table, &declared_skip);
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

    // ---------- declared expectations ----------
    if check_deadline(&mut coverage, "expectations") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("expectations") {
        match cfg.hints.as_ref() {
            Some(hints) if !hints.is_empty() => {
                let e = onceover_mines::expectations::analyze_expectations(&table, hints);
                coverage.insert("expectations".into(), PassInfo::ran(e.coverage.clone()));
                findings.extend(e.findings);
            }
            _ => {
                coverage.insert(
                    "expectations".into(),
                    PassInfo::skipped("no hints provided".to_string()),
                );
            }
        }
    }

    // ---------- model-based passes (standard / deep) ----------
    if cfg.budget != Budget::Quick {
        if check_deadline(&mut coverage, "predictability_screen") {
            deadline_hit = true;
        }
        if !deadline_hit {
            let (enc, enc_cov) = encode(&table, &tier.encode);
            coverage.insert("encoding".into(), PassInfo::sampled(enc_cov));

            let screen = if enabled("gbm") {
                screen_predictability(&table, &enc, &tier.screen)
            } else {
                onceover_mines::ScreenOutcome {
                    findings: Vec::new(),
                    results: Vec::new(),
                    coverage: "not selected".into(),
                }
            };
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
            cat_targets.truncate(if enabled("decision_tables") {
                tier.dtable_top_targets
            } else {
                0
            });
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
            prism_targets.truncate(if enabled("prism") {
                tier.prism_top_targets
            } else {
                0
            });
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
            if !deadline_hit && enabled("regimes") {
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
            if check_deadline(&mut coverage, "interactions") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("interactions") {
                let (f, cov) =
                    profile_interactions(&table, &enc, &screen.results, &tier.screen, &tier.hstat);
                coverage.insert("interactions".into(), PassInfo::sampled(cov));
                findings.extend(f);
            }
            if check_deadline(&mut coverage, "breaks") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("breaks") {
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
                let (f, cov) = profile_breaks(&table, &excluded, &tier.breaks);
                coverage.insert("breaks".into(), PassInfo::sampled(cov));
                findings.extend(f);
            }
            if check_deadline(&mut coverage, "surprises") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("surprises") {
                let excluded: Vec<usize> = info
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| {
                        (matches!(c.role.as_str(), "constant" | "all_null")
                            || c.role == "identifier")
                            .then_some(i)
                    })
                    .collect();
                let (f, cov) = profile_surprises(&table, &excluded, &tier.surprises);
                coverage.insert("surprises".into(), PassInfo::ran(cov));
                findings.extend(f);
            }
            if check_deadline(&mut coverage, "discontinuities") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("discontinuities") {
                // Mirror the breaks pass's carve-out: cardinality alone
                // labels unique real-valued measurements as identifiers;
                // those are exactly the columns this screen exists for.
                // Exclude identifier-role columns only when they are
                // integer-typed or id-named.
                let excluded: Vec<usize> = info
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| {
                        let name = table.names[i].as_str();
                        let id_name =
                            name == "id" || name.ends_with("_id") || name.ends_with("_key");
                        (matches!(c.role.as_str(), "constant" | "all_null")
                            || (c.role == "identifier"
                                && (matches!(table.cols[i], onceover_core::TypedCol::I64 { .. })
                                    || id_name)))
                            .then_some(i)
                    })
                    .collect();
                let (f, cov) = profile_discontinuities(&table, &excluded, &tier.disc);
                coverage.insert("discontinuities".into(), PassInfo::ran(cov));
                findings.extend(f);
            }
            if check_deadline(&mut coverage, "drift") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("drift") {
                // Same carve-out as the discontinuities pass: cardinality
                // alone labels unique real-valued measurements as
                // identifiers, and those are exactly the numeric targets
                // this pass exists to score. Exclude identifier-role
                // columns only when integer-typed or id-named.
                let excluded: Vec<usize> = info
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| {
                        let name = table.names[i].as_str();
                        let id_name =
                            name == "id" || name.ends_with("_id") || name.ends_with("_key");
                        (matches!(c.role.as_str(), "constant" | "all_null")
                            || (c.role == "identifier"
                                && (matches!(table.cols[i], onceover_core::TypedCol::I64 { .. })
                                    || id_name)))
                            .then_some(i)
                    })
                    .collect();
                let time_column = cfg.hints.as_ref().and_then(|h| h.time.as_deref());
                let (f, cov) = profile_drift(
                    &table,
                    &enc,
                    time_column,
                    &excluded,
                    &tier.screen,
                    &tier.drift,
                );
                coverage.insert("drift".into(), PassInfo::ran(cov));
                findings.extend(f);
            }
            if deadline_hit || check_deadline(&mut coverage, "association_rules") {
                deadline_hit = true;
                coverage.insert(
                    "association_rules".into(),
                    PassInfo::skipped("deadline reached"),
                );
            } else if enabled("association_rules") {
                let (f, cov) = association_rules(&enc, &tier.association);
                coverage.insert("association_rules".into(), PassInfo::sampled(cov));
                findings.extend(f);
            }

            // Pairwise dependence (supporting role) + identities.
            if check_deadline(&mut coverage, "dependence_screen") {
                deadline_hit = true;
            }
            if !deadline_hit && tier.dep_pair_cap > 0 && enabled("numeric_dependence") {
                let (deps_pair, dep_findings, dep_cov) = onceover_mines::dep::screen_sampled(
                    &table,
                    tier.dep_pair_cap,
                    tier.encode.row_cap,
                );
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
                        if table.n > tier.encode.row_cap {
                            PassInfo::sampled(dep_cov)
                        } else {
                            PassInfo::ran(dep_cov)
                        }
                    },
                );
                findings.extend(dep_findings);

                if tier.ident_pair_cap > 0 && enabled("identities") {
                    let (id_f, id_cov) = identities(&table, &deps_pair, tier.ident_pair_cap);
                    coverage.insert("arithmetic_identities".into(), PassInfo::ran(id_cov));
                    findings.extend(id_f);
                }
                if enabled("period_associations") {
                    let out = onceover_mines::period_associations(
                        &table,
                        &deps_pair,
                        tier.encode.row_cap,
                    );
                    let info = match out.status {
                        onceover_mines::PeriodsStatus::NotApplicable(d) => PassInfo::na(d),
                        onceover_mines::PeriodsStatus::Ran(d) => PassInfo::ran(d),
                        onceover_mines::PeriodsStatus::Capped(d) => PassInfo::capped(d),
                    };
                    coverage.insert("period_associations".into(), info);
                    findings.extend(out.findings);
                }

                if tier.ident_multiterm_cap > 0 && enabled("identities") {
                    let (mt_f, mt_cov) =
                        onceover_mines::multiterm_identities(&table, tier.ident_multiterm_cap);
                    coverage.insert("multiterm_identities".into(), PassInfo::ran(mt_cov));
                    findings.extend(mt_f);
                }
            } else if deadline_hit {
                coverage.insert(
                    "dependence_screen".into(),
                    PassInfo::skipped("deadline reached"),
                );
            }

            // Greedy view/cluster pass (standard / deep only).
            if check_deadline(&mut coverage, "views") {
                deadline_hit = true;
            }
            if !deadline_hit && enabled("views") {
                let (vf, vout) = onceover_mines::views_pass(&table, &tier.views);
                // Dropped columns must be visible in coverage, not
                // only in finding metrics: with no views findings at
                // all, the exclusion would otherwise be silent. A
                // budget-truncated run reports as capped, like the
                // other truncated passes, not as a complete run.
                let mut detail = vout.coverage.clone();
                if !vout.dropped_columns.is_empty() {
                    detail.push_str(&format!(
                        "; dropped {} column(s) past the {}-column cap: {}",
                        vout.dropped_columns.len(),
                        tier.views.max_columns,
                        vout.dropped_columns.join(", ")
                    ));
                }
                coverage.insert(
                    "views".into(),
                    if vout.budget_hit {
                        PassInfo::capped(detail)
                    } else if table.n > tier.views.row_cap {
                        PassInfo::sampled(detail)
                    } else {
                        PassInfo::ran(detail)
                    },
                );
                findings.extend(vf);
            } else if deadline_hit {
                coverage.insert("views".into(), PassInfo::skipped("deadline reached"));
            }
            shared_encoding = Some(enc);
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
            "breaks",
            "association_rules",
            "dependence_screen",
            "arithmetic_identities",
            "period_associations",
            "views",
            "drift",
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

    for detector in extensions {
        let descriptor = detector.descriptor();
        if cfg.detectors.is_some() && !active.contains(&descriptor.id) {
            continue;
        }
        if check_deadline(&mut coverage, &descriptor.id) {
            continue;
        }
        let context = crate::DetectorContext {
            table: &table,
            columns: &columns,
            encoded: shared_encoding.as_ref(),
            config: cfg,
        };
        match detector.discover(&context) {
            Ok(mut candidates) => {
                let invalid = candidates.iter().find_map(|f| {
                    if !descriptor.categories.contains(&f.category) {
                        return Some("undeclared category".to_string());
                    }
                    if f.subject.columns.iter().any(|c| !columns.contains_key(c)) {
                        return Some("unknown subject column".to_string());
                    }
                    f.validate().err()
                });
                if let Some(reason) = invalid {
                    coverage.insert(
                        descriptor.id.clone(),
                        PassInfo::skipped(format!("invalid detector output: {reason}")),
                    );
                    continue;
                }
                for f in &mut candidates {
                    for e in &mut f.evidence {
                        e.method.detector = descriptor.id.clone();
                    }
                }
                coverage.insert(
                    descriptor.id.clone(),
                    PassInfo::ran(format!("{} candidate statements", candidates.len())),
                );
                findings.extend(candidates);
            }
            Err(message) => {
                coverage.insert(
                    descriptor.id.clone(),
                    PassInfo::skipped(format!("detector failed: {message}")),
                );
            }
        }
    }
    // ---------- target-aware shapes (opt-in via a designated target) ----------
    if check_deadline(&mut coverage, "target_shapes") {
        deadline_hit = true;
    }
    if !deadline_hit && enabled("target_shapes") {
        match &cfg.target {
            Some(target) => {
                let (f, cov) = profile_target_shapes(&table, target, &tier.shapes);
                coverage.insert("target_shapes".into(), PassInfo::ran(cov));
                findings.extend(f);
            }
            None => {
                coverage.insert(
                    "target_shapes".into(),
                    PassInfo::skipped("no target designated".to_string()),
                );
            }
        }
    }

    findings.retain(|f| f.evidence.iter().any(|e| selected(&e.method.detector)));
    for f in &mut findings {
        for evidence in &mut f.evidence {
            evidence.scope.population_rows.get_or_insert(table.n);
            evidence.scope.missing_values=match evidence.method.detector.as_str(){
                "column_profile"|"keys"=>"non-missing values counted; missingness reported separately",
                "row_duplicates"=>"missing values compare equal; opaque fields excluded",
                "functional_dependencies"|"numeric_dependence"=>"rows complete on the evaluated inputs and target",
                "gbm"|"decision_tables"|"regimes"|"interactions"=>"missing targets excluded; predictor handling follows the recorded model",
                "drift"=>"rows missing the target or the time value excluded; period comparisons use equal row counts on both sides",
                "target_shapes"=>"complete cases per predictor: rows missing the predictor or the target excluded",
                "breaks"=>"rows with a missing date or value are excluded from each series",
                "association_rules"|"prism"=>"see discovery metrics; normalized rule evidence uses complete predicates and outcome",
                _=>evidence.scope.missing_values.as_str(),
            }.to_string();
            if evidence.scope.sampling == Sampling::Unspecified {
                evidence.scope.sampling = match evidence.method.detector.as_str() {
                    "column_profile" | "keys" | "row_duplicates" | "sortedness" | "dates"
                    | "identities" => Sampling::Full,
                    "association_rules" | "prism" => {
                        if shared_encoding.as_ref().is_some_and(|enc| enc.n < table.n) {
                            Sampling::Sample
                        } else {
                            Sampling::Full
                        }
                    }
                    _ => match evidence.scope.rows_evaluated {
                        Some(n) if n < table.n => Sampling::Sample,
                        Some(_) => Sampling::Full,
                        None => Sampling::Unspecified,
                    },
                };
            }
        }
    }
    let candidate_count = findings.len();
    let rule_candidates = findings
        .iter()
        .filter(|f| f.category == Category::ConditionalRules)
        .count();
    crate::rules::evaluate_rules(&table, &mut findings, tier.encode.row_cap, &cfg.selection);
    let rules_kept = findings
        .iter()
        .filter(|f| f.category == Category::ConditionalRules)
        .count();
    if rule_candidates > 0 {
        coverage.insert("rule_evaluation".into(),PassInfo::sampled(format!("{rules_kept} of {rule_candidates} candidates passed a common diagnostic partition; support and baseline evaluated on complete predicates and outcome; reused for selection")));
    }
    let verified_count = findings.len();
    let distinct_count = findings
        .iter()
        .map(Finding::canonical_key)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    for (id, pass) in [
        ("row_duplicates", "row_duplicates"),
        ("keys", "keys"),
        ("functional_dependencies", "functional_dependencies"),
        ("sortedness", "sortedness"),
        ("dates", "dates"),
        ("gbm", "predictability_screen"),
        ("decision_tables", "decision_tables"),
        ("prism", "prism_rules"),
        ("regimes", "regimes"),
        ("interactions", "interactions"),
        ("breaks", "breaks"),
        ("association_rules", "association_rules"),
        ("numeric_dependence", "dependence_screen"),
        ("identities", "arithmetic_identities"),
        ("views", "views"),
        ("target_shapes", "target_shapes"),
        ("drift", "drift"),
    ] {
        if !enabled(id) {
            coverage.insert(pass.into(), PassInfo::skipped("detector not enabled"));
        } else if !selected(id) {
            if let Some(info) = coverage.get_mut(pass) {
                info.detail
                    .push_str("; required as a resource; findings not selected");
            }
        }
    }
    let findings = select_findings(findings, &cfg.selection);
    let groups = group_findings(&findings);
    let selection = onceover_report::SelectionSummary {
        candidates: candidate_count,
        verified_candidates: verified_count,
        distinct_statements: distinct_count,
        selected: findings.len(),
    };
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
        groups,
        selection,
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
        .find(|f| {
            f.category == Category::Uniqueness
                && f.subject.columns.len() == 1
                && f.evidence
                    .iter()
                    .any(|e| e.method.algorithm == "uniqueness")
        })
        .and_then(|f| f.subject.columns.first().cloned())
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

    let n_functional = findings
        .iter()
        .filter(|f| f.is(Category::Dependencies, Some("functional")))
        .count();
    let n_predictive = findings
        .iter()
        .filter(|f| f.is(Category::Dependencies, Some("predictive")))
        .count();
    let n_rules = findings
        .iter()
        .filter(|f| f.category == Category::ConditionalRules)
        .count();
    let pk = keys_pk(findings)
        .map(|k| format!("; key candidate: {k}"))
        .unwrap_or_default();
    let headline=format!("{} rows × {} columns ({} budget){}; {} functional dependencies, {} predictive dependencies, {} conditional rules",
        table.n,table.cols.len(),cfg.budget.as_str(),pk,n_functional,n_predictive,n_rules);

    Overview {
        n_rows: table.n,
        n_columns: table.cols.len(),
        headline,
        dtype_counts,
        role_counts,
        top_findings: findings.iter().take(5).map(|f| f.claim.clone()).collect(),
    }
}
