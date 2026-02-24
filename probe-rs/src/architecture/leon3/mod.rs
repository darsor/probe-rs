//! All the interface bits for LEON3.
//!
use std::{sync::Arc, time::Duration};

use crate::{
    BreakpointCause, CoreInformation, CoreInterface, CoreStatus, HaltReason, RegisterId,
    RegisterValue,
    architecture::leon3::{
        communication_interface::{Leon3CommunicationInterface, Leon3Error},
        dsu3::{Asr17, DsuBrss, DsuCtrl, DsuDtr, Psr},
        peripherals::Peripheral,
        registers::{IuSpecialReg, Leon3RegisterId},
        sequences::Leon3DebugSequence,
    },
    memory::CoreMemoryInterface,
};

pub mod ahbjtag;
pub mod assembly;
pub mod communication_interface;
mod dsu3;
pub mod peripherals;
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
    pub(crate) fn new(
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
            this.interface.on_first_attach()?;
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
        // TODO(darsor): check on hardware if BN is always set when debug mode is active
        let ctrl: DsuCtrl = self.interface.read_dsu_reg()?;
        if self.core_halted()? {
            if ctrl.eb() {
                return Ok(CoreStatus::Halted(HaltReason::External));
            }
            let brss: DsuBrss = self.interface.read_dsu_reg()?;
            // check for error mode
            if ctrl.pe() {
                return Ok(CoreStatus::Halted(HaltReason::Exception));
            }
            // check for single-step
            if brss.ss(self.core_index) {
                return Ok(CoreStatus::Halted(HaltReason::Step));
            }
            // check for SW breakpoint
            let dtr: DsuDtr = self.interface.read_dsu_reg()?;
            match dtr.traptype() {
                0x81 => {
                    // SW trap 1, typically used as SW breakpoint
                    return Ok(CoreStatus::Halted(HaltReason::Breakpoint(
                        BreakpointCause::Software,
                    )));
                }
                0xB => {
                    // Hardware watchpoint trap
                    let CoreInformation { pc } = self.interface.core_info()?;
                    if self
                        .hw_breakpoints()?
                        .iter()
                        .any(|bp| bp.is_some_and(|addr| addr == pc))
                    {
                        return Ok(CoreStatus::Halted(HaltReason::Breakpoint(
                            BreakpointCause::Hardware,
                        )));
                    }
                    return Ok(CoreStatus::Halted(HaltReason::Request));
                }
                _ => return Ok(CoreStatus::Halted(HaltReason::Unknown)),
            }
        } else if ctrl.pw() {
            return Ok(CoreStatus::Sleeping);
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
        let dsu_ctrl = self.interface.modify_dsu_reg(|ctrl: &mut DsuCtrl| {
            ctrl.set_be(true);
            ctrl.set_bz(true);
            *ctrl
        })?;
        // TODO(darsor): better error types
        if dsu_ctrl.pe() || dsu_ctrl.hl() {
            return Err(Leon3Error::Other("core is in error mode"))?;
        }
        if !dsu_ctrl.dm() {
            return Err(Leon3Error::Other("core is not in debug mode"))?;
        }
        // TODO(darsor): always do a single step first?
        let mut brss = DsuBrss::from(0x0000_FFFF);
        brss.set_bn(self.core_index, false);
        self.interface.write_dsu_reg(brss)
    }

    fn reset(&mut self) -> Result<(), crate::Error> {
        self.reset_and_halt(Duration::default())?;
        self.run()
    }

    fn reset_and_halt(
        &mut self,
        _timeout: Duration,
    ) -> Result<crate::CoreInformation, crate::Error> {
        if !self.interface.core_in_debug_mode()? {
            self.halt(Duration::from_millis(500))?;
        }
        // reset all peripherals
        for peripheral in self.interface.peripherals.clone() {
            match peripheral {
                Peripheral::Handled(resetable_peripheral) => {
                    resetable_peripheral.reset(self.interface.as_memory_interface_mut())?
                }
                Peripheral::Unhandled(record) => self
                    .sequence
                    .reset_unhandled_peripheral(&record, &mut self.interface)?,
            };
        }
        // clear error/halt mode
        self.interface.modify_dsu_reg(|r: &mut DsuCtrl| {
            r.set_pe(true);
        })?;
        // disable hardware watchpoints
        let nwp = self.available_breakpoint_units()?;
        for wp in 0..nwp {
            self.clear_hw_breakpoint(wp as usize)?;
        }
        // set all core registers to 0
        self.interface.clear_all_core_reg()?;
        // reset special registers
        self.interface
            .write_core_reg(Leon3RegisterId::IuSpecial(IuSpecialReg::Y), 0)?;
        self.interface.write_dsu_reg(Psr::default())?;
        self.interface
            .write_core_reg(Leon3RegisterId::IuSpecial(IuSpecialReg::WIM), 2)?;
        self.interface
            .write_core_reg(Leon3RegisterId::IuSpecial(IuSpecialReg::TBR), 0)?;
        self.interface
            .write_core_reg(Leon3RegisterId::IuSpecial(IuSpecialReg::PC), 0)?;
        self.interface
            .write_core_reg(Leon3RegisterId::IuSpecial(IuSpecialReg::NPC), 4)?;
        self.interface
            .write_core_reg(Leon3RegisterId::IuSpecial(IuSpecialReg::FSR), 0)?;
        // TODO DSU: flush caches
        self.interface.core_info()
    }

    fn step(&mut self) -> Result<crate::CoreInformation, crate::Error> {
        // single step only this core
        let mut brss = DsuBrss::from(0x0000_FFFF);
        brss.set_bn(self.core_index, false);
        brss.set_ss(self.core_index, true);
        self.interface.write_dsu_reg(brss)?;
        // ensure in debug mode after step
        let new_brss: DsuBrss = self.interface.read_dsu_reg()?;
        if !new_brss.bn(self.core_index) {
            return Err(crate::Error::Other(
                "core not halted after single step".to_string(),
            ));
        }
        self.interface.core_info()
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
        let asr17: Asr17 = self.interface.read_dsu_reg()?;
        Ok(asr17.nwp())
    }

    fn hw_breakpoints(&mut self) -> Result<Vec<Option<u64>>, crate::Error> {
        let nwp = self.available_breakpoint_units()?;
        (0..nwp)
            .map(|wp| self.interface.get_hw_breakpoint(wp as usize))
            .collect()
    }

    fn enable_breakpoints(&mut self, state: bool) -> Result<(), crate::Error> {
        self.interface.modify_dsu_reg(|r: &mut DsuCtrl| {
            r.set_bs(state);
            // Always break on hardware watchpoints since this is also what
            // enables us to set the BN bit to force debug mode.
            r.set_bw(true);
        })
    }

    fn set_hw_breakpoint(&mut self, unit_index: usize, addr: u64) -> Result<(), crate::Error> {
        self.interface.set_hw_breakpoint(unit_index, addr, true)
    }

    fn clear_hw_breakpoint(&mut self, unit_index: usize) -> Result<(), crate::Error> {
        self.interface.set_hw_breakpoint(unit_index, 0, false)
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
        // The only way to disable HW breakpoints is to set the DsuCtrl
        // BW bit to 0, which blocks us from forcing debug mode. So
        // always leave them enabled.
        true
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
        // TODO(darsor): implement this
        Ok(false)
    }

    fn floating_point_register_count(&mut self) -> Result<usize, crate::Error> {
        // TODO(darsor): implement this
        Ok(0)
    }

    fn reset_catch_set(&mut self) -> Result<(), crate::Error> {
        Ok(self.sequence.reset_catch_set(&mut self.interface)?)
    }

    fn reset_catch_clear(&mut self) -> Result<(), crate::Error> {
        Ok(self.sequence.reset_catch_clear(&mut self.interface)?)
    }

    fn debug_core_stop(&mut self) -> Result<(), crate::Error> {
        Ok(())
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
