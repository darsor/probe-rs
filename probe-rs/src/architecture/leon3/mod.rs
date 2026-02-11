// TODO(darsor): pub use stuff that other architectures pub use
// TODO(darsor): rename things from LEON3 to SPARC or SPARCV8 as appropriate

use std::{sync::Arc, time::Duration};

use crate::{
    CoreInterface, CoreStatus, HaltReason, RegisterId, RegisterValue,
    architecture::leon3::{
        communication_interface::Leon3CommunicationInterface,
        dsu3::{DsuBrss, DsuCtrl},
        registers::Leon3RegisterId,
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
    fn wait_for_core_halted(&mut self, timeout: Duration) -> Result<(), crate::Error> {
        self.interface.wait_for_core_halted(timeout)
    }

    fn core_halted(&mut self) -> Result<bool, crate::Error> {
        self.interface.core_halted()
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
            } else {
                Ok(CoreStatus::Halted(HaltReason::Unknown))
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
    }

    fn halt(&mut self, timeout: Duration) -> Result<crate::CoreInformation, crate::Error> {
        self.interface.modify_dsu_reg(|reg: &mut DsuBrss| {
            reg.set_bn(self.core_index, true);
        })?;
        self.wait_for_core_halted(timeout)?;
        self.interface.core_info()
    }

    fn run(&mut self) -> Result<(), crate::Error> {
        // TODO(darsor): return error if in halted/error state, only run if in debug mode
        // TODO(darsor): clear BN bit
        todo!()
    }

    fn reset(&mut self) -> Result<(), crate::Error> {
        // Register Reset values
        // Trap Base Register       Trap Base Address field reset (value given by rstaddr VHDL generic)
        // PC                       0x0 (rstaddr VHDL generic)
        // nPC                      0x4 (rstaddr VHDL genericc + 4)
        // PSR                      ET=0, S=1
        // By default, the execution will start from address 0. This can be overridden by setting the rstaddr
        // VHDL generic in the model to a non-zero value. The reset address is always aligned on a 4 KiB
        // boundary. If rstaddr is set to 16#FFFFF#, then the reset address is taken from the signal IRQI.RST-
        // VEC. This allows the reset address to be changed dynamically
        // TODO(darsor): clear caches
        todo!()
    }

    fn reset_and_halt(
        &mut self,
        timeout: Duration,
    ) -> Result<crate::CoreInformation, crate::Error> {
        todo!()
    }

    fn step(&mut self) -> Result<crate::CoreInformation, crate::Error> {
        todo!()
    }

    fn read_core_reg(&mut self, address: RegisterId) -> Result<RegisterValue, crate::Error> {
        let leon3_address = Leon3RegisterId::try_from(address)?;
        self.interface
            .read_core_reg(leon3_address)
            .map(RegisterValue::U32)
    }

    fn write_core_reg(
        &mut self,
        address: RegisterId,
        value: RegisterValue,
    ) -> Result<(), crate::Error> {
        let leon3_address = Leon3RegisterId::try_from(address)?;
        let value: u32 = value.try_into()?;
        self.interface.write_core_reg(leon3_address, value)
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
        &registers::LEON3_CORE_REGISTERS
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
        Ok(self.sequence.reset_catch_set(&mut self.interface)?)
    }

    fn reset_catch_clear(&mut self) -> Result<(), crate::Error> {
        Ok(self.sequence.reset_catch_clear(&mut self.interface)?)
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
