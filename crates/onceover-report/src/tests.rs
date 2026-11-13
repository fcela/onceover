use super::*;
use serde_json::json;

fn evidence(id: &str) -> Evidence {
    Evidence::new(
        Method::new(id, "test_method", "test"),
        EvidenceBasis::Measured,
        BTreeMap::from([
            ("conditional_rate".into(), json!(0.95)),
            ("baseline_rate".into(), json!(0.3)),
            ("joint_count".into(), json!(95)),
            ("n".into(), json!(100)),
        ]),
    )
}
fn rule(id: &str, condition: Predicate, outcome: Predicate) -> Finding {
    Finding::from_statement(
        Category::ConditionalRules,
        Some("if_then".into()),
        Subject::default(),
        Specification::default(),
        "conditional statement".into(),
        evidence(id),
        0.5,
        None,
    )
    .with_rule(vec![condition], outcome)
}

fn fd_finding(detector_score: f64) -> Finding {
    Finding::new(
        Pattern::FunctionalDependency,
        BTreeMap::from([
            ("lhs".into(), json!(["order_id"])),
            ("rhs".into(), json!("region")),
        ]),
        "order_id → region".into(),
        EvidenceBasis::Measured,
        detector_score,
        BTreeMap::from([
            ("violations".into(), json!(0)),
            ("n_used".into(), json!(100)),
        ]),
        None,
        Method::new("functional_dependencies", "value_mapping", "test"),
    )
}

fn key_finding(duplicates: Option<u64>) -> Finding {
    let mut metrics = BTreeMap::from([("distinct".into(), json!(100)), ("n".into(), json!(100))]);
    if let Some(d) = duplicates {
        metrics.insert("duplicates".into(), json!(d));
    }
    Finding::new(
        Pattern::Uniqueness,
        BTreeMap::from([("column".into(), json!("order_id"))]),
        "order_id uniquely identifies rows".into(),
        EvidenceBasis::Measured,
        0.95,
        metrics,
        None,
        Method::new("keys", "uniqueness", "test"),
    )
}

#[test]
fn detector_score_is_preserved_and_blended() {
    let selected = finalize_ledger(vec![fd_finding(0.85)]);
    assert_eq!(selected.len(), 1);
    let f = &selected[0];
    assert_eq!(f.detector_interestingness, Some(0.85));
    // Ledger evidence score for this finding is 0.9214; the selection
    // score is the mean of the detector's 0.85 and that value.
    assert!(
        (f.interestingness - 0.8857).abs() < 0.001,
        "{}",
        f.interestingness
    );
    assert!(!f.metrics().contains_key("triviality"));
}

#[test]
fn fd_implied_by_exact_key_is_demoted_and_tagged() {
    let selected = finalize_ledger(vec![key_finding(None), fd_finding(0.85)]);
    let fd = selected
        .iter()
        .find(|f| f.category == Category::Dependencies)
        .unwrap();
    assert_eq!(fd.detector_interestingness, Some(0.85));
    // Same blend as the untriggered case, multiplied by 0.6.
    assert!(
        (fd.interestingness - 0.5314).abs() < 0.001,
        "{}",
        fd.interestingness
    );
    assert_eq!(fd.metrics()["triviality"], json!("implied_by_unique_key"));
    assert_eq!(fd.metrics()["triviality_factor"], json!(0.6));
    let key = selected
        .iter()
        .find(|f| f.category == Category::Uniqueness)
        .unwrap();
    assert!(!key.metrics().contains_key("triviality"));
}

#[test]
fn producer_declared_demotion_is_applied_and_consolidated() {
    let mut fd = fd_finding(0.85);
    fd.evidence[0]
        .metrics
        .insert("triviality".into(), json!("declared_reason"));
    fd.evidence[0]
        .metrics
        .insert("triviality_factor".into(), json!(0.6));
    let selected = finalize_ledger(vec![fd]);
    let f = &selected[0];
    // Same blend as the untriggered case, multiplied by the declared 0.6.
    assert!(
        (f.interestingness - 0.5314).abs() < 0.001,
        "{}",
        f.interestingness
    );
    assert_eq!(f.metrics()["triviality"], json!("declared_reason"));
    assert_eq!(f.metrics()["triviality_factor"], json!(0.6));
}

#[test]
fn declared_and_key_implied_demotions_multiply() {
    let mut fd = fd_finding(0.85);
    fd.evidence[0]
        .metrics
        .insert("triviality".into(), json!("declared_reason"));
    fd.evidence[0]
        .metrics
        .insert("triviality_factor".into(), json!(0.6));
    let selected = finalize_ledger(vec![key_finding(None), fd]);
    let fd = selected
        .iter()
        .find(|f| f.category == Category::Dependencies)
        .unwrap();
    // 0.8857 blend x 0.6 declared x 0.6 key-implied.
    assert!(
        (fd.interestingness - 0.3189).abs() < 0.001,
        "{}",
        fd.interestingness
    );
    assert_eq!(
        fd.metrics()["triviality"],
        json!("declared_reason+implied_by_unique_key")
    );
    assert_eq!(fd.metrics()["triviality_factor"], json!(0.36));
}

#[test]
fn dirty_key_does_not_imply_fd() {
    let selected = finalize_ledger(vec![key_finding(Some(3)), fd_finding(0.85)]);
    let fd = selected
        .iter()
        .find(|f| f.category == Category::Dependencies)
        .unwrap();
    assert!(
        (fd.interestingness - 0.8857).abs() < 0.001,
        "{}",
        fd.interestingness
    );
    assert!(!fd.metrics().contains_key("triviality"));
}

#[test]
fn equivalent_rules_merge_evidence_without_pooling_counts() {
    let a = rule(
        "prism",
        Predicate::equal("segment", json!("premium")),
        Predicate::equal("shipping", json!("expedite")),
    );
    let mut b = a.clone();
    b.evidence[0] = evidence("association_rules");
    b.evidence[0].scope.sampling = Sampling::Sample;
    b.evidence[0]
        .metrics
        .insert("joint_count".into(), json!(80));
    let selected = finalize_ledger(vec![a, b]);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].evidence.len(), 2);
    assert_eq!(selected[0].detectors().len(), 2);
    assert!(selected[0]
        .evidence
        .iter()
        .any(|e| e.metrics["joint_count"] == json!(80)));
    assert!(selected[0]
        .evidence
        .iter()
        .any(|e| e.metrics["joint_count"] == json!(95)));
}

#[test]
fn different_directions_conditions_types_and_domains_stay_distinct() {
    let a = rule(
        "a",
        Predicate::equal("x", json!("1")),
        Predicate::equal("y", json!(true)),
    );
    let b = rule(
        "b",
        Predicate::equal("y", json!(true)),
        Predicate::equal("x", json!("1")),
    );
    let c = rule(
        "c",
        Predicate::equal("x", json!(1)),
        Predicate::equal("y", json!(true)),
    );
    let d = rule(
        "d",
        Predicate {
            column: "x".into(),
            operator: Operator::Gt,
            value: json!(1),
            transform: None,
        },
        Predicate::equal("y", json!(true)),
    );
    assert_eq!(
        select_findings(
            vec![a, b, c, d],
            &SelectionPolicy {
                rules_per_outcome: 10,
                ..Default::default()
            }
        )
        .len(),
        4
    );
}

#[test]
fn canonical_predicate_order_and_ids_do_not_depend_on_rank_or_method() {
    let mut a = rule(
        "a",
        Predicate::equal("x", json!(1)),
        Predicate::equal("y", json!("yes")),
    );
    a = a.with_rule(
        vec![
            Predicate::equal("x", json!(1)),
            Predicate::equal("z", json!(2)),
        ],
        Predicate::equal("y", json!("yes")),
    );
    let mut b = a.clone();
    b.specification.conditions.reverse();
    b.evidence[0].method.detector = "b".into();
    let left = finalize_ledger(vec![a.clone()]);
    let merged = finalize_ledger(vec![b, a]);
    assert_eq!(left[0].id, merged[0].id);
    assert_eq!(merged.len(), 1);
}

#[test]
fn rule_limits_apply_to_statements_across_methods() {
    let rules = (0..6)
        .map(|i| {
            rule(
                if i % 2 == 0 { "one" } else { "two" },
                Predicate::equal("x", json!(i)),
                Predicate::equal("y", json!("yes")),
            )
        })
        .collect();
    let selected = select_findings(
        rules,
        &SelectionPolicy {
            max_rules: 1,
            ..Default::default()
        },
    );
    assert_eq!(selected.len(), 1);
}

#[test]
fn connected_groups_do_not_add_redundancy_findings() {
    let a = Finding::from_statement(
        Category::Associations,
        Some("statistical".into()),
        Subject::columns(["a".into(), "b".into()]),
        Specification::default(),
        "association".into(),
        evidence("measure"),
        0.5,
        None,
    );
    let b = Finding::from_statement(
        Category::Dependencies,
        Some("predictive".into()),
        Subject::dependency(vec!["b".into()], "c".into()),
        Specification::default(),
        "prediction".into(),
        evidence("model"),
        0.5,
        None,
    );
    let findings = finalize_ledger(vec![a, b]);
    let groups = group_findings(&findings);
    assert_eq!(findings.len(), 2);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].columns, ["a", "b", "c"]);
    assert_eq!(groups[0].finding_ids.len(), 2);
}

#[test]
fn sampling_estimation_and_validation_are_independent_fields() {
    let mut e = evidence("model");
    e.basis = EvidenceBasis::Estimated;
    e.scope.sampling = Sampling::Full;
    e.validation = Validation {
        method: "cross_validation".into(),
        folds: Some(5),
        used_for_selection: true,
        ..Default::default()
    };
    let js = serde_json::to_value(&e).unwrap();
    assert_eq!(js["basis"], "estimated");
    assert_eq!(js["scope"]["sampling"], "full");
    assert_eq!(js["validation"]["method"], "cross_validation");
}

#[test]
fn markdown_places_evidence_and_result_tables_beside_findings() {
    let mut finding = rule(
        "detector",
        Predicate::equal("segment", json!("premium")),
        Predicate::equal("shipping", json!("expedite")),
    );
    finding.evidence[0].metrics.insert(
        "periods".into(),
        json!([
            {"label": "2020", "n": 200, "pearson": 1.0},
            {"label": "2021", "n": 200, "pearson": -1.0},
        ]),
    );
    let findings = finalize_ledger(vec![finding]);
    let report = ProfileReport {
        schema_version: SCHEMA_VERSION.into(),
        frame: None,
        source: None,
        overview: Overview {
            n_rows: 100,
            n_columns: 2,
            headline: "test".into(),
            dtype_counts: BTreeMap::new(),
            role_counts: BTreeMap::new(),
            top_findings: vec![],
        },
        columns: BTreeMap::new(),
        groups: group_findings(&findings),
        findings,
        grouped_findings: vec![],
        selection: Default::default(),
        coverage: BTreeMap::new(),
        associations: Vec::new(),
    };
    let md = report.to_markdown();
    let claim_at = md.find("conditional statement").unwrap();
    let evidence_at = md
        .find("- evidence: detector (test_method), direct_comparison, 100 rows evaluated")
        .unwrap();
    assert!(claim_at < evidence_at, "{md}");
    assert!(
        md.contains("metrics: baseline_rate=0.3, conditional_rate=0.95, joint_count=95, n=100"),
        "{md}"
    );
    assert!(md.contains("| label | n | pearson |"), "{md}");
    assert!(md.contains("| 2021 | 200 | -1 |"), "{md}");
}

#[test]
fn report_schema_roundtrips_and_renders() {
    let finding = rule(
        "detector",
        Predicate::equal("segment", json!("premium")),
        Predicate::equal("shipping", json!("expedite")),
    );
    let findings = finalize_ledger(vec![finding]);
    let report = ProfileReport {
        schema_version: SCHEMA_VERSION.into(),
        frame: None,
        source: None,
        overview: Overview {
            n_rows: 100,
            n_columns: 2,
            headline: "test".into(),
            dtype_counts: BTreeMap::new(),
            role_counts: BTreeMap::new(),
            top_findings: vec![],
        },
        columns: BTreeMap::new(),
        groups: group_findings(&findings),
        findings,
        grouped_findings: vec![],
        selection: Default::default(),
        coverage: BTreeMap::new(),
        associations: Vec::new(),
    };
    let back: ProfileReport = serde_json::from_str(&report.to_json()).unwrap();
    assert_eq!(back, report);
    let js = serde_json::to_value(&report).unwrap();
    let f = &js["findings"][0];
    assert_eq!(f["category"], "conditional_rules");
    assert_eq!(f["type"], "if_then");
    assert!(f.get("kind").is_none());
    assert!(f.get("confidence").is_none());
    assert!(f["evidence"].is_array());
    assert!(report.to_markdown().contains("conditional_rules"));
}

#[test]
fn ordered_partition_finding_validates_and_survives_selection() {
    // Regression: the Changes validation allowlist is a hard-coded
    // set of type strings; a new Changes type that is missing from
    // it is silently dropped by selection.
    let finding = Finding::new(
        Pattern::OrderedPartition,
        BTreeMap::from([
            ("y".into(), json!("nv_unemployment_rate")),
            ("time_column".into(), json!("date")),
            ("model".into(), json!("level + linear trend")),
        ]),
        "nv_unemployment_rate follows 3 level+trend regimes along date".into(),
        EvidenceBasis::Measured,
        0.8,
        BTreeMap::from([
            ("response".into(), json!("nv_unemployment_rate")),
            ("relative_gain".into(), json!(0.62)),
            ("n".into(), json!(609)),
        ]),
        None,
        Method::new("ordered_partitions", "mob_level_trend", "test"),
    );
    assert_eq!(finding.finding_type.as_deref(), Some("ordered_partition"));
    assert_eq!(finding.category, Category::Changes);
    assert!(finding.validate().is_ok(), "{:?}", finding.validate());
    let selected = finalize_ledger(vec![finding]);
    assert_eq!(selected.len(), 1);
    assert_eq!(
        selected[0].finding_type.as_deref(),
        Some("ordered_partition")
    );
}

#[test]
fn facets_derive_the_stated_category_for_every_pattern() {
    use crate::finding::Pattern::*;
    let patterns = [
        MissingValues,
        SentinelValues,
        ValueConcentration,
        Hotspot,
        CategoricalRole,
        IdentifierRole,
        Uniqueness,
        Duplicates,
        FormatVariation,
        Heaping,
        DigitPreference,
        BenfordDeviation,
        FunctionalDependency,
        OrderConstraint,
        OrderViolation,
        AffineDependency,
        ArithmeticIdentity,
        PredictiveDependency,
        PiecewiseDependency,
        ConditionalRule,
        StatisticalAssociation,
        PeriodAssociation,
        Gap,
        SequenceGap,
        MissingCombinations,
        IntervalVariation,
        ColumnOrdering,
        ResidualShift,
        LevelShift,
        TrendChange,
        VarianceChange,
        LongFormat,
        ExtremeValue,
        CategoricalOutlier,
        TransientExcursion,
        StepDiscontinuity,
        DuplicateEncoding,
        DistributionShift,
        DistributionChange,
        OrderedPartition,
        OneWayFlag,
    ];
    for pattern in patterns {
        let directed = matches!(
            pattern,
            FunctionalDependency
                | OrderConstraint
                | OrderViolation
                | AffineDependency
                | ArithmeticIdentity
                | PredictiveDependency
                | PiecewiseDependency
                | ConditionalRule
        );
        let subject = Subject::columns(["a".to_string(), "b".to_string()]);
        let facets = Facets::for_pattern(
            pattern,
            &Specification::default(),
            &Details::new(),
            &subject,
        );
        assert_eq!(
            facets.derive_category(directed),
            pattern.category(),
            "pattern {pattern:?}"
        );
    }
}

#[test]
fn conditional_rule_and_dependency_differ_only_in_conditionality() {
    let subject = Subject::columns(["a".to_string(), "b".to_string()]);
    let fd = Facets::for_pattern(
        Pattern::FunctionalDependency,
        &Specification::default(),
        &Details::new(),
        &subject,
    );
    let rule = Facets::for_pattern(
        Pattern::ConditionalRule,
        &Specification::default(),
        &Details::new(),
        &subject,
    );
    assert_eq!(fd.scope, rule.scope);
    assert_eq!(fd.object, rule.object);
    assert_eq!(fd.evidence, rule.evidence);
    assert_eq!(fd.temporality, rule.temporality);
    assert_eq!(fd.status, rule.status);
    assert_ne!(fd.conditionality, rule.conditionality);
}

fn fd_on(lhs: &str, rhs: &str, detector_score: f64) -> Finding {
    Finding::new(
        Pattern::FunctionalDependency,
        BTreeMap::from([("lhs".into(), json!([lhs])), ("rhs".into(), json!(rhs))]),
        format!("{lhs} → {rhs}"),
        EvidenceBasis::Measured,
        detector_score,
        BTreeMap::from([
            ("violations".into(), json!(0)),
            ("n_used".into(), json!(100)),
        ]),
        None,
        Method::new("functional_dependencies", "value_mapping", "test"),
    )
}

#[test]
fn per_detector_cap_limits_and_counts_statements() {
    // Four distinct functional dependencies plus one key finding;
    // a cap of 2 on functional_dependencies selects the two
    // highest-ranked and counts the other two as capped. The key
    // finding's detector is uncapped and is always selected.
    let findings = vec![
        fd_on("a", "w", 0.95),
        fd_on("b", "x", 0.90),
        fd_on("c", "y", 0.85),
        fd_on("d", "z", 0.80),
        key_finding(None),
    ];
    let policy = SelectionPolicy {
        max_findings_per_detector: BTreeMap::from([("functional_dependencies".to_string(), 2)]),
        ..Default::default()
    };
    let outcome = select_findings_counted(findings, &policy);
    assert_eq!(outcome.selected.len(), 3);
    assert_eq!(
        outcome
            .selected
            .iter()
            .filter(|f| f.category == Category::Dependencies)
            .count(),
        2
    );
    assert_eq!(outcome.capped.get("functional_dependencies"), Some(&2));
    assert_eq!(outcome.capped.len(), 1);
}

fn exact_fd(lhs: &[&str], rhs: &str, score: f64) -> Finding {
    fd_with_violations(lhs, rhs, score, 0)
}

fn fd_with_violations(lhs: &[&str], rhs: &str, score: f64, violations: u64) -> Finding {
    let mut subject = Subject {
        columns: lhs
            .iter()
            .map(|s| s.to_string())
            .chain([rhs.to_string()])
            .collect(),
        inputs: lhs.iter().map(|s| s.to_string()).collect(),
        target: Some(rhs.to_string()),
        unit: None,
    };
    subject.canonicalize();
    Finding::from_statement(
        Category::Dependencies,
        Some("functional".into()),
        subject,
        Specification {
            form: Some("value_mapping".into()),
            ..Default::default()
        },
        format!("{} determines {}", lhs.join(", "), rhs),
        Evidence::new(
            Method::new("functional_dependencies", "value_mapping", "test"),
            EvidenceBasis::Measured,
            BTreeMap::from([
                ("violations".into(), json!(violations)),
                ("n_used".into(), json!(100)),
            ]),
        ),
        score,
        None,
    )
}

fn bijection(a: &str, b: &str, score: f64) -> Finding {
    let mut subject = Subject {
        columns: vec![a.to_string(), b.to_string()],
        ..Default::default()
    };
    subject.canonicalize();
    Finding::from_statement(
        Category::ColumnRoles,
        Some("duplicate_encoding".into()),
        subject,
        Specification {
            form: Some("duplicate_encoding".into()),
            ..Default::default()
        },
        format!("{a} and {b} encode the same values"),
        Evidence::new(
            Method::new("duplicate_encodings", "duplicate_encoding", "test"),
            EvidenceBasis::Measured,
            BTreeMap::from([
                ("exact".into(), json!(true)),
                ("violations_ab".into(), json!(0)),
                ("violations_ba".into(), json!(0)),
            ]),
        ),
        score,
        None,
    )
}

fn identity(form: &str, inputs: &[&str], target: &str, score: f64) -> Finding {
    let coefficients: Vec<f64> = match form {
        "difference" => inputs
            .iter()
            .enumerate()
            .map(|(i, _)| if i == 0 { 1.0 } else { -1.0 })
            .collect(),
        _ => vec![1.0; inputs.len()],
    };
    identity_with_coefficients(form, inputs, target, score, coefficients)
}

fn identity_with_coefficients(
    form: &str,
    inputs: &[&str],
    target: &str,
    score: f64,
    coefficients: Vec<f64>,
) -> Finding {
    let mut subject = Subject {
        columns: inputs
            .iter()
            .map(|s| s.to_string())
            .chain([target.to_string()])
            .collect(),
        inputs: inputs.iter().map(|s| s.to_string()).collect(),
        target: Some(target.to_string()),
        unit: None,
    };
    subject.canonicalize();
    let claim = format!("identity over {}", subject.columns.join(", "));
    Finding::from_statement(
        Category::Dependencies,
        Some("functional".into()),
        subject,
        Specification {
            form: Some(form.into()),
            ..Default::default()
        },
        claim,
        Evidence::new(
            Method::new("identities", "multiterm_scan", "test"),
            EvidenceBasis::Measured,
            BTreeMap::from([
                ("violations".into(), json!(0)),
                ("form".into(), json!(form)),
                ("terms".into(), json!(inputs)),
                ("coefficients".into(), json!(coefficients)),
                ("n".into(), json!(100)),
            ]),
        ),
        score,
        None,
    )
}

#[test]
fn bijection_family_groups_both_fd_directions() {
    // The FDs outrank the encoding statement, so the family is
    // triggered by its first member; the lead is admitted there.
    // A predictive statement over the same columns is not an exact
    // restatement and is never absorbed.
    let predictive = Finding::from_statement(
        Category::Dependencies,
        Some("predictive".into()),
        Subject {
            columns: vec!["a".into(), "b".into()],
            inputs: vec!["a".into()],
            target: Some("b".into()),
            unit: None,
        },
        Specification::default(),
        "a predicts b".into(),
        Evidence::new(
            Method::new("gbm", "hist_gradient_boosting", "test"),
            EvidenceBasis::Measured,
            BTreeMap::from([("n".into(), json!(100))]),
        ),
        0.9,
        None,
    );
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["a"], "b", 0.95),
            exact_fd(&["b"], "a", 0.94),
            bijection("a", "b", 0.80),
            predictive,
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 1);
    assert_eq!(outcome.grouped.len(), 2);
    let lead = outcome
        .selected
        .iter()
        .find(|f| f.finding_type.as_deref() == Some("duplicate_encoding"))
        .expect("the encoding statement leads");
    assert_eq!(lead.grouped_members.len(), 2);
    for member in &outcome.grouped {
        assert_eq!(member.grouped_under.as_deref(), Some(lead.id.as_str()));
        assert!(lead.grouped_members.contains(&member.id));
        assert_eq!(member.finding_type.as_deref(), Some("functional"));
    }
    let predictive = outcome
        .selected
        .iter()
        .find(|f| f.finding_type.as_deref() == Some("predictive"))
        .expect("the prediction stays selected on its own");
    assert!(predictive.grouped_members.is_empty());
    assert_eq!(outcome.selected.len(), 2);
}

fn rollup(finer: &str, coarser: &str, score: f64) -> Finding {
    let mut subject = Subject {
        columns: vec![finer.to_string(), coarser.to_string()],
        ..Default::default()
    };
    subject.canonicalize();
    Finding::from_statement(
        Category::ColumnRoles,
        Some("duplicate_encoding".into()),
        subject,
        Specification {
            form: Some("rollup".into()),
            ..Default::default()
        },
        format!("{finer} rolls up to {coarser}"),
        Evidence::new(
            Method::new("duplicate_encodings", "rollup", "test"),
            EvidenceBasis::Measured,
            BTreeMap::from([
                ("finer".into(), json!(finer)),
                ("coarser".into(), json!(coarser)),
                ("exceptions".into(), json!(0)),
                ("n_used".into(), json!(100)),
            ]),
        ),
        score,
        None,
    )
}

fn declared_hierarchy(levels: &[&str], violations: u64, score: f64) -> Finding {
    let mut subject = Subject {
        columns: levels.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    };
    subject.canonicalize();
    Finding::from_statement(
        Category::ColumnRoles,
        Some("duplicate_encoding".into()),
        subject,
        Specification {
            form: Some("hierarchy".into()),
            ..Default::default()
        },
        format!("declared hierarchy: {}", levels.join(" -> ")),
        Evidence::new(
            Method::new("expectations", "declared_hierarchy", "test"),
            EvidenceBasis::Measured,
            BTreeMap::from([
                ("reference_source".into(), json!("declared")),
                ("violations".into(), json!(violations)),
                (
                    "hierarchy_levels".into(),
                    json!(levels.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
                ),
                ("n_used".into(), json!(100)),
            ]),
        ),
        score,
        None,
    )
}

#[test]
fn hierarchy_chain_groups_pairwise_statements() {
    // unit -> team -> division -> company, stated pair by pair:
    // the adjacent FDs, two skip-level FDs, and the adjacent
    // rollups. One constructed lead states the chain; every
    // pairwise statement groups under it.
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["unit"], "team", 0.95),
            exact_fd(&["team"], "division", 0.94),
            exact_fd(&["division"], "company", 0.93),
            exact_fd(&["unit"], "division", 0.92),
            exact_fd(&["unit"], "company", 0.91),
            rollup("unit", "team", 0.90),
            rollup("team", "division", 0.89),
            rollup("division", "company", 0.88),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 1);
    assert_eq!(outcome.selected.len(), 1);
    assert_eq!(outcome.grouped.len(), 8);
    let lead = &outcome.selected[0];
    assert_eq!(lead.specification.form.as_deref(), Some("hierarchy"));
    assert_eq!(
        lead.subject.columns,
        vec!["company", "division", "team", "unit"]
    );
    assert!(lead.validate().is_ok(), "{:?}", lead.validate());
    assert!(
        lead.claim
            .contains("unit → team → division → company form one hierarchy"),
        "{}",
        lead.claim
    );
    let metrics = lead.metrics();
    assert_eq!(
        metrics["hierarchy_levels"],
        json!([["unit"], ["team"], ["division"], ["company"]])
    );
    let mappings = metrics["hierarchy_mappings"].as_array().unwrap();
    assert_eq!(mappings.len(), 3);
    assert_eq!(mappings[0]["finer"], json!(["unit"]));
    assert_eq!(mappings[0]["coarser"], json!(["team"]));
    assert_eq!(mappings[0]["rows_evaluated"], json!(100));
    let relations = metrics["member_relations"].as_array().unwrap();
    assert_eq!(relations.len(), 8);
    assert_eq!(lead.grouped_members.len(), 8);
    for member in &outcome.grouped {
        assert_eq!(member.grouped_under.as_deref(), Some(lead.id.as_str()));
    }
}

#[test]
fn two_independent_chains_stay_two_families() {
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["a1"], "a2", 0.95),
            exact_fd(&["a2"], "a3", 0.94),
            exact_fd(&["a3"], "a4", 0.93),
            exact_fd(&["b1"], "b2", 0.92),
            exact_fd(&["b2"], "b3", 0.91),
            exact_fd(&["b3"], "b4", 0.90),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 2);
    assert_eq!(outcome.selected.len(), 2);
    assert_eq!(outcome.grouped.len(), 6);
    for lead in &outcome.selected {
        assert_eq!(lead.specification.form.as_deref(), Some("hierarchy"));
    }
}

#[test]
fn branching_structure_is_not_a_chain() {
    // a and b both roll up to c, and c determines d: a and b are
    // incomparable, so the component is a merge, not a chain, and
    // no statement is grouped.
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["a"], "c", 0.95),
            exact_fd(&["b"], "c", 0.94),
            exact_fd(&["c"], "d", 0.93),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 0);
    assert!(outcome.grouped.is_empty());
    assert_eq!(outcome.selected.len(), 3);
}

#[test]
fn equivalence_class_inside_a_chain_is_one_level() {
    // a and b determine each other (one level stated twice), then
    // b -> c -> d: the chain's first level is the class {a, b}.
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["a"], "b", 0.95),
            exact_fd(&["b"], "a", 0.94),
            exact_fd(&["b"], "c", 0.93),
            exact_fd(&["c"], "d", 0.92),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 1);
    assert_eq!(outcome.selected.len(), 1);
    assert_eq!(outcome.grouped.len(), 4);
    let lead = &outcome.selected[0];
    assert_eq!(
        lead.metrics()["hierarchy_levels"],
        json!([["a", "b"], ["c"], ["d"]])
    );
    assert!(lead.claim.contains("a + b → c → d"), "{}", lead.claim);
}

#[test]
fn pure_equivalence_structure_collapses() {
    // Three columns pairwise determining each other: one grouping
    // stated six times.
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["a"], "b", 0.95),
            exact_fd(&["b"], "a", 0.94),
            exact_fd(&["b"], "c", 0.93),
            exact_fd(&["c"], "b", 0.92),
            exact_fd(&["a"], "c", 0.91),
            exact_fd(&["c"], "a", 0.90),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 1);
    assert_eq!(outcome.selected.len(), 1);
    assert_eq!(outcome.grouped.len(), 6);
    let lead = &outcome.selected[0];
    assert_eq!(lead.metrics()["hierarchy_levels"], json!([["a", "b", "c"]]));
    assert!(
        lead.claim.contains("encode the same grouping one-to-one"),
        "{}",
        lead.claim
    );
}

#[test]
fn declared_hierarchy_leads_its_restatements() {
    // The declared statement outranks nothing here — the FDs score
    // higher — but the family still forms under the declared lead
    // when its first member is reached.
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["unit"], "team", 0.95),
            exact_fd(&["team"], "division", 0.94),
            rollup("unit", "team", 0.90),
            declared_hierarchy(&["unit", "team", "division"], 0, 0.80),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 1);
    assert_eq!(outcome.selected.len(), 1);
    assert_eq!(outcome.grouped.len(), 3);
    let lead = &outcome.selected[0];
    assert_eq!(
        lead.evidence[0].method.detector, "expectations",
        "the declared statement leads"
    );
    assert_eq!(lead.grouped_members.len(), 3);
}

#[test]
fn broken_declared_hierarchy_does_not_lead() {
    // The declaration records violations, so it cannot lead; the
    // inferred chain still forms over the exact statements, and
    // the deviation statement competes on its own.
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["unit"], "team", 0.95),
            exact_fd(&["team"], "division", 0.94),
            rollup("unit", "team", 0.90),
            declared_hierarchy(&["unit", "team", "division"], 3, 0.85),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 1);
    assert_eq!(outcome.grouped.len(), 3);
    let lead = outcome
        .selected
        .iter()
        .find(|f| f.specification.form.as_deref() == Some("hierarchy"))
        .expect("the constructed chain lead is selected");
    assert!(
        lead.metrics().get("reference_source").is_none(),
        "the lead is the constructed one, not the declaration"
    );
    assert!(
        outcome
            .selected
            .iter()
            .any(|f| f.evidence[0].method.detector == "expectations"),
        "the deviation statement stays selected on its own"
    );
}

#[test]
fn cap_blocked_chain_lead_leaves_members_independent() {
    // The constructed lead inherits the rollup's detector for cap
    // attribution; capping that detector at zero turns the lead
    // away. The family does not form, the FDs are selected on
    // their own, and the capped count names only the detector's
    // own statements (the three rollups), not the lead.
    let policy = SelectionPolicy {
        max_findings_per_detector: BTreeMap::from([("duplicate_encodings".to_string(), 0)]),
        ..Default::default()
    };
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["unit"], "team", 0.95),
            exact_fd(&["team"], "division", 0.94),
            exact_fd(&["division"], "company", 0.93),
            rollup("unit", "team", 0.90),
            rollup("team", "division", 0.89),
            rollup("division", "company", 0.88),
        ],
        &policy,
    );
    assert_eq!(outcome.families, 0);
    assert!(outcome.grouped.is_empty());
    assert_eq!(outcome.selected.len(), 3);
    assert_eq!(outcome.capped.get("duplicate_encodings"), Some(&3));
}

#[test]
fn identity_family_groups_rotations_and_fds() {
    // cnt = casual + registered, its subtraction rotation, and the
    // three FDs over the same columns: one family, the sum leads.
    let outcome = select_findings_counted(
        vec![
            identity("sum", &["casual", "registered"], "cnt", 0.90),
            identity("difference", &["cnt", "registered"], "casual", 0.89),
            exact_fd(&["casual", "registered"], "cnt", 0.88),
            exact_fd(&["cnt", "registered"], "casual", 0.87),
            exact_fd(&["cnt", "casual"], "registered", 0.86),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 1);
    assert_eq!(outcome.selected.len(), 1);
    assert_eq!(outcome.grouped.len(), 4);
    let lead = &outcome.selected[0];
    assert_eq!(lead.specification.form.as_deref(), Some("sum"));
    assert_eq!(lead.grouped_members.len(), 4);
    for member in &outcome.grouped {
        assert_eq!(member.grouped_under.as_deref(), Some(lead.id.as_str()));
    }
}

#[test]
fn linear_combination_rotations_group_with_the_sum() {
    // The scan reports the sum in one direction and fitted linear
    // combinations in the rotated directions (b = -a + c,
    // a = -b + c, coefficients [intercept, ...]). The coefficient
    // vectors are proportional, so all three are one family and
    // the sum leads.
    let outcome = select_findings_counted(
        vec![
            identity("sum", &["a", "b"], "c", 0.90),
            identity_with_coefficients(
                "linear_combination",
                &["a", "c"],
                "b",
                0.89,
                vec![0.0, -1.0, 1.0],
            ),
            identity_with_coefficients(
                "linear_combination",
                &["b", "c"],
                "a",
                0.88,
                vec![0.0, -1.0, 1.0],
            ),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 1);
    assert_eq!(outcome.selected.len(), 1);
    assert_eq!(
        outcome.selected[0].specification.form.as_deref(),
        Some("sum")
    );
    assert_eq!(outcome.grouped.len(), 2);
}

#[test]
fn distinct_relations_over_one_column_set_do_not_merge() {
    // c = a + b and c = 2a + b over the same columns are different
    // relations; their coefficient vectors are not proportional,
    // so neither groups under the other.
    let outcome = select_findings_counted(
        vec![
            identity("sum", &["a", "b"], "c", 0.90),
            identity_with_coefficients(
                "linear_combination",
                &["a", "b"],
                "c",
                0.89,
                vec![0.0, 2.0, 1.0],
            ),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 0);
    assert!(outcome.grouped.is_empty());
    assert_eq!(outcome.selected.len(), 2);
}

#[test]
fn fd_rotation_cluster_forms_a_family_without_an_identity() {
    // The FD signature of an exact relation: every column is the
    // target of an FD on the rest.
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["casual", "registered"], "cnt", 0.90),
            exact_fd(&["cnt", "registered"], "casual", 0.89),
            exact_fd(&["cnt", "casual"], "registered", 0.88),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 1);
    assert_eq!(outcome.selected.len(), 1);
    assert_eq!(outcome.grouped.len(), 2);
}

#[test]
fn partial_fd_cluster_does_not_group() {
    // Two of the three rotation directions: neither FD implies
    // the other, so both stay independent statements.
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["casual", "registered"], "cnt", 0.90),
            exact_fd(&["cnt", "registered"], "casual", 0.89),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 0);
    assert!(outcome.grouped.is_empty());
    assert_eq!(outcome.selected.len(), 2);
}

#[test]
fn inexact_or_scoped_statements_do_not_group() {
    // An FD with violations is not exact; a conditional FD is
    // scoped. The bijection is left without exact members, so no
    // family forms and every statement competes on its own.
    let mut conditional = exact_fd(&["b"], "a", 0.93);
    conditional.specification.conditions = vec![Predicate::equal("segment", json!("premium"))];
    let outcome = select_findings_counted(
        vec![
            fd_with_violations(&["a"], "b", 0.95, 3),
            conditional,
            bijection("a", "b", 0.80),
        ],
        &SelectionPolicy::default(),
    );
    assert_eq!(outcome.families, 0);
    assert!(outcome.grouped.is_empty());
    assert_eq!(outcome.selected.len(), 3);
}

#[test]
fn cap_blocked_lead_leaves_members_independent() {
    // The lead's detector is capped at zero: the family does not
    // form, the lead is counted as capped, and the FDs are
    // selected on their own merits.
    let policy = SelectionPolicy {
        max_findings_per_detector: BTreeMap::from([("duplicate_encodings".to_string(), 0)]),
        ..Default::default()
    };
    let outcome = select_findings_counted(
        vec![
            exact_fd(&["a"], "b", 0.95),
            exact_fd(&["b"], "a", 0.94),
            bijection("a", "b", 0.80),
        ],
        &policy,
    );
    assert_eq!(outcome.families, 0);
    assert!(outcome.grouped.is_empty());
    assert_eq!(outcome.selected.len(), 2);
    assert_eq!(outcome.capped.get("duplicate_encodings"), Some(&1));
}

#[test]
fn default_policy_caps_the_char_heavy_detectors() {
    let policy = SelectionPolicy::default();
    assert_eq!(
        policy
            .max_findings_per_detector
            .get("functional_dependencies"),
        Some(&25)
    );
    assert_eq!(policy.max_findings_per_detector.get("gbm"), Some(&20));
    // The 2026-11-04 battery added discontinuities at 25: its
    // statements outscore every other change detector on panels and
    // took 81 of 100 slots on state-unemployment.
    assert_eq!(
        policy.max_findings_per_detector.get("discontinuities"),
        Some(&25)
    );
    assert_eq!(
        policy.max_findings_per_detector.get("missingness"),
        Some(&20)
    );
    assert_eq!(policy.max_findings_per_detector.get("series"), Some(&20));
    // Issue #271 caps breaks and views too, but only under cap
    // pressure: they live in `pressure_detector_caps`, not in
    // this standing map (see
    // `default_policy_carries_the_group_cap_reserve_and_extended_detector_caps`).
    assert_eq!(policy.max_findings_per_detector.len(), 6);
}

#[test]
fn selection_orders_by_report_band_before_score() {
    // Frame-model step 2: the finding list is ordered account
    // first, then relations, then deviations — a high-scoring
    // change never outranks the account it deviates from.
    let change = Finding::new(
        Pattern::StepDiscontinuity,
        BTreeMap::from([
            ("column".into(), json!("value")),
            ("time_column".into(), json!("date")),
        ]),
        "value steps at 2024-06".into(),
        EvidenceBasis::Measured,
        0.99,
        BTreeMap::from([("n".into(), json!(100))]),
        None,
        Method::new("breaks", "step_discontinuity", "test"),
    );
    assert_eq!(change.category, Category::Changes);
    let missing = Finding::new(
        Pattern::MissingValues,
        BTreeMap::from([("column".into(), json!("value"))]),
        "value is missing in 12% of rows".into(),
        EvidenceBasis::Measured,
        0.4,
        BTreeMap::from([("n".into(), json!(100))]),
        None,
        Method::new("column_profile", "missing_values", "test"),
    );
    let selected = finalize_ledger(vec![change, fd_finding(0.5), missing, key_finding(None)]);
    let order: Vec<Category> = selected.iter().map(|f| f.category).collect();
    assert_eq!(
        order,
        vec![
            Category::Uniqueness,   // band 0: the account's key statement
            Category::Missingness,  // band 1: completeness
            Category::Dependencies, // band 2: relations
            Category::Changes,      // band 4: deviations, despite the top detector score
        ]
    );
}

#[test]
fn frame_scope_findings_lead_the_report() {
    // A frame-scope statement (the account's grain / near-key
    // findings) is band 0 whatever its category.
    let mut near_key = Finding::new(
        Pattern::Duplicates,
        BTreeMap::from([("columns".into(), json!(["date_time"]))]),
        "the time axis does not identify rows".into(),
        EvidenceBasis::Measured,
        0.75,
        BTreeMap::from([("n".into(), json!(100))]),
        None,
        Method::new("keys", "near_key_axis", "test"),
    );
    let mut facets = near_key.facets;
    facets.scope = Scope::Frame;
    near_key = near_key.with_facets(facets);
    assert_eq!(report_band(&near_key), 0);
    let selected = finalize_ledger(vec![fd_finding(0.9), near_key]);
    assert_eq!(selected[0].category, Category::Duplicates);
    assert_eq!(selected[1].category, Category::Dependencies);
}

// ---------- statement classes and rationed selection (issue #271) ----------

fn classed(category: Category, typ: &str, col: &str, score: f64) -> Finding {
    Finding::from_statement(
        category,
        Some(typ.into()),
        Subject {
            columns: vec![col.into()],
            ..Subject::default()
        },
        Specification::default(),
        format!("statement about {col}"),
        evidence("survey_detector"),
        score,
        None,
    )
}

#[test]
fn statement_class_first_matching_rule_decides() {
    // Account: frame scope, whatever the category.
    let mut f = classed(Category::Missingness, "missing_values", "a", 0.5);
    let mut facets = f.facets;
    facets.scope = Scope::Frame;
    f = f.with_facets(facets);
    assert_eq!(statement_class(&f), StatementClass::Account);
    // Account: the uniqueness category.
    let f = classed(Category::Uniqueness, "unique_key", "a", 0.5);
    assert_eq!(statement_class(&f), StatementClass::Account);
    // Structure: duplicate_encoding by type, whatever the category.
    let f = classed(Category::Intervals, "duplicate_encoding", "a", 0.5);
    assert_eq!(statement_class(&f), StatementClass::Structure);
    // Structure: the relation categories.
    let f = classed(Category::Dependencies, "functional", "a", 0.5);
    assert_eq!(statement_class(&f), StatementClass::Structure);
    let f = classed(Category::Associations, "statistical", "a", 0.5);
    assert_eq!(statement_class(&f), StatementClass::Structure);
    // Structure: a conditioned statement of any other category.
    let mut f = classed(Category::Missingness, "missing_values", "a", 0.5);
    let mut facets = f.facets;
    facets.conditionality = Conditionality::SegmentConditioned;
    f = f.with_facets(facets);
    assert_eq!(statement_class(&f), StatementClass::Structure);
    // Shape: changes, and extreme values in intervals.
    let f = classed(Category::Changes, "level_shift", "a", 0.5);
    assert_eq!(statement_class(&f), StatementClass::Shape);
    let f = classed(Category::Intervals, "extreme_value", "a", 0.5);
    assert_eq!(statement_class(&f), StatementClass::Shape);
    // Hygiene: everything else.
    let f = classed(Category::Intervals, "value_range", "a", 0.5);
    assert_eq!(statement_class(&f), StatementClass::Hygiene);
    let f = classed(Category::Missingness, "missing_values", "a", 0.5);
    assert_eq!(statement_class(&f), StatementClass::Hygiene);
}

fn fd_col(col: &str, score: f64) -> Finding {
    Finding::new(
        Pattern::FunctionalDependency,
        BTreeMap::from([
            ("lhs".into(), json!([col])),
            ("rhs".into(), json!("region")),
        ]),
        format!("{col} determines region"),
        EvidenceBasis::Measured,
        score,
        BTreeMap::from([
            ("violations".into(), json!(0)),
            ("n_used".into(), json!(100)),
        ]),
        None,
        Method::new("survey_detector", "value_mapping", "test"),
    )
}

fn key_on(col: &str, score: f64) -> Finding {
    Finding::new(
        Pattern::Uniqueness,
        BTreeMap::from([("column".into(), json!(col))]),
        format!("{col} uniquely identifies rows"),
        EvidenceBasis::Measured,
        score,
        BTreeMap::from([("distinct".into(), json!(100)), ("n".into(), json!(100))]),
        None,
        Method::new("survey_detector", "uniqueness", "test"),
    )
}

fn missing_on(col: &str, score: f64) -> Finding {
    Finding::new(
        Pattern::MissingValues,
        BTreeMap::from([("column".into(), json!(col))]),
        format!("{col} is missing in 12% of rows"),
        EvidenceBasis::Measured,
        score,
        BTreeMap::from([("n".into(), json!(100))]),
        None,
        Method::new("survey_detector", "missing_values", "test"),
    )
}

fn dup_on(col: &str, score: f64) -> Finding {
    Finding::new(
        Pattern::Duplicates,
        BTreeMap::from([("columns".into(), json!([col]))]),
        format!("rows repeat on {col}"),
        EvidenceBasis::Measured,
        score,
        BTreeMap::from([("n".into(), json!(100))]),
        None,
        Method::new("survey_detector", "duplicate_rows", "test"),
    )
}

fn step_on(col: &str, score: f64) -> Finding {
    Finding::new(
        Pattern::StepDiscontinuity,
        BTreeMap::from([
            ("column".into(), json!(col)),
            ("time_column".into(), json!("date")),
        ]),
        format!("{col} steps at 2024-06"),
        EvidenceBasis::Measured,
        score,
        BTreeMap::from([("n".into(), json!(100))]),
        None,
        Method::new("survey_detector", "step_discontinuity", "test"),
    )
}

#[test]
fn selection_without_pressure_is_unrationed() {
    let findings = vec![fd_col("a", 0.9), missing_on("b", 0.8), step_on("c", 0.7)];
    let outcome = select_findings_counted(findings, &SelectionPolicy::default());
    assert_eq!(outcome.selected.len(), 3);
    assert!(outcome.grouped.is_empty());
    assert!(outcome.group_capped.is_empty());
    assert_eq!(outcome.reserve_capped, 0);
    // No cap pressure: every candidate is selected and no
    // per-class counts are recorded.
    assert!(outcome.candidates_by_class.is_empty());
    let claims: std::collections::BTreeSet<&str> =
        outcome.selected.iter().map(|f| f.claim.as_str()).collect();
    assert_eq!(
        claims,
        std::collections::BTreeSet::from([
            "a determines region",
            "b is missing in 12% of rows",
            "c steps at 2024-06",
        ])
    );
}

#[test]
fn group_cap_does_not_bind_without_pressure() {
    // Fifteen findings in one (category, type, form) group, under
    // the findings cap: all are selected, exactly as before.
    let findings: Vec<Finding> = (0..15).map(|i| fd_col(&format!("s{i}"), 0.9)).collect();
    let outcome = select_findings_counted(findings, &SelectionPolicy::default());
    assert_eq!(outcome.selected.len(), 15);
    assert!(outcome.group_capped.is_empty());
}

#[test]
fn group_cap_and_hygiene_reserve_bind_under_pressure() {
    // 145 distinct candidates against a cap of 100: 15 account,
    // 30 structure findings in a single group, and 100 hygiene
    // findings in two groups of 50 (so the reserve, not the
    // group cap alone, is what stops hygiene at 20).
    let mut findings = Vec::new();
    for i in 0..30 {
        findings.push(fd_col(&format!("s{i}"), 0.95));
    }
    for i in 0..50 {
        findings.push(missing_on(&format!("h{i}"), 0.5));
    }
    for i in 0..50 {
        findings.push(dup_on(&format!("d{i}"), 0.5));
    }
    for i in 0..15 {
        findings.push(key_on(&format!("k{i}"), 0.6));
    }
    let outcome = select_findings_counted(findings, &SelectionPolicy::default());
    // Account admitted in full, the structure group capped at 12,
    // hygiene admitted last up to the reserve of 20: the report
    // is shorter than the cap rather than padded with hygiene.
    assert_eq!(outcome.selected.len(), 15 + 12 + 20);
    assert_eq!(outcome.selected_by_class.get("account"), Some(&15));
    assert_eq!(outcome.selected_by_class.get("structure"), Some(&12));
    assert_eq!(outcome.selected_by_class.get("hygiene"), Some(&20));
    assert_eq!(outcome.candidates_by_class.get("account"), Some(&15));
    assert_eq!(outcome.candidates_by_class.get("structure"), Some(&30));
    assert_eq!(outcome.candidates_by_class.get("hygiene"), Some(&100));
    // The structure group turns away 18, the first hygiene group
    // 38 at its group cap, and the remaining 42 hygiene
    // statements meet the filled reserve.
    assert_eq!(outcome.group_capped.values().sum::<usize>(), 18 + 38);
    assert_eq!(outcome.reserve_capped, 42);
    assert_eq!(outcome.capped_by_class.get("structure"), Some(&18));
    assert_eq!(outcome.capped_by_class.get("hygiene"), Some(&80));
}

#[test]
fn default_policy_carries_the_group_cap_reserve_and_extended_detector_caps() {
    let policy = SelectionPolicy::default();
    assert_eq!(policy.max_findings_per_group, 12);
    assert_eq!(policy.hygiene_reserve, 20);
    // The extended detector caps bind only under cap pressure:
    // breaks and views at 25 each, in the pressure map.
    assert_eq!(policy.pressure_detector_caps.get("breaks"), Some(&25));
    assert_eq!(policy.pressure_detector_caps.get("views"), Some(&25));
    assert_eq!(policy.pressure_detector_caps.len(), 2);
    // The six standing per-detector caps are unchanged, and
    // neither new detector joins them.
    assert_eq!(
        policy
            .max_findings_per_detector
            .get("functional_dependencies"),
        Some(&25)
    );
    assert_eq!(policy.max_findings_per_detector.get("gbm"), Some(&20));
    assert!(!policy.max_findings_per_detector.contains_key("breaks"));
    assert!(!policy.max_findings_per_detector.contains_key("views"));
}

fn break_on(col: &str, score: f64) -> Finding {
    break_typed(Pattern::StepDiscontinuity, col, score)
}

fn break_typed(pattern: Pattern, col: &str, score: f64) -> Finding {
    Finding::new(
        pattern,
        BTreeMap::from([
            ("column".into(), json!(col)),
            ("time_column".into(), json!("date")),
        ]),
        format!("{col} changes at 2024-06"),
        EvidenceBasis::Measured,
        score,
        BTreeMap::from([("n".into(), json!(100))]),
        None,
        Method::new("breaks", "break_scan", "test"),
    )
}

#[test]
fn pressure_detector_caps_do_not_bind_without_pressure() {
    // Thirty statements from the breaks detector, distinct and
    // under the findings cap: every one is selected, exactly as
    // before issue #271 — the breaks/views caps ration only
    // when the candidate pool exceeds the cap. (The selection
    // study's uncapped control measured 42 views statements
    // selected on an 81-statement report; a standing cap would
    // rewrite that report's membership.)
    let findings: Vec<Finding> = (0..30).map(|i| break_on(&format!("b{i}"), 0.9)).collect();
    let outcome = select_findings_counted(findings, &SelectionPolicy::default());
    assert_eq!(outcome.selected.len(), 30);
    assert!(outcome.capped.is_empty());
}

#[test]
fn pressure_detector_caps_bind_under_pressure() {
    // 140 distinct candidates against a cap of 100: 15 account,
    // 40 breaks statements, 85 hygiene statements. The breaks
    // statements spread over four change types, ten per
    // (category, type, form) group, so the group cap cannot
    // bind first: the pressure-only per-detector cap is what
    // stops breaks at 25 selected, with the 15 turned away
    // counted per-detector. The hygiene statements spread over
    // two groups, so the reserve, not a group cap, is what
    // stops hygiene at 20.
    let mut findings = Vec::new();
    for i in 0..15 {
        findings.push(key_on(&format!("k{i}"), 0.6));
    }
    for pattern in [
        Pattern::LevelShift,
        Pattern::TrendChange,
        Pattern::VarianceChange,
        Pattern::TransientExcursion,
    ] {
        for i in 0..10 {
            findings.push(break_typed(pattern, &format!("b{i}"), 0.95));
        }
    }
    for i in 0..43 {
        findings.push(missing_on(&format!("h{i}"), 0.5));
    }
    for i in 0..42 {
        findings.push(dup_on(&format!("d{i}"), 0.5));
    }
    let outcome = select_findings_counted(findings, &SelectionPolicy::default());
    assert_eq!(outcome.selected.len(), 15 + 25 + 20);
    assert_eq!(outcome.selected_by_class.get("account"), Some(&15));
    assert_eq!(outcome.selected_by_class.get("shape"), Some(&25));
    assert_eq!(outcome.selected_by_class.get("hygiene"), Some(&20));
    assert_eq!(outcome.capped.get("breaks"), Some(&15));
    // capped_by_class counts every turned-away candidate of the
    // class: 15 shape (the pressure cap) and 65 hygiene (the
    // reserve).
    assert_eq!(outcome.capped_by_class.get("shape"), Some(&15));
    assert_eq!(outcome.capped_by_class.get("hygiene"), Some(&65));
}
