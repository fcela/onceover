//! Date and sequence intelligence: gap detection (weekday-aware for daily
//! series), modal interval, irregular intervals.

use onceover_report::{Method, Pattern};
use std::collections::BTreeMap;

use onceover_core::TypedCol;
use onceover_report::{EvidenceBasis, Finding};
use serde_json::json;

use onceover_core::dateutil::{is_weekend, iso_date};

pub struct DatesOutcome {
    pub findings: Vec<Finding>,
    /// Per-column modal interval (days) for card backfill.
    pub modal_intervals: BTreeMap<String, i64>,
    pub coverage: String,
}

/// Analyze one temporal column (days since epoch per row, None = null).
pub fn analyze_dates(name: &str, vals: &[Option<i32>]) -> (Vec<Finding>, Option<i64>) {
    let mut out = Vec::new();
    let mut days: Vec<i64> = vals.iter().flatten().map(|&d| d as i64).collect();
    if days.len() < 30 {
        return (out, None);
    }
    days.sort_unstable();
    days.dedup();

    let diffs: Vec<i64> = days.windows(2).map(|w| w[1] - w[0]).collect();
    if diffs.is_empty() {
        return (out, None);
    }

    // Modal and median intervals.
    let mut counts: BTreeMap<i64, u32> = BTreeMap::new();
    for &d in &diffs {
        *counts.entry(d).or_insert(0) += 1;
    }
    let mut counts_v: Vec<(i64, u32)> = counts.into_iter().collect();
    counts_v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let modal = counts_v[0].0;
    let mut sorted_diffs = diffs.clone();
    sorted_diffs.sort_unstable();
    let median = sorted_diffs[sorted_diffs.len() / 2];

    // Irregular intervals.
    let n_diffs = diffs.len() as f64;
    let mean = diffs.iter().sum::<i64>() as f64 / n_diffs;
    if mean > 0.0 {
        let var = diffs
            .iter()
            .map(|d| (*d as f64 - mean).powi(2))
            .sum::<f64>()
            / n_diffs;
        let cv = var.sqrt() / mean;
        if cv > 0.5 && diffs.len() >= 10 {
            out.push(Finding::new(
Pattern::IntervalVariation,
BTreeMap::from([("column".into(), json!(name))]),
format!(
                    "column {} has irregular spacing (modal {} days, coefficient of variation {:.2})",
                    name, modal, cv
                ),
EvidenceBasis::Measured,
0.35,
BTreeMap::from([
                    ("modal_interval_days".into(), json!(modal)),
                    ("median_interval_days".into(), json!(median)),
                    ("cv".into(), json!(cv)),
                ]),
Some("do not assume a regular time index".into()),
Method::new("dates", "interval_variation", "exact interval scan (onceover v0.1)")
));
        }
    }

    // Gap events.
    let mut events: Vec<(i64, i64, i64, i64, bool)> = Vec::new(); // (missing, from, to, diff, approx)
    for w in days.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let flagged = if modal == 1 {
            // Daily/business-daily: count missing weekdays only.
            if d > 1 {
                let missing_wd = (a + 1..b).filter(|&x| !is_weekend(x)).count();
                if missing_wd > 0 {
                    Some(missing_wd as i64)
                } else {
                    None // pure weekend bridge — not a gap
                }
            } else {
                None
            }
        } else if modal == 7 {
            if d > 10 {
                Some(((d - 1) / 7).max(1))
            } else {
                None
            }
        } else if d > (modal * 3) / 2 && d > modal + 2 {
            Some(((d - 1) / modal).max(1))
        } else {
            None
        };
        if let Some(missing) = flagged {
            events.push((missing, a, b, d, modal != 1 && modal != 7));
        }
    }

    events.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));
    events.truncate(10);
    let total_missing: i64 = events.iter().map(|e| e.0).sum();
    for (missing, from, to, diff, approx) in events {
        let unit = match modal {
            1 => "business day",
            7 => "week",
            _ => "interval",
        };
        out.push(Finding::new(
            Pattern::Gap,
            BTreeMap::from([
                ("column".into(), json!(name)),
                ("from".into(), json!(iso_date(from))),
                ("to".into(), json!(iso_date(to))),
            ]),
            format!(
                "column {} is missing {} {}{} at {}..{} ({} days elapsed)",
                name,
                missing,
                unit,
                if missing > 1 { "s" } else { "" },
                iso_date(from),
                iso_date(to),
                diff
            ),
            EvidenceBasis::Measured,
            0.58 + 0.2 * ((missing as f64) / 40.0).min(1.0),
            BTreeMap::from([
                ("missing".into(), json!(missing)),
                ("reference_source".into(), json!("inferred")),
                ("n".into(), json!(vals.len())),
                ("cadence_days".into(), json!(modal)),
                (
                    "reference".into(),
                    json!(if modal == 1 {
                        "weekdays"
                    } else {
                        "modal_interval"
                    }),
                ),
                ("from".into(), json!(iso_date(from))),
                ("to".into(), json!(iso_date(to))),
                ("elapsed_days".into(), json!(diff)),
                ("approximate".into(), json!(approx)),
            ]),
            Some("check for missing periods (ETL gaps, holidays not modeled, outages)".into()),
            Method::new(
                "dates",
                "calendar_intervals",
                "exact calendar scan, weekday-aware (onceover v0.1)",
            ),
        ));
    }
    let _ = total_missing;

    (out, Some(modal))
}

/// Drive date analysis over all temporal columns.
pub fn analyze_temporal(table: &onceover_core::TypedTable) -> DatesOutcome {
    let mut findings = Vec::new();
    let mut modal_intervals = BTreeMap::new();
    let mut n_cols = 0;
    for (i, col) in table.cols.iter().enumerate() {
        let name = &table.names[i];
        match col {
            TypedCol::Date { vals } => {
                n_cols += 1;
                let (f, m) = analyze_dates(name, vals);
                findings.extend(f);
                if let Some(m) = m {
                    modal_intervals.insert(name.clone(), m);
                }
            }
            TypedCol::Datetime { vals } => {
                n_cols += 1;
                // Convert to days (floor).
                let days: Vec<Option<i32>> = vals
                    .iter()
                    .map(|v| v.map(|us| (us.div_euclid(86_400_000_000)) as i32))
                    .collect();
                let (f, m) = analyze_dates(name, &days);
                findings.extend(f);
                if let Some(m) = m {
                    modal_intervals.insert(name.clone(), m);
                }
            }
            _ => {}
        }
    }
    let coverage = if n_cols == 0 {
        "skipped: no temporal columns".to_string()
    } else {
        format!(
            "{n_cols} temporal columns, {} gap events",
            findings
                .iter()
                .filter(|f| f.category == onceover_report::Category::Gaps)
                .count()
        )
    };
    DatesOutcome {
        findings,
        modal_intervals,
        coverage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bday_seq(start: i64, count: i64) -> Vec<Option<i32>> {
        // Business days starting at a Monday 2024-01-01 is a Monday (day 19723).
        let mut out = Vec::new();
        let mut d = start;
        let mut n = 0;
        while n < count {
            if !is_weekend(d) {
                out.push(Some(d as i32));
                n += 1;
            }
            d += 1;
        }
        out
    }

    #[test]
    fn flags_missing_business_days_not_weekends() {
        // 19723 = 2024-01-01, a Monday.
        let mut days = bday_seq(19723, 40);
        // Remove two *consecutive* business days: one 2-business-day gap.
        days.remove(11);
        days.remove(10);
        let (findings, modal) = analyze_dates("date", &days);
        assert_eq!(modal, Some(1));
        let gaps: Vec<_> = findings
            .iter()
            .filter(|f| f.category == onceover_report::Category::Gaps)
            .collect();
        assert_eq!(gaps.len(), 1, "expected 1 merged gap event");
        assert_eq!(gaps[0].metrics()["missing"].as_i64(), Some(2));
    }

    #[test]
    fn weekend_bridge_is_not_a_gap() {
        // Eight Mon-Fri weeks: weekend jumps must not be gaps.
        let mut days = Vec::new();
        for w in 0..8 {
            for d in 0..5 {
                days.push(Some(19723 + w * 7 + d));
            }
        }
        assert!(days.len() >= 30);
        let (findings, modal) = analyze_dates("date", &days);
        assert_eq!(modal, Some(1));
        assert!(
            !findings
                .iter()
                .any(|f| f.category == onceover_report::Category::Gaps),
            "gaps: {:?}",
            findings
                .iter()
                .filter(|f| f.category == onceover_report::Category::Gaps)
                .map(|f| f.claim.clone())
                .collect::<Vec<_>>()
        );
    }
}
