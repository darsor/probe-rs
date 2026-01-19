use crate::{
    Error as ProbeRsError, Target,
    architecture::leon3::{
        ahbjtag::{AhbJtag, AhbJtagConfig, AhbJtagState},
        dsu3::{Dsu3, Dsu3State},
        plugnplay::{Device, GaislerDevice, PlugnPlayState},
    },
    probe::{DebugProbeError, JtagAccess},
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
    pub(super) ahb: AhbJtag<'state>,
    plugnplay: &'state PlugnPlayState,
}

impl<'state> Leon3CommunicationInterface<'state> {
    /// Create the Leon3 communication interface using the JTAG probe driver
    pub fn new(
        probe: &'state mut dyn JtagAccess,
        state: &'state mut Leon3DebugInterfaceState,
    ) -> Self {
        let Leon3DebugInterfaceState {
            dsu_state,
            ahbjtag_state,
            plugnplay,
        } = state;

        Self {
            dsu: Dsu3::new(dsu_state),
            ahb: AhbJtag::new(probe, ahbjtag_state),
            plugnplay,
        }
    }
}

/// The combined state of a LEON3's DSU3 debug module and its transport interface.
#[derive(Debug)]
pub(crate) struct Leon3DebugInterfaceState {
    dsu_state: Dsu3State,
    ahbjtag_state: AhbJtagState,
    plugnplay: PlugnPlayState,
}

impl Leon3DebugInterfaceState {
    pub fn try_attach<'probe>(
        probe: &'probe mut dyn JtagAccess,
        target: &'probe Target,
    ) -> Result<Self, crate::Error> {
        let probe_rs_target::AhbJtag {
            adata_addr,
            ddata_addr,
        } = target
            .jtag
            .as_ref()
            .and_then(|j| j.ahbjtag.as_ref())
            .ok_or(DebugProbeError::Other("AHBJTAG not configured".to_string()))?;
        let ahbjtag_config = AhbJtagConfig::new(*adata_addr, *ddata_addr);
        let mut ahbjtag_state = AhbJtagState::new(ahbjtag_config);
        let mut ahbjtag = AhbJtag::new(probe, &mut ahbjtag_state);
        let plugnplay = PlugnPlayState::scan_plugnplay(&mut ahbjtag)?;
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
            ahbjtag_state: AhbJtagState::new(ahbjtag_config),
            plugnplay: plugnplay,
        })
    }
}
