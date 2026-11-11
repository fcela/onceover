//! Research harness for the FAISS-similarity study recorded in
//! `docs/benchmarks/2026-11-11-faiss-similarity.md` (issue #259).
//!
//! This is not library code. It implements the frozen protocol of
//! that record exactly: Lloyd's k-means and the mini-batch e-means
//! update in Rust over the library's numeric extraction, on a
//! standardised float32 matrix that is shared byte-identically with
//! the FAISS runs, from shared initial centroids. It changes no
//! library behaviour, no finding, and no part of the default
//! detector set.
//!
//! Subcommands:
//!   prep <csv> <out-prefix>
//!       Read a CSV, extract with `onceover_core::typed::extract`,
//!       keep numeric columns (integer, unsigned, float; decimal and
//!       duration arrive as floats through the extraction), drop
//!       rows with a null in any numeric column (complete cases),
//!       drop constant columns, standardise each remaining column to
//!       mean 0 / population standard deviation 1, and write
//!       `<out-prefix>.bin` (matrix), `<out-prefix>_init.bin`
//!       (initial centroids) and `<out-prefix>_meta.json`.
//!   prep-synth <out-prefix>
//!       Generate the protocol's synthetic frame (1,000,000 x 32,
//!       mixture of 8 spherical Gaussians, seed 259001), standardise
//!       it the same way, and write the same three files.
//!   lloyd <matrix.bin> <init.bin> <out-centroids.bin>
//!       Lloyd's algorithm, k = 16, 25 epochs, full batch, no early
//!       stop. Prints the training wall time and a final inertia
//!       cross-check (the reported inertia is recomputed uniformly
//!       for every implementation by the study driver).
//!   emeans <matrix.bin> <init.bin> <out-centroids.bin>
//!       The e-means update of the protocol: 3 epochs, mini-batch
//!       1,024, sum-and-count exponential moving average with
//!       alpha0 = 0.1 cosine-annealed toward 0.1 * alpha0, initial
//!       mass 2^-16, dead-cluster revival every 5 batches, and a
//!       final pass replacing each centroid by the unweighted mean
//!       of the rows assigned to it during the last epoch.
//!   inertia <matrix.bin> <centroids.bin>
//!       One full assignment pass printing the total squared
//!       Euclidean distance to the nearest centroid.
//!
//! Matrix and centroid files share one layout: a 16-byte header of
//! two little-endian u64 (rows, columns) followed by row-major
//! little-endian float32 values.
//!
//! Deterministic choices where the protocol leaves a detail open:
//! - Initialisation indices come from a splitmix64 generator with
//!   seed 25916 driving a partial Fisher-Yates shuffle over row
//!   indices; the chosen rows of the standardised matrix are the
//!   initial centroids for every implementation.
//! - Assignment ties go to the lowest centroid index.
//! - Lloyd empty clusters keep their previous centroid; the number
//!   of empty-cluster epoch events is printed.
//! - E-means revival: a cluster whose mass falls below 0.1 x the
//!   mean mass is revived by splitting the largest-mass cluster.
//!   The two children are the parent's centroid perturbed by
//!   -/+ 1e-4 relative per coordinate (sign alternating by
//!   coordinate index, opposite signs for the two children), and
//!   the parent's mass is halved between them.
//! - E-means state (masses and centroids) is kept in float64; the
//!   written centroids are float32, as for every implementation.

use std::io::{Read, Write};
use std::time::Instant;

use onceover_core::typed::{extract, TypedCol};
use polars::prelude::*;
use rayon::prelude::*;

const K: usize = 16;
const LLOYD_EPOCHS: usize = 25;
const EMEANS_EPOCHS: usize = 3;
const EMEANS_BATCH: usize = 1024;
const EMEANS_ALPHA0: f64 = 0.1;
const EMEANS_INIT_MASS: f64 = 0.0000152587890625; // 2^-16
const INIT_SEED: u64 = 25916;
const SYNTH_SEED: u64 = 259001;
const SYNTH_ROWS: usize = 1_000_000;
const SYNTH_COLS: usize = 32;
const SYNTH_COMPONENTS: usize = 8;

// ---------------------------------------------------------------- RNG

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1) with 53 bits of precision.
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

/// Standard normals via Box-Muller, one value at a time with the
/// second value of each pair cached. Draw order is part of the
/// synthetic frame's definition.
struct NormalGen {
    rng: SplitMix64,
    spare: Option<f64>,
}

impl NormalGen {
    fn next_normal(&mut self) -> f64 {
        if let Some(v) = self.spare.take() {
            return v;
        }
        let u1 = 1.0 - self.rng.next_f64(); // in (0, 1]
        let u2 = self.rng.next_f64();
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f64::consts::PI * u2;
        self.spare = Some(r * theta.sin());
        r * theta.cos()
    }
}

// ------------------------------------------------------------ matrix IO

struct Matrix {
    n: usize,
    d: usize,
    data: Vec<f32>,
}

fn write_matrix(path: &str, n: usize, d: usize, data: &[f32]) {
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(&(n as u64).to_le_bytes()).unwrap();
    f.write_all(&(d as u64).to_le_bytes()).unwrap();
    let bytes = unsafe {
        std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
    };
    f.write_all(bytes).unwrap();
}

fn read_matrix(path: &str) -> Matrix {
    let mut f = std::fs::File::open(path).unwrap();
    let mut hdr = [0u8; 16];
    f.read_exact(&mut hdr).unwrap();
    let n = u64::from_le_bytes(hdr[0..8].try_into().unwrap()) as usize;
    let d = u64::from_le_bytes(hdr[8..16].try_into().unwrap()) as usize;
    let mut data = vec![0f32; n * d];
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u8, std::mem::size_of_val(&*data))
    };
    f.read_exact(bytes).unwrap();
    Matrix { n, d, data }
}

// ------------------------------------------------------- initialisation

/// k distinct row indices from a splitmix64(seed) partial
/// Fisher-Yates shuffle; the shared initialisation of the protocol.
fn initial_indices(n: usize, k: usize, seed: u64) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..n).collect();
    let mut rng = SplitMix64::new(seed);
    for i in 0..k {
        let j = i + (rng.next_u64() % ((n - i) as u64)) as usize;
        idx.swap(i, j);
    }
    idx.truncate(k);
    idx
}

fn initial_centroids(m: &Matrix) -> Vec<f32> {
    let idx = initial_indices(m.n, K, INIT_SEED);
    let mut c = vec![0f32; K * m.d];
    for (ci, &row) in idx.iter().enumerate() {
        c[ci * m.d..(ci + 1) * m.d].copy_from_slice(&m.data[row * m.d..(row + 1) * m.d]);
    }
    c
}

// ------------------------------------------------------------- distances

fn nearest(row: &[f32], centroids: &[f32], d: usize, k: usize) -> (usize, f32) {
    let mut best = 0usize;
    let mut best_dist = f32::INFINITY;
    for c in 0..k {
        let cen = &centroids[c * d..(c + 1) * d];
        let mut acc = 0f32;
        for j in 0..d {
            let diff = row[j] - cen[j];
            acc += diff * diff;
        }
        if acc < best_dist {
            best_dist = acc;
            best = c;
        }
    }
    (best, best_dist)
}

fn total_inertia(m: &Matrix, centroids: &[f32], k: usize) -> f64 {
    m.data
        .par_chunks(m.d)
        .map(|row| {
            let (_, dist) = nearest(row, centroids, m.d, k);
            dist as f64
        })
        .sum()
}

// ----------------------------------------------------------------- prep

fn numeric_columns(df: &DataFrame) -> (Vec<String>, Vec<Vec<Option<f64>>>) {
    let t = extract(df);
    let mut names = Vec::new();
    let mut cols: Vec<Vec<Option<f64>>> = Vec::new();
    for (name, col) in t.names.iter().zip(t.cols.iter()) {
        let values: Option<Vec<Option<f64>>> = match col {
            TypedCol::I64 { vals } => {
                Some((0..t.n).map(|i| vals.at(i).map(|v| v as f64)).collect())
            }
            TypedCol::U64 { vals } => {
                Some((0..t.n).map(|i| vals.at(i).map(|v| v as f64)).collect())
            }
            TypedCol::F64 { vals } => Some((0..t.n).map(|i| vals.at(i)).collect()),
            _ => None,
        };
        if let Some(v) = values {
            names.push(name.clone());
            cols.push(v);
        }
    }
    (names, cols)
}

/// Complete-case filter, constant-column drop, standardisation.
/// Returns the matrix plus the kept column names and the number of
/// complete rows.
fn build_matrix(
    names: &[String],
    cols: &[Vec<Option<f64>>],
    source_rows: usize,
) -> (Matrix, Vec<String>, usize) {
    let complete: Vec<usize> = (0..source_rows)
        .filter(|&r| cols.iter().all(|c| c[r].is_some()))
        .collect();
    let n = complete.len();
    assert!(n > K, "not enough complete rows: {n}");
    let mut kept_names = Vec::new();
    let mut kept: Vec<(f64, f64)> = Vec::new(); // (mean, std)
    let mut kept_idx = Vec::new();
    for (ci, col) in cols.iter().enumerate() {
        let mut mean = 0f64;
        for &r in &complete {
            mean += col[r].unwrap();
        }
        mean /= n as f64;
        let mut var = 0f64;
        for &r in &complete {
            let diff = col[r].unwrap() - mean;
            var += diff * diff;
        }
        let std = (var / n as f64).sqrt();
        if std > 0.0 {
            kept_names.push(names[ci].clone());
            kept.push((mean, std));
            kept_idx.push(ci);
        }
    }
    let d = kept.len();
    assert!(d > 0, "no non-constant numeric columns");
    let mut data = vec![0f32; n * d];
    for (out_row, &r) in complete.iter().enumerate() {
        for (out_col, &ci) in kept_idx.iter().enumerate() {
            let (mean, std) = kept[out_col];
            data[out_row * d + out_col] = ((cols[ci][r].unwrap() - mean) / std) as f32;
        }
    }
    (Matrix { n, d, data }, kept_names, n)
}

fn write_prep(prefix: &str, m: &Matrix, meta: serde_json::Value) {
    write_matrix(&format!("{prefix}.bin"), m.n, m.d, &m.data);
    let init = initial_centroids(m);
    write_matrix(&format!("{prefix}_init.bin"), K, m.d, &init);
    let mut f = std::fs::File::create(format!("{prefix}_meta.json")).unwrap();
    f.write_all(serde_json::to_string_pretty(&meta).unwrap().as_bytes())
        .unwrap();
}

/// Parse a corpus CSV into a DataFrame. The workspace polars
/// build does not enable polars' CSV reader, so the harness
/// carries a small parser for the corpus files (comma-separated,
/// optional double-quoted fields, LF or CRLF endings) and types
/// each column from its values: integer when every non-empty
/// value parses as an integer, float when every one parses as a
/// float, string otherwise — the inference a CSV reader yields on
/// these files. The frame then enters through
/// `onceover_core::typed::extract`, as the protocol specifies.
fn read_corpus_csv(path: &str) -> DataFrame {
    let text = std::fs::read_to_string(path).unwrap();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut field = String::new();
    let mut row: Vec<String> = Vec::new();
    let mut chars = text.chars().peekable();
    let mut in_quotes = false;
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
        } else {
            match ch {
                '"' => in_quotes = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                '\r' => {}
                _ => field.push(ch),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    let header = rows.remove(0);
    let width = header.len();
    let mut columns: Vec<Series> = Vec::with_capacity(width);
    for (ci, name) in header.iter().enumerate() {
        let raw: Vec<&str> = rows
            .iter()
            .map(|r| r.get(ci).map(|s| s.as_str()).unwrap_or(""))
            .collect();
        let series = if raw
            .iter()
            .all(|v| v.is_empty() || v.trim().parse::<i64>().is_ok())
        {
            Series::new(
                name.into(),
                raw.iter()
                    .map(|v| {
                        if v.is_empty() {
                            None
                        } else {
                            v.trim().parse::<i64>().ok()
                        }
                    })
                    .collect::<Vec<Option<i64>>>(),
            )
        } else if raw
            .iter()
            .all(|v| v.is_empty() || v.trim().parse::<f64>().is_ok())
        {
            Series::new(
                name.into(),
                raw.iter()
                    .map(|v| {
                        if v.is_empty() {
                            None
                        } else {
                            v.trim().parse::<f64>().ok()
                        }
                    })
                    .collect::<Vec<Option<f64>>>(),
            )
        } else {
            Series::new(
                name.into(),
                raw.iter()
                    .map(|v| {
                        if v.is_empty() {
                            None
                        } else {
                            Some(v.to_string())
                        }
                    })
                    .collect::<Vec<Option<String>>>(),
            )
        };
        columns.push(series);
    }
    let height = rows.len();
    DataFrame::new(height, columns.into_iter().map(Column::from).collect()).unwrap()
}

fn prep(csv: &str, prefix: &str) {
    let df = read_corpus_csv(csv);
    let source_rows = df.height();
    let (names, cols) = numeric_columns(&df);
    let (m, kept, complete) = build_matrix(&names, &cols, source_rows);
    let meta = serde_json::json!({
        "source_csv": csv,
        "source_rows": source_rows,
        "numeric_columns": names,
        "kept_columns": kept,
        "complete_rows": complete,
        "n": m.n,
        "d": m.d,
        "k": K,
        "init_seed": INIT_SEED,
    });
    write_prep(prefix, &m, meta);
    println!(
        "prep source_rows={} numeric={} kept={} complete_rows={} n={} d={}",
        source_rows,
        names.len(),
        kept.len(),
        complete,
        m.n,
        m.d
    );
}

fn prep_synth(prefix: &str) {
    // Centres first (8 x 32 normals, scaled by 3), then per row a
    // uniform component draw followed by 32 noise normals.
    let mut gen = NormalGen {
        rng: SplitMix64::new(SYNTH_SEED),
        spare: None,
    };
    let mut centres = vec![0f64; SYNTH_COMPONENTS * SYNTH_COLS];
    for v in centres.iter_mut() {
        *v = 3.0 * gen.next_normal();
    }
    let mut raw = vec![0f32; SYNTH_ROWS * SYNTH_COLS];
    for r in 0..SYNTH_ROWS {
        let comp = (gen.rng.next_f64() * SYNTH_COMPONENTS as f64) as usize;
        for j in 0..SYNTH_COLS {
            raw[r * SYNTH_COLS + j] = (centres[comp * SYNTH_COLS + j] + gen.next_normal()) as f32;
        }
    }
    // Standardise exactly as the corpus matrices are standardised.
    let mut data = raw;
    for j in 0..SYNTH_COLS {
        let mut mean = 0f64;
        for r in 0..SYNTH_ROWS {
            mean += data[r * SYNTH_COLS + j] as f64;
        }
        mean /= SYNTH_ROWS as f64;
        let mut var = 0f64;
        for r in 0..SYNTH_ROWS {
            let diff = data[r * SYNTH_COLS + j] as f64 - mean;
            var += diff * diff;
        }
        let std = (var / SYNTH_ROWS as f64).sqrt();
        for r in 0..SYNTH_ROWS {
            data[r * SYNTH_COLS + j] = ((data[r * SYNTH_COLS + j] as f64 - mean) / std) as f32;
        }
    }
    let m = Matrix {
        n: SYNTH_ROWS,
        d: SYNTH_COLS,
        data,
    };
    let meta = serde_json::json!({
        "source_csv": null,
        "generation": "mixture of 8 spherical Gaussians in 32 dimensions; centres normal with standard deviation 3 per coordinate; rows assigned to a component uniformly; values centre plus standard normal noise; columns then standardised to mean 0 / population standard deviation 1",
        "seed": SYNTH_SEED,
        "source_rows": SYNTH_ROWS,
        "complete_rows": SYNTH_ROWS,
        "n": m.n,
        "d": m.d,
        "k": K,
        "init_seed": INIT_SEED,
    });
    write_prep(prefix, &m, meta);
    println!("prep-synth n={} d={}", m.n, m.d);
}

// ----------------------------------------------------------------- lloyd

fn lloyd(matrix_path: &str, init_path: &str, out_path: &str) {
    let m = read_matrix(matrix_path);
    let init = read_matrix(init_path);
    assert_eq!(init.n, K);
    assert_eq!(init.d, m.d);
    let mut centroids = init.data.clone();
    let start = Instant::now();
    let mut empty_events = 0usize;
    for _epoch in 0..LLOYD_EPOCHS {
        // Assignment + sums, in fixed row chunks merged in order so
        // the result is deterministic under parallelism.
        let chunk_rows = 8192usize;
        let n_chunks = m.n.div_ceil(chunk_rows);
        let partials: Vec<(Vec<f64>, Vec<u64>)> = (0..n_chunks)
            .into_par_iter()
            .map(|ch| {
                let lo = ch * chunk_rows;
                let hi = ((ch + 1) * chunk_rows).min(m.n);
                let mut sums = vec![0f64; K * m.d];
                let mut counts = vec![0u64; K];
                for r in lo..hi {
                    let row = &m.data[r * m.d..(r + 1) * m.d];
                    let (c, _) = nearest(row, &centroids, m.d, K);
                    counts[c] += 1;
                    for j in 0..m.d {
                        sums[c * m.d + j] += row[j] as f64;
                    }
                }
                (sums, counts)
            })
            .collect();
        let mut sums = vec![0f64; K * m.d];
        let mut counts = [0u64; K];
        for (ps, pc) in &partials {
            for i in 0..K * m.d {
                sums[i] += ps[i];
            }
            for c in 0..K {
                counts[c] += pc[c];
            }
        }
        for c in 0..K {
            if counts[c] == 0 {
                empty_events += 1;
                continue; // keep the previous centroid
            }
            for j in 0..m.d {
                centroids[c * m.d + j] = (sums[c * m.d + j] / counts[c] as f64) as f32;
            }
        }
    }
    let train_seconds = start.elapsed().as_secs_f64();
    write_matrix(out_path, K, m.d, &centroids);
    let inertia = total_inertia(&m, &centroids, K);
    println!(
        "lloyd n={} d={} epochs={} train_seconds={:.4} empty_events={} inertia={:.4}",
        m.n, m.d, LLOYD_EPOCHS, train_seconds, empty_events, inertia
    );
}

// ---------------------------------------------------------------- emeans

fn emeans(matrix_path: &str, init_path: &str, out_path: &str) {
    let m = read_matrix(matrix_path);
    let init = read_matrix(init_path);
    assert_eq!(init.n, K);
    assert_eq!(init.d, m.d);
    let d = m.d;
    let mut centroids: Vec<f64> = init.data.iter().map(|&v| v as f64).collect();
    let mut mass = [EMEANS_INIT_MASS; K];
    let batches_per_epoch = m.n.div_ceil(EMEANS_BATCH);
    let total_batches = batches_per_epoch * EMEANS_EPOCHS;
    let alpha_end = EMEANS_ALPHA0 * EMEANS_ALPHA0; // 0.1 * alpha0 = 0.01
    let start = Instant::now();
    let mut revivals = 0usize;
    // Last-epoch assignment sums for the final pass.
    let mut last_sums = vec![0f64; K * d];
    let mut last_counts = [0u64; K];
    for epoch in 0..EMEANS_EPOCHS {
        for b in 0..batches_per_epoch {
            let global_batch = epoch * batches_per_epoch + b;
            let alpha = alpha_end
                + 0.5
                    * (EMEANS_ALPHA0 - alpha_end)
                    * (1.0
                        + (std::f64::consts::PI * global_batch as f64 / total_batches as f64)
                            .cos());
            let lo = b * EMEANS_BATCH;
            let hi = ((b + 1) * EMEANS_BATCH).min(m.n);
            // Assign the batch against the current centroids.
            let cen32: Vec<f32> = centroids.iter().map(|&v| v as f32).collect();
            let mut sums = vec![0f64; K * d];
            let mut counts = [0u64; K];
            for r in lo..hi {
                let row = &m.data[r * d..(r + 1) * d];
                let (c, _) = nearest(row, &cen32, d, K);
                counts[c] += 1;
                for j in 0..d {
                    sums[c * d + j] += row[j] as f64;
                }
                if epoch == EMEANS_EPOCHS - 1 {
                    last_counts[c] += 1;
                    for j in 0..d {
                        last_sums[c * d + j] += row[j] as f64;
                    }
                }
            }
            // Sum-and-count EMA update.
            for c in 0..K {
                let mc = counts[c] as f64;
                let new_mass = (1.0 - alpha) * mass[c] + alpha * mc;
                if counts[c] > 0 {
                    let step = alpha * mc / new_mass;
                    for j in 0..d {
                        let batch_mean = sums[c * d + j] / mc;
                        centroids[c * d + j] += step * (batch_mean - centroids[c * d + j]);
                    }
                }
                mass[c] = new_mass;
            }
            // Dead-cluster revival every 5 batches.
            if (global_batch + 1).is_multiple_of(5) {
                let mean_mass: f64 = mass.iter().sum::<f64>() / K as f64;
                for c in 0..K {
                    if mass[c] < 0.1 * mean_mass {
                        let parent = (0..K)
                            .max_by(|&a, &b| mass[a].partial_cmp(&mass[b]).unwrap())
                            .unwrap();
                        if parent == c {
                            continue;
                        }
                        for j in 0..d {
                            let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
                            let parent_val = centroids[parent * d + j];
                            centroids[c * d + j] = parent_val * (1.0 + sign * 1e-4);
                            centroids[parent * d + j] = parent_val * (1.0 - sign * 1e-4);
                        }
                        mass[c] = mass[parent] / 2.0;
                        mass[parent] /= 2.0;
                        revivals += 1;
                    }
                }
            }
        }
    }
    // Final pass: unweighted mean of the rows assigned during the
    // last epoch; clusters with no such rows keep the EMA centroid.
    for c in 0..K {
        if last_counts[c] > 0 {
            for j in 0..d {
                centroids[c * d + j] = last_sums[c * d + j] / last_counts[c] as f64;
            }
        }
    }
    let train_seconds = start.elapsed().as_secs_f64();
    let out: Vec<f32> = centroids.iter().map(|&v| v as f32).collect();
    write_matrix(out_path, K, d, &out);
    let inertia = total_inertia(&m, &out, K);
    println!(
        "emeans n={} d={} epochs={} batch={} train_seconds={:.4} revivals={} inertia={:.4}",
        m.n, d, EMEANS_EPOCHS, EMEANS_BATCH, train_seconds, revivals, inertia
    );
}

// ------------------------------------------------------------------ main

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args[1].as_str() {
        "prep" => prep(&args[2], &args[3]),
        "prep-synth" => prep_synth(&args[2]),
        "lloyd" => lloyd(&args[2], &args[3], &args[4]),
        "emeans" => emeans(&args[2], &args[3], &args[4]),
        "inertia" => {
            let m = read_matrix(&args[2]);
            let c = read_matrix(&args[3]);
            assert_eq!(c.d, m.d);
            println!("inertia={:.6}", total_inertia(&m, &c.data, c.n));
        }
        other => panic!("unknown subcommand: {other}"),
    }
}
