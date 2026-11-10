//! Per-period coverage on the frame's time axis.
//!
//! The declared-expectations pass compares a frame against a grid
//! the caller declared (dimensions × periods) and reports the cells
//! that are absent. This pass makes the complementary, purely
//! observational statement: it counts what each *observed* period
//! on the assembled time axis actually carries — rows, and the
//! distinct entities of the panel's entity axis where the frame
//! model provides one — and flags a period whose volume falls far
//! below the typical period's. A period holding a fraction of the
//! usual rows (collection started mid-period, an outage week, a
//! truncated final period) otherwise reads in a report exactly
//! like a real drop in the measured quantity.
//!
//! The boundary with the declared panel check: this pass never
//! reports a period with no rows at all (a wholly absent period is
//! a gap, stated by the sequence checks or, against a declared
//! grid, by the expectations pass), and it never enumerates absent
//! cells. When the caller declares the same columns as dimensions
//! that this pass would use as the entity axis, the declared check
//! owns entity completeness for that panel, so this pass reports
//! row volume only and says so in its coverage entry.
//!
//! The rule: the typical period is the median period (of row
//! counts, and separately of entity counts). A period is partial
//! when its count is below 75% of the typical count and short by
//! at least 4 rows (at least 2 entities). The check needs at least
//! 4 observed periods before a typical period is meaningful, and
//! it reports only when the partial periods are a minority — at
//! most a quarter of all periods. Past that share the median no
//! longer names a typical full period: per-period volume varies
//! too widely (a date attribute on rows collected for other
//! reasons, counts spread over orders of magnitude), the coverage
//! entry says so, and no period is singled out. The
//! position of a flagged period — first, last, or interior — is
//! part of the statement: an edge period is the truncation shape,
//! an interior one is an outage shape.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use onceover_core::dateutil;
use onceover_core::{TypedCol, TypedTable};
use onceover_deps::hashutil;
use onceover_report::{EvidenceBasis, Finding, Method, Pattern, ProfileHints};
use serde_json::{json, Value};

use crate::frame::{axis_ticks_micros, FrameModel, MICROS_PER_DAY};

/// Smallest number of observed periods before a typical period is
/// meaningful.
const MIN_PERIODS: usize = 4;
/// Above this many distinct periods the per-period accounting is
/// not attempted; the coverage entry says so.
const MAX_PERIODS: usize = 10_000;
/// A period is partial when its volume is below this share of the
/// typical period's volume.
const SHORTFALL_LIMIT: f64 = 0.75;
/// … and short by at least this many rows, so small-count noise
/// (a period of 3 rows against a typical 4) is not a statement.
const MIN_ROW_SHORTFALL: f64 = 4.0;
/// The same floor for entity counts.
const MIN_ENTITY_SHORTFALL: f64 = 2.0;
/// At most this many partial periods are stated, worst shortfall
/// first; the coverage entry discloses the cap.
const MAX_FINDINGS: usize = 10;
/// The check reports only when partial periods make up at most
/// this share of all periods; past it the median does not name a
/// typical full period and the coverage entry says so instead.
const MAX_FLAGGED_SHARE: f64 = 0.25;
/// The per-period table in a finding's evidence is complete up to
/// this many periods; past it the payload lists the flagged
/// periods only.
const PAYLOAD_PERIOD_CAP: usize = 120;

fn method() -> Method {
    Method::new(
        "panel_coverage",
        "per_period_volume",
        "per-period coverage on the frame axis (onceover v0.1)",
    )
}

const METHOD_TEXT: &str = "per-period volume on the frame's assembled time axis: rows are counted per distinct axis value; entities are counted as distinct values of the entity axis (the grain's non-axis factors, or the frame's single identifier column) per period; the typical period is the median period; a period is reported when its row count is below 75% of the typical row count and short by at least 4 rows, or its entity count is below 75% of the typical entity count and short by at least 2 entities; wholly absent periods are not reported here (they are gaps, not partial periods); periods are singled out only when they make up at most a quarter of all periods — past that share the median names no typical full period and the coverage entry reports the spread instead";

pub enum PanelCoverageStatus {
    Ran(String),
    Capped(String),
    Skipped(String),
}

pub struct PanelCoverageOutcome {
    pub findings: Vec<Finding>,
    pub status: PanelCoverageStatus,
}

fn skipped(reason: String) -> PanelCoverageOutcome {
    PanelCoverageOutcome {
        findings: Vec::new(),
        status: PanelCoverageStatus::Skipped(reason),
    }
}

/// Per-row period keys for the axis: microsecond ticks when the
/// axis carries a tick representation, else the source column's
/// integer values (a year candidate, an integer column declared
/// via `hints.time`, a week-number column). The label formatter
/// renders one key for claims and evidence.
enum AxisKeys {
    Ticks(Vec<Option<i64>>),
    Ints(Vec<Option<i128>>),
}

fn axis_keys(table: &TypedTable, frame: &FrameModel) -> Option<AxisKeys> {
    if let Some(ticks) = axis_ticks_micros(table, frame) {
        return Some(AxisKeys::Ticks(ticks));
    }
    let src = frame.info.axis.as_ref()?.source_column.as_deref()?;
    let i = table.names.iter().position(|n| n == src)?;
    match &table.cols[i] {
        TypedCol::I64 { vals } => Some(AxisKeys::Ints(
            vals.iter().map(|v| v.copied().map(i128::from)).collect(),
        )),
        TypedCol::U64 { vals } => Some(AxisKeys::Ints(
            vals.iter().map(|v| v.copied().map(i128::from)).collect(),
        )),
        _ => None,
    }
}

/// The entity axis: the grain's non-axis factors when the account
/// found a factorized grain, else the frame's single identifier
/// column (other than the axis itself). Empty when the frame
/// provides neither.
fn entity_columns(table: &TypedTable, frame: &FrameModel) -> Vec<usize> {
    let axis = frame.info.axis.as_ref();
    if let (Some(grain), Some(axis)) = (&frame.info.grain, axis) {
        let cols: Vec<usize> = grain
            .factorization
            .iter()
            .filter(|f| f.column != axis.name)
            .filter_map(|f| table.names.iter().position(|n| n == &f.column))
            .collect();
        if !cols.is_empty() {
            return cols;
        }
    }
    let axis_cols: BTreeSet<usize> = crate::account::axis_subject_columns(frame)
        .iter()
        .filter_map(|n| table.names.iter().position(|x| x == n))
        .collect();
    let identifiers: Vec<usize> = frame
        .roles
        .iter()
        .enumerate()
        .filter(|(i, r)| r.as_deref() == Some("identifier") && !axis_cols.contains(i))
        .map(|(i, _)| i)
        .collect();
    if identifiers.len() == 1 {
        return identifiers;
    }
    Vec::new()
}

fn median(values: &[usize]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid] as f64
    } else {
        (sorted[mid - 1] as f64 + sorted[mid] as f64) / 2.0
    }
}

/// A count or median rendered for a claim: whole numbers without a
/// decimal part, halves and quarters with one decimal.
fn fmt_count(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v:.1}")
    }
}

fn json_count(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() <= 9_007_199_254_740_992.0 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

pub fn panel_coverage(
    table: &TypedTable,
    frame: &FrameModel,
    hints: Option<&ProfileHints>,
) -> PanelCoverageOutcome {
    let Some(axis) = &frame.info.axis else {
        return skipped("the frame has no time axis".into());
    };
    let axis_name = axis.name.clone();
    let Some(keys) = axis_keys(table, frame) else {
        return skipped(format!(
            "the time axis '{axis_name}' has no per-row values to count periods over"
        ));
    };
    // Normalize to one key space: ticks stay microsecond ticks;
    // integer keys are their own space. Labels are rendered per
    // space below.
    let (row_keys, labels): (Vec<Option<i128>>, Vec<String>) = match &keys {
        AxisKeys::Ticks(ticks) => {
            let mut distinct: Vec<i64> = ticks.iter().flatten().copied().collect();
            distinct.sort_unstable();
            distinct.dedup();
            let sub_day = distinct.iter().any(|t| t.rem_euclid(MICROS_PER_DAY) != 0);
            let labels = distinct
                .iter()
                .map(|&t| {
                    if sub_day {
                        dateutil::iso_datetime(t)
                    } else {
                        dateutil::iso_date(t.div_euclid(MICROS_PER_DAY))
                    }
                })
                .collect();
            (
                ticks.iter().copied().map(|t| t.map(i128::from)).collect(),
                labels,
            )
        }
        AxisKeys::Ints(vals) => {
            let mut distinct: Vec<i128> = vals.iter().flatten().copied().collect();
            distinct.sort_unstable();
            distinct.dedup();
            let labels = distinct.iter().map(|v| format!("{v}")).collect();
            (vals.clone(), labels)
        }
    };
    let period_index: HashMap<i128, usize> = {
        let mut distinct: Vec<i128> = row_keys.iter().flatten().copied().collect();
        distinct.sort_unstable();
        distinct.dedup();
        distinct.iter().enumerate().map(|(i, &k)| (k, i)).collect()
    };
    let n_periods = period_index.len();
    if n_periods > MAX_PERIODS {
        return PanelCoverageOutcome {
            findings: Vec::new(),
            status: PanelCoverageStatus::Capped(format!(
                "{n_periods} distinct periods on the '{axis_name}' axis exceeds the {MAX_PERIODS} cap for per-period coverage"
            )),
        };
    }
    if n_periods < MIN_PERIODS {
        return PanelCoverageOutcome {
            findings: Vec::new(),
            status: PanelCoverageStatus::Ran(format!(
                "{n_periods} period(s) on the '{axis_name}' axis; per-period coverage needs at least {MIN_PERIODS} periods to name a typical one"
            )),
        };
    }

    let mut rows_per: Vec<usize> = vec![0; n_periods];
    let mut rows_without_axis = 0usize;
    for k in &row_keys {
        match k {
            Some(k) => rows_per[period_index[k]] += 1,
            None => rows_without_axis += 1,
        }
    }

    // ---- entity axis ----
    let entity_cols = entity_columns(table, frame);
    let entity_names: Vec<String> = entity_cols
        .iter()
        .map(|&i| table.names[i].clone())
        .collect();
    // The declared panel check owns entity completeness when the
    // caller declared exactly this entity axis as dimensions.
    let declared_owns_entities = !entity_cols.is_empty()
        && hints.is_some_and(|h| {
            !h.dimensions.is_empty()
                && h.dimensions.iter().collect::<BTreeSet<_>>()
                    == entity_names.iter().collect::<BTreeSet<_>>()
        });
    let count_entities = !entity_cols.is_empty() && !declared_owns_entities;
    let mut entities_per: Option<Vec<usize>> = None;
    let mut rows_with_null_entity = 0usize;
    if count_entities {
        let mut sets: Vec<HashSet<Vec<u8>>> = (0..n_periods).map(|_| HashSet::new()).collect();
        let mut buf: Vec<u8> = Vec::with_capacity(32);
        for (r, k) in row_keys.iter().enumerate() {
            let Some(k) = k else { continue };
            if entity_cols.iter().any(|&c| table.cols[c].is_null_at(r)) {
                rows_with_null_entity += 1;
                continue;
            }
            buf.clear();
            for &c in &entity_cols {
                hashutil::encode_value(&table.cols[c], r, &mut buf);
            }
            sets[period_index[k]].insert(buf.clone());
        }
        entities_per = Some(sets.iter().map(|s| s.len()).collect());
    }

    let typical_rows = median(&rows_per);
    let typical_entities = entities_per.as_ref().map(|e| median(e));

    // ---- flag partial periods ----
    struct Flagged {
        idx: usize,
        row_flag: bool,
        entity_flag: bool,
        worst_share: f64,
    }
    let mut flagged: Vec<Flagged> = Vec::new();
    for idx in 0..n_periods {
        let rows = rows_per[idx];
        let row_flag = (rows as f64) < SHORTFALL_LIMIT * typical_rows
            && typical_rows - rows as f64 >= MIN_ROW_SHORTFALL;
        let entity_flag = match (&entities_per, typical_entities) {
            (Some(e), Some(te)) => {
                (e[idx] as f64) < SHORTFALL_LIMIT * te && te - e[idx] as f64 >= MIN_ENTITY_SHORTFALL
            }
            _ => false,
        };
        if row_flag || entity_flag {
            let mut worst = 1.0f64;
            if row_flag {
                worst = worst.min(rows as f64 / typical_rows);
            }
            if entity_flag {
                worst = worst.min(
                    entities_per.as_ref().expect("entities")[idx] as f64
                        / typical_entities.expect("typical entities"),
                );
            }
            flagged.push(Flagged {
                idx,
                row_flag,
                entity_flag,
                worst_share: worst,
            });
        }
    }
    flagged.sort_by(|a, b| {
        a.worst_share
            .total_cmp(&b.worst_share)
            .then(a.idx.cmp(&b.idx))
    });
    let n_flagged = flagged.len();
    if n_flagged as f64 > MAX_FLAGGED_SHARE * n_periods as f64 {
        // Too much of the panel is "partial" for the median to
        // name a typical full period: per-period volume varies
        // too widely for the statement, and singling periods out
        // would report the frame's shape, not coverage breaks.
        let mut detail = format!(
            "{n_periods} periods on the '{axis_name}' axis; typical period {} rows (median)",
            fmt_count(typical_rows)
        );
        if let Some(te) = typical_entities {
            detail.push_str(&format!(
                " and {} entities over ({})",
                fmt_count(te),
                entity_names.join(", ")
            ));
        }
        detail.push_str(&format!(
            "; {n_flagged} of {n_periods} periods ({:.0}%) fall below 75% of the typical volume — too wide a spread for a typical period to name, so no partial-period statements",
            100.0 * n_flagged as f64 / n_periods as f64
        ));
        return PanelCoverageOutcome {
            findings: Vec::new(),
            status: PanelCoverageStatus::Ran(detail),
        };
    }
    flagged.truncate(MAX_FINDINGS);

    // The per-period table carried in every finding's evidence:
    // complete up to the payload cap, otherwise the flagged
    // periods only.
    let periods_payload: Vec<Value> = {
        let include: Vec<usize> = if n_periods <= PAYLOAD_PERIOD_CAP {
            (0..n_periods).collect()
        } else {
            flagged.iter().map(|f| f.idx).collect()
        };
        include
            .iter()
            .map(|&i| {
                let mut obj = serde_json::Map::new();
                obj.insert("period".into(), json!(labels[i]));
                obj.insert("rows".into(), json!(rows_per[i]));
                if let Some(e) = &entities_per {
                    obj.insert("entities".into(), json!(e[i]));
                }
                Value::Object(obj)
            })
            .collect()
    };

    let mut subject_cols: Vec<String> = entity_names.clone();
    subject_cols.extend(crate::account::axis_subject_columns(frame));

    let mut findings = Vec::new();
    for f in &flagged {
        let idx = f.idx;
        let position = if idx == 0 {
            "first"
        } else if idx == n_periods - 1 {
            "last"
        } else {
            "interior"
        };
        let rows = rows_per[idx];
        let mut clauses: Vec<String> = Vec::new();
        if f.row_flag {
            clauses.push(format!(
                "holds {} {} against a typical {} rows per period ({}% of typical)",
                fmt_count(rows as f64),
                if rows == 1 { "row" } else { "rows" },
                fmt_count(typical_rows),
                (100.0 * rows as f64 / typical_rows).round() as i64
            ));
        }
        if f.entity_flag {
            let e = entities_per.as_ref().expect("entities")[idx];
            let te = typical_entities.expect("typical entities");
            // The clause carries its own verb when it stands alone.
            let verb = if clauses.is_empty() { "has " } else { "" };
            clauses.push(format!(
                "{verb}{} {} present against a typical {} ({}% of typical)",
                fmt_count(e as f64),
                if e == 1 { "entity" } else { "entities" },
                fmt_count(te),
                (100.0 * e as f64 / te).round() as i64
            ));
        }
        let position_phrase = match position {
            "first" => "the first period",
            "last" => "the final period",
            _ => "an interior period",
        };
        let claim = format!(
            "{} ({}) on the '{}' axis {}",
            position_phrase,
            labels[idx],
            axis_name,
            clauses.join("; ")
        );
        let interpretation = match position {
            "interior" => "an interior period carrying a fraction of the usual volume points to an outage or a reporting break inside the span; totals over this period are not comparable with full periods",
            _ => "a period at the edge of the span carrying a fraction of the usual volume is the shape of a partial collection window — collection started or stopped inside the period; totals over this period are not comparable with full periods",
        };
        let mut metrics = BTreeMap::from([
            ("axis".into(), json!(axis_name)),
            ("period".into(), json!(labels[idx])),
            ("period_index".into(), json!(idx)),
            ("position".into(), json!(position)),
            ("rows".into(), json!(rows)),
            ("typical_rows".into(), json_count(typical_rows)),
            ("share_of_typical".into(), json!(rows as f64 / typical_rows)),
            ("n_periods".into(), json!(n_periods)),
            ("n".into(), json!(table.n - rows_without_axis)),
            ("population_rows".into(), json!(table.n)),
            ("rows_without_axis_value".into(), json!(rows_without_axis)),
            ("periods".into(), json!(periods_payload)),
            ("method".into(), json!(METHOD_TEXT)),
        ]);
        if let (Some(e), Some(te)) = (&entities_per, typical_entities) {
            metrics.insert("entities".into(), json!(e[idx]));
            metrics.insert("typical_entities".into(), json_count(te));
            metrics.insert("entity_share_of_typical".into(), json!(e[idx] as f64 / te));
            metrics.insert("entity_columns".into(), json!(entity_names));
            if rows_with_null_entity > 0 {
                metrics.insert("rows_with_null_entity".into(), json!(rows_with_null_entity));
            }
        }
        let score = 0.5 + 0.35 * (1.0 - f.worst_share);
        findings.push(Finding::new(
            Pattern::PartialPeriod,
            BTreeMap::from([("columns".into(), json!(subject_cols))]),
            claim,
            EvidenceBasis::Measured,
            score,
            metrics,
            Some(interpretation.into()),
            method(),
        ));
    }

    let mut detail = format!(
        "{n_periods} periods on the '{axis_name}' axis; typical period {} rows (median)",
        fmt_count(typical_rows)
    );
    if let Some(te) = typical_entities {
        detail.push_str(&format!(
            " and {} entities over ({})",
            fmt_count(te),
            entity_names.join(", ")
        ));
    }
    if declared_owns_entities {
        detail.push_str(
            "; entity volume not compared: the declared panel check (expectations) owns entity completeness for the declared dimensions",
        );
    }
    if rows_without_axis > 0 {
        detail.push_str(&format!(
            "; {rows_without_axis} rows carry no axis value and fall in no period"
        ));
    }
    if n_flagged == 0 {
        detail.push_str("; no period below 75% of the typical volume");
    } else {
        detail.push_str(&format!(
            "; {n_flagged} period(s) below 75% of the typical volume"
        ));
        if n_flagged > MAX_FINDINGS {
            detail.push_str(&format!(
                ", worst {MAX_FINDINGS} stated (cap {MAX_FINDINGS})"
            ));
        }
    }
    PanelCoverageOutcome {
        findings,
        status: PanelCoverageStatus::Ran(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;
    use polars::prelude::*;

    fn panel_df(dip: Option<usize>, last: Option<usize>) -> DataFrame {
        // 40 entities x 10 weekly periods; `dip` scales one
        // interior period's entity count, `last` the final one's.
        let persons: Vec<String> = (0..40).map(|i| format!("p{i:02}")).collect();
        let mut person = Vec::new();
        let mut week = Vec::new();
        let mut tokens = Vec::new();
        // Weekly periods as day counts (2024-01-01 is day 19723).
        for (wi, start) in (0..10).map(|k| 19723 + 7 * k).enumerate() {
            let present = if Some(wi) == dip {
                &persons[..16]
            } else if wi == 9 {
                match last {
                    Some(k) => &persons[..k],
                    None => &persons[..],
                }
            } else {
                &persons[..]
            };
            for p in present {
                person.push(p.clone());
                week.push(start);
                tokens.push(100.0);
            }
        }
        DataFrame::new_infer_height(vec![
            Series::new("person".into(), person).into(),
            Series::new("week".into(), week)
                .cast(&DataType::Date)
                .unwrap()
                .into(),
            Series::new("tokens".into(), tokens).into(),
        ])
        .unwrap()
    }

    fn run(df: &DataFrame) -> PanelCoverageOutcome {
        let table = extract(df);
        let facts: Vec<crate::frame::CardFacts> = table
            .cols
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let card = onceover_core::column_card(&table.names[i], &table.dtypes[i], col);
                crate::frame::CardFacts {
                    card_role: card.card.role.clone(),
                    distinct: card.card.distinct,
                    null_count: card.card.null_count,
                    numeric_min: card.card.numeric_stats.as_ref().and_then(|s| s.min),
                    numeric_max: card.card.numeric_stats.as_ref().and_then(|s| s.max),
                    integral: false,
                }
            })
            .collect();
        // The grain the account would find: person x week.
        let mut frame = crate::frame::analyze_frame(&table, &facts, None);
        frame.info.grain = Some(onceover_report::FrameGrain {
            key_columns: vec!["person".into(), "week".into()],
            factorization: vec![
                onceover_report::FrameGrainFactor {
                    column: "person".into(),
                    distinct: 40,
                },
                onceover_report::FrameGrainFactor {
                    column: "week".into(),
                    distinct: 10,
                },
            ],
            balanced: false,
            combinations: Some(400),
            near_key: None,
            statement: "test grain".into(),
        });
        panel_coverage(&table, &frame, None)
    }

    #[test]
    fn truncated_final_and_interior_dip_are_flagged() {
        let df = panel_df(Some(4), Some(24));
        let out = run(&df);
        assert_eq!(
            out.findings.len(),
            2,
            "{:?}",
            out.findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let positions: Vec<&str> = out
            .findings
            .iter()
            .map(|f| f.metrics()["position"].as_str().unwrap())
            .collect();
        assert!(positions.contains(&"last"), "{positions:?}");
        assert!(positions.contains(&"interior"), "{positions:?}");
        for f in &out.findings {
            f.validate().unwrap();
            assert_eq!(f.finding_type.as_deref(), Some("partial_period"));
        }
        let last = out
            .findings
            .iter()
            .find(|f| f.metrics()["position"] == json!("last"))
            .unwrap();
        assert_eq!(last.metrics()["rows"], json!(24));
        assert_eq!(last.metrics()["entities"], json!(24));
        assert!(last.claim.contains("final period"), "{}", last.claim);
        assert!(last.claim.contains("2024-03-04"), "{}", last.claim);
    }

    #[test]
    fn uniform_panel_is_quiet() {
        let df = panel_df(None, None);
        let out = run(&df);
        assert!(out.findings.is_empty());
        match &out.status {
            PanelCoverageStatus::Ran(d) => {
                assert!(d.contains("no period below 75%"), "{d}")
            }
            _ => panic!("expected Ran"),
        }
    }

    #[test]
    fn wide_spread_names_no_typical_period() {
        // Per-period counts spread over orders of magnitude: 3 of
        // 8 periods sit far below the median, more than the
        // quarter share at which the median names a typical full
        // period. No period is singled out; the coverage entry
        // reports the spread.
        let mut day = Vec::new();
        let mut v = Vec::new();
        for (d, count) in [
            (0, 2),
            (1, 2),
            (2, 2),
            (3, 40),
            (4, 40),
            (5, 40),
            (6, 40),
            (7, 40),
        ] {
            for _ in 0..count {
                day.push(19723 + d);
                v.push(1.0);
            }
        }
        let df = DataFrame::new_infer_height(vec![
            Series::new("day".into(), day)
                .cast(&DataType::Date)
                .unwrap()
                .into(),
            Series::new("v".into(), v).into(),
        ])
        .unwrap();
        let table = extract(&df);
        let facts: Vec<crate::frame::CardFacts> = table
            .cols
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let card = onceover_core::column_card(&table.names[i], &table.dtypes[i], col);
                crate::frame::CardFacts {
                    card_role: card.card.role.clone(),
                    distinct: card.card.distinct,
                    null_count: card.card.null_count,
                    numeric_min: None,
                    numeric_max: None,
                    integral: false,
                }
            })
            .collect();
        let frame = crate::frame::analyze_frame(&table, &facts, None);
        let out = panel_coverage(&table, &frame, None);
        assert!(out.findings.is_empty());
        match &out.status {
            PanelCoverageStatus::Ran(d) => {
                assert!(d.contains("too wide a spread"), "{d}")
            }
            _ => panic!("expected Ran"),
        }
    }

    #[test]
    fn frame_without_axis_is_skipped() {
        let df = DataFrame::new_infer_height(vec![
            Series::new("x".into(), vec![1.0, 2.0, 3.0]).into(),
            Series::new("y".into(), vec![4.0, 5.0, 6.0]).into(),
        ])
        .unwrap();
        let table = extract(&df);
        let facts: Vec<crate::frame::CardFacts> = table
            .cols
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let card = onceover_core::column_card(&table.names[i], &table.dtypes[i], col);
                crate::frame::CardFacts {
                    card_role: card.card.role.clone(),
                    distinct: card.card.distinct,
                    null_count: card.card.null_count,
                    numeric_min: None,
                    numeric_max: None,
                    integral: false,
                }
            })
            .collect();
        let frame = crate::frame::analyze_frame(&table, &facts, None);
        let out = panel_coverage(&table, &frame, None);
        assert!(out.findings.is_empty());
        match &out.status {
            PanelCoverageStatus::Skipped(d) => assert!(d.contains("no time axis"), "{d}"),
            _ => panic!("expected Skipped"),
        }
    }
}
