//! lace-depprob: benchmark harness for the onceover views deep look
//! (issues #359, #360). Fits the Rust `lace` crate's CrossCat engine
//! and dumps the posterior dependence-probability (depprob) matrix
//! plus every state's column-to-view assignment.
//!
//! This crate is a benchmark harness only. It is never a dependency
//! of the shipped library.
//!
//! Modes:
//!   depprob-yaml  --data CSV --codebook YAML --seed N --iters N
//!                 --states N --out JSON
//!   depprob-kinds --data CSV --kinds KINDS.json --seed N --iters N
//!                 --states N --out JSON
//!
//! KINDS.json: {"kinds": [{"name": ..., "kind": "cat2"|"cont"}, ...]}
//! cat2 = Categorical with k = 2 and a UInt value map (the treatment
//! the bundled Animals codebook gives its binary features);
//! cont = Continuous. Both with default (None) hyper priors.

use std::time::{Duration, Instant};

use lace::codebook::data::read_csv;
use lace::codebook::ValueMap;
use lace::prelude::*;
use lace::update_handler::Timeout;
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256Plus;
use serde_json::json;

fn arg(args: &[String], name: &str) -> String {
    let pos = args
        .iter()
        .position(|a| a == name)
        .unwrap_or_else(|| panic!("missing argument {name}"));
    args.get(pos + 1)
        .unwrap_or_else(|| panic!("missing value for {name}"))
        .clone()
}

fn fit_and_dump(codebook: Codebook, data_csv: &str, seed: u64, iters: usize, states: usize, out: &str) {
    let df = read_csv(data_csv).expect("read data csv");
    let columns: Vec<String> = codebook
        .col_metadata
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let n_cols = columns.len();
    let t0 = Instant::now();
    let mut engine = Engine::new(
        states,
        codebook,
        DataSource::Polars(df),
        0,
        Xoshiro256Plus::seed_from_u64(seed),
    )
    .expect("engine new");
    let config = EngineUpdateConfig::new()
        .default_transitions()
        .n_iters(iters);
    engine
        .update(config, Timeout::new(Duration::MAX))
        .expect("engine update");
    let fit_seconds = t0.elapsed().as_secs_f64();

    let mut depprob = vec![vec![0.0f64; n_cols]; n_cols];
    for a in 0..n_cols {
        depprob[a][a] = 1.0;
        for b in (a + 1)..n_cols {
            let d = engine.depprob(a, b).expect("depprob");
            depprob[a][b] = d;
            depprob[b][a] = d;
        }
    }
    let state_views: Vec<Vec<usize>> = engine
        .states
        .iter()
        .map(|s| s.asgn().asgn.clone())
        .collect();
    let doc = json!({
        "columns": columns,
        "depprob": depprob,
        "state_views": state_views,
        "seed": seed,
        "n_iters": iters,
        "n_states": states,
        "fit_seconds": fit_seconds,
    });
    std::fs::write(out, serde_json::to_string(&doc).unwrap()).expect("write out");
    println!(
        "wrote {out}: {n_cols} columns, {states} states, fit {fit_seconds:.2}s"
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let data = arg(&args, "--data");
    let seed: u64 = arg(&args, "--seed").parse().unwrap();
    let iters: usize = arg(&args, "--iters").parse().unwrap();
    let states: usize = arg(&args, "--states").parse().unwrap();
    let out = arg(&args, "--out");
    match mode {
        "depprob-yaml" => {
            let cb_path = arg(&args, "--codebook");
            let codebook = Codebook::from_yaml(&cb_path).expect("read codebook");
            fit_and_dump(codebook, &data, seed, iters, states, &out);
        }
        "depprob-kinds" => {
            let kinds_path = arg(&args, "--kinds");
            let text = std::fs::read_to_string(&kinds_path).expect("read kinds");
            let doc: serde_json::Value = serde_json::from_str(&text).expect("parse kinds");
            let mut cols: Vec<ColMetadata> = Vec::new();
            for entry in doc["kinds"].as_array().expect("kinds array") {
                let name = entry["name"].as_str().expect("kind name").to_string();
                let coltype = match entry["kind"].as_str().expect("kind kind") {
                    "cat2" => ColType::Categorical {
                        k: 2,
                        hyper: None,
                        prior: None,
                        value_map: ValueMap::UInt(2),
                    },
                    "cont" => ColType::Continuous {
                        hyper: None,
                        prior: None,
                    },
                    other => panic!("unknown kind {other}"),
                };
                cols.push(ColMetadata {
                    name,
                    coltype,
                    notes: None,
                    missing_not_at_random: false,
                });
            }
            // The engine requires the codebook to name every row:
            // take the names from the data's first (id) column.
            let text_csv = std::fs::read_to_string(&data).expect("read data csv");
            let row_names: Vec<String> = text_csv
                .lines()
                .skip(1)
                .filter(|l| !l.is_empty())
                .map(|l| l.split(',').next().unwrap_or("").to_string())
                .collect();
            let mut codebook = Codebook::new(
                "planted".to_string(),
                ColMetadataList::new(cols).expect("unique column names"),
            );
            codebook.row_names =
                lace::codebook::RowNameList::try_from(row_names).expect("row names");
            fit_and_dump(codebook, &data, seed, iters, states, &out);
        }
        _ => panic!("unknown mode {mode}"),
    }
}
