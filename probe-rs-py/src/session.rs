//! Wraps `probe_rs::Session` and per-core operations: memory read/write,
//! run control (halt/run/reset/step), and ELF flashing.
//!
//! Design notes:
//!
//! 1. Lifetimes: `probe_rs::Session::core(&mut self, idx)` returns a
//!    `Core<'_>` borrowing from the session. PyO3 classes can't easily hold
//!    a borrow into a sibling `#[pyclass]`, so rather than expose a separate
//!    `PyCore` class, every memory/run-control method below re-acquires the
//!    `Core` from the locked session for the duration of that single call.
//!    This mirrors how you'd write it in plain Rust anyway (you don't
//!    usually hold a `Core` across unrelated operations) and keeps the
//!    Python API simple: one `Session` object, methods take a `core_index`.
//!
//! 2. Sharing: `inner` is `Arc<Mutex<RsSession>>` rather than an owned
//!    `RsSession`, so that `attach_rtt()` can hand out a `PyRtt` object that
//!    shares the same underlying connection instead of trying to move or
//!    duplicate the `Session`. The mutex also means calls from Python are
//!    serialized against the same probe connection even if a user holds
//!    both a `Session` and an `Rtt` object and (incorrectly) calls into both
//!    from separate Python threads -- you'll get correctness, not deadlock,
//!    though you should still avoid doing that since probe transactions
//!    aren't designed to be interleaved at arbitrary granularity.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use probe_rs::flashing;
use probe_rs::probe::list::Lister;
use probe_rs::{MemoryInterface, Permissions, Session as RsSession, SessionConfig};
use pyo3::prelude::*;

use crate::error::{str_to_py_err, to_py_err};
use crate::rtt::PyRtt;

pub type SharedSession = Arc<Mutex<RsSession>>;

#[pyclass(name = "Session")]
pub struct PySession {
    pub(crate) inner: SharedSession,
}

impl PySession {
    fn lock(&self) -> PyResult<std::sync::MutexGuard<'_, RsSession>> {
        self.inner
            .lock()
            .map_err(|_| str_to_py_err("session lock poisoned"))
    }
}

#[pymethods]
impl PySession {
    /// Attach to the first available probe and the given target chip.
    ///
    /// `chip` is a probe-rs target name, e.g. "STM32F407VG" or "nRF52840_xxAA".
    /// Run the fork's CLI binary's `chip list` subcommand to see valid names
    /// for your build, since target support is what's most likely to have
    /// drifted in a fork relative to upstream.
    #[new]
    #[pyo3(signature = (chip, speed_khz=None))]
    fn new(chip: String, speed_khz: Option<u32>) -> PyResult<Self> {
        let mut session_config = SessionConfig {
            permissions: Permissions::default(),
            ..Default::default()
        };
        if let Some(khz) = speed_khz {
            session_config.speed = Some(khz);
        }

        let session = RsSession::auto_attach(chip, session_config).map_err(to_py_err)?;
        Ok(PySession {
            inner: Arc::new(Mutex::new(session)),
        })
    }

    /// Attach to a specific probe by its index in `list_probes()`, rather
    /// than just grabbing the first one found. Useful when more than one
    /// debug probe is plugged in.
    #[staticmethod]
    #[pyo3(signature = (chip, probe_index, speed_khz=None))]
    fn attach_to_probe(chip: String, probe_index: usize, speed_khz: Option<u32>) -> PyResult<Self> {
        let lister = Lister::new();
        let probes = lister.list_all();
        let probe_info = probes
            .get(probe_index)
            .ok_or_else(|| str_to_py_err(format!("No probe at index {probe_index}")))?;

        let mut probe = probe_info.open().map_err(to_py_err)?;
        if let Some(khz) = speed_khz {
            probe.set_speed(khz).map_err(to_py_err)?;
        }

        let session = probe
            .attach(chip, Permissions::default())
            .map_err(to_py_err)?;
        Ok(PySession {
            inner: Arc::new(Mutex::new(session)),
        })
    }

    /// List available debug probes as (index, identifier_string) pairs, so
    /// callers can pick one for `attach_to_probe`.
    #[staticmethod]
    fn list_probes() -> Vec<(usize, String)> {
        let lister = Lister::new();
        lister
            .list_all()
            .iter()
            .enumerate()
            .map(|(i, info)| (i, format!("{info:?}")))
            .collect()
    }

    /// Number of cores on the attached target.
    fn num_cores(&self) -> PyResult<usize> {
        Ok(self.lock()?.list_cores().len())
    }

    // ---- Memory access ---------------------------------------------

    /// Read a single 32-bit word from target memory.
    fn read_word_32(&mut self, core_index: usize, address: u64) -> PyResult<u32> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.read_word_32(address).map_err(to_py_err)
    }

    /// Read a single byte from target memory.
    fn read_word_8(&mut self, core_index: usize, address: u64) -> PyResult<u8> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.read_word_8(address).map_err(to_py_err)
    }

    /// Read `length` bytes starting at `address`, returned as Python `bytes`.
    fn read_memory(&mut self, core_index: usize, address: u64, length: usize) -> PyResult<Vec<u8>> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        let mut buf = vec![0u8; length];
        core.read(address, &mut buf).map_err(to_py_err)?;
        Ok(buf)
    }

    /// Write a single 32-bit word to target memory.
    fn write_word_32(&mut self, core_index: usize, address: u64, value: u32) -> PyResult<()> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.write_word_32(address, value).map_err(to_py_err)
    }

    /// Write a single byte to target memory.
    fn write_word_8(&mut self, core_index: usize, address: u64, value: u8) -> PyResult<()> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.write_word_8(address, value).map_err(to_py_err)
    }

    /// Write arbitrary bytes to target memory starting at `address`.
    fn write_memory(&mut self, core_index: usize, address: u64, data: Vec<u8>) -> PyResult<()> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.write(address, &data).map_err(to_py_err)
    }

    // ---- Run control -------------------------------------------------

    /// Halt the core. `timeout_ms` bounds how long probe-rs waits for the
    /// halt to take effect.
    #[pyo3(signature = (core_index, timeout_ms=100))]
    fn halt(&mut self, core_index: usize, timeout_ms: u64) -> PyResult<()> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.halt(Duration::from_millis(timeout_ms))
            .map_err(to_py_err)?;
        Ok(())
    }

    /// Resume free-running execution.
    fn run(&mut self, core_index: usize) -> PyResult<()> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.run().map_err(to_py_err)
    }

    /// Reset the core (and run immediately; does not stay halted).
    fn reset(&mut self, core_index: usize) -> PyResult<()> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.reset().map_err(to_py_err)
    }

    /// Reset the core and leave it halted at the reset vector.
    #[pyo3(signature = (core_index, timeout_ms=100))]
    fn reset_and_halt(&mut self, core_index: usize, timeout_ms: u64) -> PyResult<()> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.reset_and_halt(Duration::from_millis(timeout_ms))
            .map_err(to_py_err)?;
        Ok(())
    }

    /// Single-step the core by one instruction. Core must already be halted.
    fn step(&mut self, core_index: usize) -> PyResult<()> {
        let mut session = self.lock()?;
        let mut core = session.core(core_index).map_err(to_py_err)?;
        core.step().map_err(to_py_err)?;
        Ok(())
    }

    // ---- ELF loading ----------------------------------------------

    /// Flash an ELF file to the target. This erases/programs flash as
    /// needed via the target's CMSIS-Pack flash algorithm; it does not by
    /// itself start execution. Call `reset()` (or `reset_and_halt` then
    /// `run`) afterward to start running the freshly flashed image.
    fn flash_elf(&mut self, path: String) -> PyResult<()> {
        let mut session = self.lock()?;
        flashing::download_file(
            &mut session,
            PathBuf::from(path),
            flashing::ElfLoader(flashing::ElfOptions::default()),
        )
        .map_err(to_py_err)
    }

    /// Flash an ELF and immediately start it (reset, no halt).
    /// Convenience wrapper combining `flash_elf` + `reset`.
    #[pyo3(signature = (path, core_index=0))]
    fn flash_elf_and_run(&mut self, path: String, core_index: usize) -> PyResult<()> {
        self.flash_elf(path)?;
        self.reset(core_index)
    }

    // ---- RTT ---------------------------------------------------------

    /// Attach RTT on the given core and return an `Rtt` handle for reading
    /// up channels and writing down channels. The target firmware must
    /// already have initialized its RTT control block in RAM (typically via
    /// `rtt-target` or `defmt-rtt`) before this is called, or the control
    /// block scan will fail to find it -- if you just flashed and reset,
    /// give the firmware a brief moment to run its init code first.
    #[pyo3(signature = (core_index=0))]
    fn attach_rtt(&self, core_index: usize) -> PyResult<PyRtt> {
        PyRtt::attach(self.inner.clone(), core_index)
    }
}
