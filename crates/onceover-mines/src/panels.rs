//! One-way flags on an entity × time grain: a boolean flag that,
//! within entities, only ever switches off→on and never on→off.
//!
//! An absorbing state is a fact about a panel that no other pass
//! states. The series pass describes measures along the axis for
//! the whole table; the order passes compare columns row by row.
//! Neither follows one entity through time, so a licence that is
//! granted and never revoked, a churn flag that once set stays set,
//! or a "has ever purchased" marker passes silently — and an
//! analysis that treats such a flag as a per-period event rather
//! than a state reached at onset misreads the panel.
//!
//! The check runs on the frame model's grain: an entity column
//! (the grain's single non-axis factor when the account found one,
//! else identifier- or dimension-role columns, chosen by the
//! pipeline) crossed with the time axis. Within an entity, the
//! flag's observed states are ordered by the axis; rows missing the
//! entity, the flag, or the time value are not observations, and an
//! entity holding two different flag values in one period
//! disqualifies the flag on that grain — the grain is not
//! functional for it, so no within-entity sequence is defined.
//!
//! A flag is reported when at least three entities show an
//! off→on onset and no entity shows an on→off transition. The
//! guards are the two trivial shapes: a flag that is on for every
//! entity from its first observed period has no onsets and is not
//! reported, and a flag that switches both ways within any entity
//! is not one-way. When a related activity column exists, the
//! finding also states the lag from onset to first activity: a
//! measure qualifies only when it is zero or missing on every
//! pre-onset row for at least 80% of the onset entities that have
//! pre-onset rows, and positive at or after onset for at least
//! half of the onset entities. The lag is the distance in axis
//! periods between an entity's onset period and its first positive
//! activity period, summarised by its median and middle half.

use std::collections::{BTreeMap, HashMap};

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;

/// Smallest number of distinct axis periods before within-entity
/// sequences mean anything.
const MIN_PERIODS: usize = 3;
/// Smallest number of entities with an observed flag state.
const MIN_ENTITIES: usize = 10;
/// Smallest number of entities exhibiting an onset.
const MIN_ONSETS: usize = 3;
/// Entity columns with more distinct values than this are not
/// followed: the grouping cost and the statement both stop
/// being about a panel of entities.
const MAX_ENTITIES: usize = 50_000;
/// Findings emitted by this pass per profile.
const MAX_FINDINGS: usize = 10;
/// Association rule, pre-onset side: the share of onset entities
/// with pre-onset rows whose activity is zero or missing on all of
/// them.
const MIN_PRE_ONSET_CLEAN: f64 = 0.8;
/// Association rule, post-onset side: the share of onset entities
/// whose activity turns positive at or after onset.
const MIN_POST_ONSET_ACTIVE: f64 = 0.5;

/// The panel the check runs on: the axis ticks aligned with the
/// table's rows, the candidate entity columns in priority order,
/// and the measure columns eligible as related activity.
pub struct PanelInput<'a> {
    pub axis_name: &'a str,
    pub ticks: &'a [Option<i64>],
    pub entities: Vec<usize>,
    pub measures: Vec<usize>,
}

/// A column's per-row flag states when it is a two-valued flag:
/// a boolean column, or a numeric column whose values are all
/// 0 or 1 with both values present.
fn flag_values(table: &TypedTable, col: usize) -> Option<Vec<Option<bool>>> {
    match &table.cols[col] {
        TypedCol::Bool { vals } => {
            let (mut on, mut off) = (false, false);
            for v in vals.iter().flatten() {
                if *v {
                    on = true;
                } else {
                    off = true;
                }
            }
            (on && off).then(|| vals.clone())
        }
        TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. } => {
            let (mut on, mut off) = (false, false);
            let mut out = Vec::with_capacity(table.n);
            for r in 0..table.n {
                match table.cols[col].numeric_at(r) {
                    Some(0.0) => {
                        off = true;
                        out.push(Some(false));
                    }
                    Some(1.0) => {
                        on = true;
                        out.push(Some(true));
                    }
                    Some(_) => return None,
                    None => out.push(None),
                }
            }
            (on && off).then_some(out)
        }
        _ => None,
    }
}

/// Whether the column is boolean or numeric at all (the flag
/// universe), for the coverage count.
fn flag_shaped(table: &TypedTable, col: usize) -> bool {
    matches!(
        table.cols[col],
        TypedCol::Bool { .. } | TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. }
    )
}

#[derive(PartialEq, Eq, Hash)]
enum EntityKey {
    Str(String),
    Int(i128),
    Float(u64),
    Bool(bool),
}

/// Why a column cannot key the entities of a grain.
enum EntitySkip {
    /// Fewer than two distinct values.
    TooFew,
    /// More distinct values than the entity cap.
    TooMany,
}

/// Per-row entity codes and the distinct count, or the reason the
/// column cannot key entities.
fn entity_codes(table: &TypedTable, col: usize) -> Result<(Vec<Option<u32>>, usize), EntitySkip> {
    let keys: Vec<Option<EntityKey>> = match &table.cols[col] {
        TypedCol::Str { vals } => vals
            .iter()
            .map(|&v| v.map(|s| EntityKey::Str(s.to_string())))
            .collect(),
        TypedCol::StrOwned { vals } => vals.iter().map(|v| v.clone().map(EntityKey::Str)).collect(),
        TypedCol::I64 { vals } => vals
            .iter()
            .map(|v| v.map(|&x| EntityKey::Int(x as i128)))
            .collect(),
        TypedCol::U64 { vals } => vals
            .iter()
            .map(|v| v.map(|&x| EntityKey::Int(x as i128)))
            .collect(),
        TypedCol::F64 { vals } => vals
            .iter()
            .map(|v| v.map(|&x| EntityKey::Float(x.to_bits())))
            .collect(),
        TypedCol::Bool { vals } => vals.iter().map(|&v| v.map(EntityKey::Bool)).collect(),
        TypedCol::Date { vals } => vals
            .iter()
            .map(|&v| v.map(|x| EntityKey::Int(x as i128)))
            .collect(),
        TypedCol::Datetime { vals, .. } => vals
            .iter()
            .map(|&v| v.map(|x| EntityKey::Int(x as i128)))
            .collect(),
        _ => return Err(EntitySkip::TooFew),
    };
    let mut index: HashMap<EntityKey, u32> = HashMap::new();
    let mut codes: Vec<Option<u32>> = Vec::with_capacity(keys.len());
    for key in keys {
        let Some(key) = key else {
            codes.push(None);
            continue;
        };
        let next = index.len() as u32;
        let code = *index.entry(key).or_insert(next);
        if index.len() > MAX_ENTITIES {
            return Err(EntitySkip::TooMany);
        }
        codes.push(Some(code));
    }
    let distinct = index.len();
    if distinct < 2 {
        return Err(EntitySkip::TooFew);
    }
    Ok((codes, distinct))
}

/// One entity's flag history on one grain, and the pass-level
/// aggregates the statement and the guards read.
struct FlagScan {
    entities: usize,
    onset_entities: usize,
    /// Total off→on transitions across onset entities.
    onsets: usize,
    reversal_entities: usize,
    starts_on: usize,
    never_on: usize,
    ambiguous_entities: usize,
    /// (entity code, onset period) for each onset entity.
    onset_list: Vec<(u32, usize)>,
}

fn scan_flag(
    codes: &[Option<u32>],
    distinct: usize,
    states: &[Option<bool>],
    period_of: &[Option<usize>],
) -> FlagScan {
    // Per entity: period -> state, in period order; a second,
    // different state in one period marks the entity ambiguous.
    let mut by_entity: Vec<BTreeMap<usize, bool>> = vec![BTreeMap::new(); distinct];
    let mut ambiguous: Vec<bool> = vec![false; distinct];
    for r in 0..states.len() {
        let (Some(code), Some(period), Some(state)) = (codes[r], period_of[r], states[r]) else {
            continue;
        };
        let map = &mut by_entity[code as usize];
        match map.get(&period) {
            Some(&prev) if prev != state => ambiguous[code as usize] = true,
            _ => {
                map.insert(period, state);
            }
        }
    }
    let mut scan = FlagScan {
        entities: 0,
        onset_entities: 0,
        onsets: 0,
        reversal_entities: 0,
        starts_on: 0,
        never_on: 0,
        ambiguous_entities: ambiguous.iter().filter(|a| **a).count(),
        onset_list: Vec::new(),
    };
    for (code, map) in by_entity.iter().enumerate() {
        if map.is_empty() || ambiguous[code] {
            continue;
        }
        scan.entities += 1;
        let mut prev: Option<bool> = None;
        let mut onset_period: Option<usize> = None;
        let mut entity_onsets = 0usize;
        let mut reversals = 0usize;
        for (&period, &state) in map {
            match (prev, state) {
                (None, true) => scan.starts_on += 1,
                (None, false) => {}
                (Some(false), true) => {
                    entity_onsets += 1;
                    if onset_period.is_none() {
                        onset_period = Some(period);
                    }
                }
                (Some(true), false) => reversals += 1,
                _ => {}
            }
            prev = Some(state);
        }
        if reversals > 0 {
            scan.reversal_entities += 1;
        }
        if entity_onsets > 0 {
            scan.onsets += entity_onsets;
            if reversals == 0 {
                scan.onset_entities += 1;
                scan.onset_list
                    .push((code as u32, onset_period.unwrap_or(0)));
            }
        }
        if !map.values().any(|v| *v) {
            scan.never_on += 1;
        }
    }
    scan
}

/// The lag association for one flag: the best qualifying measure
/// and its lag summary, when one qualifies.
struct Lag {
    activity: usize,
    median: f64,
    p25: f64,
    p75: f64,
    never_active: usize,
    with_pre_rows: usize,
    clean_share: f64,
    active_share: f64,
}

fn lag_for(
    table: &TypedTable,
    input: &PanelInput,
    entity_rows: &[Vec<usize>],
    period_of: &[Option<usize>],
    onset_list: &[(u32, usize)],
    skip: &[usize],
) -> Option<Lag> {
    let mut best: Option<Lag> = None;
    for &m in &input.measures {
        if skip.contains(&m) {
            continue;
        }
        // Per onset entity: pre-onset cleanliness and the first
        // positive period at or after onset.
        let (mut with_pre, mut clean) = (0usize, 0usize);
        let (mut active, mut lags) = (0usize, Vec::new());
        for &(code, onset) in onset_list {
            let mut has_pre = false;
            let mut pre_clean = true;
            let mut first_active: Option<usize> = None;
            for &r in &entity_rows[code as usize] {
                let Some(period) = period_of[r] else { continue };
                let Some(v) = table.cols[m].numeric_at(r) else {
                    continue;
                };
                if period < onset {
                    has_pre = true;
                    if v > 0.0 {
                        pre_clean = false;
                    }
                } else if v > 0.0 && first_active.is_none_or(|f| period < f) {
                    first_active = Some(period);
                }
            }
            if has_pre {
                with_pre += 1;
                clean += usize::from(pre_clean);
            }
            if let Some(first) = first_active {
                active += 1;
                lags.push((first - onset) as f64);
            }
        }
        if with_pre == 0 || onset_list.is_empty() {
            continue;
        }
        let clean_share = clean as f64 / with_pre as f64;
        let active_share = active as f64 / onset_list.len() as f64;
        if clean_share < MIN_PRE_ONSET_CLEAN || active_share < MIN_POST_ONSET_ACTIVE {
            continue;
        }
        lags.sort_by(f64::total_cmp);
        let pct =
            |q: f64| lags[((q * (lags.len() - 1) as f64).round() as usize).min(lags.len() - 1)];
        let lag = Lag {
            activity: m,
            median: pct(0.5),
            p25: pct(0.25),
            p75: pct(0.75),
            never_active: onset_list.len() - active,
            with_pre_rows: with_pre,
            clean_share,
            active_share,
        };
        let better = match &best {
            None => true,
            Some(b) => {
                lag.active_share > b.active_share
                    || (lag.active_share == b.active_share && lag.clean_share > b.clean_share)
            }
        };
        if better {
            best = Some(lag);
        }
    }
    best
}

/// Check every flag column on every entity grain the input names.
pub fn panel_flags(table: &TypedTable, input: &PanelInput) -> (Vec<Finding>, String) {
    // Period indices over the axis's distinct ticks.
    let mut ticks: Vec<i64> = input.ticks.iter().flatten().copied().collect();
    ticks.sort_unstable();
    ticks.dedup();
    if ticks.len() < MIN_PERIODS {
        return (
            vec![],
            format!(
                "{} has {} distinct periods; the one-way flag check needs at least {MIN_PERIODS}",
                input.axis_name,
                ticks.len()
            ),
        );
    }
    let period_index: HashMap<i64, usize> =
        ticks.iter().enumerate().map(|(i, &t)| (t, i)).collect();
    let period_of: Vec<Option<usize>> = input
        .ticks
        .iter()
        .map(|t| t.and_then(|t| period_index.get(&t).copied()))
        .collect();

    // Flag columns: boolean or 0/1 numeric, excluding the entity
    // columns themselves.
    let flag_cols: Vec<usize> = (0..table.cols.len())
        .filter(|c| !input.entities.contains(c) && flag_shaped(table, *c))
        .filter(|&c| flag_values(table, c).is_some())
        .collect();
    if flag_cols.is_empty() || input.entities.is_empty() {
        return (
            vec![],
            format!(
                "{} two-valued flag columns and {} entity columns along {}; nothing to check",
                flag_cols.len(),
                input.entities.len(),
                input.axis_name
            ),
        );
    }

    let mut findings: Vec<Finding> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut grains_checked = 0usize;
    for &entity in &input.entities {
        let (codes, distinct) = match entity_codes(table, entity) {
            Ok(v) => v,
            Err(EntitySkip::TooMany) => {
                notes.push(format!(
                    "{} has more than {MAX_ENTITIES} distinct values and is not followed as an entity column",
                    table.names[entity]
                ));
                continue;
            }
            Err(EntitySkip::TooFew) => continue,
        };
        if distinct < MIN_ENTITIES {
            continue;
        }
        grains_checked += 1;
        let mut entity_rows: Vec<Vec<usize>> = vec![Vec::new(); distinct];
        for (r, code) in codes.iter().enumerate() {
            if let Some(c) = code {
                entity_rows[*c as usize].push(r);
            }
        }
        for &flag in &flag_cols {
            let states = flag_values(table, flag).unwrap_or_default();
            let scan = scan_flag(&codes, distinct, &states, &period_of);
            let flag_name = &table.names[flag];
            let entity_name = &table.names[entity];
            if scan.ambiguous_entities > 0 {
                notes.push(format!(
                    "{flag_name} holds conflicting values within one {entity_name} period for {} entities, so no within-entity sequence is defined for it on that grain",
                    scan.ambiguous_entities
                ));
                continue;
            }
            if scan.entities < MIN_ENTITIES {
                continue;
            }
            if scan.reversal_entities > 0 {
                if scan.onset_entities + scan.reversal_entities > 0 && scan.onsets > 0 {
                    notes.push(format!(
                        "{flag_name} switches both ways within {entity_name} ({} entities with an on-to-off transition), so it is not one-way",
                        scan.reversal_entities
                    ));
                }
                continue;
            }
            if scan.onset_entities < MIN_ONSETS {
                if scan.onsets == 0 && scan.starts_on > 0 {
                    notes.push(format!(
                        "{flag_name} is on from the first observed period for every {entity_name} that has it, so it shows no onset"
                    ));
                }
                continue;
            }
            let lag = lag_for(
                table,
                input,
                &entity_rows,
                &period_of,
                &scan.onset_list,
                &[flag, entity],
            );
            findings.push(build_flag_finding(
                table,
                input,
                flag,
                entity,
                &scan,
                ticks.len(),
                lag,
            ));
        }
    }
    findings.sort_by(|a, b| {
        let onsets = |f: &Finding| {
            f.evidence[0]
                .metrics
                .get("entities_with_onset")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        };
        onsets(b)
            .cmp(&onsets(a))
            .then_with(|| a.claim.cmp(&b.claim))
    });
    if findings.len() > MAX_FINDINGS {
        notes.push(format!(
            "{} further one-way flags beyond the {MAX_FINDINGS} reported",
            findings.len() - MAX_FINDINGS
        ));
        findings.truncate(MAX_FINDINGS);
    }
    let mut detail = format!(
        "{} flag columns checked on {} entity columns along {}; {} one-way flags",
        flag_cols.len(),
        grains_checked,
        input.axis_name,
        findings.len()
    );
    for note in notes.iter().take(6) {
        detail.push_str("; ");
        detail.push_str(note);
    }
    (findings, detail)
}

#[allow(clippy::too_many_arguments)]
fn build_flag_finding(
    table: &TypedTable,
    input: &PanelInput,
    flag: usize,
    entity: usize,
    scan: &FlagScan,
    periods: usize,
    lag: Option<Lag>,
) -> Finding {
    let flag_name = &table.names[flag];
    let entity_name = &table.names[entity];
    let mut claim = format!(
        "{flag_name} switches on and never off within {entity_name}: {} of {} entities show an off-to-on onset and none shows an on-to-off transition over {periods} {} periods",
        scan.onset_entities, scan.entities, input.axis_name,
    );
    let mut metrics = BTreeMap::from([
        ("flag".into(), json!(flag_name)),
        ("entity_column".into(), json!(entity_name)),
        ("time_column".into(), json!(input.axis_name)),
        ("entities".into(), json!(scan.entities)),
        ("entities_with_onset".into(), json!(scan.onset_entities)),
        ("entities_starting_on".into(), json!(scan.starts_on)),
        ("entities_never_on".into(), json!(scan.never_on)),
        ("off_to_on_transitions".into(), json!(scan.onsets)),
        ("on_to_off_transitions".into(), json!(0)),
        ("periods".into(), json!(periods)),
        (
            "method".into(),
            json!("absorbing-state scan on the entity × time grain: within each entity the flag's observed states are ordered by the time axis (rows missing the entity, flag, or time value are not observations; an entity with conflicting flag values in one period disqualifies the flag on that grain); a flag is reported when at least 3 entities show an off-to-on onset and no entity shows an on-to-off transition; a related activity column is a measure that is zero or missing on every pre-onset row for at least 80% of onset entities with pre-onset rows and positive at or after onset for at least half of onset entities; the lag is measured in axis periods from onset to first positive activity"),
        ),
    ]);
    let mut interpretation = format!(
        "an absorbing state on the {entity_name} × {} grain: once {flag_name} is on it stays on for every entity observed, so the flag records a state reached at onset, not a per-period event; the onset period is the event to model",
        input.axis_name,
    );
    if let Some(lag) = &lag {
        let activity = &table.names[lag.activity];
        claim.push_str(&format!(
            "; {activity} stays at zero before onset and first turns positive a median of {} periods after onset (middle half {} to {} periods; {} of {} entities never turn positive)",
            fmt_periods(lag.median),
            fmt_periods(lag.p25),
            fmt_periods(lag.p75),
            lag.never_active,
            scan.onset_entities,
        ));
        interpretation.push_str(&format!(
            ". The lag to {activity} is measured per entity from its onset period to its first positive {activity} period"
        ));
        metrics.insert("activity_column".into(), json!(activity));
        metrics.insert("lag_periods_median".into(), json!(lag.median));
        metrics.insert("lag_periods_p25".into(), json!(lag.p25));
        metrics.insert("lag_periods_p75".into(), json!(lag.p75));
        metrics.insert("entities_never_active".into(), json!(lag.never_active));
        metrics.insert(
            "onset_entities_with_pre_onset_rows".into(),
            json!(lag.with_pre_rows),
        );
        metrics.insert("pre_onset_clean_share".into(), json!(lag.clean_share));
        metrics.insert("post_onset_active_share".into(), json!(lag.active_share));
    }
    Finding::new(
        Pattern::OneWayFlag,
        BTreeMap::from([
            ("columns".into(), json!([flag_name, entity_name])),
            ("time_column".into(), json!(input.axis_name)),
        ]),
        claim,
        EvidenceBasis::Measured,
        0.7,
        metrics,
        Some(interpretation),
        Method::new(
            "series",
            "one_way_flag_scan",
            "entity × time absorbing-state scan (onceover v0.1)",
        ),
    )
}

fn fmt_periods(v: f64) -> String {
    if v == v.trunc() {
        format!("{}", v as i64)
    } else {
        format!("{v:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;
    use polars::prelude::*;

    fn panel() -> (DataFrame, Vec<Option<i64>>) {
        // 40 entities x 20 weekly periods. Entities 20..40 switch
        // their flag on at a per-entity period and turn active two
        // periods later; entities 0..20 never switch on.
        let (entities, periods) = (40usize, 20usize);
        let mut person = Vec::new();
        let mut flag = Vec::new();
        let mut activity = Vec::new();
        let mut ticks = Vec::new();
        for e in 0..entities {
            let onset = if e >= 20 { 5 + (e % 8) } else { periods };
            for p in 0..periods {
                person.push(format!("p{e:02}"));
                flag.push(p >= onset);
                activity.push(if p >= onset + 2 { 10.0 } else { 0.0 });
                ticks.push(Some(
                    1_735_689_600_000_000i64 + (p as i64) * 604_800_000_000,
                ));
            }
        }
        let df = df! {
            "person" => person,
            "licence" => flag,
            "tokens" => activity,
        }
        .unwrap();
        (df, ticks)
    }

    fn run(df: &DataFrame, ticks: &[Option<i64>]) -> (Vec<Finding>, String) {
        let table = extract(df);
        let input = PanelInput {
            axis_name: "week",
            ticks,
            entities: vec![0],
            measures: vec![2],
        };
        panel_flags(&table, &input)
    }

    #[test]
    fn one_way_flag_with_lag_is_reported() {
        let (df, ticks) = panel();
        let (findings, _) = run(&df, &ticks);
        assert_eq!(
            findings.len(),
            1,
            "{:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let f = &findings[0];
        let m = &f.evidence[0].metrics;
        assert_eq!(m["entities_with_onset"], json!(20));
        assert_eq!(m["on_to_off_transitions"], json!(0));
        assert_eq!(m["activity_column"], json!("tokens"));
        assert_eq!(m["lag_periods_median"], json!(2.0));
        f.validate().unwrap();
    }

    #[test]
    fn two_way_flag_is_not_reported() {
        let (mut df, ticks) = panel();
        let mut flag: Vec<bool> = df
            .column("licence")
            .unwrap()
            .bool()
            .unwrap()
            .iter()
            .map(|v| v.unwrap_or(false))
            .collect();
        // Entity 25 switches back off for the last three periods.
        for item in flag.iter_mut().take(25 * 20 + 20).skip(25 * 20 + 17) {
            *item = false;
        }
        df.with_column(Series::new("licence".into(), flag).into())
            .unwrap();
        let (findings, detail) = run(&df, &ticks);
        assert!(findings.is_empty(), "{detail}");
        assert!(detail.contains("switches both ways"), "{detail}");
    }

    #[test]
    fn flag_on_from_the_first_period_is_not_reported() {
        // Entities 20..40 carry the flag from their first period;
        // the rest never carry it. There is no onset to report.
        let (mut df, ticks) = panel();
        let flag: Vec<bool> = (0..40 * 20).map(|r| r >= 20 * 20).collect();
        df.with_column(Series::new("licence".into(), flag).into())
            .unwrap();
        let (findings, detail) = run(&df, &ticks);
        assert!(findings.is_empty(), "{detail}");
        assert!(detail.contains("no onset"), "{detail}");
    }

    #[test]
    fn numeric_zero_one_flag_is_reported() {
        let (mut df, ticks) = panel();
        let flag: Vec<i64> = df
            .column("licence")
            .unwrap()
            .bool()
            .unwrap()
            .iter()
            .map(|v| i64::from(v.unwrap_or(false)))
            .collect();
        df.with_column(Series::new("licence".into(), flag).into())
            .unwrap();
        let (findings, _) = run(&df, &ticks);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].evidence[0].metrics["entities_with_onset"],
            json!(20)
        );
        findings[0].validate().unwrap();
    }
}
