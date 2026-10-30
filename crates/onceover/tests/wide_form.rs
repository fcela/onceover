//! Wide-form profiling of verified long-format frames (issue #5):
//! when the pivot pass verifies a long layout, the pipeline also
//! profiles the pivoted wide form and merges its findings, tagged
//! `source_form` in their evidence; layout artifacts on the long
//! form (the encoding FD and its predictive restatements) carry
//! the `long_format_encoding` triviality declaration.

use onceover::{profile, Budget, ProfileConfig};
use polars::prelude::*;

/// 12 countries x 5 years x 2 indicators, melted. In the wide form
/// `life` is a near-linear function of `gdp`; in the long form the
/// two level ranges are disjoint, so `value -> indicator` holds.
fn melted_panel() -> DataFrame {
    let mut country: Vec<String> = Vec::new();
    let mut year: Vec<i32> = Vec::new();
    let mut indicator: Vec<&str> = Vec::new();
    let mut value: Vec<f64> = Vec::new();
    for c in 0..12usize {
        // Three shared growth paths: the pooled measure repeats, so
        // it reads as a measure rather than an identifier.
        let gdp0 = 200.0 + 90.0 * (c % 3) as f64;
        for y in 0..5usize {
            let gdp = gdp0 * (1.0 + 0.03 * y as f64);
            let life = 52.0 + 0.021 * gdp + ((c * 7 + y * 3) % 5) as f64 * 0.4;
            for (ind, v) in [("gdp", gdp), ("life", life)] {
                country.push(format!("c{c:02}"));
                year.push(2015 + y as i32);
                indicator.push(ind);
                value.push(v);
            }
        }
    }
    DataFrame::new_infer_height(vec![
        Series::new("country".into(), country).into_column(),
        Series::new("year".into(), year).into_column(),
        Series::new("indicator".into(), indicator).into_column(),
        Series::new("value".into(), value).into_column(),
    ])
    .unwrap()
}

fn tagged(f: &onceover::Finding) -> bool {
    f.evidence.iter().any(|e| {
        e.metrics.get("source_form").and_then(|v| v.as_str()) == Some("pivoted wide form")
    })
}

#[test]
fn wide_form_findings_are_merged_and_tagged() {
    let report = profile(&melted_panel(), &ProfileConfig::default());
    let cov = report
        .coverage
        .get("pivot_profile")
        .expect("pivot_profile coverage entry");
    assert_eq!(cov.status, "ran_exact", "detail: {}", cov.detail);
    // The planted gdp/life association exists only in the wide form.
    assert!(
        report.findings.iter().any(|f| {
            tagged(f)
                && f.subject.columns.iter().any(|c| c == "gdp")
                && f.subject.columns.iter().any(|c| c == "life")
        }),
        "expected a tagged wide-form finding over the two levels"
    );
    // Long-form findings are not tagged.
    assert!(report
        .findings
        .iter()
        .any(|f| f.finding_type.as_deref() == Some("long_format") && !tagged(f)));
}

#[test]
fn encoding_artifacts_carry_the_layout_demotion() {
    let report = profile(&melted_panel(), &ProfileConfig::default());
    let demoted = |f: &onceover::Finding| {
        f.evidence.iter().any(|e| {
            e.metrics.get("triviality").and_then(|v| v.as_str())
                == Some("long_format_encoding")
        })
    };
    // The disjoint-range FD is demoted.
    assert!(report.findings.iter().any(|f| {
        f.finding_type.as_deref() == Some("functional")
            && f.subject.target.as_deref() == Some("indicator")
            && demoted(f)
    }));
    // So is any predictive restatement of the layout: the attribute
    // predicted from the pooled measure, or the pooled measure
    // predicted with the attribute among its inputs.
    for f in &report.findings {
        if f.finding_type.as_deref() == Some("predictive") {
            let t = f.subject.target.as_deref();
            let restates = (t == Some("indicator")
                && f.subject.inputs.len() == 1
                && f.subject.inputs[0] == "value")
                || (t == Some("value")
                    && f.subject.inputs.iter().any(|i| i == "indicator"));
            assert!(
                !restates || demoted(f),
                "predictive restatement not demoted: {}",
                f.claim
            );
        }
    }
}

#[test]
fn quick_budget_does_not_profile_the_wide_form() {
    let cfg = ProfileConfig {
        budget: Budget::Quick,
        ..Default::default()
    };
    let report = profile(&melted_panel(), &cfg);
    let cov = report
        .coverage
        .get("pivot_profile")
        .expect("pivot_profile coverage entry");
    assert_eq!(cov.status, "not_applicable");
    assert!(!report.findings.iter().any(tagged));
}
