//! probe_rs_py: Python bindings over the darsor/probe-rs fork.
//!
//! Exposes four capability areas as separate Python classes so usage stays
//! mutually independent and explicit about what's being accessed:
//!
//! - `Session`  -- attach to a target, read/write memory, run control,
//!                 flash an ELF, and obtain an `Rtt` handle.
//! - `Rtt`       -- bidirectional RTT up/down channel I/O, obtained via
//!                 `Session.attach_rtt()`.
//! - `RawJtag`   -- register-level raw JTAG access (scan chain, IR/DR
//!                 shifts), independent of `Session` since it needs
//!                 exclusive use of the `Probe` before any target attach.
//!
//! All error types from probe-rs are collapsed into a single
//! `probe_rs_py.ProbeRsError` Python exception; see `error.rs` for why.

mod error;
// mod jtag;
mod rtt;
mod session;

use pyo3::prelude::*;

#[pymodule]
fn probe_rs_py(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<session::PySession>()?;
    m.add_class::<rtt::PyRtt>()?;
    // m.add_class::<jtag::PyRawJtag>()?;
    m.add("ProbeRsError", py.get_type::<error::ProbeRsError>())?;
    Ok(())
}
