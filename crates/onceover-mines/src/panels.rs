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

// ---------- concentration over time and outlier entities ----------
//
// Two further readings on the same entity × time grain, both about
// where a measure's mass sits among entities — the tail the
// whole-column concentration check cannot see, because it pools
// the periods and never aggregates to the entity.
//
// Concentration over time: for each period, the share of the
// period's total held by the largest entities (the top group:
// ceil(1% of the entities), at least one). Only periods in
// which at least half of the entities are active count — on a
// frame whose periods are near-unique timestamps (one invoice
// per date) every period's top share is 100% by construction.
// The statement fires when that share is high in level (the
// median period's share is at least 30%) or when it rises
// materially (the mean share over the last third of periods
// exceeds the first third by at least 10 points and reaches at
// least 20%).
//
// Outlier entities: the measure summed per entity over all
// periods. An entity is reported when its total is at least 10
// times the typical entity's total (the median, or the upper
// quartile when the median is zero) and above the upper quartile
// plus three interquartile ranges. No single cell needs to be
// extreme — every row of such an entity is.
//
// Shares need a non-negative measure with a positive total, and
// a two-valued column is a flag, not a volume: both are excluded
// here and counted in the coverage detail. Below 20 entities one
// entity's share is mechanical, so neither statement fires.

/// Smallest number of entities before per-entity shares mean
/// anything.
const MIN_TAIL_ENTITIES: usize = 20;
/// Share-bearing periods needed before a concentration movement
/// is stated (the pass itself runs from MIN_PERIODS periods).
const MIN_CONC_PERIODS: usize = 4;
/// A period counts toward the concentration reading only when
/// at least this fraction of the entities are active in it. A
/// period a single entity occupies alone has a top share of
/// 100% by construction; shares rest on periods the entities
/// share.
const MIN_ACTIVE_FRACTION: f64 = 0.5;
/// The top group is the largest entities making up this fraction
/// of the entity count (at least one entity).
const TOP_GROUP_FRACTION: f64 = 0.01;
/// Median-period top-group share at or above which concentration
/// is high in level.
const LEVEL_SHARE: f64 = 0.30;
/// Rise in top-group share (last third of periods minus first
/// third, in share points) that counts as material movement.
const RISE_POINTS: f64 = 0.10;
/// The late-period share a rise must reach to be stated.
const RISE_FLOOR: f64 = 0.20;
/// An entity total at or above this multiple of the typical
/// entity's total is a tail candidate.
const OUTLIER_MULTIPLE: f64 = 10.0;
/// ... and it must also clear the upper quartile by this many
/// interquartile ranges.
const OUTLIER_FENCE_IQRS: f64 = 3.0;
/// Entities named in one outlier statement's evidence.
const MAX_NAMED_OUTLIERS: usize = 10;
/// Measures examined per entity grain.
const MAX_TAIL_MEASURES: usize = 12;
/// Findings emitted by this pass per profile, both kinds.
const MAX_TAIL_FINDINGS: usize = 10;

fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn fmt_pct(share: f64) -> String {
    format!("{:.1}%", 100.0 * share)
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((q * (sorted.len() - 1) as f64).round() as usize).min(sorted.len() - 1)]
}

/// One measure column's eligibility facts for the tail checks.
struct MeasureFacts {
    values: usize,
    total: f64,
    negative: bool,
    two_valued: bool,
}

fn measure_facts(table: &TypedTable, col: usize) -> MeasureFacts {
    let mut facts = MeasureFacts {
        values: 0,
        total: 0.0,
        negative: false,
        two_valued: true,
    };
    if matches!(table.cols[col], TypedCol::Bool { .. }) {
        return facts;
    }
    for r in 0..table.n {
        if let Some(v) = table.cols[col].numeric_at(r) {
            facts.values += 1;
            facts.total += v;
            if v < 0.0 {
                facts.negative = true;
            }
            if v != 0.0 && v != 1.0 {
                facts.two_valued = false;
            }
        }
    }
    facts
}

/// A multiple as claims print it: whole at 100x and above, one
/// decimal from 10x, two below.
fn fmt_ratio(x: f64) -> String {
    if x >= 100.0 {
        format!("{x:.0}")
    } else if x >= 10.0 {
        format!("{x:.1}")
    } else {
        fmt_num((x * 100.0).round() / 100.0)
    }
}

/// Display label per entity code: the value at the first row the
/// code appears on, formatted as the column's values print.
fn entity_labels(
    table: &TypedTable,
    col: usize,
    codes: &[Option<u32>],
    distinct: usize,
) -> Vec<String> {
    let mut labels: Vec<Option<String>> = vec![None; distinct];
    for (r, code) in codes.iter().enumerate() {
        let Some(c) = code else { continue };
        let slot = &mut labels[*c as usize];
        if slot.is_none() {
            *slot = Some(match &table.cols[col] {
                TypedCol::Str { vals } => vals[r].unwrap_or("").to_string(),
                TypedCol::StrOwned { vals } => vals[r].clone().unwrap_or_default(),
                TypedCol::I64 { vals } => vals.at(r).map(|v| v.to_string()).unwrap_or_default(),
                TypedCol::U64 { vals } => vals.at(r).map(|v| v.to_string()).unwrap_or_default(),
                TypedCol::F64 { vals } => vals.at(r).map(fmt_num).unwrap_or_default(),
                TypedCol::Bool { vals } => vals[r].map(|v| v.to_string()).unwrap_or_default(),
                TypedCol::Date { vals } => vals[r]
                    .map(|v| onceover_core::dateutil::iso_date(v as i64))
                    .unwrap_or_default(),
                TypedCol::Datetime { vals, unit } => vals[r]
                    .map(|v| {
                        onceover_core::dateutil::iso_date(onceover_core::dateutil::timestamp_days(
                            v, *unit,
                        ))
                    })
                    .unwrap_or_default(),
                _ => String::new(),
            });
        }
    }
    labels.into_iter().map(|l| l.unwrap_or_default()).collect()
}

/// Check concentration over time and outlier entities for every
/// eligible measure on every entity grain the input names.
pub fn panel_concentration(table: &TypedTable, input: &PanelInput) -> (Vec<Finding>, String) {
    // Period indices over the axis's distinct ticks.
    let mut ticks: Vec<i64> = input.ticks.iter().flatten().copied().collect();
    ticks.sort_unstable();
    ticks.dedup();
    if ticks.len() < MIN_PERIODS {
        return (
            vec![],
            format!(
                "{} has {} distinct periods; the panel concentration checks need at least {MIN_PERIODS}",
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

    if input.entities.is_empty() {
        return (
            vec![],
            format!(
                "no entity columns along {}; nothing to check",
                input.axis_name
            ),
        );
    }

    // Eligible measures: numeric, non-negative, positive total,
    // not two-valued, and not the axis itself.
    let mut notes: Vec<String> = Vec::new();
    let mut eligible: Vec<usize> = Vec::new();
    let (mut n_flags, mut n_negative, mut n_nonpositive) = (0usize, 0usize, 0usize);
    for &m in &input.measures {
        if table.names[m] == input.axis_name {
            continue;
        }
        let facts = measure_facts(table, m);
        if facts.values == 0 {
            continue;
        }
        if facts.two_valued {
            n_flags += 1;
        } else if facts.negative {
            n_negative += 1;
        } else if facts.total <= 0.0 {
            n_nonpositive += 1;
        } else {
            eligible.push(m);
        }
    }
    if n_flags > 0 {
        notes.push(format!(
            "{n_flags} two-valued measures not examined (they are flags, not volumes)"
        ));
    }
    if n_negative > 0 {
        notes.push(format!(
            "{n_negative} measures with negative values not examined (shares need a non-negative measure)"
        ));
    }
    if n_nonpositive > 0 {
        notes.push(format!(
            "{n_nonpositive} measures with no positive total not examined"
        ));
    }
    if eligible.len() > MAX_TAIL_MEASURES {
        notes.push(format!(
            "{} further measures beyond the {MAX_TAIL_MEASURES} examined",
            eligible.len() - MAX_TAIL_MEASURES
        ));
        eligible.truncate(MAX_TAIL_MEASURES);
    }
    if eligible.is_empty() {
        let mut detail = format!(
            "no eligible measures along {}; nothing to check",
            input.axis_name
        );
        for note in notes.iter().take(6) {
            detail.push_str("; ");
            detail.push_str(note);
        }
        return (vec![], detail);
    }

    let mut findings: Vec<Finding> = Vec::new();
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
        if distinct < MIN_TAIL_ENTITIES {
            notes.push(format!(
                "{} has {distinct} distinct values; the panel concentration checks need at least {MIN_TAIL_ENTITIES} entities",
                table.names[entity]
            ));
            continue;
        }
        grains_checked += 1;
        let labels = entity_labels(table, entity, &codes, distinct);
        for &measure in &eligible {
            if measure == entity {
                continue;
            }
            let (conc, out) = scan_measure(
                table,
                input,
                entity,
                measure,
                &codes,
                distinct,
                &labels,
                &period_of,
                ticks.len(),
            );
            findings.extend(conc);
            findings.extend(out);
        }
    }
    findings.sort_by(|a, b| {
        b.interestingness
            .total_cmp(&a.interestingness)
            .then_with(|| a.claim.cmp(&b.claim))
    });
    let (mut n_conc, mut n_out) = (0usize, 0usize);
    for f in &findings {
        match f.finding_type.as_deref() {
            Some("concentration_over_time") => n_conc += 1,
            Some("outlier_entity") => n_out += 1,
            _ => {}
        }
    }
    if findings.len() > MAX_TAIL_FINDINGS {
        notes.push(format!(
            "{} further panel concentration statements beyond the {MAX_TAIL_FINDINGS} reported",
            findings.len() - MAX_TAIL_FINDINGS
        ));
        findings.truncate(MAX_TAIL_FINDINGS);
    }
    let mut detail = format!(
        "{} measures checked on {} entity columns along {}; {} concentration and {} outlier statements",
        eligible.len(),
        grains_checked,
        input.axis_name,
        n_conc,
        n_out
    );
    for note in notes.iter().take(6) {
        detail.push_str("; ");
        detail.push_str(note);
    }
    (findings, detail)
}

/// One (entity grain, measure) scan: accumulate per-(entity,
/// period) sums once, then read both statements off them.
#[allow(clippy::too_many_arguments)]
fn scan_measure(
    table: &TypedTable,
    input: &PanelInput,
    entity: usize,
    measure: usize,
    codes: &[Option<u32>],
    distinct: usize,
    labels: &[String],
    period_of: &[Option<usize>],
    n_periods: usize,
) -> (Option<Finding>, Option<Finding>) {
    let mut cells: HashMap<(u32, u32), f64> = HashMap::new();
    let mut totals = vec![0.0f64; distinct];
    let mut observed = vec![false; distinct];
    let mut rows_used = 0usize;
    for r in 0..table.n {
        let (Some(code), Some(period)) = (codes[r], period_of[r]) else {
            continue;
        };
        let Some(v) = table.cols[measure].numeric_at(r) else {
            continue;
        };
        rows_used += 1;
        *cells.entry((code, period as u32)).or_default() += v;
        totals[code as usize] += v;
        observed[code as usize] = true;
    }
    let n_entities = observed.iter().filter(|o| **o).count();
    if n_entities < MIN_TAIL_ENTITIES {
        return (None, None);
    }
    let concentration = concentration_finding(
        table, input, entity, measure, labels, &cells, n_entities, n_periods, rows_used,
    );
    let outlier = outlier_finding(
        table, input, entity, measure, labels, &totals, &observed, rows_used,
    );
    (concentration, outlier)
}

#[allow(clippy::too_many_arguments)]
fn concentration_finding(
    table: &TypedTable,
    input: &PanelInput,
    entity: usize,
    measure: usize,
    labels: &[String],
    cells: &HashMap<(u32, u32), f64>,
    n_entities: usize,
    n_periods: usize,
    rows_used: usize,
) -> Option<Finding> {
    let mut by_period: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n_periods];
    for (&(code, period), &sum) in cells {
        by_period[period as usize].push((code, sum));
    }
    let top_k = ((n_entities as f64 * TOP_GROUP_FRACTION).ceil() as usize).max(1);
    let mut shares: Vec<f64> = Vec::new();
    let mut share_periods: Vec<usize> = Vec::new();
    let mut sparse_periods = 0usize;
    for (p, sums) in by_period.iter().enumerate() {
        if sums.is_empty() {
            continue;
        }
        let total: f64 = sums.iter().map(|(_, s)| *s).sum();
        if total <= 0.0 {
            continue;
        }
        if (sums.len() as f64) < n_entities as f64 * MIN_ACTIVE_FRACTION {
            sparse_periods += 1;
            continue;
        }
        let mut vals: Vec<f64> = sums.iter().map(|(_, s)| *s).collect();
        vals.sort_by(|a, b| b.total_cmp(a));
        let top: f64 = vals.iter().take(top_k).sum();
        shares.push(top / total);
        share_periods.push(p);
    }
    if shares.len() < MIN_CONC_PERIODS {
        return None;
    }
    let mut sorted = shares.clone();
    sorted.sort_by(f64::total_cmp);
    let median_share = quantile(&sorted, 0.5);
    let third = ((shares.len() as f64) / 3.0).ceil() as usize;
    let early = shares[..third].iter().sum::<f64>() / third as f64;
    let late = shares[shares.len() - third..].iter().sum::<f64>() / third as f64;
    let level = median_share >= LEVEL_SHARE;
    let rising = late - early >= RISE_POINTS && late >= RISE_FLOOR;
    if !level && !rising {
        return None;
    }
    let measure_name = &table.names[measure];
    let entity_name = &table.names[entity];
    // The head of the top group in the last share-bearing period.
    let last_p = *share_periods.last().unwrap();
    let mut last_sums = by_period[last_p].clone();
    last_sums.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let last_total: f64 = last_sums
        .iter()
        .map(|(_, s)| *s)
        .sum::<f64>()
        .max(f64::MIN_POSITIVE);
    let top_named: Vec<serde_json::Value> = last_sums
        .iter()
        .take(3)
        .map(|(code, s)| {
            json!({
                "entity": labels[*code as usize],
                "share": s / last_total,
            })
        })
        .collect();
    let named_text: Vec<String> = last_sums
        .iter()
        .take(3)
        .map(|(code, s)| format!("{} ({})", labels[*code as usize], fmt_pct(s / last_total)))
        .collect();
    let verb = if top_k == 1 { "holds" } else { "hold" };
    let movement = if level && rising {
        format!(
            "{verb} {} of each {} period's {} (median period), up from {} in the first periods to {} in the last",
            fmt_pct(median_share),
            input.axis_name,
            measure_name,
            fmt_pct(early),
            fmt_pct(late),
        )
    } else if level {
        format!(
            "{verb} {} of each {} period's {} (median period; {} in the first period, {} in the last)",
            fmt_pct(median_share),
            input.axis_name,
            measure_name,
            fmt_pct(shares[0]),
            fmt_pct(*shares.last().unwrap()),
        )
    } else {
        format!(
            "held {} of each {} period's {} in the first periods and {} in the last (median period {})",
            fmt_pct(early),
            input.axis_name,
            measure_name,
            fmt_pct(late),
            fmt_pct(median_share),
        )
    };
    let claim = format!(
        "the top {} of {} {} (the top 1%) {}; an equal split over {} {} would give the top {} {}; the largest in the last period: {}",
        top_k,
        n_entities,
        entity_name,
        movement,
        n_entities,
        entity_name,
        top_k,
        fmt_pct(top_k as f64 / n_entities as f64),
        named_text.join(", "),
    );
    let level_score = 0.55 + 0.40 * median_share.min(1.0);
    let rise_score = 0.55 + 0.40 * ((late - early) * 2.5).min(1.0);
    let score = if level && rising {
        level_score.max(rise_score)
    } else if level {
        level_score
    } else {
        rise_score
    };
    let metrics = BTreeMap::from([
        ("measure".into(), json!(measure_name)),
        ("entity_column".into(), json!(entity_name)),
        ("time_column".into(), json!(input.axis_name)),
        ("entities".into(), json!(n_entities)),
        ("periods".into(), json!(shares.len())),
        (
            "periods_set_aside_sparse".into(),
            json!(sparse_periods),
        ),
        ("top_group_size".into(), json!(top_k)),
        ("top_group_fraction".into(), json!(TOP_GROUP_FRACTION)),
        ("median_top_share".into(), json!(median_share)),
        ("first_period_top_share".into(), json!(shares[0])),
        (
            "last_period_top_share".into(),
            json!(*shares.last().unwrap()),
        ),
        ("early_top_share".into(), json!(early)),
        ("late_top_share".into(), json!(late)),
        ("top_share_rise".into(), json!(late - early)),
        (
            "equal_split_top_share".into(),
            json!(top_k as f64 / n_entities as f64),
        ),
        ("period_top_shares".into(), json!(shares)),
        ("top_entities_last_period".into(), json!(top_named)),
        ("n".into(), json!(rows_used)),
        ("population_rows".into(), json!(table.n)),
        (
            "method".into(),
            json!("per-period top-share scan on the entity × time grain: for each period with a positive total, the share of the period's total held by the largest entities (the top group: ceil(1% of the entities), at least one); reported when the median period share is at least 30%, or the mean share over the last third of periods exceeds the first third by at least 10 share points and reaches at least 20%; at least 20 entities and 4 share-bearing periods are required; measures with negative values, a non-positive total, or only 0/1 values are not examined"),
        ),
    ]);
    let interpretation = format!(
        "a share statement about the {entity_name} × {} grain that the whole-frame concentration check cannot make, because it pools the periods: the centre of mass of {measure_name} sits in the top group of {entity_name}{}, so period totals describe the top group more than the typical {entity_name}",
        input.axis_name,
        if rising && !level {
            " and is moving further into it"
        } else {
            ""
        },
    );
    Some(Finding::new(
        Pattern::ConcentrationOverTime,
        BTreeMap::from([
            ("columns".into(), json!([measure_name, entity_name])),
            ("time_column".into(), json!(input.axis_name)),
        ]),
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        Some(interpretation),
        Method::new(
            "series",
            "panel_concentration_scan",
            "entity × time top-share scan (onceover v0.1)",
        ),
    ))
}

#[allow(clippy::too_many_arguments)]
fn outlier_finding(
    table: &TypedTable,
    input: &PanelInput,
    entity: usize,
    measure: usize,
    labels: &[String],
    totals: &[f64],
    observed: &[bool],
    rows_used: usize,
) -> Option<Finding> {
    let mut sorted: Vec<f64> = totals
        .iter()
        .zip(observed.iter())
        .filter(|(_, o)| **o)
        .map(|(t, _)| *t)
        .collect();
    sorted.sort_by(f64::total_cmp);
    let n_entities = sorted.len();
    let median = quantile(&sorted, 0.5);
    let q1 = quantile(&sorted, 0.25);
    let q3 = quantile(&sorted, 0.75);
    let typical = if median > 0.0 { median } else { q3 };
    if typical <= 0.0 {
        return None;
    }
    let fence = q3 + OUTLIER_FENCE_IQRS * (q3 - q1);
    let grand: f64 = sorted.iter().sum();
    if grand <= 0.0 {
        return None;
    }
    let mut outliers: Vec<(u32, f64)> = totals
        .iter()
        .enumerate()
        .filter(|(i, t)| observed[*i] && **t >= OUTLIER_MULTIPLE * typical && **t > fence)
        .map(|(i, t)| (i as u32, *t))
        .collect();
    if outliers.is_empty() {
        return None;
    }
    outliers.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let combined: f64 = outliers.iter().map(|(_, t)| t).sum();
    let measure_name = &table.names[measure];
    let entity_name = &table.names[entity];
    let named: Vec<serde_json::Value> = outliers
        .iter()
        .take(MAX_NAMED_OUTLIERS)
        .map(|(code, t)| {
            json!({
                "entity": labels[*code as usize],
                "total": t,
                "ratio_to_typical": t / typical,
                "share": t / grand,
            })
        })
        .collect();
    let named_text: Vec<String> = outliers
        .iter()
        .take(3)
        .map(|(code, t)| {
            format!(
                "{} (total {}, {}x the typical {})",
                labels[*code as usize],
                fmt_num(*t),
                fmt_ratio(t / typical),
                entity_name,
            )
        })
        .collect();
    let mut claim = if outliers.len() == 1 {
        format!(
            "one {} sits far outside the {} distribution of total {}: {}; it holds {} of all {}",
            entity_name,
            entity_name,
            measure_name,
            named_text.join(", "),
            fmt_pct(combined / grand),
            measure_name,
        )
    } else {
        format!(
            "{} {} sit far outside the {} distribution of total {}: {}; together they hold {} of all {}",
            outliers.len(),
            entity_name,
            entity_name,
            measure_name,
            named_text.join(", "),
            fmt_pct(combined / grand),
            measure_name,
        )
    };
    if outliers.len() > 3 {
        claim.push_str(&format!(
            "; {} more are in the evidence",
            outliers.len() - 3
        ));
    }
    claim.push_str(&format!(
        "; the typical {} totals {}",
        entity_name,
        fmt_num(typical),
    ));
    let max_ratio = outliers[0].1 / typical;
    let score = (0.55 + 0.20 * max_ratio.log10()).min(0.95);
    let metrics = BTreeMap::from([
        ("measure".into(), json!(measure_name)),
        ("entity_column".into(), json!(entity_name)),
        ("time_column".into(), json!(input.axis_name)),
        ("entities".into(), json!(n_entities)),
        ("outlier_entities".into(), json!(outliers.len())),
        ("typical_total".into(), json!(typical)),
        ("median_total".into(), json!(median)),
        ("upper_quartile_total".into(), json!(q3)),
        ("fence_total".into(), json!(fence)),
        ("multiple_of_typical".into(), json!(OUTLIER_MULTIPLE)),
        ("grand_total".into(), json!(grand)),
        ("outlier_share".into(), json!(combined / grand)),
        ("outliers".into(), json!(named)),
        ("n".into(), json!(rows_used)),
        ("population_rows".into(), json!(table.n)),
        (
            "method".into(),
            json!("entity-total tail scan on the entity × time grain: the measure is summed per entity over all periods; an entity is reported when its total is at least 10 times the typical entity's total (the median total, or the upper quartile when the median is zero) and above the upper quartile plus three interquartile ranges; at least 20 entities are required"),
        ),
    ]);
    let interpretation = format!(
        "a statement about {entity_name}, not about cells: no single {measure_name} value needs to be extreme — the {entity_name}'s ordinary rows add up to a level the other {entity_name} never approach, so a rule written per row cannot state what a per-entity rule states in one line",
    );
    Some(Finding::new(
        Pattern::OutlierEntity,
        BTreeMap::from([
            ("columns".into(), json!([measure_name, entity_name])),
            ("time_column".into(), json!(input.axis_name)),
        ]),
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        Some(interpretation),
        Method::new(
            "series",
            "panel_concentration_scan",
            "entity × time entity-total scan (onceover v0.1)",
        ),
    ))
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

    // ----- panel concentration and outlier entities -----

    fn tail_panel(values: &[Vec<f64>]) -> (DataFrame, Vec<Option<i64>>) {
        let mut person = Vec::new();
        let mut tokens = Vec::new();
        let mut ticks = Vec::new();
        for (e, series) in values.iter().enumerate() {
            for (w, v) in series.iter().enumerate() {
                person.push(format!("p{e:02}"));
                tokens.push(*v);
                ticks.push(Some(
                    1_735_689_600_000_000i64 + (w as i64) * 604_800_000_000,
                ));
            }
        }
        let df = df! { "person" => person, "tokens" => tokens }.unwrap();
        (df, ticks)
    }

    fn run_tail(df: &DataFrame, ticks: &[Option<i64>]) -> (Vec<Finding>, String) {
        let table = extract(df);
        let input = PanelInput {
            axis_name: "week",
            ticks,
            entities: vec![0],
            measures: vec![1],
        };
        panel_concentration(&table, &input)
    }

    fn of_type<'a>(findings: &'a [Finding], ty: &str) -> Vec<&'a Finding> {
        findings
            .iter()
            .filter(|f| f.finding_type.as_deref() == Some(ty))
            .collect()
    }

    #[test]
    fn rising_top_share_is_reported() {
        // 60 entities x 12 periods at 100 per period; entity 0
        // ramps 260 -> 4_900, so its share of each period's total
        // rises from about 4% to about 45%.
        let values: Vec<Vec<f64>> = (0..60)
            .map(|e| {
                if e == 0 {
                    (0..12)
                        .map(|w| 260.0 + (4900.0 - 260.0) * w as f64 / 11.0)
                        .collect()
                } else {
                    vec![100.0; 12]
                }
            })
            .collect();
        let (df, ticks) = tail_panel(&values);
        let (findings, _) = run_tail(&df, &ticks);
        let hits = of_type(&findings, "concentration_over_time");
        assert_eq!(
            hits.len(),
            1,
            "{:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let m = &hits[0].evidence[0].metrics;
        assert_eq!(m["entities"], json!(60));
        assert_eq!(m["top_group_size"], json!(1));
        let early = m["early_top_share"].as_f64().unwrap();
        let late = m["late_top_share"].as_f64().unwrap();
        assert!(late - early >= 0.10, "early {early} late {late}");
        assert!(late >= 0.40, "late {late}");
        assert!(hits[0].claim.contains("p00"), "{}", hits[0].claim);
        hits[0].validate().unwrap();
    }

    #[test]
    fn hundredfold_entity_is_named_as_outlier() {
        // 60 entities x 12 periods at 100 per period; entity 7 at
        // 10_000 per period — 100x the typical entity's total.
        let values: Vec<Vec<f64>> = (0..60)
            .map(|e| {
                if e == 7 {
                    vec![10_000.0; 12]
                } else {
                    vec![100.0; 12]
                }
            })
            .collect();
        let (df, ticks) = tail_panel(&values);
        let (findings, _) = run_tail(&df, &ticks);
        let hits = of_type(&findings, "outlier_entity");
        assert_eq!(
            hits.len(),
            1,
            "{:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
        let m = &hits[0].evidence[0].metrics;
        assert_eq!(m["outlier_entities"], json!(1));
        assert_eq!(m["outliers"][0]["entity"], json!("p07"));
        let ratio = m["outliers"][0]["ratio_to_typical"].as_f64().unwrap();
        assert!((ratio - 100.0).abs() < 0.01, "ratio {ratio}");
        assert!(hits[0].claim.contains("p07"), "{}", hits[0].claim);
        hits[0].validate().unwrap();
    }

    #[test]
    fn balanced_panel_reports_neither() {
        let values: Vec<Vec<f64>> = (0..60)
            .map(|e| {
                (0..12)
                    .map(|w| 100.0 + ((e + w) % 3) as f64)
                    .collect::<Vec<f64>>()
            })
            .collect();
        let (df, ticks) = tail_panel(&values);
        let (findings, detail) = run_tail(&df, &ticks);
        assert!(findings.is_empty(), "{detail}");
    }

    #[test]
    fn periods_one_entity_occupies_alone_state_no_concentration() {
        // 60 entities x 12 periods, but each period holds only 5
        // entities' rows, one of them carrying 10_000 against
        // 100s: the top share of every period is about 96% by
        // construction, because the periods are not shared. No
        // concentration statement is made.
        let mut person = Vec::new();
        let mut tokens = Vec::new();
        let mut ticks = Vec::new();
        for w in 0..12usize {
            for k in 0..5usize {
                let e = w * 5 + k;
                person.push(format!("p{e:02}"));
                tokens.push(if k == 0 { 10_000.0 } else { 100.0 });
                ticks.push(Some(
                    1_735_689_600_000_000i64 + (w as i64) * 604_800_000_000,
                ));
            }
        }
        let df = df! { "person" => person, "tokens" => tokens }.unwrap();
        let (findings, _) = run_tail(&df, &ticks);
        assert!(
            of_type(&findings, "concentration_over_time").is_empty(),
            "{:?}",
            findings.iter().map(|f| &f.claim).collect::<Vec<_>>()
        );
    }

    #[test]
    fn small_panel_is_below_the_entity_floor() {
        // 8 entities, one ramping hard: shares are mechanical at
        // this size, so nothing is stated and the coverage detail
        // says why.
        let values: Vec<Vec<f64>> = (0..8)
            .map(|e| {
                if e == 0 {
                    (0..12)
                        .map(|w| 260.0 + (4900.0 - 260.0) * w as f64 / 11.0)
                        .collect()
                } else {
                    vec![100.0; 12]
                }
            })
            .collect();
        let (df, ticks) = tail_panel(&values);
        let (findings, detail) = run_tail(&df, &ticks);
        assert!(findings.is_empty(), "{detail}");
        assert!(detail.contains("at least 20 entities"), "{detail}");
    }
}
