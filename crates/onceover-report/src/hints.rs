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
    }
}
