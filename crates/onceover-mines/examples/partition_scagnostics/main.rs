//! Research runner. Pass a column-oriented JSON object on stdin, or --benchmark.

use std::io;
use std::time::Instant;

use onceover_core::stats::spearman;
use onceover_mines::partition_scagnostics::benchmark::{Cloud, Rng, Scale, Scratch};
use onceover_mines::partition_scagnostics::{default_metrics, run, Input, Metric};
use serde_json::{json, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = match std::env::args().nth(1).as_deref() {
        Some("--benchmark") => benchmark()?,
        None => serde_json::to_value(run(&serde_json::from_reader(io::stdin().lock())?)?)?,
        Some(_) => return Err("expected --benchmark or an input object on stdin".into()),
    };
    serde_json::to_writer_pretty(io::stdout().lock(), &output)?;
    println!();
    Ok(())
}

#[derive(Clone, Copy)]
enum Plant {
    OppositeSlopes,
    NumericSplit,
    Interaction,
    Stripes,
    CurvedArc,
    ClumpyRegimes,
    StandoutSubgroup,
    RangeRestriction,
    Independent,
    Homogeneous,
    ShuffledLabels,
    Latent,
}

fn data(plant: Plant, n: usize, distractors: usize, seed: u64) -> Input {
    let mut rng = Rng(seed.wrapping_add(291887));
    let mut input = Input {
        seed,
        metrics: default_metrics(),
        ..Input::default()
    };
    let mut x = Vec::with_capacity(n);
    let mut y = Vec::with_capacity(n);
    let mut group = Vec::with_capacity(n);
    let mut second = Vec::with_capacity(n);
    let mut ordered = Vec::with_capacity(n);
    let mut site10 = Vec::with_capacity(n);
    for _ in 0..n {
        let a = rng.below(2);
        let b = rng.below(2);
        let ordinal = rng.uniform();
        let s10 = rng.below(10);
        let s = match plant {
            Plant::Interaction => a ^ b,
            Plant::NumericSplit => usize::from(ordinal > 0.5),
            Plant::StandoutSubgroup => usize::from(s10 != 0),
            _ => a,
        };
        let mut t = 2.0 * rng.uniform() - 1.0;
        let noise = rng.uniform() - 0.5;
        let response = match plant {
            Plant::Independent => noise,
            Plant::Homogeneous => t + 0.2 * noise,
            Plant::Stripes => {
                if s == 0 {
                    // Wrap parallel diagonal bands to preserve both uniform marginals.
                    let offset = rng.below(3) as f64 * (2.0 / 3.0);
                    (t + offset + 0.012 * noise + 1.0).rem_euclid(2.0) - 1.0
                } else {
                    2.0 * noise
                }
            }
            Plant::CurvedArc => {
                if s == 0 {
                    // Symmetric parabola (rho ~ 0) with tight 1D stringy manifold.
                    (2.0 * t * t - 1.0) + 0.04 * noise
                } else {
                    2.0 * noise
                }
            }
            Plant::ClumpyRegimes => {
                if s == 0 {
                    // Two well-separated tight clusters (rho ~ 0 within, high clumpy).
                    let cluster = rng.below(4);
                    let (cx, cy) = match cluster {
                        0 => (-0.75, -0.75),
                        1 => (-0.75, 0.75),
                        2 => (0.75, -0.75),
                        _ => (0.75, 0.75),
                    };
                    t = cx + 0.14 * (rng.uniform() - 0.5);
                    cy + 0.14 * noise
                } else {
                    2.0 * noise
                }
            }
            Plant::StandoutSubgroup => {
                if s == 0 {
                    // 10% subgroup (site10 == "s0") follows a tight line; 90% is noise.
                    t + 0.08 * noise
                } else {
                    2.0 * noise
                }
            }
            Plant::RangeRestriction => {
                // Single smooth parabola across ALL rows; ordinal is a noisy proxy for x.
                t * t + 0.05 * noise
            }
            _ => (1.0 - 2.0 * s as f64) * t + 0.15 * noise,
        };
        let ord_val = if matches!(plant, Plant::RangeRestriction) {
            t + 0.15 * (rng.uniform() - 0.5)
        } else {
            ordinal
        };
        x.push(Some(t));
        y.push(Some(response));
        group.push(Some(a.to_string()));
        second.push(Some(b.to_string()));
        ordered.push(Some(ord_val));
        site10.push(Some(format!("s{s10}")));
    }
    match plant {
        Plant::Latent => (),
        Plant::ShuffledLabels => {
            rng.shuffle(&mut group);
            input.categorical.insert("protocol".into(), group);
        }
        Plant::StandoutSubgroup => {
            input.categorical.insert("site10".into(), site10);
        }
        _ => {
            input.categorical.insert("protocol".into(), group);
        }
    }
    input.categorical.insert("site".into(), second);
    input.ordered.insert("setting".into(), ordered);
    input.measures.insert("x".into(), x);
    input.measures.insert("y".into(), y);
    for i in 0..distractors {
        input.categorical.insert(
            format!("noise_{i:02}"),
            (0..n).map(|_| Some(rng.below(4).to_string())).collect(),
        );
    }
    input
}

fn rank_comparison() -> Value {
    let mut rng = Rng(167);
    let points: Vec<_> = (0..2_048).map(|_| [rng.uniform(), rng.uniform()]).collect();
    let assignments: Vec<Vec<u8>> = (0..512)
        .map(|_| (0..points.len()).map(|_| rng.below(4) as u8).collect())
        .collect();
    let start = Instant::now();
    let cloud = Cloud::new(&points, &Scale::fit(&points), &[Metric::Monotonic]);
    let mut scratch = Scratch::new(points.len());
    let fast: Vec<_> = assignments
        .iter()
        .map(|groups| {
            cloud
                .score(Metric::Monotonic, groups, &mut scratch)
                .map(|shape| shape.rho.unwrap())
        })
        .collect();
    let presorted = start.elapsed();
    let start = Instant::now();
    let slow: Vec<_> = assignments
        .iter()
        .map(|groups| {
            let mut per_group: [(Vec<f64>, Vec<f64>); 4] = Default::default();
            for (&[x, y], &group) in points.iter().zip(groups) {
                per_group[group as usize].0.push(x);
                per_group[group as usize].1.push(y);
            }
            per_group.map(|(x, y)| spearman(&x, &y).unwrap())
        })
        .collect();
    let resort = start.elapsed();
    let max_error = fast
        .iter()
        .zip(&slow)
        .flat_map(|(a, b)| a.iter().zip(b))
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max);
    json!({"rows": points.len(), "partitions": assignments.len(), "max_error": max_error,
        "presorted_ms": presorted.as_secs_f64() * 1000.0, "resort_ms": resort.as_secs_f64() * 1000.0})
}

fn benchmark() -> Result<Value, String> {
    let scenarios = [
        ("opposite_slopes", Plant::OppositeSlopes, 16),
        ("numeric_split", Plant::NumericSplit, 16),
        ("two_column_interaction", Plant::Interaction, 16),
        ("stripes", Plant::Stripes, 16),
        ("curved_arc", Plant::CurvedArc, 16),
        ("clumpy_regimes", Plant::ClumpyRegimes, 16),
        ("standout_subgroup", Plant::StandoutSubgroup, 16),
        ("range_restriction", Plant::RangeRestriction, 32),
        ("independent", Plant::Independent, 64),
        ("homogeneous", Plant::Homogeneous, 64),
        ("shuffled_labels", Plant::ShuffledLabels, 64),
        ("latent", Plant::Latent, 64),
    ];
    let mut summary = serde_json::Map::new();
    for (name, plant, seeds) in scenarios {
        let mut monotonic = 0;
        let mut striated = 0;
        let mut stringy = 0;
        let mut clumpy = 0;
        let mut any = 0;
        let mut times = Vec::new();
        let mut first = Value::Null;
        for seed in 0..seeds {
            let input = data(plant, 1_024, 8, seed);
            let started = Instant::now();
            let result = run(&input)?;
            times.push(started.elapsed().as_secs_f64() * 1000.0);
            monotonic += usize::from(
                result
                    .findings
                    .iter()
                    .any(|f| f.metric == Metric::Monotonic),
            );
            striated += usize::from(
                result
                    .findings
                    .iter()
                    .any(|f| f.metric == Metric::BinnedStriated),
            );
            stringy += usize::from(
                result
                    .findings
                    .iter()
                    .any(|f| f.metric == Metric::BinnedStringy),
            );
            clumpy += usize::from(
                result
                    .findings
                    .iter()
                    .any(|f| f.metric == Metric::BinnedClumpy),
            );
            any += usize::from(!result.findings.is_empty());
            if seed == 0 {
                first = json!(result);
            }
        }
        summary.insert(
            name.into(),
            json!({
                "runs": seeds,
                "monotonic": monotonic,
                "binned_striated": striated,
                "binned_stringy": stringy,
                "binned_clumpy": clumpy,
                "any": any,
                "median_ms": onceover_core::stats::median(&times),
                "example": first,
            }),
        );
    }
    let mut wide = data(Plant::OppositeSlopes, 12_000, 35, 0);
    let mut rng = Rng(86);
    for i in 0..6 {
        wide.measures.insert(
            format!("z_{i}"),
            (0..12_000).map(|_| Some(rng.uniform())).collect(),
        );
    }
    run(&wide)?;
    let mut times = Vec::new();
    let mut coverage = Value::Null;
    for _ in 0..5 {
        let started = Instant::now();
        let result = run(&wide)?;
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        coverage = json!(result.coverage);
    }
    Ok(
        json!({"scenarios": summary, "threads": rayon::current_num_threads(), "rank_comparison": rank_comparison(),
        "wide": {"median_ms": onceover_core::stats::median(&times), "coverage": coverage} }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_opposing_associations_and_interactions() {
        for (plant, kind) in [
            (Plant::OppositeSlopes, "levels"),
            (Plant::Interaction, "cross"),
        ] {
            let result = run(&data(plant, 1_024, 3, 9)).unwrap();
            let result = json!(result);
            let finding = result["findings"]
                .as_array()
                .unwrap()
                .iter()
                .find(|finding| finding["metric"] == "monotonic")
                .unwrap();
            assert_eq!(finding["rule"]["kind"], kind);
            assert!(finding["validation"]["gain"].as_f64().unwrap() > 0.9);
        }
    }

    #[test]
    fn held_out_labels_must_reproduce_the_gain() {
        let result = run(&data(Plant::ShuffledLabels, 1_024, 5, 31)).unwrap();
        assert!(result
            .findings
            .iter()
            .all(|finding| finding.metric != Metric::Monotonic));
    }

    #[test]
    fn detects_conditional_striation_with_uniform_marginals() {
        let result = run(&data(Plant::Stripes, 1_024, 3, 0)).unwrap();
        let result = json!(result);
        let finding = result["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|finding| finding["metric"] == "binned_striated")
            .unwrap();
        assert_eq!(finding["rule"]["column"], "protocol", "{finding}");
        assert!(finding["validation"]["gain"].as_f64().unwrap() > 0.1);
    }

    #[test]
    fn constant_group_does_not_emit_monotonic_gain() {
        let mut input = data(Plant::OppositeSlopes, 1_024, 0, 0);
        input.categorical.remove("site");
        input.ordered.clear();
        input.metrics = vec![Metric::Monotonic];
        let groups = input.categorical["protocol"]
            .iter()
            .map(|v| v.as_deref() == Some("0"));
        for (x, is_first) in input.measures.get_mut("x").unwrap().iter_mut().zip(groups) {
            if is_first {
                *x = Some(0.0);
            }
        }
        assert!(run(&input).unwrap().findings.is_empty());
        input.measures.get_mut("x").unwrap().fill(None);
        let result = json!(run(&input).unwrap());
        assert_eq!(result["coverage"]["pairs_without_supported_partition"], 1);
    }

    #[test]
    fn pair_axis_cannot_supply_the_partition() {
        let mut input = data(Plant::Latent, 514, 0, 4);
        input.categorical.clear();
        input.ordered.clear();
        input
            .ordered
            .insert("x".into(), input.measures["x"].clone());
        input
            .ordered
            .insert("y".into(), input.measures["y"].clone());
        assert_eq!(run(&input).unwrap().coverage.scored_partitions, 0);
    }
}
