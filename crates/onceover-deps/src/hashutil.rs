//! Hashing and encoding utilities for dependency discovery.
//!
//! Architecture: fast hash-based candidate discovery (per-column hash arrays,
//! combined per set), followed by *exact canonical verification* of anything
//! we emit — hashing can only cost us recall, never truth (P7).

use onceover_core::TypedCol;

/// Per-column fast hash arrays + null masks over a selected row subset.
pub struct ColHashes {
    pub m: usize,
    /// col-major: hashes[col][row] — meaningless where nulls[col][row].
    pub hashes: Vec<Vec<u64>>,
    pub nulls: Vec<Vec<bool>>,
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

/// Build hash arrays for the given columns over the given rows.
pub fn build_hashes(cols: &[&TypedCol], rows: &[u32]) -> ColHashes {
    let m = rows.len();
    let mut hashes = Vec::with_capacity(cols.len());
    let mut nulls = Vec::with_capacity(cols.len());
    for col in cols {
        let col = *col;
        let mut h = vec![0u64; m];
        let mut nul = vec![false; m];
        match col {
            TypedCol::I64 { vals } => {
                for (j, &r) in rows.iter().enumerate() {
                    match vals.at(r as usize) {
                        Some(v) => h[j] = v as u64,
                        None => nul[j] = true,
                    }
                }
            }
            TypedCol::F64 { vals } => {
                for (j, &r) in rows.iter().enumerate() {
                    match vals.at(r as usize) {
                        Some(v) => h[j] = v.to_bits(),
                        None => nul[j] = true,
                    }
                }
            }
            TypedCol::Str { vals } => {
                for (j, &r) in rows.iter().enumerate() {
                    match vals[r as usize] {
                        Some(s) => h[j] = fnv64(s.as_bytes()),
                        None => nul[j] = true,
                    }
                }
            }
            TypedCol::StrOwned { vals } => {
                for (j, &r) in rows.iter().enumerate() {
                    match &vals[r as usize] {
                        Some(s) => h[j] = fnv64(s.as_bytes()),
                        None => nul[j] = true,
                    }
                }
            }
            TypedCol::Bool { vals } => {
                for (j, &r) in rows.iter().enumerate() {
                    match vals[r as usize] {
                        Some(true) => h[j] = 1,
                        Some(false) => h[j] = 2,
                        None => nul[j] = true,
                    }
                }
            }
            TypedCol::Date { vals } => {
                for (j, &r) in rows.iter().enumerate() {
                    match vals[r as usize] {
                        Some(v) => h[j] = v as u64,
                        None => nul[j] = true,
                    }
                }
            }
            TypedCol::Datetime { vals } => {
                for (j, &r) in rows.iter().enumerate() {
                    match vals[r as usize] {
                        Some(v) => h[j] = v as u64,
                        None => nul[j] = true,
                    }
                }
            }
            TypedCol::Null { .. } | TypedCol::Other { .. } => {
                // Mark entirely null so all rows are excluded.
                for x in nul.iter_mut() {
                    *x = true;
                }
            }
        }
        hashes.push(h);
        nulls.push(nul);
    }
    ColHashes { m, hashes, nulls }
}

/// Combine member hashes into a set hash (order-sensitive by member index —
/// callers always pass sorted indices, so it is deterministic).
#[inline]
pub fn combine(hs: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for x in hs {
        h = (h ^ x).wrapping_mul(0x100_0000_01B3);
    }
    h
}

/// Append a column value's canonical encoding (exact bytes) for verification.
pub fn encode_value(col: &TypedCol, row: usize, out: &mut Vec<u8>) {
    macro_rules! push_u64 {
        ($v:expr) => {
            out.extend_from_slice(&$v.to_le_bytes());
        };
    }
    match col {
        TypedCol::I64 { vals } => match vals.at(row) {
            Some(v) => {
                out.push(0x01);
                push_u64!(v as u64);
            }
            None => out.push(0x00),
        },
        TypedCol::F64 { vals } => match vals.at(row) {
            Some(v) => {
                out.push(0x02);
                push_u64!(v.to_bits());
            }
            None => out.push(0x00),
        },
        TypedCol::Str { vals } => match vals[row] {
            Some(s) => {
                out.push(0x03);
                out.extend_from_slice(&(s.len() as u32).to_le_bytes());
                out.extend_from_slice(s.as_bytes());
            }
            None => out.push(0x00),
        },
        TypedCol::StrOwned { vals } => match &vals[row] {
            Some(s) => {
                out.push(0x03);
                out.extend_from_slice(&(s.len() as u32).to_le_bytes());
                out.extend_from_slice(s.as_bytes());
            }
            None => out.push(0x00),
        },
        TypedCol::Bool { vals } => match vals[row] {
            Some(true) => {
                out.push(0x04);
                out.push(1);
            }
            Some(false) => {
                out.push(0x04);
                out.push(0);
            }
            None => out.push(0x00),
        },
        TypedCol::Date { vals } => match vals[row] {
            Some(v) => {
                out.push(0x05);
                out.extend_from_slice(&v.to_le_bytes());
            }
            None => out.push(0x00),
        },
        TypedCol::Datetime { vals } => match vals[row] {
            Some(v) => {
                out.push(0x06);
                push_u64!(v as u64);
            }
            None => out.push(0x00),
        },
        TypedCol::Null { .. } | TypedCol::Other { .. } => out.push(0x00),
    }
}

/// Deterministic stratified subsample of row indices (0..n down to cap).
pub fn subsample_rows(n: usize, cap: usize) -> Vec<u32> {
    onceover_core::sample_rows(n, cap)
}

/// Deterministic pseudo-random seed (fixed — P5 determinism).
pub const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stride_sampling_bounds() {
        assert_eq!(subsample_rows(10, 100).len(), 10);
        let s = subsample_rows(1_000_000, 200_000);
        assert!(s.len() <= 200_000 && s.len() >= 200_000 / 2);
        assert!(s.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn combine_is_order_sensitive() {
        let a = combine(&[1, 2]);
        let b = combine(&[2, 1]);
        assert_ne!(a, b);
        assert_eq!(a, combine(&[1, 2]));
    }
}
