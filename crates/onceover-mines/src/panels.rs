//! One-way flags on an entity × time grain: a boolean flag that,
//! within entities, only ever switches off→on and never on→off.
//!
//! The same grain carries a second scan, of the entities
//! themselves: per-period counts of entities first and last
//! observed (entry and exit blocks, in the second half of this
//! module), and each entity's first activity on a measure,
//! dated against its first observed period.
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
pub(crate) enum EntitySkip {
    /// Fewer than two distinct values.
    TooFew,
    /// More distinct values than the entity cap.
    TooMany,
}

/// Display label for an entity key, for evidence payloads that
/// name entities.
fn key_label(key: &EntityKey) -> String {
    match key {
        EntityKey::Str(s) => s.clone(),
        EntityKey::Int(i) => i.to_string(),
        EntityKey::Float(bits) => {
            let v = f64::from_bits(*bits);
            if v == v.trunc() && v.abs() < 1e15 {
                format!("{}", v as i64)
            } else {
                format!("{v}")
            }
        }
        EntityKey::Bool(b) => b.to_string(),
    }
}

/// Per-row entity codes and the distinct count, or the reason the
/// column cannot key entities.
pub(crate) fn entity_codes(
    table: &TypedTable,
    col: usize,
) -> Result<(Vec<Option<u32>>, usize), EntitySkip> {
    entity_index(table, col).map(|(codes, distinct, _)| (codes, distinct))
}

/// Per-row entity codes, the distinct count, and each code's
/// display label, in code order.
type EntityIndex = (Vec<Option<u32>>, usize, Vec<String>);

/// Entity codes plus the distinct count and each code's display
/// label, in code order.
fn entity_index(table: &TypedTable, col: usize) -> Result<EntityIndex, EntitySkip> {
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
    let mut labels: Vec<String> = Vec::new();
    let mut codes: Vec<Option<u32>> = Vec::with_capacity(keys.len());
    for key in keys {
        let Some(key) = key else {
            codes.push(None);
            continue;
        };
        let code = match index.get(&key) {
            Some(&code) => code,
            None => {
                let code = index.len() as u32;
                labels.push(key_label(&key));
                index.insert(key, code);
                code
            }
        };
        if index.len() > MAX_ENTITIES {
            return Err(EntitySkip::TooMany);
        }
        codes.push(Some(code));
    }
    let distinct = index.len();
    if distinct < 2 {
        return Err(EntitySkip::TooFew);
    }
    Ok((codes, distinct, labels))
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

// ---------------------------------------------------------------------------
// Entry and exit blocks, and first activity.
//
// The flag scan follows one column through entities; this scan
// follows the entities themselves. On the same entity × time
// grain it takes, for every entity, the first and last axis
// periods in which the entity is observed, and counts arrivals
// and departures per period. A period whose arrivals (or
// departures) far exceed the panel's typical period is a block:
// the shape of a cohort joining at once, a data collection that
// started or ended for one source, or two extracts merged at
// that period. The first period cannot be an entry block (every
// founding entity first appears there, by construction) and the
// last period cannot be an exit block (entities present there
// are still under observation, not departed).
//
// Where a measure exists, the scan also dates each entity's
// first activity — the first period in which the measure is
// active for that entity — against its first observed period.
// Presence is not activity: an entity enrolled from the first
// period may do nothing for several periods, and a panel read
// from first appearance alone overstates the early periods.

const MICROS_PER_DAY: i64 = 86_400_000_000;
/// Smallest number of entities arriving or leaving in one period
/// for the period to be a block at all.
const MIN_BLOCK_ENTITIES: usize = 5;
/// A block's count must be at least this multiple of the typical
/// period's count for the same direction (the median per-period
/// count over the candidate periods, floored at one).
const BLOCK_RATIO: f64 = 4.0;
/// A block must also hold at least this share of all entities,
/// so a handful of simultaneous arrivals on a very large panel
/// reads as churn, not a block.
const MIN_BLOCK_SHARE: f64 = 0.02;
/// Blocks reported per direction per grain; further blocks are
/// counted in the coverage detail.
const MAX_BLOCKS_PER_DIRECTION: usize = 3;
/// Smallest number of late-starting entities for a first-activity
/// statement.
const MIN_LATE_STARTERS: usize = 3;
/// Late starters must be at least this share of the entities
/// that ever become active.
const MIN_LATE_SHARE: f64 = 0.10;
/// Per-entity spans listed in a finding's evidence; further
/// spans are counted in a metric.
const MAX_SPANS_LISTED: usize = 200;
/// Periods listed in a block finding's per-period series; longer
/// axes are truncated with the cut stated in a metric.
const MAX_PERIOD_SERIES: usize = 200;

/// A period label for a tick, as the series pass states it: a
/// date for whole-day ticks, a datetime otherwise.
fn tick_label(micros: i64) -> String {
    if micros.rem_euclid(MICROS_PER_DAY) == 0 {
        onceover_core::dateutil::iso_date(micros.div_euclid(MICROS_PER_DAY))
    } else {
        onceover_core::dateutil::iso_datetime(micros)
    }
}

/// The median of per-period counts, at the same percentile
/// position the lag summaries use.
fn median_count(counts: &[usize]) -> usize {
    if counts.is_empty() {
        return 0;
    }
    let mut sorted = counts.to_vec();
    sorted.sort_unstable();
    sorted[((0.5 * (sorted.len() - 1) as f64).round() as usize).min(sorted.len() - 1)]
}

fn is_block(count: usize, typical: usize, entities: usize) -> bool {
    count >= MIN_BLOCK_ENTITIES
        && (count as f64) >= BLOCK_RATIO * (typical.max(1) as f64)
        && (count as f64) >= MIN_BLOCK_SHARE * (entities as f64)
}

/// Block periods within one direction's candidate counts:
/// (period index, count), strongest first, plus the typical
/// count the blocks were judged against.
fn find_blocks(counts: &[usize], offset: usize, entities: usize) -> (Vec<(usize, usize)>, usize) {
    let typical = median_count(counts);
    let mut blocks: Vec<(usize, usize)> = counts
        .iter()
        .enumerate()
        .filter(|(_, &c)| is_block(c, typical, entities))
        .map(|(i, &c)| (i + offset, c))
        .collect();
    blocks.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    (blocks, typical)
}

/// Whether a measure is active on one row: non-zero for a numeric
/// measure, true for a boolean, present for any other kind. A
/// missing value is never activity.
fn measure_active(table: &TypedTable, col: usize, r: usize) -> bool {
    match &table.cols[col] {
        TypedCol::Bool { vals } => vals[r].unwrap_or(false),
        _ => match table.cols[col].numeric_at(r) {
            Some(v) => v != 0.0,
            None => !table.cols[col].is_null_at(r),
        },
    }
}

/// One grain's entry/exit structure.
struct DynamicsScan {
    /// Entities with at least one row on the axis.
    entities: usize,
    /// Entities whose rows all lack a time value.
    without_period: usize,
    entries: Vec<usize>,
    exits: Vec<usize>,
    /// (entity code, first period, last period) per observed entity.
    spans: Vec<(u32, usize, usize)>,
}

fn scan_dynamics(
    codes: &[Option<u32>],
    distinct: usize,
    period_of: &[Option<usize>],
    periods: usize,
) -> DynamicsScan {
    let mut first: Vec<Option<usize>> = vec![None; distinct];
    let mut last: Vec<Option<usize>> = vec![None; distinct];
    for r in 0..codes.len() {
        let (Some(code), Some(period)) = (codes[r], period_of[r]) else {
            continue;
        };
        let c = code as usize;
        first[c] = Some(first[c].map_or(period, |f| f.min(period)));
        last[c] = Some(last[c].map_or(period, |l| l.max(period)));
    }
    let mut scan = DynamicsScan {
        entities: 0,
        without_period: 0,
        entries: vec![0; periods],
        exits: vec![0; periods],
        spans: Vec::new(),
    };
    for code in 0..distinct {
        match (first[code], last[code]) {
            (Some(f), Some(l)) => {
                scan.entries[f] += 1;
                scan.exits[l] += 1;
                scan.spans.push((code as u32, f, l));
            }
            _ => scan.without_period += 1,
        }
    }
    scan.entities = scan.spans.len();
    scan
}

/// First activity on the principal measure: the measure with the
/// most entities ever active (the earliest such column on a tie).
struct ActivityScan {
    measure: usize,
    /// Entities with at least one active row.
    active: usize,
    /// Active entities whose first activity is their first
    /// observed period.
    on_time: usize,
    /// Active entities whose first activity is later.
    late: usize,
    /// Observed entities never active on the measure.
    never: usize,
    lag_median: f64,
    lag_p25: f64,
    lag_p75: f64,
    /// First active period per entity code.
    first_active: Vec<Option<usize>>,
}

fn scan_activity(
    table: &TypedTable,
    input: &PanelInput,
    codes: &[Option<u32>],
    period_of: &[Option<usize>],
    first_of: &[Option<usize>],
    entities: usize,
) -> Option<ActivityScan> {
    let mut best: Option<ActivityScan> = None;
    for &m in &input.measures {
        let mut first_active: Vec<Option<usize>> = vec![None; first_of.len()];
        for r in 0..codes.len() {
            let (Some(code), Some(period)) = (codes[r], period_of[r]) else {
                continue;
            };
            if measure_active(table, m, r) {
                let slot = &mut first_active[code as usize];
                *slot = Some(slot.map_or(period, |f| f.min(period)));
            }
        }
        let (mut active, mut on_time, mut late) = (0usize, 0usize, 0usize);
        let mut lags: Vec<f64> = Vec::new();
        for code in 0..first_of.len() {
            let (Some(first), Some(act)) = (first_of[code], first_active[code]) else {
                continue;
            };
            active += 1;
            if act == first {
                on_time += 1;
            } else {
                late += 1;
                lags.push((act - first) as f64);
            }
        }
        if active == 0 {
            continue;
        }
        lags.sort_by(f64::total_cmp);
        let pct =
            |q: f64| lags[((q * (lags.len() - 1) as f64).round() as usize).min(lags.len() - 1)];
        let scan = ActivityScan {
            measure: m,
            active,
            on_time,
            late,
            never: entities - active,
            lag_median: if lags.is_empty() { 0.0 } else { pct(0.5) },
            lag_p25: if lags.is_empty() { 0.0 } else { pct(0.25) },
            lag_p75: if lags.is_empty() { 0.0 } else { pct(0.75) },
            first_active,
        };
        if best.as_ref().is_none_or(|b| scan.active > b.active) {
            best = Some(scan);
        }
    }
    best
}

/// The per-entity span payload: entity, first and last observed
/// period labels, and the first activity period where the
/// activity scan dated one. Sorted by first period, then entity;
/// capped, with the number omitted returned alongside.
fn spans_payload(
    scan: &DynamicsScan,
    labels: &[String],
    axis_ticks: &[i64],
    first_active: Option<&[Option<usize>]>,
) -> (serde_json::Value, usize) {
    let mut spans: Vec<&(u32, usize, usize)> = scan.spans.iter().collect();
    spans.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| labels[a.0 as usize].cmp(&labels[b.0 as usize]))
    });
    let truncated = spans.len().saturating_sub(MAX_SPANS_LISTED);
    let list: Vec<serde_json::Value> = spans
        .iter()
        .take(MAX_SPANS_LISTED)
        .map(|&&(code, first, last)| {
            let mut entry = serde_json::Map::new();
            entry.insert("entity".into(), json!(labels[code as usize]));
            entry.insert("first_period".into(), json!(tick_label(axis_ticks[first])));
            entry.insert("last_period".into(), json!(tick_label(axis_ticks[last])));
            if let Some(active) = first_active.and_then(|fa| fa[code as usize]) {
                entry.insert(
                    "first_activity_period".into(),
                    json!(tick_label(axis_ticks[active])),
                );
            }
            serde_json::Value::Object(entry)
        })
        .collect();
    (json!(list), truncated)
}

/// The per-period series for a block finding's evidence: period
/// labels with the entry and exit counts aligned to them.
fn period_series(
    axis_ticks: &[i64],
    scan: &DynamicsScan,
) -> (
    serde_json::Value,
    serde_json::Value,
    serde_json::Value,
    bool,
) {
    let cut = axis_ticks.len().min(MAX_PERIOD_SERIES);
    (
        json!(axis_ticks[..cut]
            .iter()
            .map(|&t| tick_label(t))
            .collect::<Vec<_>>()),
        json!(scan.entries[..cut]),
        json!(scan.exits[..cut]),
        axis_ticks.len() > MAX_PERIOD_SERIES,
    )
}

const BLOCK_METHOD: &str = "entry and exit scan on the entity × time grain: an entity's first and last observed periods are taken over the rows where both the entity and the time value are present; a period is an entry block when the entities first appearing there number at least 5, at least four times the median per-period entry count over the periods after the first, and at least 2% of all entities; exit blocks are the mirror image over last-appearing entities, over the periods before the last; at most three blocks per direction per entity column are reported";

const ACTIVITY_METHOD: &str = "first-activity scan on the entity × time grain: the principal measure is the measure column with the most entities ever active (non-zero for a numeric measure, true for a boolean measure, present otherwise; a missing value is never activity); an entity's lag is its first active period minus its first observed period; the statement is reported when at least 3 entities start late and late starters are at least 10% of the entities that ever become active; the lag summary is over the late starters";

#[allow(clippy::too_many_arguments)]
fn build_block_finding(
    table: &TypedTable,
    input: &PanelInput,
    pattern: Pattern,
    entity: usize,
    period_index: usize,
    count: usize,
    typical: usize,
    scan: &DynamicsScan,
    axis_ticks: &[i64],
) -> Finding {
    let entity_name = &table.names[entity];
    let label = tick_label(axis_ticks[period_index]);
    let share = count as f64 / scan.entities.max(1) as f64;
    let (labels, entries, exits, truncated) = period_series(axis_ticks, scan);
    let entry = pattern == Pattern::EntryBlock;
    let (claim, interpretation, count_key, typical_key) = if entry {
        (
            format!(
                "{count} of {} {entity_name} entities first appear in the {} period {label} ({:.1}% of all entities), against a typical entry of {typical} per period",
                scan.entities,
                input.axis_name,
                100.0 * share,
            ),
            format!(
                "a block of {entity_name} entities first appearing in one {axis} period is the shape of a cohort starting together, a data collection that began for a second source, or two extracts merged at that period; it is not the panel's background arrival rate, and the period's rows are not comparable to a typical period's as a sample of the panel",
                axis = input.axis_name,
            ),
            "entering_entities",
            "typical_entries",
        )
    } else {
        (
            format!(
                "{count} of {} {entity_name} entities are last seen in the {} period {label} ({:.1}% of all entities), against a typical exit of {typical} per period",
                scan.entities,
                input.axis_name,
                100.0 * share,
            ),
            format!(
                "a block of {entity_name} entities last seen in one {axis} period is the shape of a programme or cohort ending together, or of one source's extract ending there; entities still present in the last period are not counted as exits, so the block is a departure, not the panel's end",
                axis = input.axis_name,
            ),
            "exiting_entities",
            "typical_exits",
        )
    };
    let mut metrics = BTreeMap::from([
        ("entity_column".into(), json!(entity_name)),
        ("time_column".into(), json!(input.axis_name)),
        ("period".into(), json!(label)),
        ("period_index".into(), json!(period_index)),
        (count_key.into(), json!(count)),
        ("entities".into(), json!(scan.entities)),
        ("entity_share".into(), json!(share)),
        (typical_key.into(), json!(typical)),
        ("periods".into(), json!(axis_ticks.len())),
        ("period_labels".into(), labels),
        ("entries_per_period".into(), entries),
        ("exits_per_period".into(), exits),
        ("method".into(), json!(BLOCK_METHOD)),
    ]);
    if truncated {
        metrics.insert("period_series_truncated".into(), json!(true));
    }
    Finding::new(
        pattern,
        BTreeMap::from([
            ("columns".into(), json!([entity_name])),
            ("time_column".into(), json!(input.axis_name)),
        ]),
        claim,
        EvidenceBasis::Measured,
        0.55 + 0.35 * share,
        metrics,
        Some(interpretation),
        Method::new(
            "series",
            "panel_dynamics_scan",
            "entity × time entry/exit scan (onceover v0.1)",
        ),
    )
}

fn build_activity_finding(
    table: &TypedTable,
    input: &PanelInput,
    entity: usize,
    scan: &DynamicsScan,
    activity: &ActivityScan,
    labels: &[String],
    axis_ticks: &[i64],
) -> Finding {
    let entity_name = &table.names[entity];
    let activity_name = &table.names[activity.measure];
    let late_share = activity.late as f64 / activity.active.max(1) as f64;
    let claim = format!(
        "{activity_name} first becomes active after the first observed period for {} of {} {entity_name} entities (median lag {} periods, middle half {} to {}); {} start in their first observed period and {} never become active",
        activity.late,
        activity.active,
        fmt_periods(activity.lag_median),
        fmt_periods(activity.lag_p25),
        fmt_periods(activity.lag_p75),
        activity.on_time,
        activity.never,
    );
    let (spans, spans_truncated) =
        spans_payload(scan, labels, axis_ticks, Some(&activity.first_active));
    let mut metrics = BTreeMap::from([
        ("entity_column".into(), json!(entity_name)),
        ("time_column".into(), json!(input.axis_name)),
        ("activity_column".into(), json!(activity_name)),
        ("entities".into(), json!(scan.entities)),
        ("entities_active".into(), json!(activity.active)),
        ("entities_starting_on_entry".into(), json!(activity.on_time)),
        ("entities_starting_late".into(), json!(activity.late)),
        ("entities_never_active".into(), json!(activity.never)),
        ("late_share".into(), json!(late_share)),
        ("lag_periods_median".into(), json!(activity.lag_median)),
        ("lag_periods_p25".into(), json!(activity.lag_p25)),
        ("lag_periods_p75".into(), json!(activity.lag_p75)),
        ("periods".into(), json!(axis_ticks.len())),
        ("entity_spans".into(), spans),
        ("method".into(), json!(ACTIVITY_METHOD)),
    ]);
    if spans_truncated > 0 {
        metrics.insert("entity_spans_truncated".into(), json!(spans_truncated));
    }
    Finding::new(
        Pattern::FirstActivity,
        BTreeMap::from([
            (
                "columns".into(),
                json!([entity_name, activity_name]),
            ),
            ("time_column".into(), json!(input.axis_name)),
        ]),
        claim,
        EvidenceBasis::Measured,
        0.5 + 0.3 * late_share,
        metrics,
        Some(format!(
            "presence in the panel is not activity: these {entity_name} entities are observed before they do anything on {activity_name}, so counts of active entities taken from first appearance overstate the early periods; each entity's first, last, and first-activity periods are in the evidence"
        )),
        Method::new(
            "series",
            "panel_dynamics_scan",
            "entity × time first-activity scan (onceover v0.1)",
        ),
    )
}

/// Check entry blocks, exit blocks, and first activity on every
/// entity grain the input names.
pub fn panel_dynamics(table: &TypedTable, input: &PanelInput) -> (Vec<Finding>, String) {
    // Period indices over the axis's distinct ticks.
    let mut axis_ticks: Vec<i64> = input.ticks.iter().flatten().copied().collect();
    axis_ticks.sort_unstable();
    axis_ticks.dedup();
    if axis_ticks.len() < MIN_PERIODS {
        return (
            vec![],
            format!(
                "{} has {} distinct periods; the entry and exit check needs at least {MIN_PERIODS}",
                input.axis_name,
                axis_ticks.len()
            ),
        );
    }
    if input.entities.is_empty() {
        return (
            vec![],
            format!(
                "0 entity columns along {}; nothing to check",
                input.axis_name
            ),
        );
    }
    let period_index: HashMap<i64, usize> = axis_ticks
        .iter()
        .enumerate()
        .map(|(i, &t)| (t, i))
        .collect();
    let period_of: Vec<Option<usize>> = input
        .ticks
        .iter()
        .map(|t| t.and_then(|t| period_index.get(&t).copied()))
        .collect();
    let periods = axis_ticks.len();

    let mut findings: Vec<Finding> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut grains_checked = 0usize;
    let (mut total_entry_blocks, mut total_exit_blocks, mut total_activity) =
        (0usize, 0usize, 0usize);
    for &entity in &input.entities {
        let (codes, distinct, labels) = match entity_index(table, entity) {
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
        let entity_name = &table.names[entity];
        let scan = scan_dynamics(&codes, distinct, &period_of, periods);
        if scan.without_period > 0 {
            notes.push(format!(
                "{} entities of {entity_name} have no row on the {} axis and are outside the entry and exit counts",
                scan.without_period, input.axis_name
            ));
        }
        let mut grain_findings: Vec<Finding> = Vec::new();
        let mut block_carriers: Vec<(usize, usize)> = Vec::new(); // (finding index, count)
        let (entry_blocks, entry_typical) = find_blocks(&scan.entries[1..], 1, scan.entities);
        let (exit_blocks, exit_typical) = find_blocks(&scan.exits[..periods - 1], 0, scan.entities);
        for (period, count) in entry_blocks.iter().take(MAX_BLOCKS_PER_DIRECTION) {
            block_carriers.push((grain_findings.len(), *count));
            grain_findings.push(build_block_finding(
                table,
                input,
                Pattern::EntryBlock,
                entity,
                *period,
                *count,
                entry_typical,
                &scan,
                &axis_ticks,
            ));
        }
        if entry_blocks.len() > MAX_BLOCKS_PER_DIRECTION {
            notes.push(format!(
                "entry blocks in {} further {} periods of {entity_name} beyond the {MAX_BLOCKS_PER_DIRECTION} reported",
                entry_blocks.len() - MAX_BLOCKS_PER_DIRECTION,
                input.axis_name
            ));
        }
        for (period, count) in exit_blocks.iter().take(MAX_BLOCKS_PER_DIRECTION) {
            block_carriers.push((grain_findings.len(), *count));
            grain_findings.push(build_block_finding(
                table,
                input,
                Pattern::ExitBlock,
                entity,
                *period,
                *count,
                exit_typical,
                &scan,
                &axis_ticks,
            ));
        }
        if exit_blocks.len() > MAX_BLOCKS_PER_DIRECTION {
            notes.push(format!(
                "exit blocks in {} further {} periods of {entity_name} beyond the {MAX_BLOCKS_PER_DIRECTION} reported",
                exit_blocks.len() - MAX_BLOCKS_PER_DIRECTION,
                input.axis_name
            ));
        }
        total_entry_blocks += grain_findings
            .iter()
            .filter(|f| f.finding_type.as_deref() == Some("entry_block"))
            .count();
        total_exit_blocks += grain_findings
            .iter()
            .filter(|f| f.finding_type.as_deref() == Some("exit_block"))
            .count();

        // First activity, where a measure exists to date it.
        let mut activity_idx: Option<usize> = None;
        if !input.measures.is_empty() {
            let mut first_of: Vec<Option<usize>> = vec![None; distinct];
            for &(code, first, _) in &scan.spans {
                first_of[code as usize] = Some(first);
            }
            match scan_activity(table, input, &codes, &period_of, &first_of, scan.entities) {
                Some(activity) => {
                    let fires = activity.late >= MIN_LATE_STARTERS
                        && (activity.late as f64)
                            >= MIN_LATE_SHARE * (activity.active.max(1) as f64);
                    if fires {
                        activity_idx = Some(grain_findings.len());
                        grain_findings.push(build_activity_finding(
                            table,
                            input,
                            entity,
                            &scan,
                            &activity,
                            &labels,
                            &axis_ticks,
                        ));
                        total_activity += 1;
                    }
                }
                None => {
                    notes.push(format!(
                        "no measure column shows any activity for {entity_name}, so first activity is not dated"
                    ));
                }
            }
        }

        // The per-entity spans ride on the first-activity finding
        // when it fired, else on the grain's largest block finding.
        if !grain_findings.is_empty() {
            let carrier = activity_idx.or_else(|| {
                block_carriers
                    .iter()
                    .max_by_key(|(_, count)| *count)
                    .map(|(idx, _)| *idx)
            });
            if let Some(carrier) = carrier {
                if activity_idx != Some(carrier) {
                    let (spans, truncated) = spans_payload(&scan, &labels, &axis_ticks, None);
                    let metrics = &mut grain_findings[carrier].evidence[0].metrics;
                    metrics.insert("entity_spans".into(), spans);
                    if truncated > 0 {
                        metrics.insert("entity_spans_truncated".into(), json!(truncated));
                    }
                }
            }
        }
        findings.append(&mut grain_findings);
    }
    findings.sort_by(|a, b| {
        b.interestingness
            .total_cmp(&a.interestingness)
            .then_with(|| a.claim.cmp(&b.claim))
    });
    if findings.len() > MAX_FINDINGS {
        notes.push(format!(
            "{} further entry, exit, or first-activity statements beyond the {MAX_FINDINGS} reported",
            findings.len() - MAX_FINDINGS
        ));
        findings.truncate(MAX_FINDINGS);
    }
    let mut detail = if grains_checked == 0 {
        format!(
            "no entity column with at least {MIN_ENTITIES} distinct values along {}; nothing to check",
            input.axis_name
        )
    } else {
        format!(
            "entry, exit, and first-activity structure checked on {grains_checked} entity columns along {} ({periods} periods): {total_entry_blocks} entry blocks, {total_exit_blocks} exit blocks, {total_activity} first-activity statements",
            input.axis_name
        )
    };
    for note in notes.iter().take(6) {
        detail.push_str("; ");
        detail.push_str(note);
    }
    (findings, detail)
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

    // ----- entry/exit blocks and first activity -----

    fn week_ticks(periods: usize) -> Vec<Option<i64>> {
        let mut ticks = Vec::new();
        for p in 0..periods {
            ticks.push(Some(
                1_735_689_600_000_000i64 + (p as i64) * 604_800_000_000,
            ));
        }
        ticks
    }

    /// A panel from per-entity (first, last) observed periods;
    /// per-row ticks aligned with the rows.
    fn span_panel(spans: &[(usize, usize)], periods: usize) -> (DataFrame, Vec<Option<i64>>) {
        let mut person = Vec::new();
        let mut ticks = Vec::new();
        let all_ticks = week_ticks(periods);
        for (e, &(first, last)) in spans.iter().enumerate() {
            for tick in all_ticks.iter().take(last + 1).skip(first) {
                person.push(format!("p{e:02}"));
                ticks.push(*tick);
            }
        }
        (df! { "person" => person }.unwrap(), ticks)
    }

    fn run_dynamics(
        df: &DataFrame,
        ticks: &[Option<i64>],
        measures: Vec<usize>,
    ) -> (Vec<Finding>, String) {
        let table = extract(df);
        let input = PanelInput {
            axis_name: "week",
            ticks,
            entities: vec![0],
            measures,
        };
        panel_dynamics(&table, &input)
    }

    fn by_type<'a>(findings: &'a [Finding], t: &str) -> Vec<&'a Finding> {
        findings
            .iter()
            .filter(|f| f.finding_type.as_deref() == Some(t))
            .collect()
    }

    #[test]
    fn entry_and_exit_blocks_are_reported() {
        // 80 entities over 12 periods: 40 present throughout, 20
        // more joining at period 5, 20 leaving after period 7.
        let mut spans: Vec<(usize, usize)> = Vec::new();
        spans.extend((0..40).map(|_| (0, 11)));
        spans.extend((0..20).map(|_| (5, 11)));
        spans.extend((0..20).map(|_| (0, 7)));
        let (df, ticks) = span_panel(&spans, 12);
        let (findings, detail) = run_dynamics(&df, &ticks, vec![]);
        let entries = by_type(&findings, "entry_block");
        let exits = by_type(&findings, "exit_block");
        assert_eq!(entries.len(), 1, "{detail}");
        assert_eq!(exits.len(), 1, "{detail}");
        let em = &entries[0].evidence[0].metrics;
        assert_eq!(em["period_index"], json!(5));
        assert_eq!(em["entering_entities"], json!(20));
        assert_eq!(em["entities"], json!(80));
        let xm = &exits[0].evidence[0].metrics;
        assert_eq!(xm["period_index"], json!(7));
        assert_eq!(xm["exiting_entities"], json!(20));
        entries[0].validate().unwrap();
        exits[0].validate().unwrap();
        // No measure: the per-entity spans ride on one block finding.
        let with_spans: Vec<_> = findings
            .iter()
            .filter(|f| f.evidence[0].metrics.contains_key("entity_spans"))
            .collect();
        assert_eq!(with_spans.len(), 1);
        assert_eq!(
            with_spans[0].evidence[0].metrics["entity_spans"]
                .as_array()
                .unwrap()
                .len(),
            80
        );
    }

    #[test]
    fn uniform_churn_has_no_blocks() {
        // 48 entities over 12 periods: 24 present throughout, 24
        // arriving two per period and staying. No period's entry
        // or exit count is a block.
        let mut spans: Vec<(usize, usize)> = Vec::new();
        spans.extend((0..24).map(|_| (0, 11)));
        spans.extend((0..24).map(|k| (k / 2, 11)));
        let (df, ticks) = span_panel(&spans, 12);
        let (findings, _) = run_dynamics(&df, &ticks, vec![]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// Entities present for all periods, with a measure that turns
    /// active `lag` periods after each entity's first period
    /// (None: never active).
    fn activity_panel(lags: &[Option<usize>], periods: usize) -> (DataFrame, Vec<Option<i64>>) {
        let mut person = Vec::new();
        let mut tokens = Vec::new();
        let mut ticks = Vec::new();
        let all_ticks = week_ticks(periods);
        for (e, lag) in lags.iter().enumerate() {
            for (p, tick) in all_ticks.iter().enumerate() {
                person.push(format!("p{e:02}"));
                tokens.push(match lag {
                    Some(l) if p >= *l => 10.0,
                    _ => 0.0,
                });
                ticks.push(*tick);
            }
        }
        let df = df! { "person" => person, "tokens" => tokens }.unwrap();
        (df, ticks)
    }

    #[test]
    fn first_activity_lag_is_reported() {
        // 20 entities over 10 periods: half active from their
        // first period, half starting three periods late.
        let mut lags: Vec<Option<usize>> = vec![Some(0); 10];
        lags.extend(vec![Some(3); 10]);
        let (df, ticks) = activity_panel(&lags, 10);
        let (findings, detail) = run_dynamics(&df, &ticks, vec![1]);
        let hits = by_type(&findings, "first_activity");
        assert_eq!(hits.len(), 1, "{detail}");
        let m = &hits[0].evidence[0].metrics;
        assert_eq!(m["activity_column"], json!("tokens"));
        assert_eq!(m["entities_active"], json!(20));
        assert_eq!(m["entities_starting_on_entry"], json!(10));
        assert_eq!(m["entities_starting_late"], json!(10));
        assert_eq!(m["entities_never_active"], json!(0));
        assert_eq!(m["lag_periods_median"], json!(3.0));
        let spans = m["entity_spans"].as_array().unwrap();
        assert_eq!(spans.len(), 20);
        assert!(spans[0].get("first_activity_period").is_some());
        hits[0].validate().unwrap();
    }

    #[test]
    fn first_activity_quiet_when_all_start_on_entry() {
        let lags: Vec<Option<usize>> = vec![Some(0); 20];
        let (df, ticks) = activity_panel(&lags, 10);
        let (findings, _) = run_dynamics(&df, &ticks, vec![1]);
        assert!(by_type(&findings, "first_activity").is_empty());
    }

    #[test]
    fn first_activity_needs_activity_somewhere() {
        let lags: Vec<Option<usize>> = vec![None; 20];
        let (df, ticks) = activity_panel(&lags, 10);
        let (findings, detail) = run_dynamics(&df, &ticks, vec![1]);
        assert!(by_type(&findings, "first_activity").is_empty());
        assert!(
            detail.contains("no measure column shows any activity"),
            "{detail}"
        );
    }

    #[test]
    fn short_axis_records_why_not() {
        let spans: Vec<(usize, usize)> = (0..12).map(|_| (0, 1)).collect();
        let (df, ticks) = span_panel(&spans, 2);
        let (findings, detail) = run_dynamics(&df, &ticks, vec![]);
        assert!(findings.is_empty());
        assert!(detail.contains("distinct periods"), "{detail}");
    }
}
