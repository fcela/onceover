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
fn report_schema_roundtrips_and_renders() {
    let finding = rule(
        "detector",
        Predicate::equal("segment", json!("premium")),
        Predicate::equal("shipping", json!("expedite")),
    );
    let findings = finalize_ledger(vec![finding]);
    let report = ProfileReport {
        schema_version: SCHEMA_VERSION.into(),
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
        selection: Default::default(),
        coverage: BTreeMap::new(),
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
