use crate::{
    Error as ProbeRsError, MemoryInterface,
    architecture::leon3::{
        dsu3::{Dsu3, Dsu3State, DsuCtrl},
        plugnplay::{Device, GaislerDevice, PlugnPlayState},
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
    dsu: Dsu3<'state>,
    probe: &'state mut BusAccess,
    plugnplay: &'state PlugnPlayState,
}

impl<'state> Leon3CommunicationInterface<'state> {
    pub fn try_attach(
        probe: &'state mut BusAccess,
        state: &'state mut Leon3DebugInterfaceState,
    ) -> Result<Self, crate::Error> {
        let Leon3DebugInterfaceState {
            dsu_state,
            plugnplay,
        } = state;
        let dsu = Dsu3::try_attach(dsu_state, probe)?;

        Ok(Self {
            dsu,
            probe,
            plugnplay,
        })
    }

    pub fn as_memory_interface(&self) -> &dyn MemoryInterface {
        self.probe
    }

    pub fn as_memory_interface_mut(&mut self) -> &mut dyn MemoryInterface {
        self.probe
    }

    pub(crate) fn core_halted(&mut self) -> Result<bool, crate::Error> {
        Ok(self.dsu.read_dsu_reg::<DsuCtrl>(self.probe)?.hl())
    }

    pub(crate) fn core_in_debug_mode(&mut self) -> Result<bool, crate::Error> {
        Ok(self.dsu.read_dsu_reg::<DsuCtrl>(self.probe)?.dm())
    }
}

/// The combined state of a LEON3's DSU3 debug module and its transport interface.
#[derive(Debug)]
pub(crate) struct Leon3DebugInterfaceState {
    dsu_state: Dsu3State,
    plugnplay: PlugnPlayState,
}

impl Leon3DebugInterfaceState {
    pub fn try_attach<'probe>(
        probe: &'probe mut dyn MemoryInterface,
    ) -> Result<Self, crate::Error> {
        let plugnplay = PlugnPlayState::scan_plugnplay(probe)?;
        let dsu_record = plugnplay
            .find_device(Device::Gaisler(GaislerDevice::LEON3DSU))
            .ok_or(Leon3Error::Dsu3NotFound)?;
        let dsu_base_address = dsu_record
            .address_spaces
            .first()
            .ok_or(Leon3Error::Dsu3NotFound)?
            .addresses
            .start
            .try_into()
            .expect("DSU3 base address should fit in u32");

        Ok(Self {
            dsu_state: Dsu3State::new(dsu_base_address),
            plugnplay: plugnplay,
        })
    }
}
