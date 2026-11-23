//! Python bindings for onceover: thin PyO3 wrapper over the Rust core.
//! Zero-copy via pyo3-polars FFI interchange; GIL released during compute.

use std::time::Duration;

use onceover::{profile, Budget, MultiTableReport, ProfileConfig};
use onceover_report::{
    compact_report_value, diff_report_values, digest_numbers, finding_by_id_or_handle,
    finding_slot_in_value, quality_line, render_digest_value, select_findings_value,
    to_json_budgeted_value, ProfileReport, SelectFilter,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_polars::PyDataFrame;
use serde_json::Value;

fn on_request_ids() -> Vec<String> {
    onceover::on_request_detector_ids()
}

/// Native single-table report handle. All rendering, compacting, budgeting,
/// diffing, lookup, and filtering run in Rust.
#[pyclass(name = "ProfileReport")]
struct ProfileReportPy {
    inner: ProfileReport,
}

#[pymethods]
impl ProfileReportPy {
    /// The complete, compact, or size-budgeted report as JSON text.
    #[pyo3(signature = (pretty=None, compact=false, max_chars=None))]
    fn to_json(
        &self,
        pretty: Option<bool>,
        compact: bool,
        max_chars: Option<usize>,
    ) -> PyResult<String> {
        if let Some(limit) = max_chars {
            return self
                .inner
                .to_json_budgeted(limit, compact)
                .map_err(PyValueError::new_err);
        }
        if compact {
            return Ok(self.inner.to_json_compact());
        }
        if pretty.unwrap_or(false) {
            Ok(self.inner.to_json_pretty())
        } else {
            Ok(self.inner.to_json())
        }
    }

    /// Compact Markdown rendering for humans/agents.
    fn to_markdown(&self) -> String {
        self.inner.to_markdown()
    }

    /// Context-budgeted plain-text digest.
    #[pyo3(signature = (budget_chars=4000, compact=false, include_schema=false))]
    fn to_digest(&self, budget_chars: usize, compact: bool, include_schema: bool) -> String {
        let on_req = on_request_ids();
        let refs: Vec<&str> = on_req.iter().map(String::as_str).collect();
        self.inner
            .to_digest(budget_chars, compact, include_schema, &refs)
    }

    /// Token-economical LLM brief (schema strip + short `[f1]` handles).
    #[pyo3(signature = (budget_chars=2500))]
    fn to_brief(&self, budget_chars: usize) -> String {
        let on_req = on_request_ids();
        let refs: Vec<&str> = on_req.iter().map(String::as_str).collect();
        self.inner.to_brief(budget_chars, &refs)
    }

    /// Resolve a stable ID or short handle (`"f1"`, `"g1"`) to
    /// `(is_grouped, index)` without re-serializing the finding.
    fn finding_slot(&self, finding_id: &str) -> Option<(bool, usize)> {
        self.inner.finding_slot(finding_id)
    }

    /// Look up one finding by stable ID or short handle (`"f1"`, `"g1"`),
    /// returned as JSON text (or None).
    fn finding(&self, finding_id: &str) -> Option<String> {
        self.inner
            .finding(finding_id)
            .map(|f| serde_json::to_string(f).expect("finding serialization"))
    }

    /// Filter findings by category, type, column, target, or facets;
    /// returned as JSON array text.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (*, category=None, r#type=None, column=None, target=None, scope=None, object=None, support=None, conditionality=None, temporality=None, quality_dimension=None, status=None, include_grouped=false))]
    fn select(
        &self,
        category: Option<&str>,
        r#type: Option<&str>,
        column: Option<&str>,
        target: Option<&str>,
        scope: Option<&str>,
        object: Option<&str>,
        support: Option<&str>,
        conditionality: Option<&str>,
        temporality: Option<&str>,
        quality_dimension: Option<&str>,
        status: Option<&str>,
        include_grouped: bool,
    ) -> String {
        let v = serde_json::to_value(&self.inner).expect("report value");
        let filter = SelectFilter {
            category,
            finding_type: r#type,
            column,
            target,
            scope,
            object,
            support,
            conditionality,
            temporality,
            quality_dimension,
            status,
            include_grouped,
        };
        serde_json::to_string(&select_findings_value(&v, &filter)).expect("select serialization")
    }

    /// Diff against another native ProfileReport; returned as JSON text.
    fn diff(&self, other: &ProfileReportPy) -> String {
        serde_json::to_string(&self.inner.diff(&other.inner)).expect("diff serialization")
    }

    fn __repr__(&self) -> String {
        format!(
            "<onceover.ProfileReport: {} findings, {}>",
            self.inner.findings.len(),
            self.inner.overview.headline
        )
    }
}

/// Native multi-table report handle (`profile_many`).
#[pyclass(name = "MultiTableReport")]
struct MultiTableReportPy {
    inner: MultiTableReport,
}

#[pymethods]
impl MultiTableReportPy {
    #[pyo3(signature = (max_chars=None))]
    fn to_json(&self, max_chars: Option<usize>) -> PyResult<String> {
        match max_chars {
            Some(limit) => self
                .inner
                .to_json_budgeted(limit)
                .map_err(PyValueError::new_err),
            None => Ok(self.inner.to_json()),
        }
    }

    #[pyo3(signature = (budget_chars=4000))]
    fn to_digest(&self, budget_chars: usize) -> String {
        self.inner.to_digest(budget_chars)
    }
}

/// Parse the hints JSON, accepting the pre-0.8 key names (`axis`, `ordinal`)
/// as aliases for one release.
fn parse_hints(text: &str) -> PyResult<onceover_report::ProfileHints> {
    let mut value: Value = serde_json::from_str(text)
        .map_err(|e| PyValueError::new_err(format!("invalid hints: {e}")))?;
    if !value.is_object() {
        return Err(PyValueError::new_err(
            "invalid hints: expected an object keyed by hint name",
        ));
    }
    let mut deprecated_keys: Vec<String> = Vec::new();
    if let Some(obj) = value.as_object_mut() {
        for (old, new) in [("axis", "ordered_axis"), ("ordinal", "level_order")] {
            if let Some(v) = obj.remove(old) {
                deprecated_keys.push(old.to_string());
                obj.entry(new.to_string()).or_insert(v);
            }
        }
    }
    let mut hints: onceover_report::ProfileHints = serde_json::from_value(value)
        .map_err(|e| PyValueError::new_err(format!("invalid hints: {e}")))?;
    hints.deprecated_keys = deprecated_keys;
    Ok(hints)
}

fn parse_report_json(text: &str) -> PyResult<Value> {
    serde_json::from_str(text)
        .map_err(|e| PyValueError::new_err(format!("invalid report json: {e}")))
}

/// Give a Polars DataFrame a once-over.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(name = "profile")]
#[pyo3(signature = (df, budget=None, deadline_seconds=None, *, sample_rows=None, max_predictors=None, max_pairs=None, detectors=None, max_findings=100, max_rules=20, hints=None, target=None, timing=false, source=None))]
fn profile_py(
    py: Python<'_>,
    df: PyDataFrame,
    budget: Option<&str>,
    deadline_seconds: Option<f64>,
    sample_rows: Option<usize>,
    max_predictors: Option<usize>,
    max_pairs: Option<usize>,
    detectors: Option<Vec<String>>,
    max_findings: usize,
    max_rules: usize,
    hints: Option<String>,
    target: Option<String>,
    timing: bool,
    source: Option<String>,
) -> PyResult<ProfileReportPy> {
    for (name, value) in [
        ("sample_rows", sample_rows),
        ("max_predictors", max_predictors),
        ("max_pairs", max_pairs),
    ] {
        if value == Some(0) {
            return Err(PyValueError::new_err(format!("{name} must be positive")));
        }
    }
    if max_findings == 0 {
        return Err(PyValueError::new_err("max_findings must be positive"));
    }
    let known: std::collections::BTreeSet<_> = onceover::builtin_detectors()
        .into_iter()
        .map(|d| d.id)
        .collect();
    if let Some(ids) = &detectors {
        for id in ids {
            if !known.contains(id) {
                let registered: Vec<&str> = known.iter().map(String::as_str).collect();
                return Err(PyValueError::new_err(format!(
                    "unknown detector: {id} (registered detectors: {})",
                    registered.join(", ")
                )));
            }
        }
    }
    let budget_name = budget.unwrap_or("standard");
    let b = Budget::parse(budget_name).map_err(PyValueError::new_err)?;
    let deadline = match deadline_seconds {
        Some(s) if s < 0.0 || !s.is_finite() => {
            return Err(PyValueError::new_err(
                "deadline_seconds must be finite and >= 0",
            ))
        }
        Some(s) => Some(Duration::try_from_secs_f64(s).map_err(|_| {
            PyValueError::new_err("deadline_seconds must be representable as a duration")
        })?),
        None => None,
    };
    let hints = match hints {
        Some(text) => Some(parse_hints(&text)?),
        None => None,
    };
    let source = match source {
        Some(text) => Some(
            serde_json::from_str::<onceover_report::SourceInfo>(&text)
                .map_err(|e| PyValueError::new_err(format!("invalid source: {e}")))?,
        ),
        None => None,
    };
    let cfg = ProfileConfig {
        budget: b,
        detectors: detectors.map(|ids| ids.into_iter().collect()),
        selection: onceover::SelectionPolicy {
            max_findings,
            max_rules,
            ..Default::default()
        },
        deadline,
        sample_rows,
        max_predictors,
        max_pairs,
        hints,
        target,
        timing,
        source,
    };
    let df = df.0;
    let inner = py.detach(move || profile(&df, &cfg));
    Ok(ProfileReportPy { inner })
}

#[pyfunction]
#[pyo3(name = "profile_many", signature = (tables, *, overlap_threshold=0.9))]
fn profile_many_py(
    py: Python<'_>,
    tables: Vec<(String, PyDataFrame)>,
    overlap_threshold: f64,
) -> PyResult<MultiTableReportPy> {
    let raw: Vec<(String, polars::prelude::DataFrame)> =
        tables.into_iter().map(|(n, df)| (n, df.0)).collect();
    let inner = py
        .detach(move || onceover::profile_many(&raw, overlap_threshold))
        .map_err(PyValueError::new_err)?;
    Ok(MultiTableReportPy { inner })
}

#[pyfunction]
#[pyo3(name = "log_choose")]
fn log_choose_py(n: u128, k: u128) -> f64 {
    onceover::log_choose(n, k)
}

#[pyfunction]
#[pyo3(name = "infer_spec", signature = (df, hints=None, target=None))]
fn infer_spec_py(
    py: Python<'_>,
    df: PyDataFrame,
    hints: Option<String>,
    target: Option<String>,
) -> PyResult<String> {
    let hints = match hints {
        Some(text) => Some(parse_hints(&text)?),
        None => None,
    };
    let df = df.0;
    let spec = py.detach(move || onceover::infer_spec(&df, hints.as_ref(), target.as_deref()));
    serde_json::to_string(&spec).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(name = "apply_spec", signature = (df, spec_json, hints=None, target=None))]
fn apply_spec_py(
    py: Python<'_>,
    df: PyDataFrame,
    spec_json: String,
    hints: Option<String>,
    target: Option<String>,
) -> PyResult<(String, Option<String>)> {
    let spec: Value = serde_json::from_str(&spec_json)
        .map_err(|e| PyValueError::new_err(format!("invalid spec: {e}")))?;
    let hints = match hints {
        Some(text) => Some(parse_hints(&text)?),
        None => None,
    };
    let df = df.0;
    let (resolved_hints, final_target) = py
        .detach(move || onceover::apply_spec(&df, &spec, hints.as_ref(), target.as_deref()))
        .map_err(PyValueError::new_err)?;
    let hints_json = serde_json::to_string(&resolved_hints)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok((hints_json, final_target))
}

#[pyfunction]
#[pyo3(signature = (report_json, budget_chars=4000, compact=false, include_schema=false))]
fn render_digest_json(
    report_json: &str,
    budget_chars: usize,
    compact: bool,
    include_schema: bool,
) -> PyResult<String> {
    let val = parse_report_json(report_json)?;
    let on_req = on_request_ids();
    let refs: Vec<&str> = on_req.iter().map(String::as_str).collect();
    Ok(render_digest_value(
        &val,
        budget_chars,
        compact,
        include_schema,
        &refs,
    ))
}

#[pyfunction]
#[pyo3(name = "digest_numbers")]
fn digest_numbers_py(text: &str) -> String {
    digest_numbers(text)
}

#[pyfunction]
fn quality_line_json(report_json: &str) -> PyResult<Option<String>> {
    let val = parse_report_json(report_json)?;
    Ok(quality_line(&val))
}

#[pyfunction]
fn compact_report_json(report_json: &str) -> PyResult<String> {
    let val = parse_report_json(report_json)?;
    serde_json::to_string(&compact_report_value(&val))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(signature = (report_json, max_chars, compact=false))]
fn budgeted_report_json(report_json: &str, max_chars: usize, compact: bool) -> PyResult<String> {
    let val = parse_report_json(report_json)?;
    to_json_budgeted_value(&val, max_chars, compact).map_err(PyValueError::new_err)
}

#[pyfunction]
fn diff_reports_json(base_json: &str, other_json: &str) -> PyResult<String> {
    let a = parse_report_json(base_json)?;
    let b = parse_report_json(other_json)?;
    serde_json::to_string(&diff_report_values(&a, &b))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction]
fn finding_slot_from_json(report_json: &str, finding_id: &str) -> PyResult<Option<(bool, usize)>> {
    let val = parse_report_json(report_json)?;
    Ok(finding_slot_in_value(&val, finding_id))
}

#[pyfunction]
fn finding_from_json(report_json: &str, finding_id: &str) -> PyResult<Option<String>> {
    let val = parse_report_json(report_json)?;
    Ok(finding_by_id_or_handle(&val, finding_id)
        .map(|f| serde_json::to_string(f).expect("finding serialization")))
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (report_json, *, category=None, r#type=None, column=None, target=None, scope=None, object=None, support=None, conditionality=None, temporality=None, quality_dimension=None, status=None, include_grouped=false))]
fn select_from_json(
    report_json: &str,
    category: Option<&str>,
    r#type: Option<&str>,
    column: Option<&str>,
    target: Option<&str>,
    scope: Option<&str>,
    object: Option<&str>,
    support: Option<&str>,
    conditionality: Option<&str>,
    temporality: Option<&str>,
    quality_dimension: Option<&str>,
    status: Option<&str>,
    include_grouped: bool,
) -> PyResult<String> {
    let val = parse_report_json(report_json)?;
    let filter = SelectFilter {
        category,
        finding_type: r#type,
        column,
        target,
        scope,
        object,
        support,
        conditionality,
        temporality,
        quality_dimension,
        status,
        include_grouped,
    };
    serde_json::to_string(&select_findings_value(&val, &filter))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(name = "partition_scagnostics", signature = (df, *, pairs=None, partition_by=None, metrics=None, seed=0))]
fn partition_scagnostics_py(
    py: Python<'_>,
    df: PyDataFrame,
    pairs: Option<Vec<[String; 2]>>,
    partition_by: Option<Vec<String>>,
    metrics: Option<Vec<String>>,
    seed: u64,
) -> PyResult<String> {
    use onceover::partition_scagnostics::{default_metrics, Metric};

    let metrics = match metrics {
        Some(names) => names
            .iter()
            .map(|name| name.parse::<Metric>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(PyValueError::new_err)?,
        None => default_metrics(),
    };
    let df = df.0;
    let result = py
        .detach(move || {
            onceover::partition_scagnostics(
                &df,
                pairs.as_deref(),
                partition_by.as_deref(),
                metrics,
                seed,
            )
        })
        .map_err(PyValueError::new_err)?;
    serde_json::to_string(&result).map_err(|err| PyRuntimeError::new_err(err.to_string()))
}

#[pyfunction]
fn available_detectors() -> String {
    serde_json::to_string(&onceover::builtin_detectors()).expect("serializable detector registry")
}

/// The hint keys the hints parser accepts, in declaration order.
#[pyfunction]
fn hint_keys() -> Vec<String> {
    onceover_report::HINT_KEYS
        .iter()
        .map(|k| k.to_string())
        .collect()
}

/// The finding taxonomy as a JSON string: every pattern with its
/// type, derived category, and facets, plus the registered forms
/// and the facet vocabularies.
#[pyfunction]
fn taxonomy() -> String {
    onceover_report::taxonomy_json()
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(profile_py, m)?)?;
    m.add_function(wrap_pyfunction!(profile_many_py, m)?)?;
    m.add_function(wrap_pyfunction!(log_choose_py, m)?)?;
    m.add_function(wrap_pyfunction!(infer_spec_py, m)?)?;
    m.add_function(wrap_pyfunction!(apply_spec_py, m)?)?;
    m.add_function(wrap_pyfunction!(render_digest_json, m)?)?;
    m.add_function(wrap_pyfunction!(digest_numbers_py, m)?)?;
    m.add_function(wrap_pyfunction!(quality_line_json, m)?)?;
    m.add_function(wrap_pyfunction!(compact_report_json, m)?)?;
    m.add_function(wrap_pyfunction!(budgeted_report_json, m)?)?;
    m.add_function(wrap_pyfunction!(diff_reports_json, m)?)?;
    m.add_function(wrap_pyfunction!(finding_slot_from_json, m)?)?;
    m.add_function(wrap_pyfunction!(finding_from_json, m)?)?;
    m.add_function(wrap_pyfunction!(select_from_json, m)?)?;
    m.add_function(wrap_pyfunction!(partition_scagnostics_py, m)?)?;
    m.add_function(wrap_pyfunction!(available_detectors, m)?)?;
    m.add_function(wrap_pyfunction!(hint_keys, m)?)?;
    m.add_function(wrap_pyfunction!(taxonomy, m)?)?;
    m.add_class::<ProfileReportPy>()?;
    m.add_class::<MultiTableReportPy>()?;
    Ok(())
}
