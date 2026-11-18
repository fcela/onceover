//! Benchmark lab for the views deep look (issues #359, #360).
//!
//! Ignored tests: they build planted-partition tables whose true
//! column grouping is known by construction, run the pass's search
//! with its trace enabled, attribute every split planted pair to the
//! code path that split it, and sweep the criterion (gamma, merge
//! attempts) and the search sample. Results are written as JSON to
//! the directory named by `VIEWS_LAB_OUT` (default:
//! `~/workspace/onceover-pkg/lace-lab/out`). Nothing here runs in
//! the normal test suite.
//!
//! Run with:
//! `cargo test -p onceover-mines --release views_lab -- --ignored --nocapture --test-threads=1`

#![allow(dead_code)]

use super::*;
use onceover_core::{NumericValues, TypedCol, TypedTable};
use std::io::Write;
use std::time::Instant;

// ---------------------------------------------------------------- RNG

/// Splitmix64: small deterministic RNG for fixture generation.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn bernoulli(&mut self, p: f64) -> bool {
        self.unit() < p
    }
    fn normal(&mut self) -> f64 {
        // Box-Muller.
        let u1 = self.unit().max(1e-300);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

// ---------------------------------------------------------------- fixtures

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BinEnc {
    /// Bool columns: the pass's categorical path.
    Bool,
    /// 0/1 integer columns: the pass's continuous path (the encoding
    /// the Lace Animals features take through the profiler).
    I64,
}

#[derive(Clone, Debug)]
enum Block {
    /// Binary block whose columns share a row partition and agree
    /// with it in the same direction: cluster 0 draws at `p_lo`,
    /// cluster 1 at `p_hi` (a third cluster, if any, at the midpoint).
    /// Pairwise dependence is obvious by construction.
    BinaryAligned {
        cols: usize,
        clusters: usize,
        p_lo: f64,
        p_hi: f64,
        enc: BinEnc,
    },
    /// Binary block whose columns share a 2-cluster row partition
    /// but each column's rates are only 0.5 -/+ `delta`: pairwise
    /// dependence is weak, the block's joint signal is strong.
    BinaryWeakJoint {
        cols: usize,
        delta: f64,
        enc: BinEnc,
    },
    /// Three binary columns, third = XOR of the first two: pairwise
    /// independent, jointly deterministic.
    Xor { enc: BinEnc },
    /// Continuous block from one shared latent factor: pairwise
    /// correlation `loading^2`.
    ContinuousFactor { cols: usize, loading: f64 },
    /// Continuous block from a shared 2-cluster row partition: each
    /// column's mean shifts by `sep` (in sd units) between clusters,
    /// same direction for every column in the block.
    ContinuousMixture { cols: usize, sep: f64 },
}

#[derive(Clone, Copy, Debug)]
enum NoiseCol {
    Binary(BinEnc),
    Continuous,
}

struct Planted {
    table: TypedTable<'static>,
    /// Column names per planted block.
    blocks: Vec<Vec<String>>,
    /// Independent columns (planted in no block).
    noise: Vec<String>,
    /// CSV rendering (for the lace harness): header + rows.
    csv: String,
    /// Column kinds for the lace codebook: name -> "cat2" | "cont".
    kinds: Vec<(String, &'static str)>,
}

fn bin_col(vals: Vec<Option<bool>>, enc: BinEnc) -> (TypedCol<'static>, &'static str) {
    match enc {
        BinEnc::Bool => (TypedCol::Bool { vals }, "cat2"),
        BinEnc::I64 => (
            TypedCol::I64 {
                vals: NumericValues::Owned(
                    vals.into_iter()
                        .map(|v| v.map(|b| if b { 1i64 } else { 0i64 }))
                        .collect(),
                ),
            },
            "cat2",
        ),
    }
}

/// Balanced random cluster assignment: cluster sizes differ by at
/// most one, order shuffled.
fn balanced_clusters(rng: &mut Rng, rows: usize, k: usize) -> Vec<usize> {
    let mut out: Vec<usize> = (0..rows).map(|r| r % k).collect();
    rng.shuffle(&mut out);
    out
}

fn plant(name: &str, rows: usize, seed: u64, blocks: &[Block], noise: &[NoiseCol]) -> Planted {
    let mut rng = Rng(seed);
    let mut names: Vec<String> = Vec::new();
    let mut cols: Vec<TypedCol<'static>> = Vec::new();
    let mut kinds: Vec<(String, &'static str)> = Vec::new();
    let mut csv_cols: Vec<Vec<String>> = Vec::new();
    let mut truth: Vec<Vec<String>> = Vec::new();
    for (bi, block) in blocks.iter().enumerate() {
        let mut block_names = Vec::new();
        match block {
            Block::BinaryAligned {
                cols: nc,
                clusters,
                p_lo,
                p_hi,
                enc,
            } => {
                let cl = balanced_clusters(&mut rng, rows, *clusters);
                let rates: Vec<f64> = (0..*clusters)
                    .map(|c| {
                        if *clusters == 1 {
                            *p_lo
                        } else {
                            p_lo + (p_hi - p_lo) * (c as f64 / (*clusters - 1) as f64)
                        }
                    })
                    .collect();
                for j in 0..*nc {
                    let cname = format!("{name}_b{bi}_{j}");
                    let vals: Vec<Option<bool>> = (0..rows)
                        .map(|r| Some(rng.bernoulli(rates[cl[r]])))
                        .collect();
                    let (col, kind) = bin_col(vals.clone(), *enc);
                    names.push(cname.clone());
                    cols.push(col);
                    kinds.push((cname.clone(), kind));
                    csv_cols.push(
                        vals.iter()
                            .map(|v| match v {
                                Some(true) => "1".to_string(),
                                Some(false) => "0".to_string(),
                                None => String::new(),
                            })
                            .collect(),
                    );
                    block_names.push(cname);
                }
            }
            Block::BinaryWeakJoint {
                cols: nc,
                delta,
                enc,
            } => {
                let cl = balanced_clusters(&mut rng, rows, 2);
                for j in 0..*nc {
                    let cname = format!("{name}_b{bi}_{j}");
                    let vals: Vec<Option<bool>> = (0..rows)
                        .map(|r| {
                            let p = if cl[r] == 0 { 0.5 - delta } else { 0.5 + delta };
                            Some(rng.bernoulli(p))
                        })
                        .collect();
                    let (col, kind) = bin_col(vals.clone(), *enc);
                    names.push(cname.clone());
                    cols.push(col);
                    kinds.push((cname.clone(), kind));
                    csv_cols.push(
                        vals.iter()
                            .map(|v| match v {
                                Some(true) => "1".to_string(),
                                Some(false) => "0".to_string(),
                                None => String::new(),
                            })
                            .collect(),
                    );
                    block_names.push(cname);
                }
            }
            Block::Xor { enc } => {
                let x1: Vec<bool> = (0..rows).map(|_| rng.bernoulli(0.5)).collect();
                let x2: Vec<bool> = (0..rows).map(|_| rng.bernoulli(0.5)).collect();
                for j in 0..3 {
                    let cname = format!("{name}_b{bi}_{j}");
                    let vals: Vec<Option<bool>> = (0..rows)
                        .map(|r| {
                            Some(match j {
                                0 => x1[r],
                                1 => x2[r],
                                _ => x1[r] ^ x2[r],
                            })
                        })
                        .collect();
                    let (col, kind) = bin_col(vals.clone(), *enc);
                    names.push(cname.clone());
                    cols.push(col);
                    kinds.push((cname.clone(), kind));
                    csv_cols.push(
                        vals.iter()
                            .map(|v| match v {
                                Some(true) => "1".to_string(),
                                Some(false) => "0".to_string(),
                                None => String::new(),
                            })
                            .collect(),
                    );
                    block_names.push(cname);
                }
            }
            Block::ContinuousFactor { cols: nc, loading } => {
                let f: Vec<f64> = (0..rows).map(|_| rng.normal()).collect();
                let resid = (1.0 - loading * loading).sqrt();
                for j in 0..*nc {
                    let cname = format!("{name}_b{bi}_{j}");
                    let vals: Vec<Option<f64>> = (0..rows)
                        .map(|r| Some(loading * f[r] + resid * rng.normal()))
                        .collect();
                    names.push(cname.clone());
                    cols.push(TypedCol::F64 {
                        vals: NumericValues::Owned(vals.clone()),
                    });
                    kinds.push((cname.clone(), "cont"));
                    csv_cols.push(
                        vals.iter()
                            .map(|v| v.map(|x| format!("{x:.6}")).unwrap_or_default())
                            .collect(),
                    );
                    block_names.push(cname);
                }
            }
            Block::ContinuousMixture { cols: nc, sep } => {
                let cl = balanced_clusters(&mut rng, rows, 2);
                for j in 0..*nc {
                    let cname = format!("{name}_b{bi}_{j}");
                    let vals: Vec<Option<f64>> = (0..rows)
                        .map(|r| {
                            let m = if cl[r] == 0 { -sep / 2.0 } else { sep / 2.0 };
                            Some(m + rng.normal())
                        })
                        .collect();
                    names.push(cname.clone());
                    cols.push(TypedCol::F64 {
                        vals: NumericValues::Owned(vals.clone()),
                    });
                    kinds.push((cname.clone(), "cont"));
                    csv_cols.push(
                        vals.iter()
                            .map(|v| v.map(|x| format!("{x:.6}")).unwrap_or_default())
                            .collect(),
                    );
                    block_names.push(cname);
                }
            }
        }
        truth.push(block_names);
    }
    let mut noise_names = Vec::new();
    for (j, nc) in noise.iter().enumerate() {
        let cname = format!("{name}_noise{j}");
        match nc {
            NoiseCol::Binary(enc) => {
                let vals: Vec<Option<bool>> = (0..rows).map(|_| Some(rng.bernoulli(0.5))).collect();
                let (col, kind) = bin_col(vals.clone(), *enc);
                names.push(cname.clone());
                cols.push(col);
                kinds.push((cname.clone(), kind));
                csv_cols.push(
                    vals.iter()
                        .map(|v| match v {
                            Some(true) => "1".to_string(),
                            Some(false) => "0".to_string(),
                            None => String::new(),
                        })
                        .collect(),
                );
            }
            NoiseCol::Continuous => {
                let vals: Vec<Option<f64>> = (0..rows).map(|_| Some(rng.normal())).collect();
                names.push(cname.clone());
                cols.push(TypedCol::F64 {
                    vals: NumericValues::Owned(vals.clone()),
                });
                kinds.push((cname.clone(), "cont"));
                csv_cols.push(
                    vals.iter()
                        .map(|v| v.map(|x| format!("{x:.6}")).unwrap_or_default())
                        .collect(),
                );
            }
        }
        noise_names.push(cname);
    }
    let dtypes: Vec<String> = kinds
        .iter()
        .map(|(_, k)| {
            if *k == "cat2" {
                "int64".to_string()
            } else {
                "float64".to_string()
            }
        })
        .collect();
    let mut csv = String::new();
    csv.push_str(&names.join(","));
    csv.push('\n');
    for r in 0..rows {
        let row: Vec<&str> = csv_cols.iter().map(|c| c[r].as_str()).collect();
        csv.push_str(&row.join(","));
        csv.push('\n');
    }
    Planted {
        table: TypedTable {
            n: rows,
            names,
            dtypes,
            cols,
        },
        blocks: truth,
        noise: noise_names,
        csv,
        kinds,
    }
}

// ---------------------------------------------------------------- runner

struct LabRun {
    /// Views as column-name lists (post-cap columns only).
    partition: Vec<Vec<String>>,
    extraction_dropped: Vec<String>,
    cap_dropped: Vec<String>,
    /// Pre-cap column names in extraction order (screen indices).
    pre_cap_names: Vec<String>,
    /// Screen scores/partners/kept, in pre-cap order; empty when the
    /// cap did not bind.
    screen_scores: Vec<f64>,
    screen_partners: Vec<usize>,
    /// Post-cap column names in order (trace indices).
    kept_names: Vec<String>,
    trace: SearchTrace,
    work_hit: bool,
    search_ms: u128,
    total_ms: u128,
}

/// Replicates the front half of `views_pass` (sampling, extraction,
/// cap, search) with the traces turned on. The final assignment
/// phase of `views_pass` does not change view membership, so the
/// partition here is the pass's partition.
fn run_lab(table: &TypedTable, cfg: &ViewsConfig) -> LabRun {
    let t0 = Instant::now();
    let sample: Vec<usize> = sample_rows(table.n, cfg.row_cap)
        .iter()
        .map(|&r| r as usize)
        .collect();
    let n_sample = sample.len();
    let (all_cols, dropped) = extract_columns(table, &sample, cfg);
    let extraction_dropped = dropped.clone();
    let search_pos: Vec<usize> = sample_rows(n_sample, cfg.search_rows)
        .iter()
        .map(|&r| r as usize)
        .collect();
    let mut work = WorkBudget {
        used: 0,
        cap: cfg.work_budget,
        hit: false,
    };
    let pre_cap_names: Vec<String> = all_cols.iter().map(|c| c.name.clone()).collect();
    let mut screen = ScreenTrace {
        scores: Vec::new(),
        partners: Vec::new(),
        kept: Vec::new(),
    };
    let (cols, cap_dropped, _prescreened) =
        select_columns(all_cols, &search_pos, cfg, &mut work, Some(&mut screen));
    let kept_names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
    let mut trace = SearchTrace {
        single_groups: Vec::new(),
        screened_pairs: 0,
        total_pairs: 0,
        pairs: Vec::new(),
        attempts: 0,
        stop: StopReason::Exhausted,
    };
    let t1 = Instant::now();
    let (views, _search_stats) =
        views_search_traced(&cols, &search_pos, cfg, &mut work, Some(&mut trace));
    let search_ms = t1.elapsed().as_millis();
    let partition: Vec<Vec<String>> = views
        .iter()
        .map(|v| v.cols.iter().map(|&ci| kept_names[ci].clone()).collect())
        .collect();
    LabRun {
        partition,
        extraction_dropped,
        cap_dropped,
        pre_cap_names,
        screen_scores: screen.scores,
        screen_partners: screen.partners,
        kept_names,
        trace,
        work_hit: work.hit,
        search_ms,
        total_ms: t0.elapsed().as_millis(),
    }
}

// ---------------------------------------------------------------- scoring

fn view_of(run: &LabRun) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for (vi, v) in run.partition.iter().enumerate() {
        for name in v {
            m.insert(name.clone(), vi);
        }
    }
    m
}

/// Cause assigned to one split planted pair.
#[derive(Debug)]
enum SplitCause {
    ExtractionDrop(String),
    CapDrop(String),
    PairNeverScreened,
    RefusedScore {
        delta: f64,
        screen_gain: f64,
        rank: usize,
    },
    RefusedSingleCluster {
        screen_gain: f64,
        rank: usize,
    },
    NotVisitedCap {
        screen_gain: f64,
        rank: usize,
    },
    NotVisitedBudget,
    BudgetDuringEval,
    Inconsistent(String),
}

struct Score {
    planted_pairs: usize,
    recovered: usize,
    cross_pairs: usize,
    wrong_joined: usize,
    splits: Vec<(String, String, SplitCause)>,
    wrong: Vec<(String, String)>,
}

fn score(run: &LabRun, planted: &Planted) -> Score {
    let view = view_of(run);
    let mut sc = Score {
        planted_pairs: 0,
        recovered: 0,
        cross_pairs: 0,
        wrong_joined: 0,
        splits: Vec::new(),
        wrong: Vec::new(),
    };
    let kept_ix: BTreeMap<String, usize> = run
        .kept_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();
    let trace_by_pair: BTreeMap<(usize, usize), &PairTrace> = run
        .trace
        .pairs
        .iter()
        .map(|p| ((p.i.min(p.j), p.i.max(p.j)), p))
        .collect();
    let cause = |a: &str, b: &str| -> SplitCause {
        for x in [a, b] {
            if run.extraction_dropped.iter().any(|d| d == x) {
                return SplitCause::ExtractionDrop(x.to_string());
            }
            if run.cap_dropped.iter().any(|d| d == x) {
                return SplitCause::CapDrop(x.to_string());
            }
        }
        let (ia, ib) = (kept_ix[a], kept_ix[b]);
        let key = (ia.min(ib), ia.max(ib));
        match trace_by_pair.get(&key) {
            None => SplitCause::PairNeverScreened,
            Some(p) => match p.outcome {
                PairOutcome::RefusedScore => SplitCause::RefusedScore {
                    delta: p.delta.unwrap_or(f64::NAN),
                    screen_gain: p.screen_gain,
                    rank: p.rank,
                },
                PairOutcome::RefusedSingleCluster => SplitCause::RefusedSingleCluster {
                    screen_gain: p.screen_gain,
                    rank: p.rank,
                },
                PairOutcome::NotVisited => match run.trace.stop {
                    StopReason::AttemptsCap => SplitCause::NotVisitedCap {
                        screen_gain: p.screen_gain,
                        rank: p.rank,
                    },
                    _ => SplitCause::NotVisitedBudget,
                },
                PairOutcome::BudgetDuringEval => SplitCause::BudgetDuringEval,
                PairOutcome::Merged | PairOutcome::AlreadySameView => {
                    SplitCause::Inconsistent(format!("outcome {:?}", p.outcome))
                }
            },
        }
    };
    for block in &planted.blocks {
        for x in 0..block.len() {
            for y in (x + 1)..block.len() {
                sc.planted_pairs += 1;
                let (a, b) = (&block[x], &block[y]);
                match (view.get(a), view.get(b)) {
                    (Some(va), Some(vb)) if va == vb => sc.recovered += 1,
                    _ => sc.splits.push((a.clone(), b.clone(), cause(a, b))),
                }
            }
        }
    }
    // Cross pairs: columns in different blocks, or block vs noise,
    // or noise vs noise. Wrong join = same view.
    let mut all: Vec<(&String, Option<usize>)> = Vec::new();
    for (bi, block) in planted.blocks.iter().enumerate() {
        for n in block {
            all.push((n, Some(bi)));
        }
    }
    for n in &planted.noise {
        all.push((n, None));
    }
    for x in 0..all.len() {
        for y in (x + 1)..all.len() {
            if all[x].1.is_some() && all[x].1 == all[y].1 {
                continue;
            }
            sc.cross_pairs += 1;
            let (a, b) = (all[x].0, all[y].0);
            if let (Some(va), Some(vb)) = (view.get(a), view.get(b)) {
                if va == vb {
                    sc.wrong_joined += 1;
                    sc.wrong.push((a.clone(), b.clone()));
                }
            }
        }
    }
    sc
}

fn cause_key(c: &SplitCause) -> &'static str {
    match c {
        SplitCause::ExtractionDrop(_) => "extraction_drop",
        SplitCause::CapDrop(_) => "cap_drop",
        SplitCause::PairNeverScreened => "pair_never_screened",
        SplitCause::RefusedScore { .. } => "refused_score",
        SplitCause::RefusedSingleCluster { .. } => "refused_single_cluster",
        SplitCause::NotVisitedCap { .. } => "not_visited_attempts_cap",
        SplitCause::NotVisitedBudget => "not_visited_budget",
        SplitCause::BudgetDuringEval => "budget_during_eval",
        SplitCause::Inconsistent(_) => "inconsistent",
    }
}

fn out_dir() -> std::path::PathBuf {
    let d = std::env::var("VIEWS_LAB_OUT").unwrap_or_else(|_| {
        format!(
            "{}/workspace/onceover-pkg/lace-lab/out",
            std::env::var("HOME").unwrap()
        )
    });
    let p = std::path::PathBuf::from(d);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_json(file: &str, v: &serde_json::Value) {
    let p = out_dir().join(file);
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(serde_json::to_string_pretty(v).unwrap().as_bytes())
        .unwrap();
    println!("wrote {}", p.display());
}

fn score_json(run: &LabRun, sc: &Score) -> serde_json::Value {
    let mut by_cause: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, _, c) in &sc.splits {
        *by_cause.entry(cause_key(c)).or_default() += 1;
    }
    json!({
        "planted_pairs": sc.planted_pairs,
        "recovered": sc.recovered,
        "cross_pairs": sc.cross_pairs,
        "wrong_joined": sc.wrong_joined,
        "splits_by_cause": by_cause,
        "attempts": run.trace.attempts,
        "stop": format!("{:?}", run.trace.stop),
        "screened_pairs": run.trace.screened_pairs,
        "total_pairs": run.trace.total_pairs,
        "work_hit": run.work_hit,
        "search_ms": run.search_ms,
        "total_ms": run.total_ms,
        "n_views": run.partition.len(),
        "multi_views": run.partition.iter().filter(|v| v.len() > 1).count(),
        "partition": run.partition,
        "extraction_dropped": run.extraction_dropped,
        "cap_dropped": run.cap_dropped,
    })
}

// ---------------------------------------------------------------- CSV

/// Minimal quote-aware CSV parser (quotes, embedded commas, doubled
/// quote escapes). Sufficient for the bundled corpus files.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    let mut field_started = false;
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
                '"' if !field_started => in_quotes = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                    field_started = false;
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    field_started = false;
                    rows.push(std::mem::take(&mut row));
                }
                '\r' => {}
                _ => {
                    field.push(ch);
                    field_started = true;
                }
            }
        }
    }
    if field_started || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    rows
}

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn read_corpus_csv(rel: &str) -> Vec<Vec<String>> {
    let p = repo_root().join(rel);
    let text = std::fs::read_to_string(&p).unwrap();
    parse_csv(&text)
}

/// Build a TypedTable from CSV rows the way the profiler's CSV
/// inference types them: a column is I64 when every non-empty cell
/// parses as an integer, F64 when every non-empty cell parses as a
/// float, else string. The first column named `id`/`ID` is a string.
fn table_from_csv(rows: &[Vec<String>]) -> TypedTable<'static> {
    let header = &rows[0];
    let body = &rows[1..];
    let n = body.len();
    let mut cols: Vec<TypedCol<'static>> = Vec::new();
    let mut dtypes = Vec::new();
    for (ci, h) in header.iter().enumerate() {
        let cells: Vec<Option<String>> = body
            .iter()
            .map(|r| {
                let v = r.get(ci).cloned().unwrap_or_default();
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            })
            .collect();
        let force_str = h == "id" || h == "ID";
        let all_int = !force_str && cells.iter().flatten().all(|v| v.parse::<i64>().is_ok());
        let all_float =
            !force_str && !all_int && cells.iter().flatten().all(|v| v.parse::<f64>().is_ok());
        if all_int {
            dtypes.push("int64".to_string());
            cols.push(TypedCol::I64 {
                vals: NumericValues::Owned(
                    cells
                        .iter()
                        .map(|v| v.as_ref().map(|s| s.parse::<i64>().unwrap()))
                        .collect(),
                ),
            });
        } else if all_float {
            dtypes.push("float64".to_string());
            cols.push(TypedCol::F64 {
                vals: NumericValues::Owned(
                    cells
                        .iter()
                        .map(|v| v.as_ref().map(|s| s.parse::<f64>().unwrap()))
                        .collect(),
                ),
            });
        } else {
            dtypes.push("string".to_string());
            cols.push(TypedCol::StrOwned { vals: cells });
        }
    }
    TypedTable {
        n,
        names: header.clone(),
        dtypes,
        cols,
    }
}

// ---------------------------------------------------------------- lab: planted battery

fn battery_tables(rows: usize) -> Vec<(String, Planted)> {
    let seed = 20261117u64;
    vec![
        (
            "bin_aligned_bool".to_string(),
            plant(
                "ba",
                rows,
                seed,
                &[
                    Block::BinaryAligned {
                        cols: 4,
                        clusters: 2,
                        p_lo: 0.15,
                        p_hi: 0.85,
                        enc: BinEnc::Bool,
                    },
                    Block::BinaryAligned {
                        cols: 4,
                        clusters: 2,
                        p_lo: 0.2,
                        p_hi: 0.8,
                        enc: BinEnc::Bool,
                    },
                    Block::BinaryAligned {
                        cols: 3,
                        clusters: 3,
                        p_lo: 0.15,
                        p_hi: 0.85,
                        enc: BinEnc::Bool,
                    },
                ],
                &[NoiseCol::Binary(BinEnc::Bool); 4],
            ),
        ),
        (
            "bin_aligned_i64".to_string(),
            plant(
                "bi",
                rows,
                seed + 1,
                &[
                    Block::BinaryAligned {
                        cols: 4,
                        clusters: 2,
                        p_lo: 0.15,
                        p_hi: 0.85,
                        enc: BinEnc::I64,
                    },
                    Block::BinaryAligned {
                        cols: 4,
                        clusters: 2,
                        p_lo: 0.2,
                        p_hi: 0.8,
                        enc: BinEnc::I64,
                    },
                    Block::BinaryAligned {
                        cols: 3,
                        clusters: 3,
                        p_lo: 0.15,
                        p_hi: 0.85,
                        enc: BinEnc::I64,
                    },
                ],
                &[NoiseCol::Binary(BinEnc::I64); 4],
            ),
        ),
        (
            "bin_weak_joint".to_string(),
            plant(
                "wj",
                rows,
                seed + 2,
                &[
                    Block::BinaryWeakJoint {
                        cols: 6,
                        delta: 0.10,
                        enc: BinEnc::Bool,
                    },
                    Block::BinaryWeakJoint {
                        cols: 6,
                        delta: 0.15,
                        enc: BinEnc::Bool,
                    },
                ],
                &[NoiseCol::Binary(BinEnc::Bool); 4],
            ),
        ),
        (
            "xor_bool".to_string(),
            plant(
                "xo",
                rows,
                seed + 3,
                &[
                    Block::Xor { enc: BinEnc::Bool },
                    Block::Xor { enc: BinEnc::Bool },
                ],
                &[NoiseCol::Binary(BinEnc::Bool); 3],
            ),
        ),
        (
            "cont_factor".to_string(),
            plant(
                "cf",
                rows,
                seed + 4,
                &[
                    Block::ContinuousFactor {
                        cols: 4,
                        loading: 0.45,
                    },
                    Block::ContinuousFactor {
                        cols: 4,
                        loading: 0.7,
                    },
                    Block::ContinuousFactor {
                        cols: 4,
                        loading: 0.9,
                    },
                ],
                &[NoiseCol::Continuous; 4],
            ),
        ),
        (
            "cont_mixture".to_string(),
            plant(
                "cm",
                rows,
                seed + 5,
                &[
                    Block::ContinuousMixture { cols: 4, sep: 1.0 },
                    Block::ContinuousMixture { cols: 4, sep: 2.0 },
                ],
                &[NoiseCol::Continuous; 4],
            ),
        ),
    ]
}

#[test]
#[ignore]
fn lab_planted_core() {
    let mut out: Vec<serde_json::Value> = Vec::new();
    for rows in [50usize, 500] {
        for (tname, planted) in battery_tables(rows) {
            for gamma in [0.0f64, 0.5, 1.0, 2.0, 4.0] {
                for cap in [256usize, usize::MAX] {
                    let cfg = ViewsConfig {
                        row_cap: rows,
                        search_rows: rows,
                        gamma,
                        tried_merges_cap: cap,
                        ..Default::default()
                    };
                    let run = run_lab(&planted.table, &cfg);
                    let sc = score(&run, &planted);
                    let mut e = score_json(&run, &sc);
                    e["table"] = json!(tname);
                    e["rows"] = json!(rows);
                    e["gamma"] = json!(gamma);
                    e["merges_cap"] = json!(if cap == usize::MAX { -1i64 } else { cap as i64 });
                    out.push(e);
                }
            }
        }
    }
    write_json("planted-core.json", &json!(out));
}

/// Write the planted tables (CSV + kinds) for the lace harness, and
/// the larger search-sample tables.
#[test]
#[ignore]
fn lab_write_planted_csvs() {
    let dir = out_dir().join("planted");
    std::fs::create_dir_all(&dir).unwrap();
    for (tname, planted) in battery_tables(500) {
        std::fs::write(dir.join(format!("{tname}.csv")), &planted.csv).unwrap();
        let kinds: Vec<serde_json::Value> = planted
            .kinds
            .iter()
            .map(|(n, k)| json!({"name": n, "kind": k}))
            .collect();
        let truth: Vec<serde_json::Value> = planted.blocks.iter().map(|b| json!(b)).collect();
        std::fs::write(
            dir.join(format!("{tname}.kinds.json")),
            serde_json::to_string_pretty(&json!({
                "kinds": kinds, "blocks": truth, "noise": planted.noise,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    println!("planted csvs written to {}", dir.display());
}

// ---------------------------------------------------------------- lab: Animals / Satellites

fn pair_trace_json(run: &LabRun, a: &str, b: &str) -> serde_json::Value {
    let kept_ix: BTreeMap<&str, usize> = run
        .kept_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let (ia, ib) = match (kept_ix.get(a), kept_ix.get(b)) {
        (Some(&x), Some(&y)) => (x, y),
        _ => {
            return json!({
                "pair": [a, b], "status": "column_not_kept",
                "a_kept": kept_ix.contains_key(a), "b_kept": kept_ix.contains_key(b),
                "a_cap_dropped": run.cap_dropped.iter().any(|d| d == a),
                "b_cap_dropped": run.cap_dropped.iter().any(|d| d == b),
                "a_extraction_dropped": run.extraction_dropped.iter().any(|d| d == a),
                "b_extraction_dropped": run.extraction_dropped.iter().any(|d| d == b),
            })
        }
    };
    let key = (ia.min(ib), ia.max(ib));
    match run
        .trace
        .pairs
        .iter()
        .find(|p| (p.i.min(p.j), p.i.max(p.j)) == key)
    {
        Some(p) => json!({
            "pair": [a, b], "status": "traced",
            "screen_gain": p.screen_gain, "rank": p.rank,
            "outcome": format!("{:?}", p.outcome),
            "merged_groups": p.merged_groups, "delta": p.delta,
        }),
        None => json!({"pair": [a, b], "status": "pair_never_screened"}),
    }
}

fn run_and_dump(
    label: &str,
    table: &TypedTable,
    cfgs: Vec<(String, ViewsConfig)>,
    named_pairs: Vec<(String, String)>,
) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for (clabel, cfg) in cfgs {
        let run = run_lab(table, &cfg);
        let screen: Vec<serde_json::Value> = run
            .pre_cap_names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                json!({
                    "name": n,
                    "screen_score": run.screen_scores.get(i),
                    "screen_partner": run.screen_partners.get(i)
                        .and_then(|&p| if p == usize::MAX { None } else { Some(run.pre_cap_names[p].clone()) }),
                    "kept": !run.cap_dropped.iter().any(|d| d == n)
                        && !run.extraction_dropped.iter().any(|d| d == n),
                })
            })
            .collect();
        let named: Vec<serde_json::Value> = named_pairs
            .iter()
            .map(|(a, b)| pair_trace_json(&run, a, b))
            .collect();
        out.push(json!({
            "label": label,
            "config": clabel,
            "gamma": cfg.gamma,
            "merges_cap": if cfg.tried_merges_cap == usize::MAX { -1i64 } else { cfg.tried_merges_cap as i64 },
            "search_rows": cfg.search_rows,
            "row_cap": cfg.row_cap,
            "partition": run.partition,
            "n_views": run.partition.len(),
            "multi_views": run.partition.iter().filter(|v| v.len() > 1).count(),
            "attempts": run.trace.attempts,
            "stop": format!("{:?}", run.trace.stop),
            "screened_pairs": run.trace.screened_pairs,
            "total_pairs": run.trace.total_pairs,
            "work_hit": run.work_hit,
            "search_ms": run.search_ms,
            "total_ms": run.total_ms,
            "extraction_dropped": run.extraction_dropped,
            "cap_dropped": run.cap_dropped,
            "screen": screen,
            "named_pairs": named,
            "all_pair_traces": run.trace.pairs.iter().map(|p| json!({
                "a": run.kept_names[p.i], "b": run.kept_names[p.j],
                "screen_gain": p.screen_gain, "rank": p.rank,
                "outcome": format!("{:?}", p.outcome),
                "merged_groups": p.merged_groups, "delta": p.delta,
            })).collect::<Vec<_>>(),
            "single_groups": run.kept_names.iter().enumerate().map(|(i, n)| json!({
                "name": n, "groups": run.trace.single_groups.get(i),
            })).collect::<Vec<_>>(),
        }));
    }
    out
}

#[test]
#[ignore]
fn lab_animals() {
    let rows = read_corpus_csv("benchmarks/corpus/data/crosscat/lace-animals/data.csv");
    let table = table_from_csv(&rows);
    let named: Vec<(String, String)> = [
        ("paws", "claws"),
        ("pads", "paws"),
        ("pads", "claws"),
        ("big", "small"),
        ("flippers", "swims"),
        ("flippers", "plankton"),
        ("flippers", "strainteeth"),
        ("meatteeth", "hunter"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    // Note: no "hunter" column exists in the table; the trace dump
    // records it as not kept, which is itself part of the record
    // (the head-to-head names the pair from Lace's side).
    let named: Vec<(String, String)> = named
        .into_iter()
        .filter(|(a, b)| table.names.contains(a) && table.names.contains(b))
        .collect();
    let mut cfgs: Vec<(String, ViewsConfig)> = vec![("default".into(), ViewsConfig::default())];
    for gamma in [0.0f64, 0.5, 1.0, 4.0] {
        cfgs.push((
            format!("gamma{gamma}"),
            ViewsConfig {
                gamma,
                ..Default::default()
            },
        ));
    }
    cfgs.push((
        "nocap".into(),
        ViewsConfig {
            tried_merges_cap: usize::MAX,
            ..Default::default()
        },
    ));
    cfgs.push((
        "gamma0_nocap".into(),
        ViewsConfig {
            gamma: 0.0,
            tried_merges_cap: usize::MAX,
            ..Default::default()
        },
    ));
    let out = run_and_dump("animals", &table, cfgs, named);
    write_json("animals.json", &json!(out));
}

#[test]
#[ignore]
fn lab_satellites() {
    let rows = read_corpus_csv("benchmarks/corpus/data/crosscat/lace-satellites/data.csv");
    let table = table_from_csv(&rows);
    let named: Vec<(String, String)> = [
        ("Apogee_km", "Class_of_Orbit"),
        ("Perigee_km", "Apogee_km"),
        ("Period_minutes", "Apogee_km"),
        ("Eccentricity", "Perigee_km"),
        ("Launch_Mass_kg", "Dry_Mass_kg"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    let mut cfgs: Vec<(String, ViewsConfig)> = vec![("default".into(), ViewsConfig::default())];
    for gamma in [0.0f64, 0.5, 1.0, 4.0] {
        cfgs.push((
            format!("gamma{gamma}"),
            ViewsConfig {
                gamma,
                ..Default::default()
            },
        ));
    }
    cfgs.push((
        "nocap".into(),
        ViewsConfig {
            tried_merges_cap: usize::MAX,
            ..Default::default()
        },
    ));
    let out = run_and_dump("satellites", &table, cfgs, named);
    write_json("satellites.json", &json!(out));
}

// ---------------------------------------------------------------- lab: search rows (#359)

fn big_tables() -> Vec<(String, Planted, usize)> {
    let seed = 20261117u64;
    vec![
        (
            "big_binary".to_string(),
            plant(
                "bb",
                100_000,
                seed + 11,
                &[
                    Block::BinaryAligned {
                        cols: 5,
                        clusters: 2,
                        p_lo: 0.2,
                        p_hi: 0.8,
                        enc: BinEnc::I64,
                    },
                    Block::BinaryAligned {
                        cols: 5,
                        clusters: 2,
                        p_lo: 0.15,
                        p_hi: 0.85,
                        enc: BinEnc::I64,
                    },
                    Block::BinaryWeakJoint {
                        cols: 6,
                        delta: 0.08,
                        enc: BinEnc::I64,
                    },
                    Block::BinaryAligned {
                        cols: 4,
                        clusters: 3,
                        p_lo: 0.15,
                        p_hi: 0.85,
                        enc: BinEnc::I64,
                    },
                ],
                &[NoiseCol::Binary(BinEnc::I64); 8],
            ),
            100_000,
        ),
        (
            "big_cont".to_string(),
            plant(
                "bc",
                100_000,
                seed + 12,
                &[
                    Block::ContinuousFactor {
                        cols: 5,
                        loading: 0.7,
                    },
                    Block::ContinuousFactor {
                        cols: 5,
                        loading: 0.5,
                    },
                    Block::ContinuousMixture { cols: 4, sep: 1.2 },
                    Block::ContinuousMixture { cols: 4, sep: 0.8 },
                ],
                &[NoiseCol::Continuous; 8],
            ),
            100_000,
        ),
    ]
}

#[test]
#[ignore]
fn lab_search_rows() {
    let mut out: Vec<serde_json::Value> = Vec::new();
    for (tname, planted, rows) in big_tables() {
        for sr in [2000usize, 8000, 32000, rows] {
            let cfg = ViewsConfig {
                row_cap: rows,
                search_rows: sr,
                ..Default::default()
            };
            let run = run_lab(&planted.table, &cfg);
            let sc = score(&run, &planted);
            let mut e = score_json(&run, &sc);
            e["table"] = json!(tname);
            e["rows"] = json!(rows);
            e["search_rows"] = json!(sr);
            out.push(e);
        }
        // 20,000-row variant of the same table shape: regenerate at
        // 20k rows with the same seed stream is not possible (the
        // generator consumes the RNG by row), so plant a fresh 20k
        // table with the same spec via a second call.
    }
    write_json("search-rows-100k.json", &json!(out));
}

#[test]
#[ignore]
fn lab_search_rows_20k() {
    let seed = 20261117u64;
    let tables: Vec<(String, Planted)> = vec![
        (
            "big_binary_20k".to_string(),
            plant(
                "bb",
                20_000,
                seed + 11,
                &[
                    Block::BinaryAligned {
                        cols: 5,
                        clusters: 2,
                        p_lo: 0.2,
                        p_hi: 0.8,
                        enc: BinEnc::I64,
                    },
                    Block::BinaryAligned {
                        cols: 5,
                        clusters: 2,
                        p_lo: 0.15,
                        p_hi: 0.85,
                        enc: BinEnc::I64,
                    },
                    Block::BinaryWeakJoint {
                        cols: 6,
                        delta: 0.08,
                        enc: BinEnc::I64,
                    },
                    Block::BinaryAligned {
                        cols: 4,
                        clusters: 3,
                        p_lo: 0.15,
                        p_hi: 0.85,
                        enc: BinEnc::I64,
                    },
                ],
                &[NoiseCol::Binary(BinEnc::I64); 8],
            ),
        ),
        (
            "big_cont_20k".to_string(),
            plant(
                "bc",
                20_000,
                seed + 12,
                &[
                    Block::ContinuousFactor {
                        cols: 5,
                        loading: 0.7,
                    },
                    Block::ContinuousFactor {
                        cols: 5,
                        loading: 0.5,
                    },
                    Block::ContinuousMixture { cols: 4, sep: 1.2 },
                    Block::ContinuousMixture { cols: 4, sep: 0.8 },
                ],
                &[NoiseCol::Continuous; 8],
            ),
        ),
    ];
    let mut out: Vec<serde_json::Value> = Vec::new();
    for (tname, planted) in &tables {
        for sr in [2000usize, 8000, 20_000] {
            let cfg = ViewsConfig {
                row_cap: 20_000,
                search_rows: sr,
                ..Default::default()
            };
            let run = run_lab(&planted.table, &cfg);
            let sc = score(&run, planted);
            let mut e = score_json(&run, &sc);
            e["table"] = json!(tname);
            e["rows"] = json!(20_000);
            e["search_rows"] = json!(sr);
            out.push(e);
        }
    }
    // CSV for the lace harness (20k binary only).
    let dir = out_dir().join("planted");
    std::fs::create_dir_all(&dir).unwrap();
    let p = &tables[0].1;
    std::fs::write(dir.join("big_binary_20k.csv"), &p.csv).unwrap();
    let kinds: Vec<serde_json::Value> = p
        .kinds
        .iter()
        .map(|(n, k)| json!({"name": n, "kind": k}))
        .collect();
    std::fs::write(
        dir.join("big_binary_20k.kinds.json"),
        serde_json::to_string_pretty(&json!({
            "kinds": kinds,
            "blocks": p.blocks.iter().map(|b| json!(b)).collect::<Vec<_>>(),
            "noise": p.noise,
        }))
        .unwrap(),
    )
    .unwrap();
    write_json("search-rows-20k.json", &json!(out));
}

// ---------------------------------------------------------------- lab: detail

/// Print the full arithmetic of one planted pair: singleton and
/// joint phase_b groupings and scores, so a refusal can be read off
/// the numbers rather than inferred.
#[test]
#[ignore]
fn lab_detail() {
    let cases: Vec<(String, Planted, String, String)> = {
        let seed = 20261117u64;
        let bool_t = plant(
            "ba",
            500,
            seed,
            &[Block::BinaryAligned {
                cols: 4,
                clusters: 2,
                p_lo: 0.15,
                p_hi: 0.85,
                enc: BinEnc::Bool,
            }],
            &[],
        );
        let i64_t = plant(
            "bi",
            500,
            seed + 1,
            &[Block::BinaryAligned {
                cols: 4,
                clusters: 2,
                p_lo: 0.15,
                p_hi: 0.85,
                enc: BinEnc::I64,
            }],
            &[],
        );
        let cf_t = plant(
            "cf",
            500,
            seed + 4,
            &[Block::ContinuousFactor {
                cols: 4,
                loading: 0.9,
            }],
            &[],
        );
        vec![
            (
                "bin_aligned_bool".into(),
                bool_t,
                "ba_b0_0".into(),
                "ba_b0_1".into(),
            ),
            (
                "bin_aligned_i64".into(),
                i64_t,
                "bi_b0_0".into(),
                "bi_b0_1".into(),
            ),
            (
                "cont_factor_0.9".into(),
                cf_t,
                "cf_b0_0".into(),
                "cf_b0_1".into(),
            ),
        ]
    };
    for gamma in [2.0f64, 4.0] {
        for (label, planted, a, b) in &cases {
            let cfg = ViewsConfig {
                row_cap: 500,
                search_rows: 500,
                gamma,
                ..Default::default()
            };
            let sample: Vec<usize> = sample_rows(planted.table.n, cfg.row_cap)
                .iter()
                .map(|&r| r as usize)
                .collect();
            let (cols, _) = extract_columns(&planted.table, &sample, &cfg);
            let ia = cols.iter().position(|c| &c.name == a).unwrap();
            let ib = cols.iter().position(|c| &c.name == b).unwrap();
            let rows: Vec<usize> = (0..sample.len()).collect();
            let mut work = WorkBudget {
                used: 0,
                cap: cfg.work_budget,
                hit: false,
            };
            let describe = |cols: &[ViewCol], col_idx: &[usize], groups: &[Vec<usize>]| -> String {
                groups
                    .iter()
                    .map(|g| {
                        let per: Vec<String> = col_idx
                            .iter()
                            .map(|&ci| {
                                let ones: usize = g
                                    .iter()
                                    .filter(|&&r| {
                                        cols[ci].vals[r].map(|v| v > 0.5).unwrap_or(false)
                                    })
                                    .count();
                                let mean: f64 =
                                    g.iter().filter_map(|&r| cols[ci].vals[r]).sum::<f64>()
                                        / g.len().max(1) as f64;
                                format!("{}:ones={} mean={:.3}", cols[ci].name, ones, mean)
                            })
                            .collect();
                        format!("[n={} {}]", g.len(), per.join(" "))
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let (ga, sa) = phase_b(&cols, &[ia], &rows, &cfg, &mut work);
            let (gb, sb) = phase_b(&cols, &[ib], &rows, &cfg, &mut work);
            let (gj, sj) = phase_b(&cols, &[ia, ib], &rows, &cfg, &mut work);
            let penalty = cfg.gamma * (rows.len() as f64).ln();
            println!(
                "== {label} gamma={gamma} pair {a}~{b}\n  single {a}: score={sa:.2} groups {}\n  single {b}: score={sb:.2} groups {}\n  joint: score={sj:.2} groups {}\n  delta = {sj:.2} - {sa:.2} - {sb:.2} + {penalty:.2} = {:.2}",
                describe(&cols, &[ia], &ga),
                describe(&cols, &[ib], &gb),
                describe(&cols, &[ia, ib], &gj),
                sj - sa - sb + penalty,
            );
        }
    }
}

// ---------------------------------------------------------------- lab: repair arms

/// Sweep max_clusters on the 500-row battery: if the singleton
/// shredding is what makes merges unattainable, capping clusters
/// should restore recovery.
#[test]
#[ignore]
fn lab_clusters() {
    let mut out: Vec<serde_json::Value> = Vec::new();
    for (tname, planted) in battery_tables(500) {
        for mc in [2usize, 3, 4, 8] {
            for gamma in [2.0f64, 4.0] {
                let cfg = ViewsConfig {
                    row_cap: 500,
                    search_rows: 500,
                    gamma,
                    max_clusters: mc,
                    ..Default::default()
                };
                let run = run_lab(&planted.table, &cfg);
                let sc = score(&run, &planted);
                let mut e = score_json(&run, &sc);
                e["table"] = json!(tname);
                e["rows"] = json!(500);
                e["gamma"] = json!(gamma);
                e["max_clusters"] = json!(mc);
                out.push(e);
            }
        }
    }
    write_json("clusters.json", &json!(out));
}

/// For every planted pair (and every cross pair) at 500 rows,
/// gamma = 2: the merge delta the current search gets (phase_b
/// joint) versus the delta under the overlay partition — the cross
/// of the two singleton partitions, scored directly. The overlay is
/// the best "shared refinement" candidate; if its delta is also
/// negative, no search improvement can repair the criterion.
#[test]
#[ignore]
fn lab_overlay() {
    let mut out: Vec<serde_json::Value> = Vec::new();
    for (tname, planted) in battery_tables(500) {
        let cfg = ViewsConfig {
            row_cap: 500,
            search_rows: 500,
            gamma: 2.0,
            ..Default::default()
        };
        let sample: Vec<usize> = sample_rows(planted.table.n, cfg.row_cap)
            .iter()
            .map(|&r| r as usize)
            .collect();
        let (cols, _) = extract_columns(&planted.table, &sample, &cfg);
        let rows: Vec<usize> = (0..sample.len()).collect();
        let mut work = WorkBudget {
            used: 0,
            cap: cfg.work_budget,
            hit: false,
        };
        let ix: BTreeMap<String, usize> = cols
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.clone(), i))
            .collect();
        // Singleton partitions and scores.
        let mut singles: BTreeMap<usize, (Vec<Vec<usize>>, f64)> = BTreeMap::new();
        for &ci in ix.values() {
            singles.insert(ci, phase_b(&cols, &[ci], &rows, &cfg, &mut work));
        }
        let penalty = cfg.gamma * (rows.len() as f64).ln();
        let pair_delta = |ia: usize, ib: usize, work: &mut WorkBudget| -> (f64, f64, usize) {
            let (ga, sa) = &singles[&ia];
            let (gb, sb) = &singles[&ib];
            // Overlay: cross of the two partitions.
            let mut cell_of: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
            let mut cell_a: BTreeMap<usize, usize> = BTreeMap::new();
            for (gi, g) in ga.iter().enumerate() {
                for &r in g {
                    cell_a.insert(r, gi);
                }
            }
            let mut cell_b: BTreeMap<usize, usize> = BTreeMap::new();
            for (gi, g) in gb.iter().enumerate() {
                for &r in g {
                    cell_b.insert(r, gi);
                }
            }
            for &r in &rows {
                cell_of.entry((cell_a[&r], cell_b[&r])).or_default().push(r);
            }
            let overlay: Vec<Vec<usize>> = cell_of.into_values().collect();
            let n_cells = overlay.len();
            let s_overlay = cluster_score(&cols, &[ia, ib], &overlay, cfg.gamma, rows.len());
            let (_, s_phase) = phase_b(&cols, &[ia, ib], &rows, &cfg, work);
            (
                s_phase - sa - sb + penalty,
                s_overlay - sa - sb + penalty,
                n_cells,
            )
        };
        let mut planted_rows = Vec::new();
        for block in &planted.blocks {
            for x in 0..block.len() {
                for y in (x + 1)..block.len() {
                    let (ia, ib) = (ix[&block[x]], ix[&block[y]]);
                    let (dp, dov, nc) = pair_delta(ia, ib, &mut work);
                    planted_rows.push(json!({
                        "a": block[x], "b": block[y],
                        "delta_phase_b": dp, "delta_overlay": dov, "overlay_cells": nc,
                    }));
                }
            }
        }
        let mut cross_rows = Vec::new();
        let mut all: Vec<(&String, Option<usize>)> = Vec::new();
        for (bi, block) in planted.blocks.iter().enumerate() {
            for n in block {
                all.push((n, Some(bi)));
            }
        }
        for n in &planted.noise {
            all.push((n, None));
        }
        for x in 0..all.len() {
            for y in (x + 1)..all.len() {
                if all[x].1.is_some() && all[x].1 == all[y].1 {
                    continue;
                }
                let (ia, ib) = (ix[all[x].0], ix[all[y].0]);
                let (dp, dov, nc) = pair_delta(ia, ib, &mut work);
                cross_rows.push(json!({
                    "a": all[x].0, "b": all[y].0,
                    "delta_phase_b": dp, "delta_overlay": dov, "overlay_cells": nc,
                }));
            }
        }
        out.push(json!({
            "table": tname,
            "planted": planted_rows,
            "cross": cross_rows,
        }));
    }
    write_json("overlay.json", &json!(out));
}
