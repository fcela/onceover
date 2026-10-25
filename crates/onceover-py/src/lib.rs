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
#[pyo3(name = "profile")]
#[pyo3(signature = (df, budget=None, deadline_seconds=None))]
fn profile_py(
    py: Python<'_>,
    df: PyDataFrame,
    budget: Option<&str>,
    deadline_seconds: Option<f64>,
) -> PyResult<ProfileReportPy> {
    let budget_name = budget.unwrap_or("standard");
    let b = Budget::parse(budget_name).map_err(PyValueError::new_err)?;
    let deadline = match deadline_seconds {
        Some(s) if s < 0.0 => return Err(PyValueError::new_err("deadline_seconds must be >= 0")),
        Some(s) => Some(Duration::from_secs_f64(s)),
        None => None,
    };
    let cfg = ProfileConfig {
        budget: b,
        deadline,
    };
    // Zero-copy extraction happens first (cheap), then the GIL is released
    // for the heavy, all-Rust compute.
    let df = df.0;
    let inner = py.detach(move || profile(&df, &cfg));
    Ok(ProfileReportPy { inner })
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(profile_py, m)?)?;
    m.add_class::<ProfileReportPy>()?;
    Ok(())
}
