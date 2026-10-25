//! Planted-ground-truth integration tests: build tables with known
//! structure, assert the profiler recovers it — and that reports are
//! byte-deterministic.

use onceover::{profile, Budget, ProfileConfig};
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
fn recovers_planted_structure() {
    let df = planted_table(4_000);
    let report = profile(&df, &ProfileConfig::default());

    let claim = |kind: &str| {
        report
            .findings
            .iter()
            .filter(|f| f.kind == kind)
            .map(|f| f.claim.as_str())
            .collect::<Vec<_>>()
    };

    // Primary key.
    assert!(
        claim("likely_primary_key")
            .iter()
            .any(|c| c.contains("order_id")),
        "pk: {:?}",
        claim("likely_primary_key")
    );
    // Exact FD region_id -> region_name.
    assert!(
        claim("functional_dependency")
            .iter()
            .any(|c| c.contains("region_id → region_name")),
        "fds: {:?}",
        claim("functional_dependency")
    );
    // total is derived (predictable and an identity).
    assert!(
        claim("arithmetic_identity")
            .iter()
            .any(|c| c.contains("total")),
        "identities: {:?}",
        claim("arithmetic_identity")
    );
    // Date gaps planted.
    assert!(
        !claim("date_gap").is_empty(),
        "expected date gaps; gaps: {:?}",
        claim("date_gap")
    );
    // Dirty key with 3 dupes.
    assert!(
        claim("dirty_key").iter().any(|c| c.contains("user_ref")),
        "dirty: {:?}",
        claim("dirty_key")
    );
    // Categorical numeric.
    assert!(
        claim("likely_categorical")
            .iter()
            .any(|c| c.contains("region_id")),
        "likely_categorical: {:?}",
        claim("likely_categorical")
    );
    // Decision table: category or region_name determined by lookups? At
    // least one table or predictable categorical column should surface.
    let cat_preds = claim("predictable_from")
        .iter()
        .any(|c| c.contains("region_name") || c.contains("region_id"));
    assert!(
        cat_preds || !claim("decision_table").is_empty(),
        "predictable: {:?}, tables: {:?}",
        claim("predictable_from"),
        claim("decision_table")
    );

    // Coverage block honesty.
    assert_eq!(
        report.coverage["predictability_screen"].status,
        "ran_sampled"
    );
    assert!(
        matches!(
            report.coverage["functional_dependencies"].status.as_str(),
            "capped" | "ran_exact"
        ),
        "fd status: {}",
        report.coverage["functional_dependencies"].status
    );
}

#[test]
fn byte_deterministic() {
    let df = planted_table(1_000);
    let a = profile(&df, &ProfileConfig::default());
    let b = profile(&df, &ProfileConfig::default());
    assert_eq!(a.to_json(), b.to_json());
}

#[test]
fn quick_tier_is_leaner() {
    let df = planted_table(800);
    let quick = profile(
        &df,
        &ProfileConfig {
            budget: Budget::Quick,
            deadline: None,
        },
    );
    let standard = profile(
        &df,
        &ProfileConfig {
            budget: Budget::Standard,
            deadline: None,
        },
    );
    assert!(quick
        .findings
        .iter()
        .all(|f| f.kind != "predictable_from" && f.kind != "functional_dependency"));
    assert!(standard
        .findings
        .iter()
        .any(|f| f.kind == "functional_dependency"));
    assert_eq!(quick.coverage["predictability_screen"].status, "skipped");
}

#[test]
fn markdown_renders() {
    let df = planted_table(500);
    let report = profile(&df, &ProfileConfig::default());
    let md = report.to_markdown();
    assert!(md.contains("# onceover report"));
    assert!(md.contains("## Findings"));
    assert!(md.contains("## Coverage"));
    assert!(!report.overview.headline.is_empty());
}

#[test]
#[ignore] // run with: cargo test --release -- --ignored
fn perf_standard_100k() {
    let df = planted_table(100_000);
    let t0 = std::time::Instant::now();
    let report = profile(&df, &ProfileConfig::default());
    let dt = t0.elapsed();
    eprintln!("standard 100k×10: {:?}", dt);
    assert!(dt.as_secs_f64() < 15.0, "took {dt:?}");
    assert!(!report.findings.is_empty());
}

#[test]
fn prism_rules_unique_and_majority_targets_skipped() {
    // Regression: PRISM used to re-emit the same rule every covering
    // iteration (growth counted covered positives), and happily mined
    // near-majority targets into lucky high-precision noise rules.
    let mut rng = Rng(77);
    let n = 4_000usize;
    let mut tenure: Vec<Option<f64>> = Vec::with_capacity(n);
    let mut churn: Vec<Option<i64>> = Vec::with_capacity(n);
    let mut country: Vec<Option<&str>> = Vec::with_capacity(n);
    for i in 0..n {
        let t = (i % 48) as f64;
        tenure.push(Some(t));
        // Step at 24 with 6% label noise (decision-table/GBM accuracy ~0.94).
        let flip = rng.next_f64() < 0.06;
        churn.push(Some(((t > 24.0) ^ flip) as i64));
        country.push(Some(if rng.next_f64() < 0.92 { "US" } else { "DE" }));
    }
    let df = df! {
        "tenure" => tenure,
        "churn" => churn,
        "country" => country,
    }
    .unwrap();
    let report = profile(&df, &ProfileConfig::default());

    let prism_claims: Vec<&str> = report
        .findings
        .iter()
        .filter(|f| f.kind == "prism_rule")
        .map(|f| f.claim.as_str())
        .collect();
    // No duplicated rules:
    let mut unique = prism_claims.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        prism_claims.len(),
        unique.len(),
        "duplicate prism rules: {prism_claims:?}"
    );
    // The 92%-majority column must not be mined into lucky rules:
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.kind == "prism_rule" && f.subject["column"] == "country"),
        "prism mined a near-majority target: {prism_claims:?}"
    );
    for rule in report.findings.iter().filter(|f| f.kind == "prism_rule") {
        let target = rule.subject["column"].as_str().unwrap();
        assert!(
            rule.evidence["conditions"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| !c.as_str().unwrap().starts_with(&format!("{target} "))),
            "self-referential rule: {}",
            rule.claim
        );
    }
    // And the planted step still yields rules for churn:
    assert!(
        prism_claims.iter().any(|c| c.contains("churn")),
        "expected churn rules: {prism_claims:?}"
    );
    assert!(
        report.findings.iter().any(|f| f.kind == "decision_table"
            && f.subject["column"] == "churn"
            && f.evidence["cv_accuracy"].as_f64().unwrap() > 0.9),
        "expected a validated churn decision table: {:?}",
        report.coverage
    );
}

#[test]
fn explanation_target_caps_are_bounded_and_reported() {
    // More eligible, perfectly predictable targets than the standard tier
    // permits. Every column can be explained using another column.
    let columns = (0..8)
        .map(|i| {
            Series::new(
                format!("category_{i}").into(),
                (0..400)
                    .map(|row| if row % 2 == 0 { "a" } else { "b" })
                    .collect::<Vec<_>>(),
            )
            .into_column()
        })
        .collect();
    let df = DataFrame::new_infer_height(columns).unwrap();
    let report = profile(&df, &ProfileConfig::default());
    for (pass, kind) in [
        ("decision_tables", "decision_table"),
        ("prism_rules", "prism_rule"),
    ] {
        assert_eq!(report.coverage[pass].status, "capped");
        assert!(report.coverage[pass]
            .detail
            .contains("6 of 8 eligible targets selected (cap 6)"));
        let mut targets: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.kind == kind)
            .map(|f| f.subject["column"].as_str().unwrap())
            .collect();
        targets.sort_unstable();
        targets.dedup();
        assert_eq!(targets.len(), 6);
    }
}

#[test]
fn dependence_cap_reports_the_full_pair_universe() {
    let columns = (0..33)
        .map(|i| Series::new(format!("x{i}").into(), (0..100i64).collect::<Vec<_>>()).into_column())
        .collect();
    let df = DataFrame::new_infer_height(columns).unwrap();
    let report = profile(&df, &ProfileConfig::default());
    let coverage = &report.coverage["dependence_screen"];
    assert_eq!(coverage.status, "capped");
    assert!(
        coverage
            .detail
            .starts_with("496 of 528 numeric pairs (capped)"),
        "{}",
        coverage.detail
    );
}
