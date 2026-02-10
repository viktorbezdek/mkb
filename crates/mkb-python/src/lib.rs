//! # mkb-python
//!
//! PyO3 bridge for MKB. Thin translation layer exposing Rust functionality
//! to Python. No business logic here — just type conversion and FFI.

use pyo3::prelude::*;

/// MKB Python module.
#[pymodule]
fn _mkb_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
