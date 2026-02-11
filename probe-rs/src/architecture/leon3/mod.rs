// TODO(darsor): pub use stuff that other architectures pub use
// TODO(darsor): rename things from LEON3 to SPARC or SPARCV8 as appropriate

use std::sync::Arc;

use crate::{
    CoreInterface, CoreStatus, HaltReason,
    architecture::leon3::{
        communication_interface::Leon3CommunicationInterface,
        dsu3::{DsuBrss, DsuCtrl},
        sequences::Leon3DebugSequence,
    },
    memory::CoreMemoryInterface,
};

pub mod ahbjtag;
pub mod communication_interface;
mod dsu3;
mod plugnplay;
pub mod registers;
pub mod sequences;

/// An interface to operate a LEON3 core.
pub struct Leon3<'state> {
    core_index: usize,
    interface: Leon3CommunicationInterface<'state>,
    state: &'state mut Leon3CoreState,
    sequence: Arc<dyn Leon3DebugSequence>,
}

impl<'state> Leon3<'state> {
    pub fn new(
        // TODO(darsor): is this used?
        core_index: usize,
        interface: Leon3CommunicationInterface<'state>,
        state: &'state mut Leon3CoreState,
        sequence: Arc<dyn Leon3DebugSequence>,
    ) -> Result<Self, crate::Error> {
        let mut this = Self {
            core_index,
            interface,
            state,
            sequence,
        };

        if !this.state.initialized {
            this.interface.on_first_attach();
            this.state.initialized = true;
        }

        // TODO(darsor)
        // this.on_attach()?;

        Ok(this)
    }
}

/// Leon3 core state.
#[derive(Debug)]
pub struct Leon3CoreState {
    /// Whether the first-attach initialization has been performed
    initialized: bool,
}

impl Leon3CoreState {
    /// Creates a new [`Leon3CoreState`].
    pub(crate) fn new() -> Self {
        Self { initialized: false }
    }
}

impl<'state> CoreInterface for Leon3<'state> {
    fn wait_for_core_halted(&mut self, timeout: std::time::Duration) -> Result<(), crate::Error> {
        todo!()
    }

    fn core_halted(&mut self) -> Result<bool, crate::Error> {
        // TODO(darsor): the core can be halted in more than one way?
        let debug_mode = self.interface.core_in_debug_mode()?;
        if debug_mode {
            Ok(true)
        } else {
            self.interface.core_halted()
        }
    }

    fn status(&mut self) -> Result<CoreStatus, crate::Error> {
        // TODO(darsor): check on hardware if BN is always set when debug mode is entered
        let ctrl: DsuCtrl = self.interface.read_dsu_reg()?;
        if ctrl.pw() {
            return Ok(CoreStatus::Sleeping);
        }
        if self.core_halted()? {
            // TODO(darsor): ensure debug mode
            let brss: DsuBrss = self.interface.read_dsu_reg()?;
            if ctrl.pe() {
                return Ok(CoreStatus::Halted(HaltReason::Exception));
            }
            if brss.ss(self.core_index) {
                // TODO(darsor): when to clear this bit?
                return Ok(CoreStatus::Halted(HaltReason::Step));
            }
            // TODO(darsor): check PC and see if executed bp instruction?
            // let pc = self.read_core_reg(registers::PC.id())?;

            // TODO(darsor): check LEON3 registers for hardware watchpoint?

            // TODO(darsor): check PC and see if executed bp instruction?

            // TODO(darsor): otherwise if BN is set then it was probably a request
            // else {
            //     return Ok(CoreStatus::Halted(HaltReason::Request));
            // }
        } else {
            return Ok(CoreStatus::Running);
        }
        todo!()
    }

    fn halt(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<crate::CoreInformation, crate::Error> {
        todo!()
    }

    fn run(&mut self) -> Result<(), crate::Error> {
        todo!()
    }

    fn reset(&mut self) -> Result<(), crate::Error> {
        todo!()
    }

    fn reset_and_halt(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<crate::CoreInformation, crate::Error> {
        todo!()
    }

    fn step(&mut self) -> Result<crate::CoreInformation, crate::Error> {
        todo!()
    }

    fn read_core_reg(
        &mut self,
        address: crate::RegisterId,
    ) -> Result<crate::RegisterValue, crate::Error> {
        todo!()
    }

    fn write_core_reg(
        &mut self,
        address: crate::RegisterId,
        value: crate::RegisterValue,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn available_breakpoint_units(&mut self) -> Result<u32, crate::Error> {
        todo!()
    }

    fn hw_breakpoints(&mut self) -> Result<Vec<Option<u64>>, crate::Error> {
        todo!()
    }

    fn enable_breakpoints(&mut self, state: bool) -> Result<(), crate::Error> {
        todo!()
    }

    fn set_hw_breakpoint(&mut self, unit_index: usize, addr: u64) -> Result<(), crate::Error> {
        todo!()
    }

    fn clear_hw_breakpoint(&mut self, unit_index: usize) -> Result<(), crate::Error> {
        todo!()
    }

    fn registers(&self) -> &'static crate::CoreRegisters {
        todo!()
    }

    fn program_counter(&self) -> &'static crate::CoreRegister {
        &registers::PC
    }

    fn frame_pointer(&self) -> &'static crate::CoreRegister {
        &registers::FP
    }

    fn stack_pointer(&self) -> &'static crate::CoreRegister {
        &registers::SP
    }

    fn return_address(&self) -> &'static crate::CoreRegister {
        &registers::RA
    }

    fn hw_breakpoints_enabled(&self) -> bool {
        todo!()
    }

    fn architecture(&self) -> probe_rs_target::Architecture {
        probe_rs_target::Architecture::Sparc
    }

    fn core_type(&self) -> probe_rs_target::CoreType {
        probe_rs_target::CoreType::Sparc
    }

    fn instruction_set(&mut self) -> Result<probe_rs_target::InstructionSet, crate::Error> {
        Ok(probe_rs_target::InstructionSet::Sparc)
    }

    fn fpu_support(&mut self) -> Result<bool, crate::Error> {
        todo!()
    }

    fn floating_point_register_count(&mut self) -> Result<usize, crate::Error> {
        todo!()
    }

    fn reset_catch_set(&mut self) -> Result<(), crate::Error> {
        todo!()
    }

    fn reset_catch_clear(&mut self) -> Result<(), crate::Error> {
        todo!()
    }

    fn debug_core_stop(&mut self) -> Result<(), crate::Error> {
        todo!()
    }

    fn spill_registers(&mut self) -> Result<(), crate::Error> {
        // For most architectures, this is not necessary. Use cases include processors
        // that have a windowed register file, where the whole register file is not visible at once.
        todo!()
    }
}

impl<'state> CoreMemoryInterface for Leon3<'state> {
    type ErrorType = crate::Error;

    fn memory(&self) -> &dyn crate::MemoryInterface<Self::ErrorType> {
        self.interface.as_memory_interface()
    }

    fn memory_mut(&mut self) -> &mut dyn crate::MemoryInterface<Self::ErrorType> {
        self.interface.as_memory_interface_mut()
    }
}
