//! Deterministic row sampling.
//!
//! Internal samples are **content-keyed**: the key of a row is derived from
//! its canonical values, so the sample chosen under a row cap is a function
//! of the table's content, not of row positions. Two profiles of the same
//! rows in a different order therefore draw the same sample and report the
//! same sampled statistics.
//!
//! Canonical values are *ranks within each column*, not rendered values.
//! That makes a key invariant to a column's name, its position in the frame,
//! and any strictly monotone rescaling of its values. Columns whose values
//! are all distinct are left out when other columns can anchor a key: an
//! independent added column is all-distinct, and it must not redraw the
//! sample of the original table. If no column has a repeated value, the
//! key is the smallest column field: no smaller content basis exists, and
//! one extra all-distinct column displaces only the rows for which it is
//! the minimum.
//!
//! The per-column hashes are combined as a multiset, so column order cannot
//! change a key. Exact duplicate rows cannot be told apart by content; they
//! receive occurrence ordinals `0..k-1`, mixed into the content hash, so the
//! tie-break is documented and stable: the cap takes the lowest ordinals of
//! that group, which are content-identical copies. The positional helper
//! [`positional_rows`] remains only for passes whose semantics are the row
//! sequence itself (axis-order scans).

use std::collections::BinaryHeap;

use rayon::prelude::*;
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
    ///
    /// Processes columns in parallel with bounded memory: all-distinct
    /// columns are folded directly into a running per-row minimum (used
    /// only if every column is all-distinct) and immediately freed, while
    /// included columns store compact `Const`/`u8`/`u16`/`u32` ranks.
    /// Produces bit-for-bit identical keys to the canonical multiset
    /// specification.
    pub fn for_table(table: &TypedTable) -> Self {
        let n = table.n;
        if n == 0 {
            return Self { keys: Vec::new() };
        }
        let n_cols = table.cols.len();
        if n_cols == 0 {
            let empty_base = content_hash_fields(&[0]);
            let keys = (0..n as u64)
                .map(|ord| key_with_ordinal(empty_base, ord))
                .collect();
            return Self { keys };
        }

        // Process columns in parallel; fold all-distinct columns into a
        // per-worker running minimum so we never retain O(P * N) ranks for
        // unique columns (e.g. 197 noise columns on a 2M-row table).
        struct Acc {
            included: Vec<CompactColumn>,
            min_all_distinct: Vec<u64>,
            has_all_distinct: bool,
        }

        let acc = table
            .cols
            .par_iter()
            .fold(
                || Acc {
                    included: Vec::new(),
                    min_all_distinct: Vec::new(),
                    has_all_distinct: false,
                },
                |mut acc, col| {
                    match build_column_plan(col, n) {
                        ColPlanOutcome::Included(c) => {
                            acc.included.push(c);
                            // Once any column is included, the all-distinct
                            // fallback will never be used; release its buffer.
                            if !acc.min_all_distinct.is_empty() {
                                acc.min_all_distinct = Vec::new();
                            }
                        }
                        ColPlanOutcome::AllDistinct { sig, row_of_rank } => {
                            if acc.included.is_empty() {
                                if acc.min_all_distinct.is_empty() {
                                    acc.min_all_distinct = vec![u64::MAX; n];
                                }
                                for (rank, &row) in row_of_rank.iter().enumerate() {
                                    let f = column_field(sig, rank as u64);
                                    let slot = &mut acc.min_all_distinct[row as usize];
                                    if f < *slot {
                                        *slot = f;
                                    }
                                }
                                acc.has_all_distinct = true;
                            }
                        }
                    }
                    acc
                },
            )
            .reduce(
                || Acc {
                    included: Vec::new(),
                    min_all_distinct: Vec::new(),
                    has_all_distinct: false,
                },
                |mut a, b| {
                    a.included.extend(b.included);
                    if !a.included.is_empty() {
                        a.min_all_distinct = Vec::new();
                        a.has_all_distinct = false;
                    } else if b.has_all_distinct {
                        if !a.has_all_distinct {
                            a.min_all_distinct = b.min_all_distinct;
                            a.has_all_distinct = true;
                        } else {
                            for (dst, src) in
                                a.min_all_distinct.iter_mut().zip(b.min_all_distinct.iter())
                            {
                                if *src < *dst {
                                    *dst = *src;
                                }
                            }
                        }
                    }
                    a
                },
            );

        // Compute base RowKey per row in parallel, reusing one scratch
        // `fields` vector per chunk with zero per-row heap allocations.
        let bases: Vec<RowKey> = if !acc.included.is_empty() {
            let included = &acc.included;
            let k = included.len();
            let mut bases = vec![0u128; n];
            let chunk = (n / (rayon::current_num_threads().max(1) * 4)).clamp(1024, 16_384);
            bases
                .par_chunks_mut(chunk)
                .enumerate()
                .for_each(|(ci, out_chunk)| {
                    let start = ci * chunk;
                    let mut fields = Vec::with_capacity(k);
                    for (offset, slot) in out_chunk.iter_mut().enumerate() {
                        let row = start + offset;
                        fields.clear();
                        for col in included {
                            fields.push(col.field_at(row));
                        }
                        fields.sort_unstable();
                        *slot = content_hash_fields(&fields);
                    }
                });
            bases
        } else {
            let mins = &acc.min_all_distinct;
            let has = acc.has_all_distinct;
            (0..n)
                .into_par_iter()
                .map(|row| {
                    let f = if has { mins[row] } else { 0 };
                    content_hash_fields(&[f])
                })
                .collect()
        };

        // Release column rank buffers before allocating the duplicate-ordinal map.
        drop(acc);

        let mut counts: FxHashMap<RowKey, u64> = FxHashMap::default();
        counts.reserve(n.min(65_536));
        let mut keys = Vec::with_capacity(n);
        for base in bases {
            let ordinal = counts.entry(base).or_insert(0);
            keys.push(key_with_ordinal(base, *ordinal));
            *ordinal += 1;
        }
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

/// Keys for a whole table (cached on `table`).
pub fn row_keys(table: &TypedTable) -> RowKeys {
    table.row_keys().clone()
}

/// Content-keyed sample of original row indices, down to `cap`.
/// Uses the `RowKeys` cached on `table` without cloning.
pub fn sample_rows(table: &TypedTable, cap: usize) -> Vec<u32> {
    table.row_keys().sample(cap)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct OrderedF64(u64);

impl OrderedF64 {
    #[inline]
    fn new(value: f64) -> Self {
        let value = if value == 0.0 { 0.0 } else { value };
        let bits = value.to_bits();
        // Flip the sign bit for negatives so the integer order matches the
        // float order, including -0.0 normalized above.
        Self(if bits >> 63 != 0 {
            !bits
        } else {
            bits ^ (1 << 63)
        })
    }
}

/// Compact per-column rank representation for included columns (`distinct < n`).
/// Precomputes `column_field(sig, rank)` lookup tables for `distinct <= 65536`
/// so row hashing is a single table lookup with 1–2 bytes/row RAM.
enum CompactColumn {
    Const(u64),
    U8 { lut: Vec<u64>, ranks: Vec<u8> },
    U16 { lut: Vec<u64>, ranks: Vec<u16> },
    U32 { sig: u64, ranks: Vec<u32> },
}

impl CompactColumn {
    #[inline]
    fn field_at(&self, row: usize) -> u64 {
        match self {
            CompactColumn::Const(f) => *f,
            CompactColumn::U8 { lut, ranks } => lut[ranks[row] as usize],
            CompactColumn::U16 { lut, ranks } => lut[ranks[row] as usize],
            CompactColumn::U32 { sig, ranks } => column_field(*sig, ranks[row] as u64),
        }
    }
}

enum ColPlanOutcome {
    Included(CompactColumn),
    AllDistinct { sig: u64, row_of_rank: Vec<u32> },
}

const SIG_SEED: u64 = 0x8a5c_d7d3_1d2c_9f41;

#[inline]
fn fnv64_u64(mut h: u64, v: u64) -> u64 {
    for b in v.to_le_bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01B3);
    }
    h
}

fn build_from_sorted<K: Ord>(mut items: Vec<(K, u32)>, n: usize) -> ColPlanOutcome {
    items.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    // Count distinct values in one sequential pass over sorted keys.
    let mut distinct = 1usize;
    for i in 1..n {
        if items[i].0 != items[i - 1].0 {
            distinct += 1;
        }
    }

    if distinct == n {
        let mut sig = fnv64_u64(SIG_SEED, n as u64);
        for _ in 0..n {
            sig = fnv64_u64(sig, 1);
        }
        let row_of_rank: Vec<u32> = items.into_iter().map(|(_, r)| r).collect();
        return ColPlanOutcome::AllDistinct { sig, row_of_rank };
    }

    if distinct == 1 {
        let sig = fnv64_u64(fnv64_u64(SIG_SEED, 1), n as u64);
        return ColPlanOutcome::Included(CompactColumn::Const(column_field(sig, 0)));
    }

    let mut sig = fnv64_u64(SIG_SEED, distinct as u64);
    if distinct <= 256 {
        let mut ranks = vec![0u8; n];
        let mut rank = 0usize;
        let mut run = 0u64;
        for i in 0..n {
            if i > 0 && items[i].0 != items[i - 1].0 {
                sig = fnv64_u64(sig, run);
                rank += 1;
                run = 0;
            }
            ranks[items[i].1 as usize] = rank as u8;
            run += 1;
        }
        sig = fnv64_u64(sig, run);
        let lut: Vec<u64> = (0..distinct).map(|r| column_field(sig, r as u64)).collect();
        ColPlanOutcome::Included(CompactColumn::U8 { lut, ranks })
    } else if distinct <= 65_536 {
        let mut ranks = vec![0u16; n];
        let mut rank = 0usize;
        let mut run = 0u64;
        for i in 0..n {
            if i > 0 && items[i].0 != items[i - 1].0 {
                sig = fnv64_u64(sig, run);
                rank += 1;
                run = 0;
            }
            ranks[items[i].1 as usize] = rank as u16;
            run += 1;
        }
        sig = fnv64_u64(sig, run);
        let lut: Vec<u64> = (0..distinct).map(|r| column_field(sig, r as u64)).collect();
        ColPlanOutcome::Included(CompactColumn::U16 { lut, ranks })
    } else {
        let mut ranks = vec![0u32; n];
        let mut rank = 0u32;
        let mut run = 0u64;
        for i in 0..n {
            if i > 0 && items[i].0 != items[i - 1].0 {
                sig = fnv64_u64(sig, run);
                rank += 1;
                run = 0;
            }
            ranks[items[i].1 as usize] = rank;
            run += 1;
        }
        sig = fnv64_u64(sig, run);
        ColPlanOutcome::Included(CompactColumn::U32 { sig, ranks })
    }
}

fn build_column_plan(col: &TypedCol<'_>, n: usize) -> ColPlanOutcome {
    match col {
        TypedCol::Null { .. } | TypedCol::Other { .. } => {
            let sig = fnv64_u64(fnv64_u64(SIG_SEED, 1), n as u64);
            if n <= 1 {
                ColPlanOutcome::AllDistinct {
                    sig,
                    row_of_rank: (0..n as u32).collect(),
                }
            } else {
                ColPlanOutcome::Included(CompactColumn::Const(column_field(sig, 0)))
            }
        }
        TypedCol::Bool { vals } => {
            let (mut c_null, mut c_false, mut c_true) = (0u64, 0u64, 0u64);
            for v in vals {
                match v {
                    None => c_null += 1,
                    Some(false) => c_false += 1,
                    Some(true) => c_true += 1,
                }
            }
            let counts = [c_null, c_false, c_true];
            let mut present_counts = [0u64; 3];
            let mut rank_of = [0u8; 3];
            let mut distinct = 0usize;
            for (i, &c) in counts.iter().enumerate() {
                if c > 0 {
                    rank_of[i] = distinct as u8;
                    present_counts[distinct] = c;
                    distinct += 1;
                }
            }
            let mut sig = fnv64_u64(SIG_SEED, distinct as u64);
            for &c in &present_counts[..distinct] {
                sig = fnv64_u64(sig, c);
            }
            if distinct == n {
                let mut row_of_rank = vec![0u32; n];
                for (r, v) in vals.iter().enumerate() {
                    let k = match v {
                        None => rank_of[0],
                        Some(false) => rank_of[1],
                        Some(true) => rank_of[2],
                    };
                    row_of_rank[k as usize] = r as u32;
                }
                return ColPlanOutcome::AllDistinct { sig, row_of_rank };
            }
            if distinct == 1 {
                return ColPlanOutcome::Included(CompactColumn::Const(column_field(sig, 0)));
            }
            let ranks: Vec<u8> = vals
                .iter()
                .map(|v| match v {
                    None => rank_of[0],
                    Some(false) => rank_of[1],
                    Some(true) => rank_of[2],
                })
                .collect();
            let lut: Vec<u64> = (0..distinct).map(|r| column_field(sig, r as u64)).collect();
            ColPlanOutcome::Included(CompactColumn::U8 { lut, ranks })
        }
        TypedCol::I64 { vals } => match vals {
            crate::typed::NumericValues::Borrowed(slice) => {
                let items: Vec<(i64, u32)> = slice
                    .iter()
                    .enumerate()
                    .map(|(r, &v)| (v, r as u32))
                    .collect();
                build_from_sorted(items, n)
            }
            _ => {
                let items: Vec<(Option<i64>, u32)> = vals
                    .iter()
                    .enumerate()
                    .map(|(r, v)| (v.copied(), r as u32))
                    .collect();
                build_from_sorted(items, n)
            }
        },
        TypedCol::U64 { vals } => match vals {
            crate::typed::NumericValues::Borrowed(slice) => {
                let items: Vec<(u64, u32)> = slice
                    .iter()
                    .enumerate()
                    .map(|(r, &v)| (v, r as u32))
                    .collect();
                build_from_sorted(items, n)
            }
            _ => {
                let items: Vec<(Option<u64>, u32)> = vals
                    .iter()
                    .enumerate()
                    .map(|(r, v)| (v.copied(), r as u32))
                    .collect();
                build_from_sorted(items, n)
            }
        },
        TypedCol::F64 { vals } => match vals {
            crate::typed::NumericValues::Borrowed(slice) => {
                let items: Vec<(OrderedF64, u32)> = slice
                    .iter()
                    .enumerate()
                    .map(|(r, &v)| (OrderedF64::new(v), r as u32))
                    .collect();
                build_from_sorted(items, n)
            }
            _ => {
                let items: Vec<(Option<OrderedF64>, u32)> = vals
                    .iter()
                    .enumerate()
                    .map(|(r, v)| {
                        (
                            v.copied().filter(|x| x.is_finite()).map(OrderedF64::new),
                            r as u32,
                        )
                    })
                    .collect();
                build_from_sorted(items, n)
            }
        },
        TypedCol::Decimal { vals, .. } => {
            let items: Vec<(Option<i128>, u32)> = vals
                .iter()
                .enumerate()
                .map(|(r, &v)| (v, r as u32))
                .collect();
            build_from_sorted(items, n)
        }
        TypedCol::Date { vals } => {
            let items: Vec<(Option<i32>, u32)> = vals
                .iter()
                .enumerate()
                .map(|(r, &v)| (v, r as u32))
                .collect();
            build_from_sorted(items, n)
        }
        TypedCol::Datetime { vals, .. } => {
            let items: Vec<(Option<i64>, u32)> = vals
                .iter()
                .enumerate()
                .map(|(r, &v)| (v, r as u32))
                .collect();
            build_from_sorted(items, n)
        }
        TypedCol::Str { vals } => {
            let items: Vec<(Option<&str>, u32)> = vals
                .iter()
                .enumerate()
                .map(|(r, &v)| (v, r as u32))
                .collect();
            build_from_sorted(items, n)
        }
        TypedCol::StrOwned { vals } => {
            let items: Vec<(Option<&str>, u32)> = vals
                .iter()
                .enumerate()
                .map(|(r, v)| (v.as_deref(), r as u32))
                .collect();
            build_from_sorted(items, n)
        }
    }
}

/// Stream length-prefixed canonical bytes for `fields` directly into both
/// FNV-1a accumulators (zero heap allocation, bit-identical to `push_field`).
#[inline]
fn content_hash_fields(fields: &[u64]) -> RowKey {
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h2: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut feed = |v: u64| {
        h1 = fnv64_u64(h1, v);
        h2 = fnv64_u64(h2, v);
    };
    feed(8);
    feed(fields.len() as u64);
    for &field in fields {
        feed(8);
        feed(field);
    }
    (h1 as RowKey) << 64 | h2 as RowKey
}

fn splitmix64(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

const FIELD_SALT: u64 = 0x9e37_79b9_7f4a_7c15;

fn column_field(signature: u64, rank: u64) -> u64 {
    splitmix64(signature ^ splitmix64(rank.wrapping_add(FIELD_SALT)))
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
        let b: Vec<f64> = order
            .iter()
            .map(|&i| ((i % 29) as f64 * 0.25).sin())
            .collect();
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
    fn keys_ignore_names_column_order_scale_and_added_all_distinct_column() {
        let df = df! {
            "a" => (0..600i64).map(|i| i % 31).collect::<Vec<_>>(),
            "b" => (0..600i64).map(|i| (i % 17) as f64 / 7.0).collect::<Vec<_>>(),
        }
        .unwrap();
        let table = extract(&df);
        let reversed = df! {
            "b" => df.column("b").unwrap().f64().unwrap().into_no_null_iter().collect::<Vec<_>>(),
            "a" => df.column("a").unwrap().i64().unwrap().into_no_null_iter().collect::<Vec<_>>(),
        }
        .unwrap();
        let reversed = extract(&reversed);
        let mut renamed_df = df.clone();
        renamed_df
            .rename("a", "x".into())
            .unwrap()
            .rename("b", "y".into())
            .unwrap();
        let renamed = extract(&renamed_df);
        let scaled = df! {
            "a" => df.column("a").unwrap().i64().unwrap().into_no_null_iter().collect::<Vec<_>>(),
            "b" => df.column("b").unwrap().f64().unwrap().into_no_null_iter().map(|v| v * 1000.0).collect::<Vec<_>>(),
        }
        .unwrap();
        let scaled = extract(&scaled);
        let noise: Vec<f64> = (0..600).map(|i| (i as f64 * 0.618).sin()).collect();
        let noisy = df! {
            "a" => df.column("a").unwrap().i64().unwrap().into_no_null_iter().collect::<Vec<_>>(),
            "b" => df.column("b").unwrap().f64().unwrap().into_no_null_iter().collect::<Vec<_>>(),
            "zz_noise" => noise,
        }
        .unwrap();
        let noisy = extract(&noisy);

        let expected = sample_rows(&table, 80);
        assert_eq!(expected, sample_rows(&reversed, 80));
        assert_eq!(expected, sample_rows(&renamed, 80));
        assert_eq!(expected, sample_rows(&scaled, 80));
        assert_eq!(expected, sample_rows(&noisy, 80));
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

    #[test]
    fn all_distinct_only_table_is_deterministic_and_order_free() {
        let df1 = df! {
            "x" => (0..200i64).collect::<Vec<_>>(),
            "y" => (0..200).map(|i| (i as f64) * 1.5 - 50.0).collect::<Vec<_>>(),
        }
        .unwrap();
        let df2 = df! {
            "y" => (0..200).rev().map(|i| (i as f64) * 1.5 - 50.0).collect::<Vec<_>>(),
            "x" => (0..200i64).rev().collect::<Vec<_>>(),
        }
        .unwrap();
        let t1 = extract(&df1);
        let t2 = extract(&df2);
        let s1 = sample_rows(&t1, 40);
        let s2 = sample_rows(&t2, 40);
        assert_eq!(s1.len(), 40);
        // Same content rows in the same key order, despite reversed rows & columns.
        assert_eq!(
            s1.iter()
                .map(|&r| t1.cols[0].render_cell(r as usize))
                .collect::<Vec<_>>(),
            s2.iter()
                .map(|&r| t2.cols[1].render_cell(r as usize))
                .collect::<Vec<_>>(),
        );
        // Cache returns identical pointer/values on repeat calls.
        assert!(std::ptr::eq(t1.row_keys(), t1.row_keys()));
    }

    use crate::typed::extract;
}
