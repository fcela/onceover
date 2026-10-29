//! Python bindings for onceover: `profile(df)` over a Polars DataFrame,
//! zero-copy via pyo3-polars' FFI interchange, GIL released during compute.

use std::time::Duration;

use onceover::{profile, Budget, ProfileConfig};
use onceover_report::ProfileReport;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3_polars::PyDataFrame;

/// The native report handle. The Python layer (`onceover.ProfileReport`)
/// parses `to_json()` for rich dict access.
#[pyclass(name = "ProfileReport")]
struct ProfileReportPy {
    inner: ProfileReport,
}

#[pymethods]
impl ProfileReportPy {
    /// The complete report as a JSON string (schema-versioned, deterministic).
    #[pyo3(signature = (pretty=None))]
    fn to_json(&self, pretty: Option<bool>) -> String {
        if pretty.unwrap_or(false) {
            self.inner.to_json_pretty()
        } else {
            self.inner.to_json()
        }
    }

    /// Compact Markdown rendering for humans/agents.
    fn to_markdown(&self) -> String {
        self.inner.to_markdown()
    }

    fn __repr__(&self) -> String {
        format!(
            "<onceover.ProfileReport: {} findings, {}>",
            self.inner.findings.len(),
            self.inner.overview.headline
        )
    }
}

/// Give a Polars DataFrame a once-over.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(name = "profile")]
#[pyo3(signature = (df, budget=None, deadline_seconds=None, *, sample_rows=None, max_predictors=None, max_pairs=None, detectors=None, max_findings=100, max_rules=20, hints=None, target=None))]
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
                return Err(PyValueError::new_err(format!("unknown detector: {id}")));
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
        Some(s) => Some(Duration::from_secs_f64(s)),
        None => None,
    };
    let hints = match hints {
        Some(text) => Some(
            serde_json::from_str::<onceover_report::ProfileHints>(&text)
                .map_err(|e| PyValueError::new_err(format!("invalid hints: {e}")))?,
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
    };
    // Zero-copy extraction happens first (cheap), then the GIL is released
    // for the heavy, all-Rust compute.
    let df = df.0;
    let inner = py.detach(move || profile(&df, &cfg));
    Ok(ProfileReportPy { inner })
}

#[pyfunction]
fn available_detectors() -> String {
    serde_json::to_string(&onceover::builtin_detectors()).expect("serializable detector registry")
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(profile_py, m)?)?;
    m.add_function(wrap_pyfunction!(available_detectors, m)?)?;
    m.add_class::<ProfileReportPy>()?;
    Ok(())
}
