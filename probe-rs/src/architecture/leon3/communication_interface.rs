//! Interface for controlling LEON3 cores.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    CoreInformation, Error as ProbeRsError, MemoryInterface, RegisterId,
    architecture::leon3::{
        ahbjtag::AhbJtagError,
        dsu3::{Dsu3, Dsu3State, DsuCtrl, DsuRegister, Psr},
        peripherals::{Peripheral, irqmp::Irqmp},
        plugnplay::{self, Device, GaislerDevice},
        registers::Leon3RegisterId,
    },
    probe::DebugProbeError,
    session::BusAccess,
};

/// Some error occurred when working with the Leon3 core.
#[derive(thiserror::Error, Debug)]
pub enum Leon3Error {
    /// A timeout occurred during AHB access.
    #[error("Timeout during AHB access.")]
    Timeout,
    /// An error with operating the debug probe occurred.
    #[error("Debug Probe Error")]
    DebugProbe(#[from] DebugProbeError),
    /// Failed to scan plugnplay region.
    #[error("Failed to scan plug&play region")]
    PlugnPlayFailure {
        /// Source of the error
        source: Box<dyn std::error::Error + 'static + Send + Sync>,
    },
    /// DSU3 not found.
    #[error("DSU3 plug&play record not found")]
    Dsu3NotFound,
    /// Core out of range.
    #[error("Core index {core_index} out of range (max 15)")]
    CoreOutOfRange {
        /// The invalid core index that attempted access
        core_index: usize,
    },
    /// Invalid register ID.
    #[error("Invalid Register ID: {0:?}")]
    InvalidRegisterId(RegisterId),
    /// Reset halt request not supported by this chip.
    #[error("Reset halt request not supported")]
    ResetHaltRequestNotSupported,
    /// Breakpoint operation requested on invalid breakpoint
    #[error("Breakpoint {0} out of range (must be 0-4)")]
    BreakpointOutOfRange(usize),
    /// Error in AHBJTAG acces
    #[error("AHBJTAG access error")]
    AhbJtag(#[source] AhbJtagError),
    /// Some uncategorized LEON3 error occurred.
    #[error("{0}")]
    Other(&'static str),
}

impl From<Leon3Error> for ProbeRsError {
    fn from(err: Leon3Error) -> Self {
        match err {
            other => ProbeRsError::Leon3(other),
        }
    }
}

/// An interface that implements controls for Leon3 cores.
#[derive(Debug)]
pub struct Leon3CommunicationInterface<'state> {
    /// Which core we are controlling.
    ///
    /// Everything else in this struct specifically for the communication interface
    /// and doesn't change for different cores, but this temporary struct is constructed
    /// anew for each core we talk to.
    core_index: usize,
    probe: &'state mut BusAccess,
    pub(crate) dsu: Dsu3<'state>,
    pub(crate) peripherals: &'state Vec<Peripheral>,
}

impl<'state> Leon3CommunicationInterface<'state> {
    /// Construct a new communication interface
    pub(crate) fn try_attach(
        core_index: usize,
        probe: &'state mut BusAccess,
        state: &'state mut Leon3DebugInterfaceState,
    ) -> Result<Self, crate::Error> {
        let Leon3DebugInterfaceState {
            dsu: dsu_state,
            peripherals,
        } = state;
        let dsu = Dsu3::new(dsu_state);

        Ok(Self {
            core_index,
            probe,
            dsu,
            peripherals,
        })
    }

    /// Use the communication interface as a memory interface.
    pub fn as_memory_interface(&self) -> &dyn MemoryInterface {
        self.probe
    }

    /// Use the communication interface as a memory interface.
    pub fn as_memory_interface_mut(&mut self) -> &mut dyn MemoryInterface {
        self.probe
    }

    pub(crate) fn on_first_attach(&mut self) -> Result<(), crate::Error> {
        // From DSU3 section in GRLIB IP Core User's Manual:
        //   For the break-now BN bit to have effect the Break-on-IU-watchpoint
        //   (BW) bit must be set in the DSU control register.  This bit should
        //   be set by debug monitor software when initializing the DSU.
        Ok(self
            .dsu
            .modify_reg::<DsuCtrl, _>(self.probe, self.core_index, |ctrl| {
                ctrl.set_bw(true);
            })?)
    }

    pub(crate) fn core_halted(&mut self) -> Result<bool, crate::Error> {
        let ctrl: DsuCtrl = self.read_dsu_reg()?;
        Ok(ctrl.hl() || ctrl.pe() || ctrl.dm())
    }

    pub(crate) fn core_in_debug_mode(&mut self) -> Result<bool, crate::Error> {
        let ctrl: DsuCtrl = self.read_dsu_reg()?;
        Ok(ctrl.dm())
    }

    pub(crate) fn read_dsu_reg<R: DsuRegister>(&mut self) -> Result<R, crate::Error> {
        self.dsu.read_reg(self.probe, self.core_index)
    }

    pub(crate) fn write_dsu_reg<R: DsuRegister>(&mut self, value: R) -> Result<(), crate::Error> {
        self.dsu.write_reg(value, self.probe, self.core_index)
    }

    /// Read-modify-write a DSU register.
    pub fn modify_dsu_reg<R: DsuRegister, T>(
        &mut self,
        f: impl Fn(&mut R) -> T,
    ) -> Result<T, crate::Error> {
        self.dsu.modify_reg(self.probe, self.core_index, f)
    }

    /// Read a LEON3 core register.
    pub fn read_core_reg(&mut self, reg: Leon3RegisterId) -> Result<u32, crate::Error> {
        match reg {
            Leon3RegisterId::IuCore(iu_core_reg) => {
                // TODO(darsor): cache this
                let psr: Psr = self.read_dsu_reg()?;
                let cwp = psr.cwp();
                self.dsu
                    .read_core_reg(iu_core_reg, self.probe, self.core_index, cwp)
            }
            Leon3RegisterId::IuSpecial(iu_special_reg) => {
                self.dsu
                    .read_special_reg(iu_special_reg, self.probe, self.core_index)
            }
            Leon3RegisterId::Fpu(_fpu_reg) => todo!(),
        }
    }

    /// Write a LEON3 core register.
    pub fn write_core_reg(&mut self, reg: Leon3RegisterId, value: u32) -> Result<(), crate::Error> {
        match reg {
            Leon3RegisterId::IuCore(iu_core_reg) => {
                // TODO(darsor): cache this
                let psr: Psr = self.read_dsu_reg()?;
                let cwp = psr.cwp();
                self.dsu
                    .write_core_reg(iu_core_reg, value, self.probe, self.core_index, cwp)
            }
            Leon3RegisterId::IuSpecial(iu_special_reg) => {
                self.dsu
                    .write_special_reg(iu_special_reg, value, self.probe, self.core_index)
            }
            Leon3RegisterId::Fpu(_fpu_reg) => todo!(),
        }
    }

    pub(crate) fn clear_all_core_reg(&mut self) -> Result<(), crate::Error> {
        self.dsu.clear_all_core_reg(self.probe, self.core_index)
    }

    pub(crate) fn set_hw_breakpoint(
        &mut self,
        unit_index: usize,
        addr: u64,
        enable: bool,
    ) -> Result<(), crate::Error> {
        self.dsu
            .set_hw_breakpoint(self.probe, self.core_index, unit_index, addr, enable)
    }

    pub(crate) fn get_hw_breakpoint(
        &mut self,
        unit_index: usize,
    ) -> Result<Option<u64>, crate::Error> {
        self.dsu
            .get_hw_breakpoint(self.probe, self.core_index, unit_index)
    }

    pub(crate) fn wait_for_core_halted(&mut self, timeout: Duration) -> Result<(), crate::Error> {
        // Wait until halted state is active again.
        let start = Instant::now();

        while !self.core_halted()? {
            if start.elapsed() >= timeout {
                return Err(crate::Error::Leon3(Leon3Error::Timeout));
            }
            // Wait a bit before polling again.
            std::thread::sleep(Duration::from_millis(1));
        }

        Ok(())
    }

    pub(crate) fn core_info(&mut self) -> Result<CoreInformation, crate::Error> {
        let pc: u32 = self.read_core_reg(super::registers::PC.id().try_into()?)?;

        Ok(CoreInformation { pc: pc.into() })
    }

    pub(crate) fn flush_caches(&mut self) -> Result<(), crate::Error> {
        self.dsu.flush_caches(self.probe, self.core_index)
    }
}

/// The combined state of a LEON3's DSU3 debug module and its transport interface.
#[derive(Debug)]
pub(crate) struct Leon3DebugInterfaceState {
    dsu: Dsu3State,
    peripherals: Vec<Peripheral>,
}

impl Leon3DebugInterfaceState {
    pub fn try_attach<'probe>(
        probe: &'probe mut dyn MemoryInterface,
    ) -> Result<Self, crate::Error> {
        let mut plugnplay = plugnplay::scan_plugnplay(probe)?;
        let dsu_idx = plugnplay
            .iter()
            .position(|record| record.device == Device::Gaisler(GaislerDevice::LEON3DSU))
            .ok_or(Leon3Error::Dsu3NotFound)?;
        let dsu3_record = plugnplay.swap_remove(dsu_idx);
        let dsu3_base_address = dsu3_record
            .address_spaces
            .first()
            .ok_or(Leon3Error::Dsu3NotFound)?
            .addresses
            .start;

        let peripherals = plugnplay
            .into_iter()
            .map(|record| match record {
                plugnplay::Record {
                    device: Device::Gaisler(GaislerDevice::IRQMP),
                    ..
                } => Peripheral::Handled(Arc::new(Irqmp::new(record))),
                _ => Peripheral::Unhandled(record),
            })
            .collect();

        Ok(Self {
            dsu: Dsu3State::new(dsu3_base_address),
            peripherals,
        })
    }
}
