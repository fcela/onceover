//! Exact value counts, concentration measures, and top-k values.
//!
//! Adaptive hash counting / sorted runs retain exact counts at any cardinality.
//! Only reported top values own string data; high-cardinality sorting borrows strings.

use rustc_hash::FxHashMap;
use serde_json::json;
use std::{cmp::Reverse, collections::BinaryHeap, hash::Hash};

use onceover_report::Concentration;

use crate::typed::TypedCol;

/// A hashable/equatable/comparable value key for counting.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum VKey {
    I(i64),
    U(u64),
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
            VKey::U(v) => v.to_string(),
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
            (VKey::U(a), VKey::U(b)) => a.cmp(b),
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
        VKey::U(_) => 4,
        VKey::F(_) => 1,
        VKey::S(_) => 2,
        VKey::B(_) => 3,
    }
}

/// Result of counting one column's values.
#[derive(Debug, Clone, Default)]
pub struct ColCounts {
    /// Exact distinct count over all non-null values.
    pub distinct: usize,
    /// Retained for compatibility; exact counting no longer truncates.
    pub capped: bool,
    pub cap: usize,
    pub n_nonnull: usize,
    /// Top values, sorted by (count desc, key asc).
    pub top: Vec<(VKey, usize)>,
    /// Exact concentration over the full value distribution.
    pub concentration: Option<Concentration>,
}

const MAX_TOP: usize = 64;

/// Exact counts: use a small hash map for low cardinality, then switch to
/// sorting compact values when the map limit is reached. The limit controls
/// the algorithm, never the accuracy or number of input rows examined.
pub fn build_counts(col: &TypedCol, hash_limit: usize) -> ColCounts {
    match col {
        TypedCol::I64 { vals } => count_values(vals.iter().flatten().copied(), hash_limit, VKey::I),
        TypedCol::U64 { vals } => count_values(vals.iter().flatten().copied(), hash_limit, VKey::U),
        TypedCol::F64 { vals } => count_values(
            vals.iter().flatten().map(|v| v.to_bits()),
            hash_limit,
            VKey::F,
        ),
        TypedCol::Date { vals } => count_values(
            vals.iter().flatten().map(|v| i64::from(*v)),
            hash_limit,
            VKey::I,
        ),
        TypedCol::Datetime { vals, .. } => {
            count_values(vals.iter().flatten().copied(), hash_limit, VKey::I)
        }
        TypedCol::Bool { vals } => {
            count_values(vals.iter().flatten().copied(), hash_limit, VKey::B)
        }
        TypedCol::Str { vals } => count_values(vals.iter().flatten().copied(), hash_limit, |v| {
            VKey::S(Box::from(v))
        }),
        TypedCol::StrOwned { vals } => {
            count_values(vals.iter().flatten().map(String::as_str), hash_limit, |v| {
                VKey::S(Box::from(v))
            })
        }
        TypedCol::Null { .. } | TypedCol::Other { .. } => ColCounts::default(),
    }
}

fn count_values<T, I>(values: I, hash_limit: usize, key: impl Fn(T) -> VKey) -> ColCounts
where
    T: Ord + Hash + Clone,
    I: Iterator<Item = T> + Clone,
{
    let mut counts = FxHashMap::<T, usize>::default();
    let mut overflow = false;
    for v in values.clone() {
        if counts.len() >= hash_limit && !counts.contains_key(&v) {
            overflow = true;
            break;
        }
        *counts.entry(v).or_default() += 1;
    }
    let mut summary = CountSummary::default();
    if overflow {
        drop(counts);
        let mut sorted: Vec<T> = values.collect();
        sorted.sort_unstable();
        let mut start = 0;
        while start < sorted.len() {
            let mut end = start + 1;
            while end < sorted.len() && sorted[end] == sorted[start] {
                end += 1;
            }
            summary.add(key(sorted[start].clone()), end - start);
            start = end;
        }
    } else {
        // Stable accumulation keeps concentration byte-deterministic.
        let mut entries: Vec<_> = counts.into_iter().collect();
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for (value, count) in entries {
            summary.add(key(value), count);
        }
    }
    summary.finish(hash_limit)
}

#[derive(Default)]
struct CountSummary {
    distinct: usize,
    n: usize,
    squares: f64,
    count_log_count: f64,
    // The worst retained entry is at the root: lowest count, largest key.
    top: BinaryHeap<(Reverse<usize>, VKey)>,
}

impl CountSummary {
    fn add(&mut self, key: VKey, count: usize) {
        self.distinct += 1;
        self.n += count;
        let c = count as f64;
        self.squares += c * c;
        self.count_log_count += c * c.ln();
        if let Some((Reverse(worst_count), worst_key)) = self.top.peek() {
            if self.top.len() == MAX_TOP
                && (count < *worst_count || (count == *worst_count && key >= *worst_key))
            {
                return;
            }
        }
        self.top.push((Reverse(count), key));
        if self.top.len() > MAX_TOP {
            self.top.pop();
        }
    }

    fn finish(self, hash_limit: usize) -> ColCounts {
        let mut top: Vec<_> = self.top.into_iter().map(|(Reverse(c), k)| (k, c)).collect();
        top.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let n = self.n as f64;
        let concentration = (self.n > 0).then(|| {
            let entropy = n.ln() - self.count_log_count / n;
            Concentration {
                hhi: self.squares / (n * n),
                entropy_nats: entropy,
                effective_cardinality: entropy.exp(),
                top1_share: top.first().map(|(_, c)| *c as f64 / n),
                top3_share: Some(top.iter().take(3).map(|(_, c)| *c).sum::<usize>() as f64 / n),
            }
        });
        ColCounts {
            distinct: self.distinct,
            capped: false,
            cap: hash_limit,
            n_nonnull: self.n,
            top,
            concentration,
        }
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
            vals: vec![Some(1), Some(1), Some(1), Some(2), Some(3)].into(),
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
    fn hash_limit_switches_algorithm_without_truncating_counts() {
        let col = TypedCol::I64 {
            vals: (0..100).map(Some).collect::<Vec<_>>().into(),
        };
        let c = build_counts(&col, 10);
        assert!(!c.capped);
        assert_eq!(c.distinct, 100);
        assert_eq!(c.n_nonnull, 100);
        assert!((c.concentration.unwrap().hhi - 0.01).abs() < 1e-12);
        assert_eq!(c.top.len(), MAX_TOP);
    }

    #[test]
    fn vkey_ordering() {
        let a = VKey::F((2.5f64).to_bits());
        let b = VKey::F((-1.0f64).to_bits());
        assert!(a > b);
        assert!(VKey::I(2) > VKey::I(1));
    }
}
