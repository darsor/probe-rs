//! Wraps `probe_rs::rtt::Rtt` for bidirectional RTT channel access.
//!
//! Design note: in this fork's API, `UpChannel::read` and `DownChannel::write`
//! both take a `&mut Core` as an argument on every call (RTT channels don't
//! hold their own core handle internally). That means a `PyRtt` object can't
//! just wrap an `Rtt` in isolation -- every read/write needs a live `Core`,
//! which itself borrows from the `Session`. To avoid fighting the borrow
//! checker across the Python FFI boundary, `PyRtt` stores the owning
//! `Session` directly (taken from the Python `Session` object at attach
//! time) rather than trying to hold a `Core<'_>` or share `&mut Session`
//! with `PySession`. This means once you call `session.attach_rtt()`, you
//! get back a new `Rtt` handle object; the original `Session` object in
//! Python and the `Rtt` object both still point at the same underlying
//! connection, but you should avoid interleaving raw memory calls and RTT
//! calls in a racy way across two object handles. In practice this is fine
//! for typical usage (you either drive things via plain memory/run-control
//! calls, or you attach RTT and drive the device through that).

use probe_rs::rtt::Rtt as RsRtt;
use pyo3::prelude::*;

use crate::error::{str_to_py_err, to_py_err};
use crate::session::SharedSession;

#[pyclass(name = "Rtt")]
pub struct PyRtt {
    session: SharedSession,
    core_index: usize,
    rtt: RsRtt,
}

impl PyRtt {
    pub fn attach(session: SharedSession, core_index: usize) -> PyResult<Self> {
        let rtt = {
            let mut guard = session.lock().map_err(to_py_err)?;
            let mut core = guard.core(core_index).map_err(to_py_err)?;
            RsRtt::attach(&mut core).map_err(to_py_err)?
        };
        Ok(PyRtt {
            session,
            core_index,
            rtt,
        })
    }
}

#[pymethods]
impl PyRtt {
    /// List detected up (target -> host) channel indices.
    fn up_channel_indices(&mut self) -> Vec<usize> {
        self.rtt.up_channels().iter().map(|c| c.number()).collect()
    }

    /// List detected down (host -> target) channel indices.
    fn down_channel_indices(&mut self) -> Vec<usize> {
        self.rtt
            .down_channels()
            .iter()
            .map(|c| c.number())
            .collect()
    }

    /// Non-blocking read from an up channel. Returns whatever bytes are
    /// currently available (possibly empty), up to `max_len`. Call this
    /// in a polling loop; RTT has no blocking "wait for data" primitive
    /// since it's just a ring buffer in target RAM.
    #[pyo3(signature = (channel, max_len=1024))]
    fn read_up(&mut self, channel: usize, max_len: usize) -> PyResult<Vec<u8>> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| str_to_py_err("session lock poisoned"))?;
        let mut core = guard.core(self.core_index).map_err(to_py_err)?;

        let up = self
            .rtt
            .up_channel(channel)
            .ok_or_else(|| str_to_py_err(format!("no up channel {channel}")))?;

        let mut buf = vec![0u8; max_len];
        let n = up.read(&mut core, &mut buf).map_err(to_py_err)?;
        buf.truncate(n);
        Ok(buf)
    }

    /// Write bytes to a down channel (host -> target). Returns the number
    /// of bytes actually written (may be less than `len(data)` if the
    /// target-side buffer is full; call again with the remainder).
    fn write_down(&mut self, channel: usize, data: Vec<u8>) -> PyResult<usize> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| str_to_py_err("session lock poisoned"))?;
        let mut core = guard.core(self.core_index).map_err(to_py_err)?;

        let down = self
            .rtt
            .down_channel(channel)
            .ok_or_else(|| str_to_py_err(format!("no down channel {channel}")))?;

        down.write(&mut core, &data).map_err(to_py_err)
    }
}
