//! Planted-ground-truth integration tests: build tables with known
//! structure, assert the profiler recovers it — and that reports are
//! byte-deterministic.

use onceover::{profile, Budget, Category, ProfileConfig};
use polars::prelude::*;

/// Deterministic xorshift64* RNG (same as the profiler's).
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn next_range(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

fn business_days(start: i64, count: usize) -> Vec<i32> {
    let mut out = Vec::with_capacity(count);
    let mut d = start;
    while out.len() < count {
        let dow = (d + 4).rem_euclid(7);
        if dow != 0 && dow != 6 {
            out.push(d as i32);
        }
        d += 1;
    }
    out
}

fn planted_table(n: usize) -> DataFrame {
    let mut rng = Rng(2024);
    let regions = ["emea", "amer", "apac"];
    let mut ids: Vec<Option<i64>> = Vec::with_capacity(n);
    let mut region_id: Vec<Option<i64>> = Vec::new();
    let mut region_name: Vec<Option<String>> = Vec::new();
    let mut category: Vec<Option<String>> = Vec::new();
    let mut amount: Vec<Option<f64>> = Vec::new();
    let mut quantity: Vec<Option<f64>> = Vec::new();
    let mut unit_price: Vec<Option<f64>> = Vec::new();
    let mut total: Vec<Option<f64>> = Vec::new();
    let mut event_date: Vec<Option<i32>> = Vec::new();
    let days = business_days(19_723, n);
    let mut date_idx = 0usize;

    for i in 0..n {
        ids.push(Some(i as i64));
        let r = rng.next_range(3) as usize;
        region_id.push(Some(r as i64));
        region_name.push(Some(regions[r].to_string()));
        category.push(Some(match rng.next_range(4) {
            0 => "electronics".to_string(),
            1 => "clothing".to_string(),
            2 => "food".to_string(),
            _ => "tools".to_string(),
        }));
        let q = 1.0 + (rng.next_range(10) as f64);
        let p = 5.0 + rng.next_f64() * 95.0;
        quantity.push(Some(q));
        unit_price.push(Some((p * 100.0).round() / 100.0));
        amount.push(Some(q * unit_price.last().unwrap().unwrap()));
        total.push(Some(q * unit_price.last().unwrap().unwrap()));
        // Business-daily dates with two planted missing days.
        if i == n / 3 || i == n / 3 + 1 {
            date_idx += 2; // skip two business days
        }
        event_date.push(Some(days[date_idx.min(n - 1)]));
        date_idx += 1;
    }

    // Plant a dirty key: user_ref nearly unique with 3 dupes.
    let mut user_ref: Vec<Option<String>> = (0..n).map(|i| Some(format!("U{i:07}"))).collect();
    for k in 0..3 {
        user_ref[n - 1 - k] = Some(format!("U{:07}", k));
    }

    let ids_s = Series::new("order_id".into(), ids);
    let region_id_s = Series::new("region_id".into(), region_id);
    let region_name_s = Series::new("region_name".into(), region_name);
    let category_s = Series::new("category".into(), category);
    let amount_s = Series::new("amount".into(), amount);
    let quantity_s = Series::new("quantity".into(), quantity);
    let unit_price_s = Series::new("unit_price".into(), unit_price);
    let total_s = Series::new("total".into(), total);
    let user_ref_s = Series::new("user_ref".into(), user_ref);
    let event_date_s = Series::new("event_date".into(), event_date)
        .cast(&DataType::Date)
        .unwrap();

    DataFrame::new_infer_height(vec![
        ids_s.into_column(),
        region_id_s.into_column(),
        region_name_s.into_column(),
        category_s.into_column(),
        amount_s.into_column(),
        quantity_s.into_column(),
        unit_price_s.into_column(),
        total_s.into_column(),
        user_ref_s.into_column(),
        event_date_s.into_column(),
    ])
    .unwrap()
}

#[test]
fn recovers_planted_structure_with_semantic_categories() {
    let df = planted_table(4000);
    let report = profile(&df, &ProfileConfig::default());
    assert_eq!(report.schema_version, "0.2");
    assert!(report
        .findings
        .iter()
        .any(|f| f.category == Category::Uniqueness && f.subject.columns == ["order_id"]));
    assert!(report
        .findings
        .iter()
        .any(|f| f.is(Category::Dependencies, Some("functional"))
            && f.subject.inputs == ["region_id"]
            && f.subject.target.as_deref() == Some("region_name")));
    assert!(report
        .findings
        .iter()
        .any(|f| f.specification.form.as_deref() == Some("affine")
            && f.subject.columns.contains(&"total".into())));
    assert!(report.findings.iter().any(|f| f.category == Category::Gaps));
    assert!(report
        .findings
        .iter()
        .any(|f| f.category == Category::Uniqueness
            && f.subject.columns == ["user_ref"]
            && f.metrics()
                .get("duplicates")
                .and_then(serde_json::Value::as_u64)
                == Some(3)));
    assert!(report
        .findings
        .iter()
        .any(|f| f.category == Category::ColumnRoles && f.subject.columns == ["region_id"]));
    assert!(report
        .findings
        .iter()
        .all(|f| !f.evidence.is_empty() && f.validate().is_ok()));
    assert!(!report.groups.is_empty());
    let json: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
    assert!(json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f.get("kind").is_none()
            && f.get("confidence").is_none()
            && f["evidence"].is_array()));
}

#[test]
fn deterministic_report_and_statement_ids() {
    let df = planted_table(1000);
    assert_eq!(
        profile(&df, &ProfileConfig::default()).to_json(),
        profile(&df, &ProfileConfig::default()).to_json()
    );
}

#[test]
fn quick_tier_and_rendering_preserve_the_contract() {
    let report = profile(
        &planted_table(800),
        &ProfileConfig {
            budget: Budget::Quick,
            ..Default::default()
        },
    );
    assert!(!report.findings.iter().any(|f| matches!(
        f.category,
        Category::Dependencies | Category::ConditionalRules
    )));
    assert!(report.to_markdown().contains("## Findings"));
    assert!(report.to_markdown().contains("## Coverage"));
    assert_eq!(report.coverage["predictability_screen"].status, "skipped");
}

#[test]
fn selected_detector_does_not_emit_its_resource_findings() {
    let report = profile(
        &planted_table(1500),
        &ProfileConfig {
            detectors: Some(["prism".into()].into_iter().collect()),
            ..Default::default()
        },
    );
    assert!(report
        .findings
        .iter()
        .all(|f| f.category == Category::ConditionalRules));
    assert!(report
        .findings
        .iter()
        .all(|f| f.evidence.iter().any(|e| e.method.detector == "prism")));
    assert!(report.coverage["predictability_screen"]
        .detail
        .contains("required as a resource"));
}

#[test]
fn rules_are_structured_and_evaluated_with_common_denominators() {
    let n = 4000;
    let mut rng = Rng(77);
    let tenure: Vec<i64> = (0..n).map(|i| (i % 48) as i64).collect();
    let churn: Vec<i64> = tenure
        .iter()
        .map(|&t| i64::from((t > 24) ^ (rng.next_f64() < 0.06)))
        .collect();
    let df = df!("tenure"=>tenure,"churn"=>churn).unwrap();
    let report = profile(&df, &ProfileConfig::default());
    let rules: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.category == Category::ConditionalRules)
        .collect();
    assert!(!rules.is_empty());
    for rule in rules {
        assert!(!rule.specification.conditions.is_empty());
        let outcome = rule.specification.outcome.as_ref().unwrap();
        assert!(rule
            .specification
            .conditions
            .iter()
            .all(|p| p.column != outcome.column));
        let e = rule
            .evidence
            .iter()
            .find(|e| e.method.detector == "rule_evaluator")
            .unwrap();
        let m = &e.metrics;
        let rate = m["joint_count"].as_f64().unwrap() / m["antecedent_count"].as_f64().unwrap();
        assert!((m["conditional_rate"].as_f64().unwrap() - rate).abs() < 1e-12);
        assert!(e.validation.used_for_selection);
    }
}

#[test]
fn selection_limits_apply_across_rule_detectors() {
    let df = planted_table(2000);
    let report = profile(
        &df,
        &ProfileConfig {
            selection: onceover::SelectionPolicy {
                max_rules: 2,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert!(
        report
            .findings
            .iter()
            .filter(|f| f.category == Category::ConditionalRules)
            .count()
            <= 2
    );
}

#[test]
fn dependence_cap_reports_full_pair_universe() {
    let cols = (0..33)
        .map(|i| Series::new(format!("x{i}").into(), (0..100i64).collect::<Vec<_>>()).into_column())
        .collect();
    let report = profile(
        &DataFrame::new_infer_height(cols).unwrap(),
        &ProfileConfig::default(),
    );
    assert!(report.coverage["dependence_screen"]
        .detail
        .starts_with("496 of 528 numeric pairs (capped)"));
}

#[test]
fn extension_detector_uses_existing_category_and_shared_context() {
    struct SequenceGap;
    impl onceover::Detector for SequenceGap {
        fn descriptor(&self) -> onceover::DetectorDescriptor {
            onceover::DetectorDescriptor::new("integer_sequence", vec![Category::Gaps], vec![])
        }
        fn discover(
            &self,
            ctx: &onceover::DetectorContext<'_, '_>,
        ) -> Result<Vec<onceover::Finding>, String> {
            use onceover::{Evidence, EvidenceBasis, Finding, Method, Specification, Subject};
            use serde_json::json;
            assert_eq!(ctx.table.n, 3);
            assert!(ctx.columns.contains_key("sequence"));
            let mut spec = Specification::default();
            spec.definition.insert("absent_values".into(), json!([2]));
            spec.definition
                .insert("domain".into(), json!("integer_sequence"));
            spec.definition.insert(
                "reference".into(),
                json!({"origin":"declared","increment":1}),
            );
            Ok(vec![Finding::from_statement(
                Category::Gaps,
                None,
                Subject::columns(["sequence".into()]),
                spec,
                "Sequence value 2 is absent".into(),
                Evidence::new(
                    Method::new("integer_sequence", "integer_scan", "test extension"),
                    EvidenceBasis::Measured,
                    std::collections::BTreeMap::from([
                        ("n".into(), json!(3)),
                        ("absent_values".into(), json!([2])),
                    ]),
                ),
                0.5,
                None,
            )])
        }
    }
    let df = df!("sequence"=>[1i64,3,4]).unwrap();
    let cfg = ProfileConfig {
        detectors: Some(["integer_sequence".into()].into_iter().collect()),
        ..Default::default()
    };
    let report = onceover::profile_with_detectors(&df, &cfg, &[&SequenceGap]);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].category, Category::Gaps);
    assert_eq!(
        report.findings[0].specification.definition["domain"],
        serde_json::json!("integer_sequence")
    );
    assert_eq!(
        report.findings[0].evidence[0].method.detector,
        "integer_sequence"
    );
}
