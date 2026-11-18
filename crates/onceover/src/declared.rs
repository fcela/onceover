//! Accounting for the structural hints that steer the frame model
//! and the along-axis passes.
//!
//! The metadata keys are copied into `frame.declarations` and
//! resolved in the pipeline's declarations block. These four keys
//! are instructions, not metadata: they never appear in the
//! frame account, but the caller still needs to know whether the
//! profiler read and used them. Each one is resolved once here,
//! against the extracted table and the computed frame model, and
//! its applied/refused decision is added to the same
//! `declarations` coverage entry. Missing columns and existing
//! but unusable columns are both stated; neither raises.

use onceover_core::{TypedCol, TypedTable};
use onceover_report::ProfileHints;
use std::collections::{BTreeMap, BTreeSet};

use crate::frame::{FrameModel, ROLE_VOCABULARY};

#[derive(Clone, Debug, Default)]
pub(crate) struct DeclaredHintAccount {
    /// Lines for the `declarations` coverage entry.
    pub notes: Vec<String>,
    /// Level orders that were checked and can be used by the
    /// ordered-axis battery. Refused entries are omitted.
    pub level_order: BTreeMap<String, Vec<String>>,
    /// The ordered-axis hint when it was checked and usable.
    pub ordered_axis: Option<String>,
}

pub(crate) fn account(
    table: &TypedTable<'_>,
    frame: &FrameModel,
    hints: Option<&ProfileHints>,
) -> DeclaredHintAccount {
    let mut out = DeclaredHintAccount::default();
    let Some(hints) = hints else {
        return out;
    };
    let col_exists = |name: &str| table.names.iter().any(|n| n == name);

    if !hints.roles.is_empty() {
        let mut applied = 0usize;
        let mut refused: Vec<String> = Vec::new();
        for (name, role) in &hints.roles {
            if !col_exists(name) {
                refused.push(format!("'{name}' (not a column of the table)"));
                continue;
            }
            if !ROLE_VOCABULARY.contains(&role.as_str()) {
                refused.push(format!(
                    "'{name}' declares '{role}' (not one of {})",
                    ROLE_VOCABULARY.join(", ")
                ));
                continue;
            }
            applied += 1;
        }
        out.notes.push(format!(
            "roles: {applied} declaration(s) applied to the frame model{}",
            if refused.is_empty() {
                String::new()
            } else {
                format!("; {} refused — {}", refused.len(), refused.join("; "))
            }
        ));
    }

    if !hints.level_order.is_empty() {
        let mut applied = 0usize;
        let mut refused: Vec<String> = Vec::new();
        for (name, levels) in &hints.level_order {
            let Some(i) = table.names.iter().position(|n| n == name) else {
                refused.push(format!("'{name}' (not a column of the table)"));
                continue;
            };
            let col = &table.cols[i];
            if !matches!(col, TypedCol::Str { .. } | TypedCol::StrOwned { .. }) {
                refused.push(format!("'{name}' (not a string/categorical column)"));
                continue;
            }
            if levels.is_empty() {
                refused.push(format!("'{name}' (no levels declared)"));
                continue;
            }
            let observed: BTreeSet<&str> = (0..table.n).filter_map(|r| col.str_at(r)).collect();
            let unknown: BTreeSet<&str> = levels
                .iter()
                .map(String::as_str)
                .filter(|level| !observed.contains(*level))
                .collect();
            if !unknown.is_empty() {
                refused.push(format!(
                    "'{name}' (level(s) absent from the column: {})",
                    unknown.into_iter().collect::<Vec<_>>().join(", ")
                ));
                continue;
            }
            let mut seen = BTreeSet::new();
            let duplicates: BTreeSet<&str> = levels
                .iter()
                .map(String::as_str)
                .filter(|level| !seen.insert(*level))
                .collect();
            if !duplicates.is_empty() {
                refused.push(format!(
                    "'{name}' (level(s) declared twice: {})",
                    duplicates.into_iter().collect::<Vec<_>>().join(", ")
                ));
                continue;
            }
            applied += 1;
            out.level_order.insert(name.clone(), levels.clone());
        }
        out.notes.push(format!(
            "level_order: {applied} declaration(s) applied as ordered axes{}",
            if refused.is_empty() {
                String::new()
            } else {
                format!("; {} refused — {}", refused.len(), refused.join("; "))
            }
        ));
    }

    if let Some(name) = hints.time.as_deref() {
        let applied = frame.axis_name.as_deref() == Some(name)
            && frame.axis_source.as_deref() == Some("hint");
        if applied {
            out.notes.push(format!(
                "time: '{name}' applied — the frame model uses it as the time axis"
            ));
        } else if !col_exists(name) {
            out.notes.push(format!(
                "time: '{name}' refused — not a column of the table"
            ));
        } else {
            out.notes.push(format!(
                "time: '{name}' refused — an existing column, but not usable as a time axis (expected a date, datetime, integer, or a string whose values parse as timestamps)"
            ));
        }
    }

    if let Some(name) = hints.ordered_axis.as_deref() {
        if onceover_mines::declared_ordered_axis_usable(table, name, &out.level_order) {
            out.notes.push(format!(
                "ordered_axis: '{name}' applied — the ordered-axis battery uses it as its primary axis"
            ));
            out.ordered_axis = Some(name.to_string());
        } else if !col_exists(name) {
            out.notes.push(format!(
                "ordered_axis: '{name}' refused — not a column of the table"
            ));
        } else {
            out.notes.push(format!(
                "ordered_axis: '{name}' refused — an existing column, but not usable as an ordered axis (expected a numeric or temporal column, or a string column with a valid level_order)"
            ));
        }
    }

    out
}
