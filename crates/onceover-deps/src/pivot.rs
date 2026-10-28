//! Long-format (skinny) detection: identify (index key columns, attribute
//! column, measure column) triples for which (index, attribute) is jointly
//! unique, so the frame can be pivoted to wide form — one column per
//! attribute level, the measure as values.
//!
//! The pass is detect-and-advise: it reports the triple and the pivot call,
//! it does not pivot the frame the other detectors see.

use std::collections::BTreeMap;

use onceover_core::{TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use serde_json::json;

use crate::fd::count_distinct_tuples;
use crate::ColInfo;

#[derive(Clone, Debug)]
pub struct PivotConfig {
    /// Most attribute levels a column may have and still count as an
    /// attribute (aligned with the encoder's nominal level cap).
    pub max_levels: usize,
    /// Most pivot findings reported per profile.
    pub max_findings: usize,
    /// Smallest frame the pass runs on.
    pub min_rows: usize,
}

impl Default for PivotConfig {
    fn default() -> Self {
        Self {
            max_levels: 32,
            max_findings: 3,
            min_rows: 30,
        }
    }
}

#[derive(Default, Debug)]
pub struct PivotOutcome {
    pub findings: Vec<Finding>,
    /// (attribute, measure) pairs that passed the role/distinct screens.
    pub checked: usize,
    /// Pairs whose (index, attribute) tuples verified jointly unique.
    pub verified: usize,
}

struct Candidate {
    attr: usize,
    measure: usize,
    index: Vec<usize>,
    /// Sorted names of index columns plus the attribute column; two
    /// candidates with the same measure and the same involved set are two
    /// readings of one key, and only one is reported.
    involved: Vec<String>,
    levels: Vec<String>,
    min_level_count: usize,
    n_index: usize,
    rectangularity: f64,
}

/// Rendered level values of a column with their row counts, sorted by
/// level. Only called for attribute candidates (categorical role), so the
/// column is a string or a low-cardinality numeric column.
fn level_counts(col: &TypedCol) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut bump = |v: Option<String>| {
        if let Some(s) = v {
            *counts.entry(s).or_insert(0) += 1;
        }
    };
    match col {
        TypedCol::Str { vals } => {
            for v in vals {
                bump(v.as_ref().map(|s| (*s).to_owned()));
            }
        }
        TypedCol::StrOwned { vals } => {
            for v in vals {
                bump(v.clone());
            }
        }
        TypedCol::I64 { vals } => {
            for v in vals.iter() {
                bump(v.map(|x| x.to_string()));
            }
        }
        TypedCol::F64 { vals } => {
            for v in vals.iter() {
                bump(v.map(|x| x.to_string()));
            }
        }
        _ => {}
    }
    counts
}

pub fn analyze_pivot(table: &TypedTable, info: &[ColInfo], cfg: &PivotConfig) -> PivotOutcome {
    let mut out = PivotOutcome::default();
    let n = table.n;
    if n < cfg.min_rows || info.len() < 3 {
        return out;
    }
    // A long frame has exactly one measure column. Several continuous
    // columns are read as an already-wide frame: each is a would-be
    // attribute level that has already been pivoted out.
    let measures: Vec<usize> = (0..info.len())
        .filter(|&i| info[i].role == "continuous")
        .collect();
    if measures.len() != 1 {
        return out;
    }
    let measure = measures[0];
    let attrs: Vec<usize> = (0..info.len())
        .filter(|&i| {
            i != measure
                && info[i].role == "categorical"
                && info[i].distinct >= 2
                && info[i].distinct <= cfg.max_levels
                && info[i].null_count == 0
        })
        .collect();
    let mut candidates: Vec<Candidate> = Vec::new();
    for &a in &attrs {
        // Level counts are a property of the attribute alone: every level
        // must repeat at least twice, or it is not a repeated measure.
        let counts = level_counts(&table.cols[a]);
        if counts.len() < 2 || counts.len() > cfg.max_levels {
            continue;
        }
        let min_level_count = counts.values().copied().min().unwrap_or(0);
        if min_level_count < 2 {
            continue;
        }
        let levels: Vec<String> = counts.keys().cloned().collect();
        out.checked += 1;
        // Index: every column except the attribute and the measure.
        let index: Vec<usize> = (0..info.len())
            .filter(|&i| i != a && i != measure)
            .collect();
        if index.is_empty() || index.iter().any(|&i| info[i].null_count > 0) {
            continue;
        }
        let n_index = count_distinct_tuples(table, &index);
        if n_index == usize::MAX || n_index < 2 || n_index >= n {
            continue;
        }
        let mut joint = index.clone();
        joint.push(a);
        if count_distinct_tuples(table, &joint) != n {
            continue;
        }
        out.verified += 1;
        let mut involved: Vec<String> = index.iter().map(|&i| info[i].name.clone()).collect();
        involved.push(info[a].name.clone());
        involved.sort();
        candidates.push(Candidate {
            attr: a,
            measure,
            index,
            involved,
            levels,
            min_level_count,
            n_index,
            rectangularity: n as f64 / (n_index * counts.len()) as f64,
        });
    }
    // Two attributes spanning the same column set with the same measure
    // are two readings of one key (in a panel, the entity, the year, and
    // the indicator each complete the key). Report the reading with the
    // fewest levels: levels become columns, so the smallest level set is
    // the most plausible attribute.
    candidates.sort_by(|x, y| {
        x.involved
            .cmp(&y.involved)
            .then(x.levels.len().cmp(&y.levels.len()))
            .then(info[x.attr].name.cmp(&info[y.attr].name))
    });
    candidates.dedup_by(|next, keep| next.involved == keep.involved);
    // Rank: most rectangular first; ties by fewer levels, then names.
    candidates.sort_by(|x, y| {
        y.rectangularity
            .total_cmp(&x.rectangularity)
            .then(x.levels.len().cmp(&y.levels.len()))
            .then(info[x.attr].name.cmp(&info[y.attr].name))
            .then(info[x.measure].name.cmp(&info[y.measure].name))
    });
    candidates.truncate(cfg.max_findings);
    for c in &candidates {
        out.findings.push(pivot_finding(info, c, n));
    }
    out
}

fn pivot_finding(info: &[ColInfo], c: &Candidate, n: usize) -> Finding {
    let index_names: Vec<String> = c.index.iter().map(|&i| info[i].name.clone()).collect();
    let attr_name = info[c.attr].name.clone();
    let measure_name = info[c.measure].name.clone();
    let mut columns = index_names.clone();
    columns.push(attr_name.clone());
    columns.push(measure_name.clone());
    let shown_levels = if c.levels.len() <= 8 {
        c.levels.join(", ")
    } else {
        format!("{}, +{} more", c.levels[..8].join(", "), c.levels.len() - 8)
    };
    Finding::new(
        Pattern::LongFormat,
        BTreeMap::from([
            ("columns".into(), json!(columns)),
            ("index".into(), json!(index_names)),
            ("attribute".into(), json!(attr_name)),
            ("measure".into(), json!(measure_name)),
        ]),
        format!(
            "frame is in long format: ({}) × {} → {}; pivoting on {} gives one column per level ({})",
            index_names.join(", "),
            attr_name,
            measure_name,
            attr_name,
            shown_levels
        ),
        EvidenceBasis::Measured,
        0.85,
        BTreeMap::from([
            ("n_rows".into(), json!(n)),
            ("n_index_tuples".into(), json!(c.n_index)),
            ("n_levels".into(), json!(c.levels.len())),
            ("levels".into(), json!(c.levels)),
            ("rectangularity".into(), json!(c.rectangularity)),
            ("min_level_count".into(), json!(c.min_level_count)),
        ]),
        Some(format!(
            "relationships between levels of {} are not visible in this layout; pivot first, e.g. in Polars: df.pivot(on={:?}, index={:?}, values={:?})",
            attr_name, attr_name, index_names, measure_name
        )),
        Method::new(
            "pivot",
            "long_format",
            "exact joint-uniqueness verification of (index, attribute) tuples (onceover v0.1)",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use onceover_core::extract;
    use polars::df;

    fn info_with_roles(table: &TypedTable, roles: &[&str]) -> Vec<ColInfo> {
        table
            .cols
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let counts = onceover_core::build_counts(col, 1_000_000);
                ColInfo {
                    name: table.names[i].clone(),
                    dtype: table.dtypes[i].clone(),
                    role: roles[i].to_string(),
                    n: table.n,
                    null_count: col.null_count(),
                    distinct: counts.distinct,
                    dup_examples: vec![],
                }
            })
            .collect()
    }

    /// 6 entities x 4 years x 3 indicators = 72 rows, melted panel.
    fn skinny() -> (Vec<String>, Vec<i64>, Vec<String>, Vec<f64>) {
        let entities = ["E1", "E2", "E3", "E4", "E5", "E6"];
        let indicators = ["gdp_pc", "life_exp", "co2_pc"];
        let (mut e, mut y, mut ind, mut v) = (vec![], vec![], vec![], vec![]);
        for (ei, ent) in entities.iter().enumerate() {
            for year in 2010..2014 {
                for (ii, name) in indicators.iter().enumerate() {
                    e.push(ent.to_string());
                    y.push(year);
                    ind.push(name.to_string());
                    v.push(100.0 * ei as f64 + 10.0 * (year - 2010) as f64 + ii as f64);
                }
            }
        }
        (e, y, ind, v)
    }

    const ROLES: [&str; 4] = ["categorical", "categorical", "categorical", "continuous"];

    #[test]
    fn finds_the_indicator_triple() {
        let (e, y, ind, v) = skinny();
        let df = df! { "entity" => e, "year" => y, "indicator" => ind, "value" => v }.unwrap();
        let table = extract(&df);
        let info = info_with_roles(&table, &ROLES);
        let out = analyze_pivot(&table, &info, &PivotConfig::default());
        assert_eq!(out.findings.len(), 1, "findings: {:?}", out.findings);
        let f = &out.findings[0];
        assert_eq!(f.specification.form.as_deref(), Some("pivot"));
        let def = &f.specification.definition;
        assert_eq!(def["attribute"], json!("indicator"));
        assert_eq!(def["measure"], json!("value"));
        assert_eq!(def["index"], json!(["entity", "year"]));
        assert_eq!(f.metrics()["n_index_tuples"], json!(24));
        assert_eq!(f.metrics()["n_levels"], json!(3));
        assert_eq!(f.metrics()["rectangularity"], json!(1.0));
    }

    #[test]
    fn wide_frame_is_not_flagged() {
        // Same panel already pivoted: three continuous columns.
        let entities = ["E1", "E2", "E3", "E4", "E5", "E6"];
        let (mut e, mut y, mut g, mut l, mut c) = (vec![], vec![], vec![], vec![], vec![]);
        for (ei, ent) in entities.iter().enumerate() {
            for year in 2010..2014 {
                e.push(ent.to_string());
                y.push(year);
                g.push(100.0 * ei as f64 + (year - 2010) as f64);
                l.push(70.0 + ei as f64);
                c.push(5.0 + 0.1 * ei as f64);
            }
        }
        let df = df! { "entity" => e, "year" => y, "gdp_pc" => g, "life_exp" => l, "co2_pc" => c }
            .unwrap();
        let table = extract(&df);
        let info = info_with_roles(
            &table,
            &[
                "categorical",
                "categorical",
                "continuous",
                "continuous",
                "continuous",
            ],
        );
        let out = analyze_pivot(&table, &info, &PivotConfig::default());
        assert!(out.findings.is_empty(), "findings: {:?}", out.findings);
    }

    #[test]
    fn duplicated_combination_is_not_flagged() {
        let (mut e, mut y, mut ind, mut v) = skinny();
        // Duplicate one (entity, year, indicator) combination.
        e.push("E1".into());
        y.push(2010);
        ind.push("gdp_pc".into());
        v.push(999.0);
        let df = df! { "entity" => e, "year" => y, "indicator" => ind, "value" => v }.unwrap();
        let table = extract(&df);
        let info = info_with_roles(&table, &ROLES);
        let out = analyze_pivot(&table, &info, &PivotConfig::default());
        assert!(out.findings.is_empty(), "findings: {:?}", out.findings);
    }

    #[test]
    fn level_seen_once_disqualifies_that_attribute() {
        let (e, y, mut ind, mut v) = skinny();
        // Rename one row's indicator to a level that occurs only once: the
        // indicator reading is disqualified (a level must repeat), and the
        // pass falls back to the year reading of the same key.
        ind[0] = "rare_idx".into();
        v[0] = 1.0;
        let df = df! { "entity" => e, "year" => y, "indicator" => ind, "value" => v }.unwrap();
        let table = extract(&df);
        let info = info_with_roles(&table, &ROLES);
        let out = analyze_pivot(&table, &info, &PivotConfig::default());
        assert_eq!(out.findings.len(), 1, "findings: {:?}", out.findings);
        assert_eq!(
            out.findings[0].specification.definition["attribute"],
            json!("year")
        );
    }

    #[test]
    fn null_in_attribute_is_not_flagged() {
        let (e, y, ind, v) = skinny();
        let ind: Vec<Option<String>> = ind
            .into_iter()
            .enumerate()
            .map(|(i, s)| if i == 5 { None } else { Some(s) })
            .collect();
        let df = df! { "entity" => e, "year" => y, "indicator" => ind, "value" => v }.unwrap();
        let table = extract(&df);
        let info = info_with_roles(&table, &ROLES);
        let out = analyze_pivot(&table, &info, &PivotConfig::default());
        assert!(out.findings.is_empty(), "findings: {:?}", out.findings);
    }
}
