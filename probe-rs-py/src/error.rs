//! Bridges probe-rs's various `Error` enums into a single Python exception type.
//!
//! probe-rs returns different error enums from different subsystems
//! (`probe_rs::Error`, `probe_rs::flashing::FileDownloadError`, the RTT
//! module's `Error`, and `DebugProbeError` from raw JTAG calls). Rather than
//! mapping each one to a distinct Python exception class (which would force
//! Python users to import and catch several exception types for what is,
//! from their side, just "the probe operation failed"), we collapse all of
//! them into one `ProbeRsError` Python exception carrying the original
//! Rust error's Display string. If you later want finer-grained handling on
//! the Python side (e.g. distinguishing "probe not found" from "memory
//! fault"), extend this to inspect the error variant and pick a subclass.

use std::fmt::Display;

use itertools::Itertools;
use pyo3::{PyErr, create_exception, exceptions::PyException};

create_exception!(probe_rs_py, ProbeRsError, PyException);

/// Convert any error implementing `std::fmt::Display` into a `ProbeRsError`.
/// Used at every FFI boundary call site via `.map_err(to_py_err)`.
pub fn to_py_err<E: std::error::Error>(err: E) -> PyErr {
    if err.source().is_none() {
        ProbeRsError::new_err(err.to_string())
    } else {
        let err_msg = std::iter::successors(Some(&err as &dyn std::error::Error), |&e| e.source())
            .enumerate()
            .map(|(i, e)| format!("\n{i}: {e}"))
            .join("");
        ProbeRsError::new_err(err_msg)
    }
}

pub fn str_to_py_err<E: Display>(err: E) -> PyErr {
    ProbeRsError::new_err(err.to_string())
}
