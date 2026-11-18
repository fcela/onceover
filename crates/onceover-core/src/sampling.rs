//! Deterministic row sampling.
//!
//! Internal samples are **content-keyed**: the key of a row is a stable
//! 128-bit hash of its canonical values in every column, so the sample
//! chosen under a row cap is a function of the table's multiset of rows,
//! not of their positions. Returned sample indices are themselves ordered
//! by those keys, so passes that read the sample as a sequence (tie breaks,
//! first-seen labels, split boundaries) are invariant too. Two profiles of
//! the same rows in a different order therefore draw the same sample and
//! report the same sampled statistics.
//!
//! Exact duplicate rows cannot be told apart by content. They receive
//! occurrence ordinals `0..k-1`, mixed into the content hash, so the
//! tie-break is documented and stable: the cap takes the lowest ordinals
//! of that group, which are content-identical copies. The positional
//! helper [`positional_rows`] remains only for passes whose semantics
//! are the row sequence itself (axis-order scans).

use std::collections::BinaryHeap;

use rustc_hash::FxHashMap;

use crate::typed::{TypedCol, TypedTable};

/// Content-derived key of one row.
pub type RowKey = u128;

/// Stable keys for the rows of a table, in row order. The same keys can
/// be carried alongside a subset of rows so later caps and splits stay
/// content-keyed too.
#[derive(Clone, Debug, Default)]
pub struct RowKeys {
    keys: Vec<RowKey>,
}

impl RowKeys {
    /// Compute one key per row from its canonical values in every column.
    pub fn for_table(table: &TypedTable) -> Self {
        let mut counts: FxHashMap<RowKey, u64> = FxHashMap::default();
        let keys = (0..table.n)
            .map(|row| {
                let bytes = canonical_row(table, row);
                let base = content_hash(&bytes);
                let ordinal = counts.entry(base).or_insert(0);
                let key = key_with_ordinal(base, *ordinal);
                *ordinal += 1;
                key
            })
            .collect();
        Self { keys }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn key(&self, row: usize) -> RowKey {
        self.keys[row]
    }

    /// Keys aligned with `rows`, which are indices into this key list.
    pub fn subset(&self, rows: &[u32]) -> Self {
        Self {
            keys: rows.iter().map(|&r| self.keys[r as usize]).collect(),
        }
    }

    /// Sample row indices `0..len` by content.
    pub fn sample(&self, cap: usize) -> Vec<u32> {
        let all: Vec<u32> = (0..self.len() as u32).collect();
        self.sample_from(&all, cap)
    }

    /// Sample row indices from `candidates` (indices into this key list)
    /// by content. Returned indices are in content-key order, so every
    /// downstream pass consumes the same sequence for the same row
    /// multiset regardless of the table's input order.
    pub fn sample_from(&self, candidates: &[u32], cap: usize) -> Vec<u32> {
        if cap == 0 || candidates.is_empty() {
            return Vec::new();
        }
        let selected = if candidates.len() <= cap {
            candidates.to_vec()
        } else {
            // Max-heap of the current best (smallest-key) candidates, so
            // the largest key in the heap can be replaced by a smaller new
            // one without a full sort.
            let mut heap: BinaryHeap<(RowKey, u32)> = BinaryHeap::with_capacity(cap + 1);
            for &index in candidates {
                let item = (self.keys[index as usize], index);
                if heap.len() < cap {
                    heap.push(item);
                } else if let Some(&top) = heap.peek() {
                    if item < top {
                        heap.pop();
                        heap.push(item);
                    }
                }
            }
            heap.into_iter().map(|(_, i)| i).collect()
        };
        // The sample is an ordered sequence, not just a set: passes such
        // as Chatterjee's xi break value ties in sample order, and level
        // or format examples take the first occurrence. Sorting by key
        // (with the original index only as an exact-collision tie-break)
        // makes that sequence a function of row content alone.
        let mut out = selected;
        out.sort_unstable_by_key(|&i| (self.keys[i as usize], i));
        out
    }

    /// Content-derived order of `candidates`. Salts give independent
    /// train/holdout and fold assignments without consulting row
    /// positions.
    pub fn order_from(&self, candidates: &[u32], salt: u64) -> Vec<u32> {
        let mut out = candidates.to_vec();
        out.sort_by_key(|&i| (salted_key(self.keys[i as usize], salt), i));
        out
    }
}

/// Keys for a whole table.
pub fn row_keys(table: &TypedTable) -> RowKeys {
    RowKeys::for_table(table)
}

/// Content-keyed sample of original row indices, down to `cap`.
pub fn sample_rows(table: &TypedTable, cap: usize) -> Vec<u32> {
    row_keys(table).sample(cap)
}

/// The former positional sampler, retained for passes that read the row
/// sequence as an axis. New sampled passes should use [`sample_rows`].
pub fn positional_rows(n: usize, cap: usize) -> Vec<u32> {
    if n <= cap {
        return (0..n as u32).collect();
    }
    if cap == 0 {
        return Vec::new();
    }
    (0..cap)
        .map(|j| {
            let start = (j as u128 * n as u128 / cap as u128) as usize;
            let end = ((j + 1) as u128 * n as u128 / cap as u128) as usize;
            let mut x = (j as u64).wrapping_add(0x9e3779b97f4a7c15);
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
            (start + ((x ^ (x >> 31)) % (end - start) as u64) as usize) as u32
        })
        .collect()
}

/// Length-prefixed canonical bytes for one cell.
fn push_field(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value);
}

fn canonical_row(table: &TypedTable, row: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for ((name, dtype), col) in table.names.iter().zip(&table.dtypes).zip(&table.cols) {
        push_field(&mut out, name.as_bytes());
        push_field(&mut out, dtype.as_bytes());
        out.push(u8::from(!null_at(col, row)));
        push_field(&mut out, col.render_cell(row).as_bytes());
    }
    out
}

fn null_at(col: &TypedCol, row: usize) -> bool {
    col.is_null_at(row)
}

fn fnv64(bytes: &[u8], seed: u64) -> u64 {
    let mut h = seed;
    for &b in bytes {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01B3);
    }
    h
}

fn content_hash(bytes: &[u8]) -> RowKey {
    let first = fnv64(bytes, 0xcbf2_9ce4_8422_2325);
    let second = fnv64(bytes, 0x9E37_79B9_7F4A_7C15);
    (first as RowKey) << 64 | second as RowKey
}

fn splitmix64(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn key_with_ordinal(base: RowKey, ordinal: u64) -> RowKey {
    let lo = splitmix64(base as u64 ^ ordinal.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let hi = splitmix64((base >> 64) as u64 ^ ordinal.wrapping_mul(0xbf58_476d_1ce4_e5b9));
    (hi as RowKey) << 64 | lo as RowKey
}

fn salted_key(key: RowKey, salt: u64) -> RowKey {
    let lo = splitmix64(key as u64 ^ salt);
    let hi = splitmix64((key >> 64) as u64 ^ salt.rotate_left(31));
    (hi as RowKey) << 64 | lo as RowKey
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::df;
    use polars::prelude::DataFrame;

    fn frame(order: &[usize]) -> DataFrame {
        let a: Vec<i64> = order.iter().map(|&i| (i % 17) as i64).collect();
        let b: Vec<f64> = order.iter().map(|&i| (i as f64 * 0.25).sin()).collect();
        let c: Vec<&str> = order.iter().map(|&i| ["x", "y", "z", "w"][i % 4]).collect();
        df! { "a" => a, "b" => b, "c" => c }.unwrap()
    }

    fn rendered_sample(table: &TypedTable, rows: &[u32]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|&r| {
                (0..table.cols.len())
                    .map(|c| table.cols[c].render_cell(r as usize))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn samples_cover_deterministic_content_not_positions() {
        let order: Vec<usize> = (0..1000).collect();
        let forward = frame(&order);
        let reverse: Vec<usize> = order.iter().rev().copied().collect();
        let backward = frame(&reverse);
        let t1 = extract(&forward);
        let t2 = extract(&backward);
        let s1 = sample_rows(&t1, 100);
        let s2 = sample_rows(&t2, 100);
        assert_eq!(s1, sample_rows(&t1, 100));
        assert_eq!(s1.len(), 100);
        assert_eq!(rendered_sample(&t1, &s1), rendered_sample(&t2, &s2));
        assert!(sample_rows(&t1, 0).is_empty());
    }

    #[test]
    fn duplicate_rows_use_stable_occurrence_ordinals() {
        let df = df! {
            "a" => vec![7i64; 20],
            "b" => vec!["same"; 20],
        }
        .unwrap();
        let table = extract(&df);
        let keys = row_keys(&table);
        assert_eq!(keys.len(), 20);
        let first = keys.sample(4);
        assert_eq!(first.len(), 4);
        assert_eq!(
            keys.subset(&first).keys,
            keys.sample_from(&first, 4)
                .iter()
                .map(|&r| keys.key(r as usize))
                .collect::<Vec<_>>()
        );
        let mut distinct: Vec<_> = (0..20).map(|r| keys.key(r)).collect();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 20);
    }

    #[test]
    fn positional_rows_keep_strata_without_periodic_aliasing() {
        let rows = positional_rows(100_000, 1000);
        assert_eq!(rows, positional_rows(100_000, 1000));
        assert_eq!(rows.len(), 1000);
        assert!(rows.windows(2).all(|r| r[0] < r[1]));
        assert!(rows.iter().any(|r| r % 100 != 0));
        for (stratum, &row) in rows.iter().enumerate() {
            assert_eq!(row as usize / 100, stratum);
        }
        assert!(positional_rows(10, 0).is_empty());
    }

    use crate::typed::extract;
}
