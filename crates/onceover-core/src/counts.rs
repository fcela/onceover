//! Exact value counts, concentration measures, and top-k values.
//!
//! One pass per column builds a value-count map (owned keys). `VKey` owns its
//! data (strings are copied in) so counts outlive borrows; columns are
//! uniform-dtype so mixed-variant comparisons never occur in practice.

use rustc_hash::FxHashMap;
use serde_json::json;

use onceover_report::Concentration;

use crate::typed::TypedCol;

/// A hashable/equatable/comparable value key for counting.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum VKey {
    I(i64),
    /// f64 bit pattern (NaN never stored; -0.0 normalized upstream).
    F(u64),
    S(Box<str>),
    B(bool),
}

impl VKey {
    /// Display string for reports.
    pub fn display(&self) -> String {
        match self {
            VKey::I(v) => v.to_string(),
            VKey::F(bits) => format_f64(f64::from_bits(*bits)),
            VKey::S(s) => s.to_string(),
            VKey::B(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
        }
    }
}

/// Shortest reasonable float display (no trailing noise).
pub fn format_f64(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.6}", v);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_string()
    }
}

impl PartialOrd for VKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        match (self, other) {
            (VKey::I(a), VKey::I(b)) => a.cmp(b),
            (VKey::F(a), VKey::F(b)) => {
                // Order by value (both non-NaN; -0 normalized).
                f64::from_bits(*a)
                    .partial_cmp(&f64::from_bits(*b))
                    .unwrap_or(Equal)
            }
            (VKey::S(a), VKey::S(b)) => a.cmp(b),
            (VKey::B(a), VKey::B(b)) => a.cmp(b),
            // Cross-variant never happens within a column; keep it total:
            _ => discriminant(self).cmp(&discriminant(other)),
        }
    }
}

fn discriminant(v: &VKey) -> u8 {
    match v {
        VKey::I(_) => 0,
        VKey::F(_) => 1,
        VKey::S(_) => 2,
        VKey::B(_) => 3,
    }
}

/// Result of counting one column's values.
#[derive(Debug, Clone, Default)]
pub struct ColCounts {
    /// Exact distinct count over non-null values (lower bound if `capped`).
    pub distinct: usize,
    /// True when distinct exceeded `cap` and full counting was abandoned.
    pub capped: bool,
    pub cap: usize,
    pub n_nonnull: usize,
    /// Top values, sorted by (count desc, key asc). Empty when capped.
    pub top: Vec<(VKey, u32)>,
    /// Concentration measures (None when capped).
    pub concentration: Option<Concentration>,
}

const MAX_TOP: usize = 64;

/// Count values of a column in one pass. `cap` bounds memory; when exceeded,
/// counting is abandoned and reported honestly (`capped = true`).
pub fn build_counts(col: &TypedCol, cap: usize) -> ColCounts {
    let mut map: FxHashMap<VKey, u32> = FxHashMap::default();
    let mut n_nonnull: usize = 0;
    let mut capped = false;

    match col {
        TypedCol::I64 { vals } => {
            for v in vals.iter().flatten() {
                n_nonnull += 1;
                if map.len() >= cap {
                    capped = true;
                    break;
                }
                *map.entry(VKey::I(*v)).or_insert(0) += 1;
            }
        }
        TypedCol::F64 { vals } => {
            for v in vals.iter().flatten() {
                n_nonnull += 1;
                if map.len() >= cap {
                    capped = true;
                    break;
                }
                *map.entry(VKey::F(v.to_bits())).or_insert(0) += 1;
            }
        }
        TypedCol::Str { vals } => {
            for v in vals.iter().flatten() {
                n_nonnull += 1;
                if map.len() >= cap {
                    capped = true;
                    break;
                }
                *map.entry(VKey::S(Box::from(*v))).or_insert(0) += 1;
            }
        }
        TypedCol::StrOwned { vals } => {
            for v in vals.iter().flatten() {
                n_nonnull += 1;
                if map.len() >= cap {
                    capped = true;
                    break;
                }
                *map.entry(VKey::S(Box::from(v.as_str()))).or_insert(0) += 1;
            }
        }
        TypedCol::Bool { vals } => {
            for v in vals.iter().flatten() {
                n_nonnull += 1;
                *map.entry(VKey::B(*v)).or_insert(0) += 1;
            }
        }
        TypedCol::Date { vals } => {
            for v in vals.iter().flatten() {
                n_nonnull += 1;
                if map.len() >= cap {
                    capped = true;
                    break;
                }
                *map.entry(VKey::I(*v as i64)).or_insert(0) += 1;
            }
        }
        TypedCol::Datetime { vals } => {
            for v in vals.iter().flatten() {
                n_nonnull += 1;
                if map.len() >= cap {
                    capped = true;
                    break;
                }
                *map.entry(VKey::I(*v)).or_insert(0) += 1;
            }
        }
        TypedCol::Null { .. } | TypedCol::Other { .. } => {}
    }

    if capped {
        return ColCounts {
            distinct: cap,
            capped,
            cap,
            n_nonnull,
            top: Vec::new(),
            concentration: None,
        };
    }

    let distinct = map.len();
    let mut top: Vec<(VKey, u32)> = map.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top.truncate(MAX_TOP);

    let concentration = if distinct >= 1 && n_nonnull > 0 {
        Some(compute_concentration(&top, distinct, n_nonnull))
    } else {
        None
    };

    ColCounts {
        distinct,
        capped,
        cap,
        n_nonnull,
        top,
        concentration,
    }
}

/// HHI / entropy / effective cardinality / top-k shares.
/// `top` must be the full sorted count list (desc) — shares over all values.
fn compute_concentration(top: &[(VKey, u32)], distinct: usize, n_nonnull: usize) -> Concentration {
    // Full shares require all values; recompute from counts. `top` may be
    // truncated to MAX_TOP, but shares of truncated tail are negligible —
    // still, compute exactly over what we have plus the tail.
    // NOTE: caller passes truncated list; we compensate with the tail.
    let listed: u64 = top.iter().map(|(_, c)| *c as u64).sum();
    let tail = n_nonnull as u64 - listed.min(n_nonnull as u64);
    let n = n_nonnull as f64;

    let mut hhi = 0.0f64;
    let mut entropy = 0.0f64;
    for (_, c) in top {
        let p = *c as f64 / n;
        hhi += p * p;
        entropy -= p * p.ln();
    }
    if tail > 0 && distinct > top.len() {
        // Approximate tail as uniform over remaining distinct values.
        let rest = (distinct - top.len()) as f64;
        let p_each = tail as f64 / rest / n;
        hhi += rest * p_each * p_each;
        entropy -= rest * p_each * p_each.ln();
    }

    let top1 = top.first().map(|(_, c)| *c as f64 / n);
    let top3: Option<f64> = {
        let s3: u64 = top.iter().take(3).map(|(_, c)| *c as u64).sum();
        if top.is_empty() {
            None
        } else {
            Some(s3 as f64 / n)
        }
    };

    Concentration {
        hhi,
        entropy_nats: entropy,
        effective_cardinality: entropy.exp(),
        top1_share: top1,
        top3_share: top3,
    }
}

/// Convenience: JSON evidence map with a single key.
pub fn ev(k: &str, v: serde_json::Value) -> serde_json::Value {
    json!({k: v})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_concentration() {
        let col = TypedCol::I64 {
            vals: vec![Some(1), Some(1), Some(1), Some(2), Some(3)],
        };
        let c = build_counts(&col, 1_000_000);
        assert_eq!(c.distinct, 3);
        assert_eq!(c.n_nonnull, 5);
        assert!(!c.capped);
        let conc = c.concentration.unwrap();
        assert!((conc.hhi - (9.0 / 25.0 + 1.0 / 25.0 + 1.0 / 25.0)).abs() < 1e-12);
        assert!((conc.top1_share.unwrap() - 0.6).abs() < 1e-12);
        assert_eq!(c.top[0].0, VKey::I(1));
        assert_eq!(c.top[0].1, 3);
    }

    #[test]
    fn cap_triggers_honestly() {
        let col = TypedCol::I64 {
            vals: (0..100).map(Some).collect(),
        };
        let c = build_counts(&col, 10);
        assert!(c.capped);
        assert_eq!(c.distinct, 10);
        assert!(c.concentration.is_none());
        assert!(c.top.is_empty());
    }

    #[test]
    fn vkey_ordering() {
        let a = VKey::F((2.5f64).to_bits());
        let b = VKey::F((-1.0f64).to_bits());
        assert!(a > b);
        assert!(VKey::I(2) > VKey::I(1));
    }
}
