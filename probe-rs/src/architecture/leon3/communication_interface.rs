use std::time::{Duration, Instant};

use crate::{
    CoreInformation, Error as ProbeRsError, MemoryInterface, MemoryMappedRegister, RegisterId,
    architecture::leon3::{
        dsu3::{Dsu3, Dsu3State, DsuCtrl, Psr},
        plugnplay::{Device, GaislerDevice, PlugnPlayState},
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
    /// A region outside of the AHB address space was accessed.
    #[error("Out of bounds memory access")]
    OutOfBounds,
    /// Failed to scan plugnplay region.
    #[error("Failed to scan plug&play region")]
    PlugnPlayFailure {
        source: Box<dyn std::error::Error + 'static + Send + Sync>,
    },
    /// DSU3 not found.
    #[error("DSU3 plug&play record not found")]
    Dsu3NotFound,
    /// Core out of range.
    #[error("Core index {core_index} out of range (max 15)")]
    CoreOutOfRange { core_index: usize },
    /// Invalid register ID.
    #[error("Invalid Register ID: {0:?}")]
    InvalidRegisterId(RegisterId),
    /// Reset halt request not supported by this chip.
    #[error("Reset halt request not supported")]
    ResetHaltRequestNotSupported,
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
    plugnplay: &'state PlugnPlayState,
}

impl<'state> Leon3CommunicationInterface<'state> {
    pub fn try_attach(
        core_index: usize,
        probe: &'state mut BusAccess,
        state: &'state mut Leon3DebugInterfaceState,
    ) -> Result<Self, crate::Error> {
        let Leon3DebugInterfaceState {
            plugnplay,
            dsu: dsu_state,
        } = state;
        let dsu = Dsu3::new(dsu_state);

        Ok(Self {
            core_index,
            probe,
            dsu,
            plugnplay,
        })
    }

    pub fn as_memory_interface(&self) -> &dyn MemoryInterface {
        self.probe
    }

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

    pub(crate) fn read_dsu_reg<R: MemoryMappedRegister<u32>>(&mut self) -> Result<R, crate::Error> {
        self.dsu.read_reg(self.probe, self.core_index)
    }

    pub(crate) fn write_dsu_reg<R: MemoryMappedRegister<u32>>(
        &mut self,
        value: R,
    ) -> Result<(), crate::Error> {
        self.dsu.write_reg(value, self.probe, self.core_index)
    }

    pub fn modify_dsu_reg<R: MemoryMappedRegister<u32>, T>(
        &mut self,
        f: impl Fn(&mut R) -> T,
    ) -> Result<T, crate::Error> {
        self.dsu.modify_reg(self.probe, self.core_index, f)
    }

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
}

/// The combined state of a LEON3's DSU3 debug module and its transport interface.
#[derive(Debug)]
pub(crate) struct Leon3DebugInterfaceState {
    plugnplay: PlugnPlayState,
    dsu: Dsu3State,
}

impl Leon3DebugInterfaceState {
    pub fn try_attach<'probe>(
        probe: &'probe mut dyn MemoryInterface,
    ) -> Result<Self, crate::Error> {
        let plugnplay = PlugnPlayState::scan_plugnplay(probe)?;
        let dsu3_record = plugnplay
            .find_device(Device::Gaisler(GaislerDevice::LEON3DSU))
            .ok_or(Leon3Error::Dsu3NotFound)?;
        let dsu3_base_address = dsu3_record
            .address_spaces
            .first()
            .ok_or(Leon3Error::Dsu3NotFound)?
            .addresses
            .start;

        Ok(Self {
            plugnplay: plugnplay,
            dsu: Dsu3State::new(dsu3_base_address),
        })
    }
}
