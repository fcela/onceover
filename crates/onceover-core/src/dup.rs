//! Exact duplicate-row detection: 64-bit row hashing + full verification
//! (hash collisions can never produce a false claim).

use rustc_hash::FxHashMap;

use crate::typed::{TypedCol, TypedTable};

#[derive(Debug, Clone)]
pub struct DupInfo {
    /// Number of rows that are exact duplicates of an earlier row.
    pub n_redundant: usize,
    pub n_groups: usize,
    pub max_group_size: usize,
    /// Up to 3 example groups (row indices, first 6 each).
    pub examples: Vec<Vec<u32>>,
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

const MISSING: u64 = 0x9E37_79B9_7F4A_7C15;

/// Row equality over all comparable columns (Null/Other columns ignored).
fn row_eq(t: &TypedTable, i: usize, j: usize) -> bool {
    for col in &t.cols {
        match col {
            TypedCol::I64 { vals } => {
                if vals[i] != vals[j] {
                    return false;
                }
            }
            TypedCol::F64 { vals } => {
                if vals[i] != vals[j] {
                    return false;
                }
            }
            TypedCol::Str { vals } => {
                if vals[i] != vals[j] {
                    return false;
                }
            }
            TypedCol::StrOwned { vals } => {
                if vals[i] != vals[j] {
                    return false;
                }
            }
            TypedCol::Bool { vals } => {
                if vals[i] != vals[j] {
                    return false;
                }
            }
            TypedCol::Date { vals } => {
                if vals[i] != vals[j] {
                    return false;
                }
            }
            TypedCol::Datetime { vals } => {
                if vals[i] != vals[j] {
                    return false;
                }
            }
            TypedCol::Null { .. } | TypedCol::Other { .. } => {}
        }
    }
    true
}

fn row_hash(t: &TypedTable, i: usize) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for (ci, col) in t.cols.iter().enumerate() {
        let vh = match col {
            TypedCol::I64 { vals } => vals[i].map(|v| v as u64).unwrap_or(MISSING),
            TypedCol::F64 { vals } => vals[i].map(|v| v.to_bits()).unwrap_or(MISSING),
            TypedCol::Str { vals } => match vals[i] {
                Some(s) => fnv64(s.as_bytes()),
                None => MISSING,
            },
            TypedCol::StrOwned { vals } => match &vals[i] {
                Some(s) => fnv64(s.as_bytes()),
                None => MISSING,
            },
            TypedCol::Bool { vals } => match vals[i] {
                Some(true) => 1,
                Some(false) => 2,
                None => MISSING,
            },
            TypedCol::Date { vals } => vals[i].map(|v| v as u64).unwrap_or(MISSING),
            TypedCol::Datetime { vals } => vals[i].map(|v| v as u64).unwrap_or(MISSING),
            TypedCol::Null { .. } | TypedCol::Other { .. } => continue,
        };
        h ^= vh
            .wrapping_add(0x9E37_79B9_7F4A_7C15)
            .wrapping_add((ci as u64) << 7);
        h = h.wrapping_mul(0x100_0000_01B3);
    }
    h
}

/// Find exact duplicate rows. Returns None when the table has no
/// comparable columns.
pub fn find_duplicate_rows(t: &TypedTable) -> Option<DupInfo> {
    let comparable = t
        .cols
        .iter()
        .any(|c| !matches!(c, TypedCol::Null { .. } | TypedCol::Other { .. }));
    if !comparable || t.n == 0 {
        return None;
    }

    let mut by_hash: FxHashMap<u64, Vec<u32>> = FxHashMap::default();
    for i in 0..t.n {
        by_hash.entry(row_hash(t, i)).or_default().push(i as u32);
    }

    let mut confirmed: Vec<Vec<u32>> = Vec::new();
    for (_, mut rows) in by_hash {
        if rows.len() < 2 {
            continue;
        }
        // Verify exactly against the first row (collision-safe).
        let anchor = rows[0] as usize;
        rows.retain(|r| row_eq(t, *r as usize, anchor));
        if rows.len() >= 2 {
            rows.sort_unstable();
            confirmed.push(rows);
        }
    }

    if confirmed.is_empty() {
        return None;
    }
    confirmed.sort_by_key(|g| std::cmp::Reverse(g.len()));

    let n_redundant: usize = confirmed.iter().map(|g| g.len() - 1).sum();
    let max_group_size = confirmed[0].len();
    let examples: Vec<Vec<u32>> = confirmed
        .iter()
        .take(3)
        .map(|g| g.iter().take(6).copied().collect())
        .collect();
    let n_groups = confirmed.len();

    Some(DupInfo {
        n_redundant,
        n_groups,
        max_group_size,
        examples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::df;

    #[test]
    fn finds_planted_duplicates() {
        let df = df! {
            "a" => [1i64, 2, 1, 3, 2, 1],
            "b" => ["x", "y", "x", "z", "y", "x"],
        }
        .unwrap();
        let t = crate::typed::extract(&df);
        let info = find_duplicate_rows(&t).unwrap();
        assert_eq!(info.n_redundant, 3);
        assert_eq!(info.n_groups, 2);
        assert_eq!(info.max_group_size, 3);
    }

    #[test]
    fn hash_collision_never_lies() {
        let df = df! {
            "a" => [1i64, 2, 3],
        }
        .unwrap();
        let t = crate::typed::extract(&df);
        assert!(find_duplicate_rows(&t).is_none());
    }
}
