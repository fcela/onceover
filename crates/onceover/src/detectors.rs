//! Detector registration is separate from the public finding taxonomy.
use crate::ProfileConfig;
use onceover_core::TypedTable;
use onceover_mines::EncodedMatrix;
use onceover_report::{Category, ColumnCard, Finding};
use std::collections::{BTreeMap, BTreeSet};

/// Whether a detector is in the set an unadorned `profile()` call
/// runs. Membership answers one question: is the detector in the
/// default set, or does it run only when requested?
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Membership {
    /// In the default set: runs in an unadorned `profile()` call
    /// (subject to its trigger).
    DefaultSet,
    /// Registered and runnable, but only when explicitly selected
    /// via `detectors=[...]` (or, for target-triggered detectors,
    /// when a target is designated). On-request detectors record a
    /// skipped coverage entry in default runs, so their absence is
    /// never silent.
    OnRequest,
}

/// What engages a detector within a run. Trigger answers a second
/// question, independent of membership: does the detector run on
/// every call, only when the caller declares the structure it
/// checks, or only when a target is designated?
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// Engages on every run that includes the detector.
    Always,
    /// Engages only when the caller declares the structure it
    /// checks (profile hints); without hints it records a skipped
    /// coverage entry.
    Hints,
    /// Engages only when a target is designated (or when explicitly
    /// selected together with a target).
    Target,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct DetectorDescriptor {
    pub id: String,
    pub categories: Vec<Category>,
    pub requires: Vec<String>,
    pub membership: Membership,
    pub trigger: Trigger,
    /// What the detector looks for, in one line. The registry is
    /// the single source for this text: the capability manifest
    /// and the book's registry table render it from here.
    pub purpose: String,
}
impl DetectorDescriptor {
    /// Extension detectors default to the default set with an
    /// always trigger; the built-in registry overrides membership
    /// and trigger per detector.
    pub fn new(id: &str, categories: Vec<Category>, requires: Vec<String>) -> Self {
        Self {
            id: id.into(),
            categories,
            requires,
            membership: Membership::DefaultSet,
            trigger: Trigger::Always,
            purpose: String::new(),
        }
    }

    pub fn with_membership(mut self, membership: Membership) -> Self {
        self.membership = membership;
        self
    }

    pub fn with_trigger(mut self, trigger: Trigger) -> Self {
        self.trigger = trigger;
        self
    }

    pub fn with_purpose(mut self, purpose: &str) -> Self {
        self.purpose = purpose.into();
        self
    }
}

pub fn builtin_detectors() -> Vec<DetectorDescriptor> {
    use Category::*;
    use Membership::*;
    use Trigger::*;
    let make = |id: &str,
                categories: Vec<Category>,
                requires: &[&str],
                membership: Membership,
                trigger: Trigger,
                purpose: &str| {
        DetectorDescriptor::new(
            id,
            categories,
            requires.iter().map(|s| s.to_string()).collect(),
        )
        .with_membership(membership)
        .with_trigger(trigger)
        .with_purpose(purpose)
    };
    vec![
        make(
            "column_profile",
            vec![Missingness, Concentration, ColumnRoles, Formats],
            &[],
            DefaultSet, Always,
        "Column summaries, role suggestions, missingness, concentration, string formats"),
        make("row_duplicates", vec![Duplicates], &[], DefaultSet, Always, "Repeated rows under the stated comparison and missing-value policy"),
        make("keys", vec![Uniqueness], &[], DefaultSet, Always, "Single-column, composite, and nearly unique keys"),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs (20 datasets x harness and default configs).
        // The detector is unchanged and runs when explicitly selected.
        make("pivot", vec![ColumnRoles], &[], OnRequest, Always, "Verified long layout and relationships in its wide form"),
        make("functional_dependencies", vec![Dependencies], &[], DefaultSet, Always, "Exact or approximate value mappings, including multi-column determinants"),
        make("order_constraints", vec![Dependencies], &[], DefaultSet, Always, "Pairwise order between two columns (a <= b), exact or with counted violations"),
        make("duplicate_encodings", vec![ColumnRoles], &[], DefaultSet, Always, "Alternative labels for a grouping and finer-to-coarser rollups"),
        make("sortedness", vec![Ordering], &[], DefaultSet, Always, "Order in the row sequence and late appended backfills"),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs at first measurement.
        make("granularity", vec![Formats], &[], OnRequest, Always, "Changes between fractional and integer-valued recording"),
        make("sentinels", vec![Missingness], &[], DefaultSet, Always, "Repeated, windowed candidate missing-value codes"),
        make("boundary_mass", vec![Formats], &[], DefaultSet, Always, "Mass piled at a numeric column's minimum or maximum, and interior mass points in zero-inflated columns"),
        make("missingness", vec![Missingness], &[], DefaultSet, Always, "Each column's missing rate, the grouping level, threshold, or time point it follows, and columns missing on the same rows"),
        make("content_anomalies", vec![Formats], &[], DefaultSet, Always, "What text columns hold (numbers, dates, recognized formats), label variants and likely misspellings, repeated extremes, stray signs, zero inflation, separated modes, and far-out dates"),
        make("series", vec![Gaps, Changes], &[], DefaultSet, Always, "Absent steps on the time axis; trend, cycle and persistence of each measure; grouping levels that appear, end or shift share over time"),
        make(
            "row_consistency",
            vec![Duplicates, Anomalies],
            &["numeric_dependence"],
            DefaultSet, Always,
        "Rows repeating another on every column but one, rows far from the line two tight columns follow, and rows with two columns' values swapped"),
        make("digit_fingerprints", vec![Formats], &[], DefaultSet, Always, "Heaping, terminal-digit preferences, and eligible Benford deviations"),
        // In the default set again since issue #446: on metro traffic
        // it states a 218-business-day hole in the series (2014-08 to
        // 2015-06) that no default pass reports.
        make("date_gaps", vec![Gaps, Anomalies], &[], DefaultSet, Always, "Inferred calendar gaps and interval irregularity"),
        // Hint-gated: a default-set member that engages only on
        // declared hints and otherwise records itself skipped.
        make("expectations", vec![Gaps, Ordering], &[], DefaultSet, Hints, "Declared sequences, calendars, ordering, and panel completeness"),
        // Per-period coverage on the frame's time axis: observed
        // periods whose row, entity, or principal-measure volume
        // falls far below the typical period's, stated with their
        // position.
        make("panel_coverage", vec![Gaps], &[], DefaultSet, Always, "Per-period row and entity volume on the time axis, against the typical period"),
        // Hint-gated like expectations: engages only when the
        // caller declares a group contrast in the hints.
        make("group_contrast", vec![Associations], &[], DefaultSet, Hints, "How one declared subset of the rows differs from the rest of the table"),
        // The automatic per-level scan (issue #275): every level
        // of every low-cardinality categorical against the pooled
        // rest on every numeric measure — location, scale, and
        // the constant-multiple form — with rank-sum tests under
        // one Benjamini-Hochberg family. The declared group
        // contrast's undeclared counterpart. Its scans are exact
        // quantile work in the breaks cost class, so it runs in
        // the default set like the mixed-pair screen whose
        // scale/shape blindness it covers.
        make("level_contrasts", vec![Associations], &[], DefaultSet, Always, "Each level of a categorical column against the pooled rest on each numeric measure: location, scale, and constant-multiple contrasts, plus the date axis's calendar parts"),
        make("predictability", vec![Dependencies], &[], DefaultSet, Always, "Predictability of eligible columns from the other columns"),
        // Opt-in since the 2026-11-16 per-detector audit (issue
        // #317 phase 3): across the gate tables none of its
        // findings matched a reference statement, and it is not an
        // integrity check. The record is
        // docs/benchmarks/2026-11-16-detector-audit.md.
        make("decision_tables", vec![Dependencies], &["predictability"], OnRequest, Always, "Compact predictive tables selected by cross-validation"),
        make("covering_rules", vec![ConditionalRules], &["predictability"], DefaultSet, Always, "Multi-condition rules for categorical targets"),
        make("regimes", vec![Dependencies, Changes], &["predictability"], DefaultSet, Always, "Local linear equations and dated reference-model residual shifts"),
        // Opt-in since the 2026-11-16 per-detector audit (issue
        // #317 phase 3): one finding across the gate tables and no
        // matched reference statement. The record is
        // docs/benchmarks/2026-11-16-detector-audit.md.
        make("interactions", vec![Dependencies], &["predictability"], OnRequest, Always, "Non-additive predictor pairs under a fitted target model"),
        make("outcome_account", vec![Concentration], &[], OnRequest, Target, "The designated outcome's own distribution: center, spread, and range for a numeric outcome; the share of each level for a categorical one, with the declared event's rate when an event is declared"),
        make("outcome_contrasts", vec![Dependencies], &[], OnRequest, Target, "Per-level outcome contrasts for a designated target: each categorical level's event rate (or median, for a numeric outcome) against the pooled outcome, with measured absences stated as nulls in the pointed mode"),
        make("target_shapes", vec![Dependencies], &[], OnRequest, Target, "Line, curve, and step comparisons for a designated numeric target"),
        make("leakage", vec![Dependencies], &[], OnRequest, Target, "A column that determines the designated target exactly, or relates to it far more strongly than any other column does — the signature of a column derived from the outcome or recorded after it"),
        make("model_failures", vec![Dependencies], &[], OnRequest, Target, "Slices where a cross-fitted model of the designated target fails: out-of-fold errors one-sided or materially larger than the model's overall error"),
        make("breaks", vec![Changes, Gaps], &[], DefaultSet, Always, "Sustained level, trend, variance changes, and transient excursions"),
        make("ordered_partitions", vec![Changes], &[], DefaultSet, Always, "Model-based recursive partitions of a numeric response along a time axis or a categorical dimension"),
        make("outliers", vec![Anomalies], &[], DefaultSet, Always, "Isolated numeric values and context-breaking categorical values"),
        make("discontinuities", vec![Changes], &[], DefaultSet, Always, "Axis-aligned and diagonal threshold jumps"),
        // Opt-in since the 2026-11-02 battery: zero findings in 40
        // measured runs; it engages on a time hint agent callers
        // rarely pass, and issue #95 tracks a panic on long-form
        // data with a time hint.
        make("drift", vec![Changes], &["predictability"], OnRequest, Always, "Reference-period model-routing changes along a declared time axis"),
        // Opt-in: a model-based segmentation over the joint
        // measure distribution; stated splits carry permutation
        // p-values, and the pass is heavier than the breaks scan.
        make("distribution_change", vec![Changes], &[], OnRequest, Always, "Distribution changes in the measures jointly along the time axis, from classifier-based changepoint detection"),
        // Opt-in: a searched conjunction-conditional distribution
        // statement (issue #272). The search evaluates every
        // supported conjunction of conditions against every
        // target on the shared sample — the heaviest single pass
        // in the registry — and its statements are the only ones
        // in the report whose selection is itself the risk the
        // evidence has to answer for, so the control stack
        // (split-sample confirmation, BH over the confirmation
        // family, candidate count in coverage) is part of the
        // pass. Same cost and inference class as row_clusters and
        // distribution_change: explicit selection.
        make("distribution_slices", vec![Associations], &[], OnRequest, Always, "Searched regions with shifted numeric or categorical marginal distributions, confirmed on held-out rows"),
        make("slice_shifts", vec![Associations], &[], OnRequest, Always, "Searched conjunctions of conditions under which a target column's distribution shifts, confirmed on held-out rows"),
        // The ordered-axis battery (#273, #277): shape statements
        // along any ordered axis and located breaks in a
        // relationship or inside a slice, deduped against the
        // breaks/changeforest/partition statements. Its scans are
        // the breaks scans' cost class (prefix-sum scans plus
        // shuffle calibration on aggregated series), so it runs
        // in the default set like breaks and ordered_partitions.
        make("ordered_axis", vec![Changes, Associations], &[], DefaultSet, Always, "Shape of a response along an ordered axis (monotonicity, reversal blocks, saturation) and located level, trend, slope, regime, and spike events along it, including inside one categorical level"),
        make(
            "association_rules",
            vec![ConditionalRules, Associations],
            &[],
            DefaultSet, Always,
        "Pairwise nominal rules with train and diagnostic evidence"),
        // Opt-in since the 2026-11-16 per-detector audit (issue
        // #317 phase 3): across the gate tables none of its
        // findings matched a reference statement, and it is not an
        // integrity check. The record is
        // docs/benchmarks/2026-11-16-detector-audit.md.
        make("hotspots", vec![Concentration], &[], DefaultSet, Always, "Combinations concentrated beyond their marginal frequencies, in rows or in a declared weight"),
        make("numeric_dependence", vec![Associations], &[], DefaultSet, Always, "Pearson, Spearman, and both directions of Chatterjee's xi"),
        make("partition_scagnostics", vec![Associations], &[], OnRequest, Always, "Scatterplot structure that strengthens within an observed partition, confirmed on held-out rows"),
        make(
            "identities",
            vec![Dependencies],
            &["numeric_dependence"],
            DefaultSet, Always,
        "Affine and small multi-term arithmetic relationships"),
        make(
            "period_associations",
            vec![Associations],
            &["numeric_dependence"],
            // Opt-in since the 2026-11-02 battery: zero findings in
            // 40 measured runs across two batteries.
            OnRequest, Always,
        "Changes in pairwise correlation across calendar or year periods"),
        // Opt-in: experimental until its criterion review closes.
        make(
            "row_clusters",
            vec![Associations, Changes, Concentration, Gaps],
            &[],
            OnRequest, Always,
        "Experimental shared row partitions, cluster differences, and density findings"),
        make("entity_fingerprints", vec![Associations, Anomalies, Concentration, Changes], &[], OnRequest, Always, "Entity groups, continuums, misfits, specialists, and drift by dimension or pair, using row counts or declared weights"),
    ]
}

pub fn active_detectors(selected: Option<&BTreeSet<String>>) -> BTreeSet<String> {
    let specs = builtin_detectors();
    let mut active = selected.cloned().unwrap_or_else(|| {
        // The default set: detectors whose membership is
        // DefaultSet, including the two hint-triggered members
        // (expectations, group_contrast), which engage only when
        // hints declare their structure. On-request detectors
        // (row_clusters, date_gaps, pivot, period_associations,
        // drift, granularity, distribution_change, distribution_slices, slice_shifts,
        // decision_tables, interactions)
        // run only when explicitly requested via detectors=[...];
        // the target-triggered detectors (outcome_account,
        // outcome_contrasts, target_shapes, leakage,
        // model_failures) run only when a target is designated
        // (the pipeline adds them then) or when explicitly
        // requested together with a target.
        specs
            .iter()
            .filter(|d| d.membership == Membership::DefaultSet)
            .map(|d| d.id.clone())
            .collect()
    });
    loop {
        let previous = active.len();
        for d in &specs {
            if active.contains(&d.id) {
                active.extend(d.requires.iter().cloned());
            }
        }
        if active.len() == previous {
            return active;
        }
    }
}

pub struct DetectorContext<'a, 'data> {
    pub table: &'a TypedTable<'data>,
    pub columns: &'a BTreeMap<String, ColumnCard>,
    pub encoded: Option<&'a EncodedMatrix>,
    pub config: &'a ProfileConfig,
}

/// Extensions return statements and evidence, not detector-named public kinds.
pub trait Detector: Send + Sync {
    fn descriptor(&self) -> DetectorDescriptor;
    fn discover(&self, context: &DetectorContext<'_, '_>) -> Result<Vec<Finding>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_set_is_the_31_measured_detectors() {
        let active = active_detectors(None);
        // 48 registered: 31 in the default set (29 always-triggered
        // plus the hint-triggered expectations and group_contrast),
        // 17 on request (11 always-triggered, 5 target-triggered, and
        // entity_fingerprints, hint-triggered).
        assert_eq!(builtin_detectors().len(), 48);
        assert_eq!(active.len(), 31);
        for id in [
            "row_clusters",
            "pivot",
            "period_associations",
            "drift",
            "granularity",
            "distribution_change",
            "slice_shifts",
            "distribution_slices",
            "decision_tables",
            "interactions",
            "outcome_account",
            "outcome_contrasts",
            "target_shapes",
            "leakage",
            "model_failures",
            "partition_scagnostics",
        ] {
            assert!(!active.contains(id), "{id} must not be in the default set");
        }
        assert!(active.contains("expectations"));
        assert!(active.contains("group_contrast"));
        assert!(active.contains("discontinuities"));
        assert!(active.contains("hotspots"));
    }

    #[test]
    fn explicit_selection_still_reaches_opt_in_detectors() {
        let selected: BTreeSet<String> = ["drift".to_string()].into_iter().collect();
        let active = active_detectors(Some(&selected));
        // drift plus its predictability resource.
        assert!(active.contains("drift"));
        assert!(active.contains("predictability"));
    }

    #[test]
    fn membership_and_trigger_partition_the_registry() {
        let mut membership = BTreeMap::new();
        let mut trigger = BTreeMap::new();
        for d in builtin_detectors() {
            *membership.entry(format!("{:?}", d.membership)).or_insert(0) += 1;
            *trigger.entry(format!("{:?}", d.trigger)).or_insert(0) += 1;
        }
        assert_eq!(membership.get("DefaultSet"), Some(&31));
        assert_eq!(membership.get("OnRequest"), Some(&17));
        assert_eq!(trigger.get("Always"), Some(&41));
        assert_eq!(trigger.get("Hints"), Some(&2));
        assert_eq!(trigger.get("Target"), Some(&5));
    }

    #[test]
    fn every_builtin_detector_declares_a_purpose() {
        for d in builtin_detectors() {
            assert!(
                !d.purpose.trim().is_empty(),
                "detector {} declares no purpose",
                d.id
            );
        }
    }

    #[test]
    fn selection_policy_detector_caps_are_registered() {
        // Issue #343: cap keys are free strings in the report crate, so a
        // detector rename can silently uncap a pass. Check them against the
        // registry here, where both halves are visible.
        let registered: BTreeSet<String> =
            builtin_detectors().iter().map(|d| d.id.clone()).collect();
        let policy = onceover_report::SelectionPolicy::default();
        assert!(!policy.max_findings_per_detector.is_empty());
        assert!(!policy.pressure_detector_caps.is_empty());
        for key in policy.max_findings_per_detector.keys() {
            assert!(
                registered.contains(key),
                "standing detector cap key {key} is not registered"
            );
        }
        for key in policy.pressure_detector_caps.keys() {
            assert!(
                registered.contains(key),
                "pressure detector cap key {key} is not registered"
            );
        }
    }

    #[test]
    fn registry_and_pipeline_dispatch_agree() {
        let sources = [
            include_str!("pipeline/mod.rs"),
            include_str!("pipeline/setup.rs"),
            include_str!("pipeline/structural.rs"),
            include_str!("pipeline/temporal.rs"),
            include_str!("pipeline/models.rs"),
            include_str!("pipeline/target.rs"),
            include_str!("pipeline/finalize.rs"),
        ];
        let mut dispatched: BTreeSet<String> = BTreeSet::new();
        for src in sources {
            let mut rest = src;
            while let Some(i) = rest.find("enabled(\"") {
                let after = &rest[i + "enabled(\"".len()..];
                let end = after.find('"').expect("dispatch literal terminates");
                dispatched.insert(after[..end].to_string());
                rest = &after[end..];
            }
        }
        let registered: BTreeSet<String> =
            builtin_detectors().iter().map(|d| d.id.clone()).collect();
        assert_eq!(
            dispatched, registered,
            "every registry entry must have a dispatch site in pipeline/ and every dispatch site must name a registry entry"
        );
    }

    #[test]
    fn pipeline_statement_wording_keys_are_registered() {
        let src = include_str!("pipeline/finalize.rs");
        let region = src
            .split("missing_values=match")
            .nth(1)
            .expect("wording match present")
            .split("let candidate_count")
            .next()
            .expect("region bounded");
        let registered: BTreeSet<String> =
            builtin_detectors().iter().map(|d| d.id.clone()).collect();
        let mut keys: BTreeSet<String> = BTreeSet::new();
        let mut buf = String::new();
        for line in region.lines() {
            buf.push_str(line);
            if buf.contains("=>") {
                let head = buf.split("=>").next().unwrap_or("");
                let mut rest = head;
                while let Some(i) = rest.find('"') {
                    let after = &rest[i + 1..];
                    match after.find('"') {
                        Some(end) => {
                            keys.insert(after[..end].to_string());
                            rest = &after[end + 1..];
                        }
                        None => break,
                    }
                }
                buf.clear();
            }
        }
        assert!(!keys.is_empty(), "wording/sampling keys extracted");
        for key in &keys {
            assert!(
                registered.contains(key),
                "pipeline wording/sampling key {key} is not a registered detector"
            );
        }
    }
}
