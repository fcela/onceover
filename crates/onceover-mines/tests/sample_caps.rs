//! Measurement harness for issue #363: what the shared encoded-sample
//! cap, the screen's model-row cap, and the missingness search sample
//! cost in statements, measured on planted tables where the caps bind.
//!
//! Every sweep test is `#[ignore]`d: run them deliberately with
//! `cargo test --release --test sample_caps -- --ignored --nocapture`.
//! Output lines are CSV so the record in docs/benchmarks can be built
//! from them directly. The non-ignored tests at the bottom are the
//! regression tests: planted structures the old caps lost, stated at
//! the current defaults.

use onceover_core::extract;
use onceover_mines::encode::{encode, EncodeConfig};
use onceover_mines::gbm::{screen, ScreenConfig};
use onceover_mines::missingness::profile_missingness_with;
use onceover_mines::modelfail::{screen_model_failures, ModelFailureConfig};
use onceover_mines::slices::{profile_slice_shifts, SliceConfig};
use polars::prelude::*;
use std::time::Instant;

// ---------- deterministic generation ----------

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn uni(seed: u64, i: usize) -> f64 {
    (splitmix(seed ^ (i as u64).wrapping_mul(0xa24b_aed4_963e_e407)) >> 11) as f64
        / (1u64 << 53) as f64
}

/// Standard normal from two uniforms (Box-Muller, cosine half).
fn gauss(seed: u64, i: usize) -> f64 {
    let u1 = uni(seed, 2 * i).max(1e-12);
    let u2 = uni(seed, 2 * i + 1);
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

fn is_rare(seed: u64, i: usize, share: f64) -> bool {
    uni(seed, i) < share
}

// ---------- probe tables ----------

/// Screen probe: `y` depends on `x` only inside the rare group.
/// Outside it, `y` is independent noise.
fn screen_table(n: usize, share: f64) -> DataFrame {
    let mut grp: Vec<&str> = Vec::with_capacity(n);
    let mut x: Vec<f64> = Vec::with_capacity(n);
    let mut z1: Vec<f64> = Vec::with_capacity(n);
    let mut z2: Vec<f64> = Vec::with_capacity(n);
    let mut y: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        let rare = is_rare(11, i, share);
        grp.push(if rare { "rare" } else { "common" });
        let xv = uni(21, i);
        x.push(xv);
        z1.push(uni(31, i));
        z2.push(uni(41, i));
        y.push(if rare {
            3.0 * xv + 0.25 * gauss(51, i)
        } else {
            gauss(61, i)
        });
    }
    df! { "grp" => grp, "x" => x, "z1" => z1, "z2" => z2, "y" => y }.unwrap()
}

/// Slice probe: `y` is shifted inside the rare segment; the binary
/// target `t` fires at 50% inside it against 5% outside.
fn slice_table(n: usize, share: f64, depth2: bool) -> DataFrame {
    let mut seg: Vec<&str> = Vec::with_capacity(n);
    let mut w: Vec<f64> = Vec::with_capacity(n);
    let mut y: Vec<f64> = Vec::with_capacity(n);
    let mut t: Vec<&str> = Vec::with_capacity(n);
    for i in 0..n {
        let rare = is_rare(13, i, share);
        seg.push(if rare { "rare" } else { "common" });
        let wv = uni(23, i);
        w.push(wv);
        let in_slice = rare && (!depth2 || wv > 0.5);
        y.push(gauss(53, i) + if in_slice { 2.0 } else { 0.0 });
        let p = if in_slice { 0.5 } else { 0.05 };
        t.push(if uni(73, i) < p { "yes" } else { "no" });
    }
    df! { "seg" => seg, "w" => w, "y" => y, "t" => t }.unwrap()
}

/// Model-failure probe: `y` is a clean linear function of `x1`, `x2`
/// everywhere, but inside the rare segment its noise standard
/// deviation is ten times larger — a failure no fit can absorb.
fn modelfail_table(n: usize, share: f64) -> DataFrame {
    let mut seg: Vec<&str> = Vec::with_capacity(n);
    let mut x1: Vec<f64> = Vec::with_capacity(n);
    let mut x2: Vec<f64> = Vec::with_capacity(n);
    let mut y: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        let rare = is_rare(17, i, share);
        seg.push(if rare { "rare" } else { "common" });
        let a = uni(27, i);
        let b = uni(37, i);
        x1.push(a);
        x2.push(b);
        let sd = if rare { 3.0 } else { 0.3 };
        y.push(2.0 * a + b + sd * gauss(57, i));
    }
    df! { "seg" => seg, "x1" => x1, "x2" => x2, "y" => y }.unwrap()
}

/// Missingness probe: `m` is missing at rate `p_rare` inside the rare
/// group and `p_common` outside it.
fn missing_table(n: usize, share: f64, p_rare: f64, p_common: f64) -> DataFrame {
    let mut grp: Vec<&str> = Vec::with_capacity(n);
    let mut x: Vec<f64> = Vec::with_capacity(n);
    let mut m: Vec<Option<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let rare = is_rare(19, i, share);
        grp.push(if rare { "rare" } else { "common" });
        x.push(uni(29, i));
        let p = if rare { p_rare } else { p_common };
        m.push(if uni(79, i) < p {
            None
        } else {
            Some(uni(39, i))
        });
    }
    df! { "grp" => grp, "x" => x, "m" => m }.unwrap()
}

// ---------- sweeps ----------

#[test]
#[ignore]
fn sweep_screen() {
    println!("probe,n,share,enc_cap,model_rows,metric,drivers,n_train,secs");
    for &n in &[200_000usize, 1_000_000usize] {
        for &share in &[0.05f64, 0.01, 0.005, 0.001] {
            let df = screen_table(n, share);
            let table = extract(&df);
            // Grid A: encoding uncapped, model rows vary.
            for &mmr in &[5_000usize, 20_000, 50_000, 200_000, n] {
                let (enc, _) = encode(
                    &table,
                    &EncodeConfig {
                        row_cap: n,
                        ..Default::default()
                    },
                );
                let cfg = ScreenConfig {
                    max_model_rows: mmr,
                    ..Default::default()
                };
                let t0 = Instant::now();
                let out = screen(&table, &enc, &cfg, &[], &[]);
                let secs = t0.elapsed().as_secs_f64();
                let line = match out.results.iter().find(|r| r.name == "y") {
                    Some(r) => {
                        let drivers: Vec<&str> =
                            r.drivers.iter().map(|(nm, _)| nm.as_str()).collect();
                        format!("{:.4},{}", r.metric, drivers.join("+"))
                    }
                    None => "none,".to_string(),
                };
                println!("screenA,{n},{share},{},{mmr},{line},,{:.2}", enc.n, secs);
            }
            // Grid B: model rows uncapped, encoding cap varies.
            for &cap in &[20_000usize, 50_000, 200_000, n] {
                let (enc, _) = encode(
                    &table,
                    &EncodeConfig {
                        row_cap: cap,
                        ..Default::default()
                    },
                );
                let cfg = ScreenConfig {
                    max_model_rows: n,
                    ..Default::default()
                };
                let t0 = Instant::now();
                let out = screen(&table, &enc, &cfg, &[], &[]);
                let secs = t0.elapsed().as_secs_f64();
                let line = match out.results.iter().find(|r| r.name == "y") {
                    Some(r) => {
                        let drivers: Vec<&str> =
                            r.drivers.iter().map(|(nm, _)| nm.as_str()).collect();
                        format!("{:.4},{}", r.metric, drivers.join("+"))
                    }
                    None => "none,".to_string(),
                };
                println!("screenB,{n},{share},{},uncapped,{line},,{:.2}", enc.n, secs);
            }
        }
    }
}

#[test]
#[ignore]
fn sweep_slices() {
    println!("probe,n,share,depth2,row_cap,findings_on_plant,findings_total,status,first_plant_claim,secs");
    for &n in &[200_000usize, 1_000_000usize] {
        for &share in &[0.05f64, 0.02, 0.01, 0.005, 0.001] {
            for &depth2 in &[false, true] {
                let df = slice_table(n, share, depth2);
                let table = extract(&df);
                for &cap in &[20_000usize, 50_000, 200_000, n] {
                    let cfg = SliceConfig {
                        row_cap: cap,
                        max_depth: if depth2 { 3 } else { 2 },
                        ..Default::default()
                    };
                    let t0 = Instant::now();
                    let out = profile_slice_shifts(&table, &[], &cfg);
                    let secs = t0.elapsed().as_secs_f64();
                    let on_plant = out
                        .findings
                        .iter()
                        .filter(|f| f.claim.contains("'rare'"))
                        .count();
                    let first = out
                        .findings
                        .iter()
                        .find(|f| f.claim.contains("'rare'"))
                        .map(|f| f.claim.replace(',', ";"))
                        .unwrap_or_default();
                    let status = match &out.status {
                        onceover_mines::slices::SliceStatus::Ran(d) => {
                            format!("ran[{}]", d.replace(',', ";"))
                        }
                        onceover_mines::slices::SliceStatus::Capped(d) => {
                            format!("capped[{}]", d.replace(',', ";"))
                        }
                        onceover_mines::slices::SliceStatus::NotApplicable(d) => {
                            format!("na[{}]", d.replace(',', ";"))
                        }
                    };
                    println!(
                        "slices,{n},{share},{depth2},{cap},{on_plant},{},{status},{first}, {:.2}",
                        out.findings.len(),
                        secs
                    );
                }
            }
        }
    }
}

#[test]
#[ignore]
fn sweep_modelfail() {
    println!("probe,n,share,enc_cap,model_rows,findings_on_plant,skill,secs");
    let arms: &[(usize, usize)] = &[
        (20_000, 5_000),  // standard tier today
        (50_000, 10_000), // deep tier today
        (200_000, 50_000),
        (usize::MAX, usize::MAX),
        (usize::MAX, 5_000),  // attribution: screen cap alone binds
        (20_000, usize::MAX), // attribution: encode cap alone binds
    ];
    for &n in &[200_000usize, 1_000_000usize] {
        for &share in &[0.05f64, 0.02, 0.01, 0.005, 0.001] {
            let df = modelfail_table(n, share);
            let table = extract(&df);
            for &(ecap, mmr) in arms {
                let (enc, _) = encode(
                    &table,
                    &EncodeConfig {
                        row_cap: ecap.min(n),
                        ..Default::default()
                    },
                );
                let scfg = ScreenConfig {
                    max_model_rows: if mmr == usize::MAX { n } else { mmr },
                    ..Default::default()
                };
                let t0 = Instant::now();
                let (findings, coverage) = screen_model_failures(
                    &table,
                    &enc,
                    "y",
                    &scfg,
                    &ModelFailureConfig::default(),
                    &[],
                );
                let secs = t0.elapsed().as_secs_f64();
                let on_plant = findings
                    .iter()
                    .filter(|f| f.claim.contains("'rare'"))
                    .count();
                println!(
                    "modelfail,{n},{share},{},{mmr},{on_plant},{}, {:.2}",
                    enc.n,
                    coverage.replace(',', ";"),
                    secs
                );
            }
        }
    }
}

#[test]
#[ignore]
fn sweep_missingness() {
    println!("probe,n,share,p_rare,p_common,search_rows,pattern_stated,claims,secs");
    let n = 1_000_000usize;
    for &(share, p_rare, p_common) in &[
        (0.05f64, 0.6, 0.02),
        (0.01, 0.6, 0.02),
        (0.005, 0.6, 0.02),
        (0.001, 0.6, 0.02),
        (0.05, 0.10, 0.05),
        (0.01, 0.10, 0.05),
    ] {
        let df = missing_table(n, share, p_rare, p_common);
        let table = extract(&df);
        for &sr in &[20_000usize, 50_000, 200_000, n] {
            let t0 = Instant::now();
            let (findings, _coverage) = profile_missingness_with(&table, &[], None, 4, sr);
            let secs = t0.elapsed().as_secs_f64();
            let stated = findings
                .iter()
                .any(|f| f.claim.contains("'rare'") || f.claim.contains("grp"));
            let first = findings
                .iter()
                .find(|f| f.claim.contains("'rare'") || f.claim.contains("grp"))
                .map(|f| f.claim.replace(',', ";"))
                .unwrap_or_default();
            println!(
                "missing,{n},{share},{p_rare},{p_common},{sr},{stated},{first}, {:.2}",
                secs
            );
        }
    }
}

#[test]
#[ignore]
fn sweep_floors() {
    // For each probe family: the planted group's row count in the
    // full table and in each stride sample, against the pass's
    // support floor at that sample size. A group below the floor
    // cannot be stated at any cap; the table shows where the floor,
    // not the cap, is the binding constraint.
    println!("family,n,share,sample,group_rows,floor,min_support_frac");
    for &n in &[200_000usize, 1_000_000usize] {
        for &share in &[0.05f64, 0.02, 0.01, 0.005, 0.001] {
            let full = (0..n).filter(|&i| is_rare(13, i, share)).count();
            for &cap in &[20_000usize, 50_000, 200_000, n] {
                let sample = onceover_core::sample_rows(n, cap);
                let rows = sample
                    .iter()
                    .filter(|&&r| is_rare(13, r as usize, share))
                    .count();
                let floor = 50.max((0.01 * sample.len() as f64).ceil() as usize);
                println!("slices,{n},{share},{},{rows},{floor},0.01", sample.len());
                let _ = full;
            }
        }
    }
}

#[test]
#[ignore]
fn sweep_nulls() {
    // Matched nulls at the largest caps: no planted structure, so no
    // statement about the planted columns may appear.
    let n = 1_000_000usize;
    // Screen null: y is pure noise.
    let df = screen_table_null(n);
    let table = extract(&df);
    let (enc, _) = encode(
        &table,
        &EncodeConfig {
            row_cap: n,
            ..Default::default()
        },
    );
    let out = screen(
        &table,
        &enc,
        &ScreenConfig {
            max_model_rows: n,
            ..Default::default()
        },
        &[],
        &[],
    );
    match out.results.iter().find(|r| r.name == "y") {
        Some(r) => println!(
            "null_screen,y_metric,{:.4},drivers,{}",
            r.metric,
            r.drivers.len()
        ),
        None => println!("null_screen,y_result,none"),
    }
    // Slices null: no shift.
    let df = slice_table_null(n);
    let table = extract(&df);
    let out = profile_slice_shifts(
        &table,
        &[],
        &SliceConfig {
            row_cap: n,
            ..Default::default()
        },
    );
    println!("null_slices,findings,{}", out.findings.len());
    // Missingness null: missing completely at random at 10%.
    let df = missing_table(n, 0.01, 0.10, 0.10);
    let table = extract(&df);
    let (findings, _) = profile_missingness_with(&table, &[], None, 4, n);
    let pattern = findings
        .iter()
        .filter(|f| f.claim.contains("grp") || f.claim.contains("'rare'"))
        .count();
    println!("null_missing,pattern_findings,{pattern}");
}

fn screen_table_null(n: usize) -> DataFrame {
    let mut grp: Vec<&str> = Vec::with_capacity(n);
    let mut x: Vec<f64> = Vec::with_capacity(n);
    let mut z1: Vec<f64> = Vec::with_capacity(n);
    let mut z2: Vec<f64> = Vec::with_capacity(n);
    let mut y: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        grp.push(if is_rare(11, i, 0.01) {
            "rare"
        } else {
            "common"
        });
        x.push(uni(21, i));
        z1.push(uni(31, i));
        z2.push(uni(41, i));
        y.push(gauss(61, i));
    }
    df! { "grp" => grp, "x" => x, "z1" => z1, "z2" => z2, "y" => y }.unwrap()
}

fn slice_table_null(n: usize) -> DataFrame {
    let mut seg: Vec<&str> = Vec::with_capacity(n);
    let mut w: Vec<f64> = Vec::with_capacity(n);
    let mut y: Vec<f64> = Vec::with_capacity(n);
    let mut t: Vec<&str> = Vec::with_capacity(n);
    for i in 0..n {
        seg.push(if is_rare(13, i, 0.01) {
            "rare"
        } else {
            "common"
        });
        w.push(uni(23, i));
        y.push(gauss(53, i));
        t.push(if uni(73, i) < 0.05 { "yes" } else { "no" });
    }
    df! { "seg" => seg, "w" => w, "y" => y, "t" => t }.unwrap()
}

// ---------- regression tests (run in the gate) ----------

#[test]
fn regression_screen_default_model_rows_measured() {
    // docs/benchmarks/2026-11-17-sample-caps.md: at 5,000 fitting
    // rows the screen reaches only about half the attainable
    // holdout R² for a driver confined to 1% of rows; 50,000
    // fitting rows reach the knee of the curve at ~2.5x the
    // 5,000-row runtime on a 1M-row probe.
    assert_eq!(ScreenConfig::default().max_model_rows, 50_000);
}

#[test]
fn regression_slices_coverage_states_carryable_share() {
    // The 60,000-row table is searched on the default 20,000-row
    // sample; the coverage entry must state the smallest slice
    // share that sample can carry, and the 5% plant must state.
    let df = slice_table(60_000, 0.05, false);
    let table = extract(&df);
    let out = profile_slice_shifts(&table, &[], &SliceConfig::default());
    let detail = match &out.status {
        onceover_mines::slices::SliceStatus::Ran(d) => d.clone(),
        _ => panic!("expected the slice search to run"),
    };
    assert!(
        detail.contains("the sample is 20000 of 60000 rows (row cap 20000)"),
        "{detail}"
    );
    assert!(
        detail.contains(
            "slices holding under 1% of the rows cannot be stated on this sample \
             (support floor 200 of 20000 rows)"
        ),
        "{detail}"
    );
    assert!(
        out.findings.iter().any(|f| f.claim.contains("'rare'")),
        "the planted slice must be stated"
    );
}

#[test]
fn regression_slices_below_support_floor_states_nothing() {
    // A 0.5% slice holds ~100 rows of the 20,000-row sample,
    // under the 200-row support floor: no statement at any cap
    // (measured); the pass must stay silent, not approximate.
    let df = slice_table(60_000, 0.005, false);
    let table = extract(&df);
    let out = profile_slice_shifts(&table, &[], &SliceConfig::default());
    assert_eq!(
        out.findings
            .iter()
            .filter(|f| f.claim.contains("'rare'"))
            .count(),
        0
    );
}

#[test]
fn regression_modelfail_coverage_states_carryable_share() {
    // Encoding capped at 2,000 rows and 500 model rows: the
    // residual set is 2,000 of the table's 30,000 rows, and the
    // coverage entry must state the share floor over those rows.
    let df = modelfail_table(30_000, 0.05);
    let table = extract(&df);
    let (enc, _) = encode(
        &table,
        &EncodeConfig {
            row_cap: 2_000,
            ..Default::default()
        },
    );
    let (_findings, coverage) = screen_model_failures(
        &table,
        &enc,
        "y",
        &ScreenConfig {
            max_model_rows: 500,
            ..Default::default()
        },
        &ModelFailureConfig::default(),
        &[],
    );
    assert!(
        coverage.contains("the modelled rows are 2000 of 30000 rows"),
        "{coverage}"
    );
    assert!(
        coverage.contains("cannot be stated on these rows (support floor 50 of 2000 rows)"),
        "{coverage}"
    );
}

#[test]
fn regression_missingness_search_sample_gates_discovery() {
    // The pattern search runs on a row sample and reported
    // patterns are recounted over all rows, so the sample gates
    // discovery only. A 1% group holds ~200 rows of the standing
    // 20,000-row sample (over the group floor: discovered) and
    // ~20 rows of a 2,000-row sample (under the 30-row floor:
    // not discovered).
    let df = missing_table(100_000, 0.01, 0.6, 0.02);
    let table = extract(&df);
    let (findings, _) = profile_missingness_with(&table, &[], None, 4, 20_000);
    assert!(
        findings.iter().any(|f| f.claim.contains("grp = rare")),
        "the planted pattern must be discovered at the standing sample"
    );
    let (findings, _) = profile_missingness_with(&table, &[], None, 4, 2_000);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.claim.contains("grp = rare"))
            .count(),
        0
    );
}
