use std::{fmt::Debug, sync::Arc};

use crate::{
    Session,
    architecture::leon3::communication_interface::{Leon3CommunicationInterface, Leon3Error},
};

/// A interface to operate debug sequences for Leon3 targets.
///
/// Should be implemented on a custom handle for chips that require special sequence code.
pub trait Leon3DebugSequence: Send + Sync + Debug {
    // TODO(darsor): call this when appropriate
    /// Executed when the probe establishes a connection to the target.
    fn on_connect(&self, _interface: &mut Leon3CommunicationInterface) -> Result<(), crate::Error> {
        Ok(())
    }

    // TODO(darsor): call this when appropriate
    /// Executed when the target is halted.
    fn on_halt(&self, _interface: &mut Leon3CommunicationInterface) -> Result<(), crate::Error> {
        Ok(())
    }

    /// Configure the target to stop code execution after a reset. After this, the core will halt when it comes
    /// out of reset.
    fn reset_catch_set(
        &self,
        interface: &mut Leon3CommunicationInterface,
    ) -> Result<(), Leon3Error> {
        return Err(Leon3Error::ResetHaltRequestNotSupported);
    }

    /// Free hardware resources allocated by ResetCatchSet.
    fn reset_catch_clear(
        &self,
        interface: &mut Leon3CommunicationInterface,
    ) -> Result<(), Leon3Error> {
        return Err(Leon3Error::ResetHaltRequestNotSupported);
    }

    /// This LEON3 sequence is called if an image was flashed to RAM directly.
    /// It will perform the necessary preparation to run that image.
    ///
    /// Core should be already `reset_and_halt`ed right before this call.
    fn prepare_running_on_ram(
        &self,
        vector_table_addr: u64,
        session: &mut Session,
    ) -> Result<(), crate::Error> {
        tracing::info!("Performing RAM flash start");
        const SP_MAIN_OFFSET: usize = 0;
        const RESET_VECTOR_OFFSET: usize = 1;

        if session.list_cores().len() > 1 {
            return Err(crate::Error::NotImplemented(
                "multi-core ram flash start not implemented yet",
            ));
        }

        tracing::debug!("RAM flash start for LEON3 single core target");
        // TODO(darsor): set stack pointer and PC? And TBR?
        todo!()
    }
}

/// The default sequences that is used for Leon3 chips that do not specify a specific sequence.
#[derive(Debug)]
pub struct DefaultLeon3Sequence(pub(crate) ());

impl DefaultLeon3Sequence {
    /// Creates a new default Leon3 debug sequence.
    pub fn create() -> Arc<dyn Leon3DebugSequence> {
        Arc::new(Self(()))
    }
}

impl Leon3DebugSequence for DefaultLeon3Sequence {}
