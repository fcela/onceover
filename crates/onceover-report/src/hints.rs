//! User-declared hints about a table's intended structure.
//!
//! Inference has limits: a monthly cadence cannot be inferred in days,
//! a panel's completeness cannot be tested without knowing the
//! dimensions and the axis, and a column that should be ordered but is
//! not is silent. Hints supply that knowledge. They are configuration,
//! not findings: the expectations pass checks them exactly and tags
//! its evidence `reference_source: "declared"`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A declared cadence for gap checks: a fixed step in days (for date
/// columns, the spacing of expected periods; for integer columns, the
/// sequence step), or a named calendar cadence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Cadence {
    Days(i64),
    Named(String),
}

impl Cadence {
    /// The named calendar cadences, when this is one.
    pub fn named(&self) -> Option<&str> {
        match self {
            Cadence::Named(s) => Some(s.as_str()),
            Cadence::Days(_) => None,
        }
    }
    pub fn days(&self) -> Option<i64> {
        match self {
            Cadence::Days(d) => Some(*d),
            Cadence::Named(_) => None,
        }
    }
    pub fn is_valid_named(s: &str) -> bool {
        matches!(s, "daily" | "business_day" | "weekly" | "monthly")
    }
}

/// Declared structure for one table. Every field is optional; unknown
/// columns and unusable types are reported in a coverage entry (the
/// expectations pass's, or `column_exclusions` for the exclusion
/// list, which is applied before any pass runs) rather than raising
/// an error.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProfileHints {
    /// The time / ordering axis column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// Columns the rows are claimed to be ordered by, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order_by: Vec<String>,
    /// Columns expected to form a complete sequence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_gaps: Vec<String>,
    /// Per-column cadence for gap checks.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cadence: BTreeMap<String, Cadence>,
    /// Declared dimension columns (panel structure).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<String>,
    /// Declared measure columns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub measures: Vec<String>,
    /// Declared semantic roles, per column: one of the frame model's
    /// role vocabulary (identifier, index, timestamp, timestamp_part,
    /// dimension, measure, text). An entry overrides the inferred
    /// role for that column; entries naming an unknown column or a
    /// role outside the vocabulary are ignored.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, String>,
    /// Declared hierarchies: each entry is one hierarchy's level
    /// columns, ordered finest to coarsest (each level determines
    /// the next). The expectations pass verifies every adjacent
    /// mapping exactly. A hierarchy that holds yields one structural
    /// statement, and the inferred restatements of the same chain
    /// group under it; a hierarchy that does not hold yields one
    /// deviation statement naming every broken mapping. Entries
    /// naming an unknown column, repeating a column, or holding
    /// fewer than two levels are reported in the expectations
    /// coverage entry rather than raising an error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hierarchies: Vec<Vec<String>>,
    /// Columns to exclude from profiling entirely, by user
    /// declaration. An excluded column is removed before any pass
    /// runs: it gets no column card and no finding can name it.
    /// The report's `column_exclusions` coverage entry states the
    /// exclusion and its source (user-declared); names that are not
    /// columns of the table are ignored and stated there too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_columns: Vec<String>,
    /// Display labels, per column: where a label is declared, the
    /// report's prose — finding claims and interpretations — names
    /// the column by its label instead of its raw name. Labels are
    /// presentation, never identity: subjects, specifications,
    /// evidence metrics, and column cards keep the raw name, and
    /// finding ids (derived from the canonical statement, not the
    /// claim text) are unchanged. Unusable entries are not applied
    /// and are stated in the report's `labels` coverage entry rather
    /// than raising an error: a label for a column the table does
    /// not have, an empty label, and a colliding label — one that
    /// names a different column of the table, or one label declared
    /// for two columns. A colliding column keeps its raw name in
    /// prose. See `resolve_labels`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

impl ProfileHints {
    pub fn is_empty(&self) -> bool {
        self.time.is_none()
            && self.order_by.is_empty()
            && self.no_gaps.is_empty()
            && self.cadence.is_empty()
            && self.dimensions.is_empty()
            && self.measures.is_empty()
            && self.roles.is_empty()
            && self.hierarchies.is_empty()
            && self.exclude_columns.is_empty()
            && self.labels.is_empty()
    }
}

/// The outcome of resolving declared labels against a table's
/// columns: the substitutions to apply to report prose, and one
/// note for every declared entry that was not applied. The caller
/// states the notes in the report's `labels` coverage entry, so a
/// rejected label is visible, never silent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedLabels {
    /// Column name -> display label, for the entries that apply.
    pub applied: BTreeMap<String, String>,
    /// Why each remaining declared entry was not applied.
    pub notes: Vec<String>,
}

/// Resolve `ProfileHints::labels` against the table's column names.
///
/// An entry applies when its column exists, its label is non-empty,
/// and it does not collide. A label collides when it names a
/// different column of the table (prose would then state one
/// column's facts under another column's name) or when the same
/// label is declared for two columns (prose could no longer tell
/// them apart). Colliding entries fall back to the raw name; the
/// collision is reported in the notes, following the hints
/// convention that unusable declarations are stated in coverage
/// rather than raised.
pub fn resolve_labels(declared: &BTreeMap<String, String>, columns: &[String]) -> ResolvedLabels {
    let column_set: std::collections::BTreeSet<&str> = columns.iter().map(String::as_str).collect();
    let mut notes: Vec<String> = Vec::new();
    let mut candidates: BTreeMap<String, String> = BTreeMap::new();
    for (name, label) in declared {
        if !column_set.contains(name.as_str()) {
            notes.push(format!(
                "label for '{name}' ignored: not a column of the table"
            ));
        } else if label.trim().is_empty() {
            notes.push(format!("label for '{name}' ignored: the label is empty"));
        } else {
            candidates.insert(name.clone(), label.clone());
        }
    }
    // Collisions, over the surviving entries: a label that is the
    // name of a different column, or a label shared by two entries.
    let mut label_counts: BTreeMap<String, usize> = BTreeMap::new();
    for label in candidates.values() {
        *label_counts.entry(label.clone()).or_default() += 1;
    }
    let mut applied: BTreeMap<String, String> = BTreeMap::new();
    for (name, label) in candidates {
        if label != name && column_set.contains(label.as_str()) {
            notes.push(format!(
                "label for '{name}' collides with the column '{label}' and was not applied"
            ));
        } else if label_counts[label.as_str()] > 1 {
            notes.push(format!(
                "label '{label}' is declared for more than one column and was not applied to '{name}' (colliding labels)"
            ));
        } else {
            applied.insert(name, label);
        }
    }
    ResolvedLabels { applied, notes }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replace one raw column name in prose text, but only where the
/// name stands as its own token: an occurrence extends a longer
/// identifier — and is left alone — when the character beside it
/// and the name's own edge character are both word characters, so
/// replacing `rev` never touches `revenue`.
fn replace_tokens(text: &str, name: &str, replacement: &str) -> String {
    if name.is_empty() {
        return text.to_string();
    }
    let name_first = name.chars().next().unwrap();
    let name_last = name.chars().next_back().unwrap();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(name) {
        let end = pos + name.len();
        let left_extends = rest[..pos]
            .chars()
            .next_back()
            .is_some_and(|c| is_word_char(c) && is_word_char(name_first));
        let right_extends = rest[end..]
            .chars()
            .next()
            .is_some_and(|c| is_word_char(c) && is_word_char(name_last));
        if left_extends || right_extends {
            // Not a standalone token: copy through the match's
            // first character and keep scanning from there.
            let adv = name_first.len_utf8();
            out.push_str(&rest[..pos + adv]);
            rest = &rest[pos + adv..];
        } else {
            out.push_str(&rest[..pos]);
            out.push_str(replacement);
            rest = &rest[end..];
        }
    }
    out.push_str(rest);
    out
}

/// Rewrite report prose (a claim or an interpretation) so labelled
/// columns appear under their labels. Longer names are substituted
/// first, through private-use placeholders, so a label's own text is
/// never rescanned: a label that happens to contain another
/// column's raw name keeps that text verbatim.
pub fn substitute_labels(text: &str, labels: &BTreeMap<String, String>) -> String {
    if labels.is_empty() || text.is_empty() {
        return text.to_string();
    }
    let mut entries: Vec<(&String, &String)> = labels.iter().collect();
    entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(b.0)));
    let mut out = text.to_string();
    for (i, (name, _)) in entries.iter().enumerate() {
        out = replace_tokens(&out, name, &format!("\u{E000}{i}\u{E001}"));
    }
    for (i, (_, label)) in entries.iter().enumerate() {
        let marker = format!("\u{E000}{i}\u{E001}");
        out = out.replace(marker.as_str(), label.as_str());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    fn columns(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn resolve_applies_plain_labels() {
        let r = resolve_labels(
            &declared(&[
                ("acc_cnt", "accepted count"),
                ("gen_cnt", "generated count"),
            ]),
            &columns(&["acc_cnt", "gen_cnt"]),
        );
        assert_eq!(r.applied.len(), 2);
        assert!(r.notes.is_empty(), "{:?}", r.notes);
    }

    #[test]
    fn resolve_ignores_unknown_columns_and_empty_labels() {
        let r = resolve_labels(
            &declared(&[("nope", "nothing"), ("acc_cnt", "  ")]),
            &columns(&["acc_cnt"]),
        );
        assert!(r.applied.is_empty());
        assert_eq!(r.notes.len(), 2, "{:?}", r.notes);
        assert!(r.notes.iter().any(|n| n.contains("nope")));
        assert!(r.notes.iter().any(|n| n.contains("acc_cnt")));
    }

    #[test]
    fn resolve_falls_back_on_collisions() {
        // A label that names another column.
        let r = resolve_labels(
            &declared(&[("acc_cnt", "gen_cnt")]),
            &columns(&["acc_cnt", "gen_cnt"]),
        );
        assert!(r.applied.is_empty());
        assert!(r.notes[0].contains("collides"), "{}", r.notes[0]);
        // One label declared for two columns: neither applies.
        let r = resolve_labels(
            &declared(&[("acc_cnt", "count"), ("gen_cnt", "count")]),
            &columns(&["acc_cnt", "gen_cnt"]),
        );
        assert!(r.applied.is_empty());
        assert_eq!(r.notes.len(), 2, "{:?}", r.notes);
        // A column's own name as its label is not a collision.
        let r = resolve_labels(
            &declared(&[("acc_cnt", "acc_cnt")]),
            &columns(&["acc_cnt", "gen_cnt"]),
        );
        assert_eq!(r.applied.len(), 1);
        assert!(r.notes.is_empty(), "{:?}", r.notes);
    }

    #[test]
    fn substitute_replaces_standalone_tokens_only() {
        let labels = declared(&[("rev", "revenue (labelled)")]);
        // `rev` alone is replaced; the longer identifier `revenue`
        // and the word `previous` are untouched.
        assert_eq!(
            substitute_labels("rev <= revenue on 10 rows; previous rev noted", &labels),
            "revenue (labelled) <= revenue on 10 rows; previous revenue (labelled) noted"
        );
    }

    #[test]
    fn substitute_prefers_longer_names_and_never_rescans_labels() {
        let labels = declared(&[("rev", "R"), ("rev_total", "rev plus tax")]);
        // rev_total is substituted first; its label contains the raw
        // name `rev` and that text survives verbatim.
        assert_eq!(
            substitute_labels("rev_total = rev + tax", &labels),
            "rev plus tax = R + tax"
        );
    }

    #[test]
    fn substitute_handles_edges_and_punctuation() {
        let labels = declared(&[("acc_cnt", "accepted count")]);
        assert_eq!(
            substitute_labels("acc_cnt: the column 'acc_cnt' (acc_cnt).", &labels),
            "accepted count: the column 'accepted count' (accepted count)."
        );
        assert_eq!(substitute_labels("", &labels), "");
        assert_eq!(
            substitute_labels("no names here", &BTreeMap::new()),
            "no names here"
        );
    }
}
