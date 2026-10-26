use onceover::{profile, Budget, Category, ProfileConfig};
use polars::prelude::*;

#[test]
fn numeric_regimes_and_nominal_associations_reach_the_public_report() {
    let n = 1600;
    let mut state = 1234567u64;
    let mut next = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        (state.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    };
    let (mut gate, mut x, mut y, mut a, mut b) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for _ in 0..n {
        let g = next() * 2.0 - 1.0;
        let v = next() * 2.0 - 1.0;
        gate.push(g);
        x.push(v);
        y.push(if g < 0.0 {
            1.0 + 2.0 * v
        } else {
            -2.0 - 3.0 * v
        });
        let category = next() < 0.3;
        a.push(if category { "rare" } else { "common" });
        b.push(if category { "yes" } else { "no" });
    }
    let df = DataFrame::new_infer_height(vec![
        Series::new("gate".into(), gate).into_column(),
        Series::new("x".into(), x).into_column(),
        Series::new("y".into(), y).into_column(),
        Series::new("a".into(), a).into_column(),
        Series::new("b".into(), b).into_column(),
    ])
    .unwrap();
    let cfg = ProfileConfig::default();
    let report = profile(&df, &cfg);
    assert!(
        report.findings.iter().any(
            |f| f.specification.form.as_deref() == Some("piecewise_linear")
                && f.subject.target.as_deref() == Some("y")
        ),
        "{:?}",
        report.coverage
    );
    assert!(report
        .findings
        .iter()
        .any(|f| f.category == Category::ConditionalRules));
    assert!(report.coverage.contains_key("regimes"));
    assert!(report.coverage.contains_key("association_rules"));
    assert_eq!(report.to_json(), profile(&df, &cfg).to_json());
    let quick = profile(
        &df,
        &ProfileConfig {
            budget: Budget::Quick,
            deadline: None,
            ..Default::default()
        },
    );
    assert!(!quick.findings.iter().any(|f| matches!(
        f.category,
        Category::Dependencies | Category::ConditionalRules | Category::Changes
    )));
    assert_eq!(quick.coverage["regimes"].status, "skipped");
    assert_eq!(quick.coverage["association_rules"].status, "skipped");
    let expired = profile(
        &df,
        &ProfileConfig {
            budget: Budget::Standard,
            deadline: Some(std::time::Duration::ZERO),
            ..Default::default()
        },
    );
    assert_eq!(expired.coverage["regimes"].status, "skipped");
    assert_eq!(expired.coverage["association_rules"].status, "skipped");
}
