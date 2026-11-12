//! `changeforest`: classifier-based changepoint detection over the
//! joint distribution of the measure columns along the time axis.
//!
//! The breaks pass fits two-segment models of one series at a time
//! (level, trend, variance), so it cannot state a change that leaves
//! every single-series summary intact — a covariance change at
//! constant means and variances is the standard example. This pass
//! runs binary segmentation with a random-forest classifier gain
//! (Londschien, Bühlmann & Kovács, JMLR 24 (2023), via the
//! `changeforest` crate): for a candidate split, a forest is trained
//! to distinguish the rows before the split from the rows after it,
//! and the split's gain is the classifier log-likelihood ratio of
//! its out-of-bag predictions. A split is stated when the
//! permutation p-value of its segment's maximum gain clears
//! [`ChangeforestConfig::alpha`].
//!
//! Rows are the frame's rows in axis order: on an entity × time
//! grain the pass segments the sequence of period-ordered
//! observations, so what changes is the joint row distribution
//! along the axis. Rows missing the axis value or any measure value
//! are excluded (complete cases), as in the breaks pass.
//!
//! Bounds, all stated in coverage when they bind: at most
//! [`ChangeforestConfig::row_cap`] rows are segmented (a stride
//! sample in axis order past the cap); the segmentation evaluates
//! at most [`ChangeforestConfig::node_budget`] segments, because
//! on a series whose distribution changes at a fine grain the
//! recursion otherwise subdivides until the minimum segment length
//! stops it; and at most [`ChangeforestConfig::max_splits`] splits
//! are stated, the largest by classifier gain, because a statement
//! budget is what makes the findings readable — the coverage entry
//! always gives the total number of significant splits found.
//!
//! Determinism: the segmentation seed is fixed
//! ([`SEGMENTATION_SEED`]); the crate draws its permutation streams
//! from that seed, and the forest's per-tree seeds are drawn from a
//! fixed seed inside the crate before any parallel work, so two
//! runs over the same rows produce the same findings. A regression
//! test pins this.

use std::collections::BTreeMap;

use onceover_core::{dateutil, TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::{json, Value};

/// Seed for the segmentation's permutation streams, fixed so that
/// findings are reproducible. The value itself is arbitrary.
pub const SEGMENTATION_SEED: u64 = 260;

const MICROS_PER_DAY: i64 = 86_400_000_000;

/// Configuration for the changeforest pass.
#[derive(Clone, Debug)]
pub struct ChangeforestConfig {
    /// Maximum rows segmented. Past the cap, a deterministic
    /// stride sample in axis order is segmented instead and the
    /// coverage entry states it.
    pub row_cap: usize,
    /// Maximum measure columns segmented jointly. Past the cap the
    /// first measures in table order are kept and the coverage
    /// entry names the rest.
    pub max_measures: usize,
    /// Minimum rows in a segment on either side of a stated split.
    pub min_segment: usize,
    /// Permutation p-value bar: a split is stated only when its
    /// segment's permutation p-value is at most this.
    pub alpha: f64,
    /// Permutations per segment in the model-selection test. The
    /// smallest attainable p-value is 1 / (n_permutations + 1).
    pub n_permutations: usize,
    /// Maximum splits stated in findings. When the segmentation
    /// finds more significant splits, the largest by classifier
    /// gain are stated and the coverage entry gives the total.
    pub max_splits: usize,
    /// Maximum segments the binary segmentation evaluates. Each
    /// evaluation fits several forests, so the budget bounds the
    /// pass's cost on series that change at a fine grain; coverage
    /// states when it binds.
    pub node_budget: usize,
}

impl Default for ChangeforestConfig {
    fn default() -> Self {
        Self {
            row_cap: 10_000,
            max_measures: 32,
            min_segment: 30,
            alpha: 0.01,
            n_permutations: 199,
            max_splits: 10,
            node_budget: 512,
        }
    }
}

/// The pass's input: the frame model's time axis and the measure
/// columns to segment jointly, by table index.
pub struct ChangeforestInput {
    pub axis_name: String,
    /// Per-row axis ticks in microseconds (days × MICROS_PER_DAY
    /// for date axes), as produced by the frame model.
    pub ticks: Vec<Option<i64>>,
    pub measures: Vec<usize>,
    pub axis_is_datetime: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChangeforestStatus {
    NotApplicable(String),
    Ran(String),
    Capped(String),
}

pub struct ChangeforestOutcome {
    pub findings: Vec<Finding>,
    pub status: ChangeforestStatus,
}

fn display_tick(input: &ChangeforestInput, tick: i64) -> String {
    if input.axis_is_datetime {
        dateutil::iso_datetime(tick)
    } else {
        dateutil::iso_date(tick.div_euclid(MICROS_PER_DAY))
    }
}

/// One significant split of the segmentation tree.
struct Split {
    /// Segment the split was chosen on, as row positions in the
    /// segmented sequence: `[start, stop)`.
    start: usize,
    stop: usize,
    /// Row position of the first row after the split.
    at: usize,
    gain: f64,
    p_value: Option<f64>,
}

/// Grow a segmentation tree in the crate's own order, spending at
/// most `budget` segment evaluations. When the budget runs out on
/// a segment long enough to split further, `budget_hit` is set and
/// the rest of the tree is left ungrown.
fn grow_budgeted(
    segmentation: &mut changeforest::Segmentation,
    node: &mut changeforest::BinarySegmentationTree,
    min_segment: usize,
    budget: &mut usize,
    budget_hit: &mut bool,
) {
    if *budget == 0 {
        if node.stop - node.start >= 2 * min_segment {
            *budget_hit = true;
        }
        return;
    }
    *budget -= 1;
    if let Ok(optimizer_result) = segmentation.find_best_split(node.start, node.stop) {
        node.model_selection_result = segmentation.model_selection(&optimizer_result);
        if node.model_selection_result.is_significant {
            let split = optimizer_result.best_split;
            let mut left = Box::new(changeforest::BinarySegmentationTree {
                start: node.start,
                stop: split,
                n: node.n,
                model_selection_result: changeforest::ModelSelectionResult::default(),
                left: None,
                right: None,
                optimizer_result: None,
            });
            grow_budgeted(segmentation, &mut left, min_segment, budget, budget_hit);
            node.left = Some(left);
            let mut right = Box::new(changeforest::BinarySegmentationTree {
                start: split,
                stop: node.stop,
                n: node.n,
                model_selection_result: changeforest::ModelSelectionResult::default(),
                left: None,
                right: None,
                optimizer_result: None,
            });
            grow_budgeted(segmentation, &mut right, min_segment, budget, budget_hit);
            node.right = Some(right);
        }
        node.optimizer_result = Some(optimizer_result);
    }
}

fn collect_splits(node: &changeforest::BinarySegmentationResult, out: &mut Vec<Split>) {
    if let Some(left) = &node.left {
        collect_splits(left, out);
    }
    if node.model_selection_result.is_significant {
        if let Some(opt) = &node.optimizer_result {
            out.push(Split {
                start: node.start,
                stop: node.stop,
                at: opt.best_split,
                gain: opt.max_gain,
                p_value: node.model_selection_result.p_value,
            });
        }
    }
    if let Some(right) = &node.right {
        collect_splits(right, out);
    }
}

/// The f64 values of a column, or None when the column does not
/// extract to numbers.
fn numeric_values(col: &TypedCol) -> Option<Vec<Option<f64>>> {
    match col {
        TypedCol::I64 { vals } => Some(vals.iter().map(|v| v.map(|x| *x as f64)).collect()),
        TypedCol::U64 { vals } => Some(vals.iter().map(|v| v.map(|x| *x as f64)).collect()),
        TypedCol::F64 { vals } => Some(vals.iter().map(|v| v.copied()).collect()),
        _ => None,
    }
}

pub fn changeforest_pass(
    table: &TypedTable,
    input: &ChangeforestInput,
    cfg: &ChangeforestConfig,
) -> ChangeforestOutcome {
    let na = |detail: String| ChangeforestOutcome {
        findings: Vec::new(),
        status: ChangeforestStatus::NotApplicable(detail),
    };

    // Eligible measures: numeric, in table order.
    let mut measure_names: Vec<String> = Vec::new();
    let mut measure_values: Vec<Vec<Option<f64>>> = Vec::new();
    for &i in &input.measures {
        if let Some(vals) = numeric_values(&table.cols[i]) {
            measure_names.push(table.names[i].clone());
            measure_values.push(vals);
        }
    }
    if measure_names.is_empty() {
        return na("no numeric measure columns to segment along the time axis".into());
    }

    // Complete cases in axis order: rows with an axis value and a
    // value in every measure. The sort is stable, so rows sharing a
    // tick keep their table order.
    let mut rows: Vec<(i64, Vec<f64>)> = Vec::new();
    for r in 0..table.n {
        let Some(tick) = input.ticks.get(r).copied().flatten() else {
            continue;
        };
        let mut vals: Vec<f64> = Vec::with_capacity(measure_values.len());
        let mut complete = true;
        for col in &measure_values {
            match col.get(r).copied().flatten() {
                Some(v) if v.is_finite() => vals.push(v),
                _ => {
                    complete = false;
                    break;
                }
            }
        }
        if complete {
            rows.push((tick, vals));
        }
    }
    rows.sort_by_key(|(tick, _)| *tick);
    let n_complete = rows.len();
    if n_complete <= 2 * cfg.min_segment {
        return na(format!(
            "fewer than {} complete rows along the time axis ({}), so no segment of {} rows fits on both sides of a split",
            2 * cfg.min_segment + 1,
            n_complete,
            cfg.min_segment
        ));
    }

    let mut capped = false;
    let mut sampled_rows = false;
    let mut cap_notes: Vec<String> = Vec::new();

    // Row cap: a stride sample in axis order (the same sampler the
    // other capped passes use), so split positions stay
    // interpretable as axis periods.
    if n_complete > cfg.row_cap {
        let keep = crate::stride_rows(n_complete, cfg.row_cap);
        let mut idx: Vec<usize> = keep.iter().map(|&i| i as usize).collect();
        idx.sort_unstable();
        rows = idx.iter().map(|&i| rows[i].clone()).collect();
        capped = true;
        sampled_rows = true;
        cap_notes.push(format!(
            "row cap {} binds: segmented a stride sample of {} of {} complete rows in axis order",
            cfg.row_cap,
            rows.len(),
            n_complete
        ));
    }

    // Measure cap and constant measures.
    if measure_names.len() > cfg.max_measures {
        let dropped: Vec<String> = measure_names[cfg.max_measures..].to_vec();
        measure_names.truncate(cfg.max_measures);
        for row in rows.iter_mut() {
            row.1.truncate(cfg.max_measures);
        }
        capped = true;
        cap_notes.push(format!(
            "measure cap {} binds: dropped {}",
            cfg.max_measures,
            dropped.join(", ")
        ));
    }
    let mut constant: Vec<String> = Vec::new();
    let mut varying: Vec<usize> = Vec::new();
    for (j, name) in measure_names.iter().enumerate() {
        let first = rows[0].1[j];
        if rows.iter().all(|r| r.1[j] == first) {
            constant.push(name.clone());
        } else {
            varying.push(j);
        }
    }
    if varying.is_empty() {
        return na("every measure column is constant along the time axis".into());
    }
    if !constant.is_empty() {
        let keep = varying.clone();
        measure_names = keep.iter().map(|&j| measure_names[j].clone()).collect();
        for row in rows.iter_mut() {
            row.1 = keep.iter().map(|&j| row.1[j]).collect();
        }
        cap_notes.push(format!(
            "constant measure(s) excluded from the segmentation: {}",
            constant.join(", ")
        ));
    }

    let n = rows.len();
    let k = measure_names.len();
    let x = ndarray::Array2::from_shape_fn((n, k), |(r, c)| rows[r].1[c]);

    let control = changeforest::Control::default()
        .with_minimal_relative_segment_length(cfg.min_segment as f64 / n as f64)
        .with_model_selection_alpha(cfg.alpha)
        .with_model_selection_n_permutations(cfg.n_permutations)
        .with_seed(SEGMENTATION_SEED);

    // Grow the segmentation tree in the crate's own order (node,
    // then left subtree, then right), bounded by the node budget:
    // the crate's wrapper grows the tree to completion, which on a
    // series that changes at a fine grain is hundreds of segments.
    let view = x.view();
    let classifier = changeforest::classifier::RandomForest::new(&view, &control);
    let gain = changeforest::gain::ClassifierGain { classifier };
    let optimizer = changeforest::optimizer::TwoStepSearch { gain };
    let mut segmentation =
        changeforest::Segmentation::new(changeforest::SegmentationType::BS, &optimizer);
    let mut tree = changeforest::BinarySegmentationTree::new(&view);
    let mut budget = cfg.node_budget;
    let mut budget_hit = false;
    grow_budgeted(
        &mut segmentation,
        &mut tree,
        cfg.min_segment,
        &mut budget,
        &mut budget_hit,
    );
    let result = changeforest::BinarySegmentationResult::from_tree(tree);
    if budget_hit {
        capped = true;
        cap_notes.push(format!(
            "node budget {} binds: the segmentation evaluated {} segments and stopped; later segments were not examined",
            cfg.node_budget, cfg.node_budget
        ));
    }

    let mut splits: Vec<Split> = Vec::new();
    collect_splits(&result, &mut splits);
    let significant_total = splits.len();
    if significant_total > cfg.max_splits {
        // State the largest changes: rank by classifier gain
        // (ties by position, so the choice is deterministic),
        // keep the top max_splits, present them in axis order.
        splits.sort_by(|a, b| b.gain.total_cmp(&a.gain).then(a.at.cmp(&b.at)));
        splits.truncate(cfg.max_splits);
        capped = true;
        cap_notes.push(format!(
            "{} significant splits found; the {} largest by classifier gain are stated",
            significant_total, cfg.max_splits
        ));
    }
    splits.sort_by_key(|s| s.at);

    // The final segmentation, for the bespoke evidence payload:
    // one entry per segment between consecutive stated splits.
    let mut bounds: Vec<usize> = vec![0];
    bounds.extend(splits.iter().map(|s| s.at));
    bounds.push(n);
    let segments_json: Vec<Value> = bounds
        .windows(2)
        .enumerate()
        .map(|(j, w)| {
            let (start, end) = (w[0], w[1]);
            json!({
                "segment": j + 1,
                "from": display_tick(input, rows[start].0),
                "to": display_tick(input, rows[end - 1].0),
                "rows": end - start,
            })
        })
        .collect();

    let measures_label = if k == 1 {
        format!("the distribution of {}", measure_names[0])
    } else if k <= 4 {
        format!("the joint distribution of {}", measure_names.join(", "))
    } else {
        format!(
            "the joint distribution of {} and {} more measures",
            measure_names[..4].join(", "),
            k - 4
        )
    };

    let mut findings = Vec::with_capacity(splits.len());
    for split in &splits {
        let display = display_tick(input, rows[split.at].0);
        let p_text = match split.p_value {
            Some(p) => format!("permutation p = {}", crate::fmt_float(p)),
            None => format!("permutation p <= {}", crate::fmt_float(cfg.alpha)),
        };
        let claim = format!(
            "{} changes along {} at {}: a random forest distinguishes the rows before the split from the rows after it (classifier gain {}; {})",
            measures_label,
            input.axis_name,
            display,
            crate::fmt_float(split.gain),
            p_text
        );
        let score = match split.p_value {
            Some(p) if cfg.alpha > 0.0 => {
                (0.65 + 0.25 * (1.0 - (p / cfg.alpha).min(1.0))).clamp(0.65, 0.9)
            }
            _ => 0.65,
        };
        let mut metrics: BTreeMap<String, Value> = BTreeMap::from([
            ("breakpoint".into(), json!(display)),
            ("breakpoint_raw".into(), json!(rows[split.at].0)),
            ("split_row".into(), json!(split.at)),
            (
                "segment_start".into(),
                json!(display_tick(input, rows[split.start].0)),
            ),
            (
                "segment_end".into(),
                json!(display_tick(input, rows[split.stop - 1].0)),
            ),
            ("gain".into(), json!(split.gain)),
            ("p_value".into(), json!(split.p_value)),
            ("significant_splits".into(), json!(significant_total)),
            ("alpha".into(), json!(cfg.alpha)),
            ("seed".into(), json!(SEGMENTATION_SEED)),
            ("n".into(), json!(n)),
            ("population_rows".into(), json!(n_complete)),
            ("n_measures".into(), json!(k)),
            ("measures".into(), json!(measure_names)),
            (
                "validation".into(),
                json!(format!(
                    "classifier binary segmentation (changeforest, random forest, seed {}); a split is stated when the permutation p-value of its segment's maximum gain ({} permutations) is at most {}; the gain and p-value are model outputs and are not recounted by report.verify",
                    SEGMENTATION_SEED, cfg.n_permutations, cfg.alpha
                )),
            ),
        ]);
        if sampled_rows {
            metrics.insert("sampled".into(), json!(true));
        }
        metrics.insert("segments".into(), json!(segments_json.clone()));
        findings.push(Finding::new(
            Pattern::DistributionChange,
            BTreeMap::from([
                ("columns".into(), json!(measure_names)),
                ("time_column".into(), json!(input.axis_name)),
            ]),
            claim,
            EvidenceBasis::Estimated,
            score,
            metrics,
            Some(
                "check for changes in collection, processing, or business behavior around this date"
                    .into(),
            ),
            Method::new(
                "changeforest",
                "classifier_binary_segmentation",
                "binary segmentation with random-forest classifier gains and permutation model selection (changeforest crate)",
            ),
        ));
    }

    let mut detail = format!(
        "classifier binary segmentation over {} measure(s) on {} complete rows along {}; {} significant split(s) at permutation p <= {} ({} permutations, seed {})",
        k,
        n,
        input.axis_name,
        significant_total,
        crate::fmt_float(cfg.alpha),
        cfg.n_permutations,
        SEGMENTATION_SEED
    );
    for note in &cap_notes {
        detail.push_str("; ");
        detail.push_str(note);
    }
    let status = if capped {
        ChangeforestStatus::Capped(detail)
    } else {
        ChangeforestStatus::Ran(detail)
    };
    ChangeforestOutcome { findings, status }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::NumericValues;

    /// SplitMix64 + Box-Muller: a small deterministic normal
    /// generator for the planted-change fixtures (no extra
    /// dependencies for test data).
    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn unit(&mut self) -> f64 {
            (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
        }
        fn normal(&mut self) -> f64 {
            let u1 = self.unit().max(1e-12);
            let u2 = self.unit();
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        }
    }

    /// A table with a daily date axis (column 0, days since epoch)
    /// and the given measure columns.
    fn table_with(
        measures: Vec<Vec<f64>>,
        start_day: i64,
    ) -> (TypedTable<'static>, ChangeforestInput) {
        let n = measures[0].len();
        let days: Vec<Option<i32>> = (0..n)
            .map(|i| Some((start_day + i as i64) as i32))
            .collect();
        let mut cols: Vec<TypedCol> = vec![TypedCol::Date { vals: days }];
        let mut names = vec!["date".to_string()];
        let mut dtypes = vec!["date".to_string()];
        for (j, m) in measures.iter().enumerate() {
            cols.push(TypedCol::F64 {
                vals: NumericValues::from(m.iter().map(|v| Some(*v)).collect::<Vec<_>>()),
            });
            names.push(format!("m{j}"));
            dtypes.push("float64".to_string());
        }
        let ticks: Vec<Option<i64>> = (0..n)
            .map(|i| Some((start_day + i as i64) * MICROS_PER_DAY))
            .collect();
        let table = TypedTable {
            n,
            names,
            dtypes,
            cols,
        };
        let input = ChangeforestInput {
            axis_name: "date".to_string(),
            ticks,
            measures: (0..measures.len()).map(|j| j + 1).collect(),
            axis_is_datetime: false,
        };
        (table, input)
    }

    /// The paper's change-in-covariance setup: 600 rows, 5
    /// coordinates, correlation 0 on rows [0, 200), 0.7 on
    /// [200, 400), 0 on [400, 600); every coordinate keeps mean 0
    /// and variance 1 throughout.
    fn covariance_fixture() -> (TypedTable<'static>, ChangeforestInput) {
        let mut rng = Rng(0x2600_2026);
        let rho = |r: usize| -> f64 {
            if (200..400).contains(&r) {
                0.7
            } else {
                0.0
            }
        };
        let mut measures: Vec<Vec<f64>> = vec![Vec::new(); 5];
        for r in 0..600 {
            let common = rng.normal();
            for m in measures.iter_mut() {
                let own = rng.normal();
                m.push(rho(r).sqrt() * common + (1.0 - rho(r)).sqrt() * own);
            }
        }
        table_with(measures, 19_000)
    }

    #[test]
    fn covariance_shift_is_stated_at_both_planted_splits() {
        let (table, input) = covariance_fixture();
        let out = changeforest_pass(&table, &input, &ChangeforestConfig::default());
        let ats: Vec<usize> = out
            .findings
            .iter()
            .map(|f| f.metrics()["split_row"].as_u64().unwrap() as usize)
            .collect();
        // The planted splits must both be stated, within tolerance.
        // The count is not asserted exactly: at a per-segment
        // permutation bar of 0.01 a further split can be stated on
        // a remainder segment (on this fixture the second split is
        // located two rows early, and the contaminated remainder
        // states one more split; the same segment in isolation
        // states nothing, and a re-seeded fixture states exactly
        // the two planted splits — see the benchmark record).
        assert!(
            ats.iter().any(|a| (185..=215).contains(a)),
            "no stated split near planted 200: {ats:?}"
        );
        assert!(
            ats.iter().any(|a| (385..=415).contains(a)),
            "no stated split near planted 400: {ats:?}"
        );
        for f in &out.findings {
            assert_eq!(f.finding_type.as_deref(), Some("distribution_change"));
            assert_eq!(f.category, onceover_report::Category::Changes);
        }
    }

    #[test]
    fn shape_change_with_level_and_variance_held_is_stated() {
        // One series: normal noise, then the same noise folded to
        // its absolute value and re-centred and re-scaled to the
        // same mean and variance — the level and the variance do
        // not move; the shape does. The series runs to 1,200 rows:
        // at 300 rows a side this contrast sits at the edge of the
        // method's power at the 0.01 bar; at 600 a side it is
        // stated on nearly every probed draw (see the benchmark
        // record for the probe counts).
        let mut rng = Rng(0x2600_2030);
        let raw: Vec<f64> = (0..1200).map(|_| rng.normal()).collect();
        let folded: Vec<f64> = raw.iter().map(|v| v.abs()).collect();
        let mean = folded.iter().sum::<f64>() / folded.len() as f64;
        let var = folded.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / folded.len() as f64;
        let mut series: Vec<f64> = raw[..600].to_vec();
        series.extend(folded[600..].iter().map(|v| (v - mean) / var.sqrt()));
        let (table, input) = table_with(vec![series], 19_000);
        let out = changeforest_pass(&table, &input, &ChangeforestConfig::default());
        assert!(
            out.findings.iter().any(|f| {
                let at = f.metrics()["split_row"].as_u64().unwrap() as usize;
                (570..=630).contains(&at)
            }),
            "no stated split near planted 600: {:?}",
            out.findings
                .iter()
                .map(|f| f.metrics()["split_row"].as_u64().unwrap())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_change_series_states_nothing() {
        let mut rng = Rng(0x2600_2028);
        let measures: Vec<Vec<f64>> = (0..3)
            .map(|_| (0..600).map(|_| rng.normal()).collect())
            .collect();
        let (table, input) = table_with(measures, 19_000);
        let out = changeforest_pass(&table, &input, &ChangeforestConfig::default());
        assert!(
            out.findings.is_empty(),
            "no-change control stated {} finding(s)",
            out.findings.len()
        );
    }

    #[test]
    fn two_runs_produce_identical_findings() {
        let (table, input) = covariance_fixture();
        let cfg = ChangeforestConfig::default();
        let a = changeforest_pass(&table, &input, &cfg);
        let b = changeforest_pass(&table, &input, &cfg);
        let ja: Vec<String> = a
            .findings
            .iter()
            .map(|f| serde_json::to_string(f).unwrap())
            .collect();
        let jb: Vec<String> = b
            .findings
            .iter()
            .map(|f| serde_json::to_string(f).unwrap())
            .collect();
        assert_eq!(ja, jb);
        assert_eq!(a.status, b.status);
    }

    #[test]
    fn too_few_rows_is_not_applicable() {
        let mut rng = Rng(0x2600_2029);
        let series: Vec<f64> = (0..40).map(|_| rng.normal()).collect();
        let (table, input) = table_with(vec![series], 19_000);
        let out = changeforest_pass(&table, &input, &ChangeforestConfig::default());
        assert!(matches!(out.status, ChangeforestStatus::NotApplicable(_)));
        assert!(out.findings.is_empty());
    }
}
