//! Wraps `probe_rs::probe::JtagAccess` for raw, register-level JTAG access
//! (IR/DR shifting, scan chain enumeration) -- below the level of memory
//! reads/writes and core control.
//!
//! Design note: raw JTAG access requires holding the `Probe` itself.
//! `Probe::attach(...)` *consumes* the `Probe` to produce a `Session`, so a
//! `Probe` used for raw JTAG and one used to form a `Session` are mutually
//! exclusive at any given moment -- you cannot have both a `Session` and a
//! `RawJtag` handle open on the same physical probe simultaneously. This is
//! a constraint in probe-rs itself, not an artifact of these bindings.
//!
//! If you need to do raw scan-chain operations *and* normal debugging on
//! the same target in one script, do them sequentially: open `RawJtag`,
//! do your scan-chain work, drop it (which releases the probe), then open
//! a `Session` for memory/RTT/run-control work, or vice versa.
//!
//! This exposes `JtagAccess`, the register/scan level (shift IR, shift DR,
//! enumerate the scan chain). It does not expose `RawJtagIo`, the
//! underlying single-bit TMS/TDI/TDO interface that some probes implement,
//! since that's a much rawer primitive mainly used by probe-rs's own
//! protocol implementations -- almost nothing outside probe-rs itself needs
//! to shift individual bits by hand. Ask if you specifically need that
//! level; it can be added the same way.

use probe_rs::probe::list::Lister;
use probe_rs::probe::{JtagAccess, Probe, WireProtocol};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::error::to_py_err;

#[pyclass(name = "RawJtag")]
pub struct PyRawJtag {
    probe: Probe,
}

impl PyRawJtag {
    fn jtag(&mut self) -> PyResult<&mut dyn JtagAccess> {
        self.probe
            .try_as_jtag_probe()
            .ok_or_else(|| to_py_err("probe does not support raw JTAG access"))
    }
}

#[pymethods]
impl PyRawJtag {
    /// Open the first available probe in raw JTAG mode (no target attach).
    /// `speed_khz` sets the JTAG clock if given.
    #[staticmethod]
    #[pyo3(signature = (probe_index=0, speed_khz=None))]
    fn open(probe_index: usize, speed_khz: Option<u32>) -> PyResult<Self> {
        let lister = Lister::new();
        let probes = lister.list_all();
        let probe_info = probes
            .get(probe_index)
            .ok_or_else(|| to_py_err(format!("No probe at index {probe_index}")))?;

        let mut probe = probe_info.open().map_err(to_py_err)?;
        probe
            .select_protocol(WireProtocol::Jtag)
            .map_err(to_py_err)?;
        if let Some(khz) = speed_khz {
            probe.set_speed(khz).map_err(to_py_err)?;
        }

        Ok(PyRawJtag { probe })
    }

    /// Reset the JTAG TAP state machine.
    fn tap_reset(&mut self) -> PyResult<()> {
        self.jtag()?.tap_reset().map_err(to_py_err)
    }

    /// Scan the chain and return the number of detected TAPs (devices).
    /// Use this to sanity-check wiring/connectivity before doing targeted
    /// register access.
    fn scan_chain_length(&mut self) -> PyResult<usize> {
        let chain = self.jtag()?.scan_chain().map_err(to_py_err)?;
        Ok(chain.len())
    }

    /// Select which TAP in the scan chain subsequent register operations
    /// target, by its position (0-indexed) in the chain.
    fn select_target(&mut self, index: usize) -> PyResult<()> {
        self.jtag()?.select_target(index).map_err(to_py_err)
    }

    /// Write to a JTAG register at the given IR `address`, shifting `data`
    /// (a bytes-like object, little-endian bit order matching probe-rs's
    /// `BitVec` convention) of `len_bits` bits into DR. Returns the bits
    /// that were shifted *out* of DR during the write, as bytes.
    fn write_register(
        &mut self,
        py: Python<'_>,
        address: u32,
        data: Vec<u8>,
        len_bits: u32,
    ) -> PyResult<Py<PyBytes>> {
        let bits = self
            .jtag()?
            .write_register(address, &data, len_bits)
            .map_err(to_py_err)?;
        Ok(PyBytes::new(py, &bits.into_vec()).into())
    }

    /// Read a JTAG register (emulated as a write of all-zero data; this
    /// matches probe-rs's own `read_register` semantics). Returns the
    /// shifted-out DR bits as bytes.
    fn read_register(
        &mut self,
        py: Python<'_>,
        address: u32,
        len_bits: u32,
    ) -> PyResult<Py<PyBytes>> {
        let bits = self
            .jtag()?
            .read_register(address, len_bits)
            .map_err(to_py_err)?;
        Ok(PyBytes::new(py, &bits.into_vec()).into())
    }

    /// Shift a value directly into the DR register without an IR write
    /// (assumes IR is already set correctly, e.g. by a prior
    /// `write_register` call). Returns the shifted-out bits as bytes.
    fn write_dr(&mut self, py: Python<'_>, data: Vec<u8>, len_bits: u32) -> PyResult<Py<PyBytes>> {
        let bits = self.jtag()?.write_dr(&data, len_bits).map_err(to_py_err)?;
        Ok(PyBytes::new(py, &bits.into_vec()).into())
    }
}
