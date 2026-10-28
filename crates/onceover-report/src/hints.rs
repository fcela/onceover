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
/// columns and unusable types are reported by the expectations pass in
/// its coverage entry rather than raising an error.
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
}

impl ProfileHints {
    pub fn is_empty(&self) -> bool {
        self.time.is_none()
            && self.order_by.is_empty()
            && self.no_gaps.is_empty()
            && self.cadence.is_empty()
            && self.dimensions.is_empty()
            && self.measures.is_empty()
    }
}
