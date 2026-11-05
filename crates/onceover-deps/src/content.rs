//! Column content: what the values are, and which ones do not fit.
//!
//! Text columns are checked for values that are really numbers, dates or
//! booleans, for recognized formats (email, URL, UUID, IP address), for
//! labels that differ only in case or spacing, for rare labels one or two
//! edits from a common one, and for values far longer than the rest.
//!
//! Numeric columns are checked for a value repeated at an isolated extreme,
//! a few values of the opposite sign, a large share of zeros, and two
//! separated modes. Temporal columns are checked for values far outside the
//! range of the rest. Every count is exact.

use std::collections::{BTreeMap, HashMap};

use onceover_core::{dateutil, TypedCol, TypedTable};
use onceover_report::{EvidenceBasis, Finding, Method, Pattern};
use rayon::prelude::*;
use serde_json::{json, Value};

const MIN_VALUES: usize = 50;
/// A text column reads as a type or format when this share of values fits.
const MIN_CONFORMING: f64 = 0.90;
const MAX_EXAMPLES: usize = 5;
const MAX_LABEL_LEVELS: usize = 500;
/// A misspelling candidate: rare, and this many times less common than the
/// label it resembles.
const MAX_RARE_SHARE: f64 = 0.01;
const RARE_RATIO: usize = 20;
const MIN_LABEL_CHARS: usize = 5;
const MAX_LONG_SHARE: f64 = 0.01;
const LONG_RATIO: f64 = 10.0;
const MIN_LONG_CHARS: usize = 100;
/// A repeated extreme: more rows than the extreme-value pass's clusters hold,
/// few enough to be a code rather than part of the distribution.
const MIN_REPEATS: usize = 4;
const MAX_REPEAT_SHARE: f64 = 0.05;
const FAR_FENCE_IQR: f64 = 3.0;
const MIN_DISTINCT_FOR_SHAPE: usize = 20;
/// The opposite sign is an anomaly at or below this share.
const MAX_MINORITY_SIGN_SHARE: f64 = 0.01;
const MIN_ZERO_SHARE: f64 = 0.2;
const MAX_ZERO_SHARE: f64 = 0.9;
const HISTOGRAM_BINS: usize = 40;
const MIN_MODE_MASS: f64 = 0.15;
const MAX_VALLEY_RATIO: f64 = 0.2;
const MIN_ROWS_FOR_MODES: usize = 500;
/// Fewer distinct values than this leave gaps between them that read as valleys.
const MIN_DISTINCT_FOR_MODES: usize = 50;
const MAX_TEMPORAL_OUTLIER_SHARE: f64 = 0.01;
const MIN_TEMPORAL_GAP_IQR: f64 = 3.0;

fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn pct(share: f64) -> String {
    let p = 100.0 * share;
    if p >= 10.0 {
        format!("{p:.0}%")
    } else {
        format!("{p:.2}%")
    }
}

fn statement(
    name: &str,
    kind: &str,
    claim: String,
    score: f64,
    mut metrics: BTreeMap<String, Value>,
    interpretation: &str,
) -> Finding {
    metrics.insert("kind".to_string(), json!(kind));
    Finding::new(
        Pattern::ContentAnomaly,
        BTreeMap::from([("column".to_string(), json!(name))]),
        claim,
        EvidenceBasis::Measured,
        score,
        metrics,
        Some(interpretation.to_string()),
        Method::new(
            "content",
            "column_content_scan",
            "exact counts over every value of the column (onceover v0.1)",
        ),
    )
}

// ---------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------

fn is_number(s: &str) -> bool {
    let t = s.trim();
    let t = t
        .strip_prefix(['$', '€', '£'])
        .unwrap_or(t)
        .trim_end_matches('%');
    if t.is_empty() || !t.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let plain: String = t.chars().filter(|c| *c != ',').collect();
    plain.parse::<f64>().is_ok_and(f64::is_finite)
}

fn is_date(s: &str) -> bool {
    let t = s.trim();
    let digits = |p: &str, n: usize| p.len() == n && p.chars().all(|c| c.is_ascii_digit());
    for sep in ['-', '/', '.'] {
        let parts: Vec<&str> = t.split(sep).collect();
        if parts.len() == 3 {
            let year_first = digits(parts[0], 4)
                && (1..=2).contains(&parts[1].len())
                && (1..=2).contains(&parts[2].len());
            let year_last = digits(parts[2], 4)
                && (1..=2).contains(&parts[0].len())
                && (1..=2).contains(&parts[1].len());
            if (year_first || year_last)
                && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
            {
                return true;
            }
        }
    }
    false
}

fn is_boolean(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "true" | "false" | "yes" | "no" | "y" | "n" | "t" | "f"
    )
}

fn is_email(s: &str) -> bool {
    let Some((local, domain)) = s.trim().split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !local.contains(' ')
        && !domain.contains('@')
        && !domain.contains(' ')
        && domain.split('.').count() >= 2
        && domain.split('.').all(|p| !p.is_empty())
}

fn is_url(s: &str) -> bool {
    let t = s.trim();
    ["http://", "https://", "ftp://"].iter().any(|p| {
        t.strip_prefix(p)
            .is_some_and(|r| r.contains('.') && !r.contains(' '))
    })
}

fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.trim().split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.trim().split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
}

struct Recognizer {
    label: &'static str,
    kind: &'static str,
    test: fn(&str) -> bool,
    interpretation: &'static str,
}

const TEXT_TYPES: [Recognizer; 3] = [
    Recognizer {
        label: "numbers",
        kind: "numbers_as_text",
        test: is_number,
        interpretation: "parse the column to a numeric type and decide how to treat the values that do not parse",
    },
    Recognizer {
        label: "dates",
        kind: "dates_as_text",
        test: is_date,
        interpretation: "parse the column to a date type and decide how to treat the values that do not parse",
    },
    Recognizer {
        label: "true/false values",
        kind: "booleans_as_text",
        test: is_boolean,
        interpretation: "map the column to a boolean and decide how to treat the values that do not map",
    },
];

const FORMATS: [Recognizer; 4] = [
    Recognizer {
        label: "email addresses",
        kind: "email",
        test: is_email,
        interpretation: "review the values that do not conform before relying on this column",
    },
    Recognizer {
        label: "URLs",
        kind: "url",
        test: is_url,
        interpretation: "review the values that do not conform before relying on this column",
    },
    Recognizer {
        label: "UUIDs",
        kind: "uuid",
        test: is_uuid,
        interpretation: "review the values that do not conform before relying on this column",
    },
    Recognizer {
        label: "IP addresses",
        kind: "ipv4",
        test: is_ipv4,
        interpretation: "review the values that do not conform before relying on this column",
    },
];

/// Non-conforming values, most frequent first, ties in text order.
fn exceptions<'a>(
    counts: &HashMap<&'a str, usize>,
    test: fn(&str) -> bool,
) -> (usize, Vec<(&'a str, usize)>) {
    let mut bad: Vec<(&str, usize)> = counts
        .iter()
        .filter(|(v, _)| !test(v))
        .map(|(v, c)| (*v, *c))
        .collect();
    bad.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    (bad.iter().map(|(_, c)| c).sum(), bad)
}

fn example_list(bad: &[(&str, usize)]) -> String {
    bad.iter()
        .take(MAX_EXAMPLES)
        .map(|(v, c)| format!("'{v}' ({c})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn edit_distance_at_most(a: &[char], b: &[char], limit: usize) -> bool {
    if a.len().abs_diff(b.len()) > limit {
        return false;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        if cur.iter().min().copied().unwrap_or(0) > limit {
            return false;
        }
        prev = cur;
    }
    prev[b.len()] <= limit
}

/// Whether two labels could be one label misspelled. `Atlas 3` and `Atlas 5`
/// differ by a digit and `Proton D` and `Proton M` by a one-letter word, and
/// those are usually different things with similar names.
fn same_words_apart_from_spelling(a: &str, b: &str) -> bool {
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    if digits(a) != digits(b) {
        return false;
    }
    let words = |s: &str| {
        s.split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    };
    let (wa, wb) = (words(a), words(b));
    wa.len() == wb.len()
        && wa
            .iter()
            .zip(&wb)
            .all(|(x, y)| x == y || x.chars().count().min(y.chars().count()) >= MIN_LABEL_CHARS)
}

fn label_findings(name: &str, counts: &HashMap<&str, usize>, n: usize, out: &mut Vec<Finding>) {
    if counts.len() < 2 || counts.len() > MAX_LABEL_LEVELS {
        return;
    }
    // Labels that differ only in case or surrounding space.
    let mut folded: BTreeMap<String, Vec<(&str, usize)>> = BTreeMap::new();
    for (v, c) in counts {
        folded
            .entry(v.trim().to_lowercase())
            .or_default()
            .push((*v, *c));
    }
    let mut groups: Vec<Vec<(&str, usize)>> =
        folded.into_values().filter(|g| g.len() >= 2).collect();
    for g in &mut groups {
        g.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    }
    groups.sort_by(|a, b| {
        let total = |g: &Vec<(&str, usize)>| g.iter().map(|(_, c)| c).sum::<usize>();
        total(b).cmp(&total(a)).then(a[0].0.cmp(b[0].0))
    });
    if !groups.is_empty() {
        let shown: Vec<String> = groups
            .iter()
            .take(3)
            .map(|g| {
                g.iter()
                    .map(|(v, c)| format!("'{v}' ({c})"))
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .collect();
        out.push(statement(
            name,
            "label_variants",
            format!(
                "column {name} spells {} label{} with different case or spacing: {}",
                groups.len(),
                if groups.len() == 1 { "" } else { "s" },
                shown.join("; ")
            ),
            0.72,
            BTreeMap::from([
                ("n".to_string(), json!(n)),
                ("variant_groups".to_string(), json!(groups.len())),
                (
                    "groups".to_string(),
                    json!(groups
                        .iter()
                        .take(10)
                        .map(|g| g
                            .iter()
                            .map(|(v, c)| json!({"value": v, "rows": c}))
                            .collect::<Vec<_>>())
                        .collect::<Vec<_>>()),
                ),
            ]),
            "normalize case and spacing before grouping by this column",
        ));
    }

    // Rare labels one or two edits from a common one.
    let mut levels: Vec<(&str, usize, Vec<char>)> = counts
        .iter()
        .map(|(v, c)| (*v, *c, v.trim().to_lowercase().chars().collect()))
        .collect();
    levels.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let rare_limit = (MAX_RARE_SHARE * n as f64) as usize;
    let mut typos: Vec<(&str, usize, &str, usize)> = Vec::new();
    for (i, (v, c, chars)) in levels.iter().enumerate() {
        if *c > rare_limit || chars.len() < MIN_LABEL_CHARS {
            continue;
        }
        let limit = if chars.len() >= 10 { 2 } else { 1 };
        let close = levels[..i].iter().find(|(common, cc, other)| {
            *cc >= RARE_RATIO * *c
                && common.trim().to_lowercase() != v.trim().to_lowercase()
                && same_words_apart_from_spelling(v, common)
                && edit_distance_at_most(chars, other, limit)
        });
        if let Some((common, cc, _)) = close {
            typos.push((v, *c, common, *cc));
        }
    }
    if !typos.is_empty() {
        let shown: Vec<String> = typos
            .iter()
            .take(MAX_EXAMPLES)
            .map(|(v, c, common, cc)| format!("'{v}' ({c}) resembles '{common}' ({cc})"))
            .collect();
        out.push(statement(
            name,
            "possible_misspelling",
            format!(
                "column {name} has {} rare label{} close to a common one: {}",
                typos.len(),
                if typos.len() == 1 { "" } else { "s" },
                shown.join(", ")
            ),
            0.7,
            BTreeMap::from([
                ("n".to_string(), json!(n)),
                ("candidates".to_string(), json!(typos.len())),
                (
                    "labels".to_string(),
                    json!(typos
                        .iter()
                        .map(|(v, c, common, cc)| json!({"value": v, "rows": c, "resembles": common, "resembles_rows": cc}))
                        .collect::<Vec<_>>()),
                ),
            ]),
            "check whether these are misspellings of the common label; similarity alone does not prove it",
        ));
    }
}

fn text_findings(name: &str, col: &TypedCol, n_rows: usize) -> Vec<Finding> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for r in 0..n_rows {
        if let Some(v) = col.str_at(r) {
            *counts.entry(v).or_default() += 1;
        }
    }
    let n: usize = counts.values().sum();
    if n < MIN_VALUES || counts.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();

    let mut typed = false;
    for r in &TEXT_TYPES {
        let (bad_rows, bad) = exceptions(&counts, r.test);
        let share = (n - bad_rows) as f64 / n as f64;
        if share < MIN_CONFORMING {
            continue;
        }
        typed = true;
        // A clean yes/no column is an ordinary encoding, not a finding.
        if bad_rows == 0 && r.kind == "booleans_as_text" {
            break;
        }
        let exceptions_text = if bad_rows == 0 {
            "none fail".to_string()
        } else {
            format!("{bad_rows} do not: {}", example_list(&bad))
        };
        out.push(statement(
            name,
            r.kind,
            format!(
                "column {name} holds {} stored as text: {} of {n} values parse ({}); {exceptions_text}",
                r.label,
                n - bad_rows,
                pct(share),
            ),
            0.74,
            BTreeMap::from([
                ("n".to_string(), json!(n)),
                ("conforming".to_string(), json!(n - bad_rows)),
                ("exceptions".to_string(), json!(bad_rows)),
                (
                    "exception_values".to_string(),
                    json!(bad.iter().take(MAX_EXAMPLES).map(|(v, _)| *v).collect::<Vec<_>>()),
                ),
            ]),
            r.interpretation,
        ));
        break;
    }

    if !typed {
        for r in &FORMATS {
            let (bad_rows, bad) = exceptions(&counts, r.test);
            let share = (n - bad_rows) as f64 / n as f64;
            // A fully conforming column has nothing to report.
            if share < MIN_CONFORMING || bad_rows == 0 {
                continue;
            }
            out.push(statement(
                name,
                r.kind,
                format!(
                    "column {name} holds {}: {} of {n} values conform ({}); {bad_rows} do not: {}",
                    r.label,
                    n - bad_rows,
                    pct(share),
                    example_list(&bad)
                ),
                0.72,
                BTreeMap::from([
                    ("n".to_string(), json!(n)),
                    ("conforming".to_string(), json!(n - bad_rows)),
                    ("exceptions".to_string(), json!(bad_rows)),
                    (
                        "exception_values".to_string(),
                        json!(bad
                            .iter()
                            .take(MAX_EXAMPLES)
                            .map(|(v, _)| *v)
                            .collect::<Vec<_>>()),
                    ),
                ]),
                r.interpretation,
            ));
            break;
        }
        label_findings(name, &counts, n, &mut out);
    }

    // Values far longer than the rest.
    let mut lengths: Vec<(usize, usize)> = counts
        .iter()
        .map(|(v, c)| (v.chars().count(), *c))
        .collect();
    lengths.sort_unstable();
    let at = |q: f64| -> usize {
        let target = (q * (n - 1) as f64) as usize;
        let mut seen = 0;
        for (len, c) in &lengths {
            seen += c;
            if seen > target {
                return *len;
            }
        }
        0
    };
    let median = at(0.5).max(1);
    let bar = (LONG_RATIO * median as f64).max(MIN_LONG_CHARS as f64) as usize;
    let long_rows: usize = lengths
        .iter()
        .filter(|(l, _)| *l >= bar)
        .map(|(_, c)| c)
        .sum();
    if long_rows > 0 && (long_rows as f64) <= MAX_LONG_SHARE * n as f64 {
        let longest = lengths.last().map_or(0, |(l, _)| *l);
        out.push(statement(
            name,
            "long_text",
            format!(
                "column {name} has {long_rows} value{} of at least {bar} characters, up to {longest}, against a median length of {median}",
                if long_rows == 1 { "" } else { "s" }
            ),
            0.62,
            BTreeMap::from([
                ("n".to_string(), json!(n)),
                ("long_values".to_string(), json!(long_rows)),
                ("longest".to_string(), json!(longest)),
                ("median_length".to_string(), json!(median)),
            ]),
            "check whether these are pasted documents, concatenated fields or truncated exports",
        ));
    }
    out
}

// ---------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------

fn quantile(sorted: &[f64], q: f64) -> f64 {
    let pos = q * (sorted.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// Runs of equal values, ascending.
fn runs(sorted: &[f64]) -> Vec<(f64, usize)> {
    let mut out: Vec<(f64, usize)> = Vec::new();
    for &v in sorted {
        match out.last_mut() {
            Some((x, c)) if *x == v => *c += 1,
            _ => out.push((v, 1)),
        }
    }
    out
}

fn repeated_extreme(name: &str, sorted: &[f64], support: &[(f64, usize)], out: &mut Vec<Finding>) {
    let n = sorted.len();
    let (q1, q3) = (quantile(sorted, 0.25), quantile(sorted, 0.75));
    let iqr = q3 - q1;
    let spread = quantile(sorted, 0.9) - quantile(sorted, 0.1);
    if iqr <= 0.0 || spread <= 0.0 || support.len() < 3 {
        return;
    }
    let last = support.len() - 1;
    for (value, count, nearest, far, side) in [
        (
            support[last].0,
            support[last].1,
            support[last - 1].0,
            support[last].0 > q3 + FAR_FENCE_IQR * iqr,
            "high",
        ),
        (
            support[0].0,
            support[0].1,
            support[1].0,
            support[0].0 < q1 - FAR_FENCE_IQR * iqr,
            "low",
        ),
    ] {
        if count < MIN_REPEATS || count as f64 > MAX_REPEAT_SHARE * n as f64 || !far {
            continue;
        }
        // The boundary pass states a pile that dominates its neighbours.
        let neighbours: Vec<usize> = if side == "high" {
            support[..last].iter().rev().take(10).map(|r| r.1).collect()
        } else {
            support[1..].iter().take(10).map(|r| r.1).collect()
        };
        let mut sorted_counts = neighbours;
        sorted_counts.sort_unstable();
        let baseline = sorted_counts[sorted_counts.len() / 2].max(1);
        if count >= 10 && count >= 4 * baseline {
            continue;
        }
        let gap = (value - nearest).abs();
        if gap < spread {
            continue;
        }
        out.push(statement(
            name,
            "repeated_extreme",
            format!(
                "column {name} has the value {} on {count} rows ({}), far from the next value, {} (a gap of {}, against a central range of {})",
                fmt_num(value),
                pct(count as f64 / n as f64),
                fmt_num(nearest),
                fmt_num(gap),
                fmt_num(spread),
            ),
            0.78,
            BTreeMap::from([
                ("n".to_string(), json!(n)),
                ("value".to_string(), json!(value)),
                ("rows".to_string(), json!(count)),
                ("nearest".to_string(), json!(nearest)),
                ("side".to_string(), json!(side)),
            ]),
            "a repeated isolated value is often a code for missing or capped data; check before using statistics of this column",
        ));
    }
}

fn minority_sign(name: &str, sorted: &[f64], out: &mut Vec<Finding>) {
    let n = sorted.len();
    let negative = sorted.iter().filter(|v| **v < 0.0).count();
    let positive = sorted.iter().filter(|v| **v > 0.0).count();
    let (minority, majority_word, minority_word, furthest) = if negative > 0 && negative < positive
    {
        (negative, "positive", "negative", sorted[0])
    } else if positive > 0 && positive < negative {
        (positive, "negative", "positive", sorted[n - 1])
    } else {
        return;
    };
    if minority as f64 > MAX_MINORITY_SIGN_SHARE * n as f64 {
        return;
    }
    out.push(statement(
        name,
        "minority_sign",
        format!(
            "column {name} is {majority_word} except for {minority} {minority_word} value{} ({}), reaching {}",
            if minority == 1 { "" } else { "s" },
            pct(minority as f64 / n as f64),
            fmt_num(furthest),
        ),
        0.74,
        BTreeMap::from([
            ("n".to_string(), json!(n)),
            ("minority_rows".to_string(), json!(minority)),
            ("sign".to_string(), json!(minority_word)),
            ("furthest".to_string(), json!(furthest)),
        ]),
        "check whether these are reversals, corrections or entry errors",
    ));
}

fn zero_share(name: &str, sorted: &[f64], out: &mut Vec<Finding>) {
    let n = sorted.len();
    let zeros = sorted.iter().filter(|v| **v == 0.0).count();
    let share = zeros as f64 / n as f64;
    if !(MIN_ZERO_SHARE..=MAX_ZERO_SHARE).contains(&share) {
        return;
    }
    let others: Vec<f64> = sorted.iter().copied().filter(|v| *v != 0.0).collect();
    let median = quantile(&others, 0.5);
    out.push(statement(
        name,
        "zero_inflation",
        format!(
            "column {name} is exactly 0 on {zeros} of {n} rows ({}); the median of the other values is {}",
            pct(share),
            fmt_num(median),
        ),
        0.66,
        BTreeMap::from([
            ("n".to_string(), json!(n)),
            ("zeros".to_string(), json!(zeros)),
            ("median_nonzero".to_string(), json!(median)),
        ]),
        "averages mix two groups here; decide whether zero means none, not recorded, or not applicable",
    ));
}

fn two_modes(name: &str, sorted: &[f64], out: &mut Vec<Finding>) {
    let n = sorted.len();
    if n < MIN_ROWS_FOR_MODES {
        return;
    }
    let (lo, hi) = (quantile(sorted, 0.005), quantile(sorted, 0.995));
    if hi <= lo {
        return;
    }
    let width = (hi - lo) / HISTOGRAM_BINS as f64;
    let mut bins = [0f64; HISTOGRAM_BINS];
    for &v in sorted {
        if v >= lo && v <= hi {
            let b = (((v - lo) / width) as usize).min(HISTOGRAM_BINS - 1);
            bins[b] += 1.0;
        }
    }
    let smooth: Vec<f64> = (0..HISTOGRAM_BINS)
        .map(|i| {
            let a = i.saturating_sub(1);
            let b = (i + 1).min(HISTOGRAM_BINS - 1);
            bins[a..=b].iter().sum::<f64>() / (b - a + 1) as f64
        })
        .collect();
    let total: f64 = bins.iter().sum();
    let mut best: Option<(usize, usize, usize, f64)> = None;
    for valley in 2..HISTOGRAM_BINS - 2 {
        let left_mass: f64 = bins[..valley].iter().sum();
        let right_mass: f64 = bins[valley + 1..].iter().sum();
        if left_mass < MIN_MODE_MASS * total || right_mass < MIN_MODE_MASS * total {
            continue;
        }
        let peak = |range: std::ops::Range<usize>| {
            range
                .max_by(|&x, &y| smooth[x].total_cmp(&smooth[y]))
                .unwrap_or(0)
        };
        let (left, right) = (peak(0..valley), peak(valley + 1..HISTOGRAM_BINS));
        let ratio = smooth[valley] / smooth[left].min(smooth[right]).max(1e-9);
        if ratio <= MAX_VALLEY_RATIO && best.is_none_or(|(_, _, _, r)| ratio < r) {
            best = Some((left, valley, right, ratio));
        }
    }
    let Some((left, valley, right, ratio)) = best else {
        return;
    };
    // Bin centres locate a mode roughly, so state them to three figures.
    let centre = |b: usize| {
        let v = lo + (b as f64 + 0.5) * width;
        if v == 0.0 {
            return 0.0;
        }
        let scale = 10f64.powi(2 - v.abs().log10().floor() as i32);
        (v * scale).round() / scale
    };
    out.push(statement(
        name,
        "two_modes",
        format!(
            "column {name} has two separated modes, near {} and {}, with few values around {}",
            fmt_num(centre(left)),
            fmt_num(centre(right)),
            fmt_num(centre(valley)),
        ),
        0.68,
        BTreeMap::from([
            ("n".to_string(), json!(n)),
            ("lower_mode".to_string(), json!(centre(left))),
            ("upper_mode".to_string(), json!(centre(right))),
            ("valley".to_string(), json!(centre(valley))),
            ("valley_ratio".to_string(), json!(ratio)),
        ]),
        "two groups may be mixed in this column; look for the column that separates them",
    ));
}

fn numeric_findings(name: &str, col: &TypedCol, n_rows: usize) -> Vec<Finding> {
    let mut sorted: Vec<f64> = (0..n_rows)
        .filter_map(|r| col.numeric_at(r))
        .filter(|v| v.is_finite())
        .collect();
    if sorted.len() < MIN_VALUES {
        return Vec::new();
    }
    sorted.sort_by(f64::total_cmp);
    let support = runs(&sorted);
    if support.len() < MIN_DISTINCT_FOR_SHAPE {
        return Vec::new();
    }
    let mut out = Vec::new();
    repeated_extreme(name, &sorted, &support, &mut out);
    minority_sign(name, &sorted, &mut out);
    zero_share(name, &sorted, &mut out);
    if support.len() >= MIN_DISTINCT_FOR_MODES {
        two_modes(name, &sorted, &mut out);
    }
    out
}

// ---------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------

fn temporal_findings(name: &str, col: &TypedCol, n_rows: usize) -> Vec<Finding> {
    let mut days: Vec<i64> = (0..n_rows)
        .filter_map(|r| match col {
            TypedCol::Date { vals } => vals[r].map(i64::from),
            TypedCol::Datetime { .. } => col.datetime_days(r),
            _ => None,
        })
        .collect();
    if days.len() < MIN_VALUES {
        return Vec::new();
    }
    days.sort_unstable();
    let n = days.len();
    let q1 = days[n / 4] as f64;
    let q3 = days[3 * n / 4] as f64;
    let iqr = q3 - q1;
    if iqr <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let limit = ((MAX_TEMPORAL_OUTLIER_SHARE * n as f64) as usize).max(1);
    // Largest gap inside each tail, within the allowed outlier count.
    for high in [true, false] {
        let mut best: Option<(usize, i64)> = None;
        for k in 1..=limit.min(n - 1) {
            let (edge, bulk) = if high {
                (days[n - k], days[n - k - 1])
            } else {
                (days[k - 1], days[k])
            };
            let gap = (edge - bulk).abs();
            if best.is_none_or(|(_, g)| gap > g) {
                best = Some((k, gap));
            }
        }
        let Some((k, gap)) = best else { continue };
        if (gap as f64) < MIN_TEMPORAL_GAP_IQR * iqr {
            continue;
        }
        let (extreme, bulk_edge) = if high {
            (days[n - 1], days[n - k - 1])
        } else {
            (days[0], days[k])
        };
        out.push(statement(
            name,
            "temporal_outlier",
            format!(
                "column {name} has {k} value{} {} {}, while the other {} {} {}",
                if k == 1 { "" } else { "s" },
                if high { "reaching" } else { "going back to" },
                dateutil::iso_date(extreme),
                n - k,
                if high { "end by" } else { "start at" },
                dateutil::iso_date(bulk_edge),
            ),
            0.76,
            BTreeMap::from([
                ("n".to_string(), json!(n)),
                ("rows".to_string(), json!(k)),
                ("extreme".to_string(), json!(dateutil::iso_date(extreme))),
                (
                    "bulk_edge".to_string(),
                    json!(dateutil::iso_date(bulk_edge)),
                ),
                ("gap_days".to_string(), json!(gap)),
            ]),
            "check for placeholder dates and entry errors",
        ));
    }
    out
}

/// One pass over every column's values.
pub fn column_content(table: &TypedTable) -> Vec<Finding> {
    let per_column: Vec<Vec<Finding>> = table
        .cols
        .par_iter()
        .enumerate()
        .map(|(i, col)| {
            let name = table.names[i].as_str();
            match col {
                TypedCol::Str { .. } | TypedCol::StrOwned { .. } => {
                    text_findings(name, col, table.n)
                }
                TypedCol::I64 { .. } | TypedCol::U64 { .. } | TypedCol::F64 { .. } => {
                    numeric_findings(name, col, table.n)
                }
                TypedCol::Date { .. } | TypedCol::Datetime { .. } => {
                    temporal_findings(name, col, table.n)
                }
                _ => Vec::new(),
            }
        })
        .collect();
    per_column.into_iter().flatten().collect()
}
