use onceover::{profile, Budget, Category, ProfileConfig};
use polars::prelude::*;

struct Lcg(u64);

impl Lcg {
    fn next_u64(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.0 = state;
        state.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn noise(&mut self, scale: f64) -> f64 {
        (self.next_f64() + self.next_f64() + self.next_f64() - 1.5) * scale
    }
}

fn breaks_only() -> ProfileConfig {
    ProfileConfig {
        detectors: Some(["breaks".into()].into_iter().collect()),
        ..Default::default()
    }
}

fn day_series(name: &str, start_day: i32, n: usize) -> Series {
    Series::new(
        name.into(),
        (0..n).map(|i| start_day + i as i32).collect::<Vec<i32>>(),
    )
    .cast(&DataType::Date)
    .unwrap()
}

#[test]
fn global_level_shift_is_reported() {
    let mut rng = Lcg(5);
    let n = 400;
    let values: Vec<f64> = (0..n)
        .map(|i| {
            if i < 200 {
                rng.noise(1.2)
            } else {
                4.0 + rng.noise(1.2)
            }
        })
        .collect();
    let df = DataFrame::new_infer_height(vec![
        day_series("day", 10957, n).into_column(),
        Series::new("value".into(), values).into_column(),
    ])
    .unwrap();
    let report = profile(&df, &breaks_only());
    assert!(report.coverage.get("breaks").map(|c| c.status.as_str()) == Some("ran_sampled"));
    assert!(report
        .findings
        .iter()
        .any(|f| f.finding_type.as_deref() == Some("level_shift")
            && f.category == Category::Changes
            && f.claim.contains("value")));
}

#[test]
fn region_conditional_shift_is_reported() {
    let year_days: [i32; 15] = [
        10957, 11323, 11688, 12053, 12418, 12784, 13149, 13514, 13879, 14245, 14610, 14975, 15340,
        15706, 16071,
    ];
    let regions = ["North", "South", "East", "West"];
    let mut rng = Lcg(17);
    let (mut entity, mut region, mut year, mut life) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for e in 0..40usize {
        for (yi, day) in year_days.iter().enumerate() {
            entity.push(format!("E{:02}", e));
            region.push(regions[e / 10].to_string());
            year.push(*day);
            let shift = if regions[e / 10] == "South" && yi >= 8 {
                4.0
            } else {
                0.0
            };
            life.push(70.0 + rng.noise(4.0) + shift);
        }
    }
    let df = DataFrame::new_infer_height(vec![
        Series::new("entity".into(), entity).into_column(),
        Series::new("region".into(), region).into_column(),
        Series::new("year".into(), year)
            .cast(&DataType::Date)
            .unwrap()
            .into_column(),
        Series::new("life".into(), life).into_column(),
    ])
    .unwrap();
    let report = profile(&df, &breaks_only());
    assert!(
        report.findings.iter().any(|f| {
            f.finding_type.as_deref() == Some("level_shift")
                && f.evidence.iter().any(|e| {
                    e.metrics.get("group_column").and_then(|v| v.as_str()) == Some("region")
                        && e.metrics.get("group_value").and_then(|v| v.as_str()) == Some("South")
                        && e.metrics
                            .get("breakpoint")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| s.starts_with("2008"))
                })
        }),
        "no South level-shift finding in {:?}",
        report.coverage
    );
}

#[test]
fn region_conditional_shift_on_integer_year_axis_is_reported() {
    // The issue #6 panel shape with the year kept as plain integers:
    // the break scan must engage on the year-like integer axis
    // without a date cast and without a declared hint.
    let regions = ["North", "South", "East", "West"];
    let mut rng = Lcg(17);
    let (mut entity, mut region, mut year, mut life) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for e in 0..40usize {
        for yi in 0..15usize {
            entity.push(format!("E{:02}", e));
            region.push(regions[e / 10].to_string());
            year.push(2000i64 + yi as i64);
            let shift = if regions[e / 10] == "South" && yi >= 8 {
                4.0
            } else {
                0.0
            };
            life.push(70.0 + rng.noise(4.0) + shift);
        }
    }
    let df = DataFrame::new_infer_height(vec![
        Series::new("entity".into(), entity).into_column(),
        Series::new("region".into(), region).into_column(),
        Series::new("year".into(), year).into_column(),
        Series::new("life".into(), life).into_column(),
    ])
    .unwrap();
    let report = profile(&df, &breaks_only());
    assert!(
        report.findings.iter().any(|f| {
            f.finding_type.as_deref() == Some("level_shift")
                && f.evidence.iter().any(|e| {
                    e.metrics.get("group_column").and_then(|v| v.as_str()) == Some("region")
                        && e.metrics.get("group_value").and_then(|v| v.as_str()) == Some("South")
                        && e.metrics
                            .get("breakpoint")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| s == "2008")
                })
        }),
        "no South level-shift finding on the integer year axis in {:?}",
        report.coverage
    );
}

#[test]
fn noise_series_yields_no_break_findings() {
    let mut rng = Lcg(11);
    let n = 400;
    let values = (0..n).map(|_| rng.noise(2.4)).collect::<Vec<f64>>();
    let df = DataFrame::new_infer_height(vec![
        day_series("day", 10957, n).into_column(),
        Series::new("value".into(), values).into_column(),
    ])
    .unwrap();
    let cfg = breaks_only();
    let report = profile(&df, &cfg);
    assert!(report
        .findings
        .iter()
        .all(|f| f.category != Category::Changes));
}

#[test]
fn missing_date_column_skips_the_pass() {
    let df = DataFrame::new_infer_height(vec![
        Series::new("x".into(), (0..200).map(|i| i as f64).collect::<Vec<f64>>()).into_column(),
        Series::new(
            "label".into(),
            (0..200)
                .map(|i| if i % 2 == 0 { "a" } else { "b" })
                .collect::<Vec<&str>>(),
        )
        .into_column(),
    ])
    .unwrap();
    let cfg = breaks_only();
    let report = profile(&df, &cfg);
    let cov = report.coverage.get("breaks").expect("breaks coverage");
    assert!(cov.detail.contains("no date, datetime, or year-like integer column"));
    assert!(report
        .findings
        .iter()
        .all(|f| f.category != Category::Changes));
}

#[test]
fn quick_budget_skips_breaks() {
    let mut rng = Lcg(5);
    let n = 400;
    let values = (0..n)
        .map(|i| {
            if i < 200 {
                rng.noise(1.2)
            } else {
                4.0 + rng.noise(1.2)
            }
        })
        .collect::<Vec<f64>>();
    let df = DataFrame::new_infer_height(vec![
        day_series("day", 10957, n).into_column(),
        Series::new("value".into(), values).into_column(),
    ])
    .unwrap();
    let cfg = ProfileConfig {
        budget: Budget::Quick,
        ..Default::default()
    };
    let report = profile(&df, &cfg);
    let cov = report.coverage.get("breaks").expect("breaks coverage");
    assert_eq!(cov.status, "skipped");
    assert!(cov.detail.contains("quick tier"));
}

#[test]
fn trend_change_is_reported() {
    let mut rng = Lcg(9);
    let n = 400;
    let values = (0..n)
        .map(|i| {
            let base = if i < 200 {
                i as f64 * 0.05
            } else {
                10.0 - (i - 200) as f64 * 0.05
            };
            base + rng.noise(0.6)
        })
        .collect::<Vec<f64>>();
    let df = DataFrame::new_infer_height(vec![
        day_series("day", 10957, n).into_column(),
        Series::new("value".into(), values).into_column(),
    ])
    .unwrap();
    let cfg = breaks_only();
    let report = profile(&df, &cfg);
    assert!(report
        .findings
        .iter()
        .any(|f| f.finding_type.as_deref() == Some("trend_change")));
}

#[test]
fn two_level_shifts_yield_two_findings() {
    let mut rng = Lcg(13);
    let n = 450;
    let values = (0..n)
        .map(|i| {
            if i < 150 {
                rng.noise(1.2)
            } else if i < 300 {
                5.0 + rng.noise(1.2)
            } else {
                1.0 + rng.noise(1.2)
            }
        })
        .collect::<Vec<f64>>();
    let df = DataFrame::new_infer_height(vec![
        day_series("day", 10957, n).into_column(),
        Series::new("value".into(), values).into_column(),
    ])
    .unwrap();
    let cfg = breaks_only();
    let report = profile(&df, &cfg);
    assert!(
        report
            .findings
            .iter()
            .filter(|f| f.finding_type.as_deref() == Some("level_shift"))
            .count()
            >= 2
    );
}
