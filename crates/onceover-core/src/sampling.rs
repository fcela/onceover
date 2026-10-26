//! Deterministic stratified row sampling without periodic stride aliasing.

pub fn sample_rows(n: usize, cap: usize) -> Vec<u32> {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samples_cover_all_strata_without_periodic_aliasing() {
        let rows = sample_rows(100_000, 1000);
        assert_eq!(rows, sample_rows(100_000, 1000));
        assert_eq!(rows.len(), 1000);
        assert!(rows.windows(2).all(|r| r[0] < r[1]));
        assert!(rows.iter().any(|r| r % 100 != 0));
        for (stratum, &row) in rows.iter().enumerate() {
            assert_eq!(row as usize / 100, stratum);
        }
        assert!(sample_rows(10, 0).is_empty());
    }
}
