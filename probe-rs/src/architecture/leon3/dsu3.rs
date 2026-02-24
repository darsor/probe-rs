use crate::{
    MemoryInterface, MemoryMappedRegister,
    architecture::leon3::{
        communication_interface::Leon3Error,
        registers::{IuCoreReg, IuSpecialReg},
    },
    memory::valid_32bit_address,
    memory_mapped_bitfield_register,
};

#[derive(Debug)]
pub(crate) struct Dsu3<'state> {
    /// DSU3 state (not for any specific core)
    state: &'state mut Dsu3State,
}

impl<'state> Dsu3<'state> {
    // TODO(darsor): may not always be 8, read from ASR17
    const NUM_WINDOWS: u32 = 8;
    pub fn new(state: &'state mut Dsu3State) -> Self {
        Self { state }
    }

    /// Base address of the registers for controlling a single core.
    ///
    /// NOTE: Some registers are only implemented for core 0 and have bits
    /// for each available core.
    fn base_address(&self, core_index: usize) -> Result<u64, Leon3Error> {
        if core_index >= 16 {
            return Err(Leon3Error::CoreOutOfRange { core_index });
        }
        Ok(self.state.base_addr + ((core_index as u64) << 24))
    }

    /// Get the address of a register with the core offset included.
    fn dsu_reg_address<R: DsuRegister>(&self, mut core_index: usize) -> Result<u64, crate::Error> {
        if R::USES_CORE0_OFFSET {
            core_index = 0;
        }
        Ok(R::get_mmio_address_from_base(
            self.base_address(core_index)?,
        )?)
    }

    pub fn read_reg<R: DsuRegister>(
        &self,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
    ) -> Result<R, crate::Error> {
        let addr = self.dsu_reg_address::<R>(core_index)?;
        Ok(R::from(ahb.read_word_32(addr)?))
    }

    pub fn write_reg<R: DsuRegister>(
        &self,
        value: R,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
    ) -> Result<(), crate::Error> {
        let addr = self.dsu_reg_address::<R>(core_index)?;
        ahb.write_word_32(addr, value.into())
    }

    pub fn modify_reg<R: DsuRegister, T>(
        &self,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
        f: impl Fn(&mut R) -> T,
    ) -> Result<T, crate::Error> {
        let mut value = self.read_reg::<R>(ahb, core_index)?;
        let result = f(&mut value);
        self.write_reg(value, ahb, core_index)?;
        Ok(result)
    }

    pub fn read_core_reg(
        &self,
        reg: IuCoreReg,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
        cwp: u32,
    ) -> Result<u32, crate::Error> {
        let addr = self.base_address(core_index)? + reg.dsu3_addr(Self::NUM_WINDOWS, cwp);
        ahb.read_word_32(addr)
    }

    pub fn write_core_reg(
        &self,
        reg: IuCoreReg,
        value: u32,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
        cwp: u32,
    ) -> Result<(), crate::Error> {
        let addr = self.base_address(core_index)? + reg.dsu3_addr(Self::NUM_WINDOWS, cwp);
        ahb.write_word_32(addr, value)
    }

    pub fn read_special_reg(
        &self,
        reg: IuSpecialReg,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
    ) -> Result<u32, crate::Error> {
        let addr = self.base_address(core_index)? + reg.dsu3_addr();
        ahb.read_word_32(addr)
    }

    pub fn write_special_reg(
        &self,
        reg: IuSpecialReg,
        value: u32,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
    ) -> Result<(), crate::Error> {
        let addr = self.base_address(core_index)? + reg.dsu3_addr();
        ahb.write_word_32(addr, value)
    }

    /// Set all core IU registers (Gx, Ix, Lx, Ox) to 0.
    pub(crate) fn clear_all_core_reg(
        &self,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
    ) -> Result<(), crate::Error> {
        // clear g1-g7
        for i in 1..=7 {
            self.write_core_reg(IuCoreReg::G(i), 0, ahb, core_index, 0)?;
        }
        // clear lx, ix, ox
        let base_address = self.base_address(core_index)?;
        let num_registers = u64::from(Self::NUM_WINDOWS) * 16;
        for i in 0..num_registers {
            ahb.write_word_32(base_address + 0x30_0000 + 4 * i, 0u32)?;
        }
        Ok(())
    }

    pub(crate) fn set_hw_breakpoint(
        &self,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
        unit_index: usize,
        addr: u64,
        enable: bool,
    ) -> Result<(), crate::Error> {
        if unit_index > 4 {
            return Err(Leon3Error::BreakpointOutOfRange(unit_index))?;
        }
        let addr = valid_32bit_address(addr)?;
        {
            let reg_addr = self.dsu_reg_address::<Asr24>(core_index)?;
            let mut bp_addr = Asr24::from(0);
            bp_addr.set_waddr(addr >> 2);
            bp_addr.set_ifetch(enable);
            ahb.write_word_32(reg_addr + 8 * (unit_index as u64), bp_addr.into())?;
        }
        {
            let reg_addr = self.dsu_reg_address::<Asr25>(core_index)?;
            let mut bp_mask = Asr25::from(0);
            bp_mask.set_wmask(0xFFFF_FFFF);
            bp_mask.set_dl(false);
            bp_mask.set_ds(false);
            ahb.write_word_32(reg_addr + 8 * (unit_index as u64), bp_mask.into())
        }
    }

    pub(crate) fn get_hw_breakpoint(
        &self,
        ahb: &mut dyn MemoryInterface,
        core_index: usize,
        unit_index: usize,
    ) -> Result<Option<u64>, crate::Error> {
        if unit_index > 4 {
            return Err(Leon3Error::BreakpointOutOfRange(unit_index))?;
        }
        let reg_addr = self.dsu_reg_address::<Asr24>(core_index)?;
        let value = ahb.read_word_32(reg_addr + 8 * (unit_index as u64))?;
        let asr24 = Asr24::from(value);
        let enabled = asr24.ifetch();
        let addr = u64::from(asr24.waddr() << 2);
        Ok(enabled.then_some(addr))
    }
}

/// State of the DSU3 (not for any specific core).
#[derive(Debug)]
pub(crate) struct Dsu3State {
    /// Base address of the DSU3 addresss space
    base_addr: u64,
}

impl Dsu3State {
    pub(crate) fn new(base_addr: u64) -> Self {
        Self { base_addr }
    }
}

impl IuCoreReg {
    fn dsu3_addr(&self, num_windows: u32, cwp: u32) -> u64 {
        fn addr(num_windows: u32, cwp: u32, offset: u64, n: u8) -> u64 {
            0x30_0000
                + ((u64::from(cwp) * 64) + offset + (u64::from(n) * 4))
                    % (u64::from(num_windows) * 64)
        }
        match *self {
            IuCoreReg::G(n) => 0x30_0000 + (u64::from(num_windows) * 64) + (u64::from(n) * 4),
            IuCoreReg::O(n) => addr(num_windows, cwp, 32, n),
            IuCoreReg::L(n) => addr(num_windows, cwp, 64, n),
            IuCoreReg::I(n) => addr(num_windows, cwp, 96, n),
        }
    }
}

impl IuSpecialReg {
    fn dsu3_addr(&self) -> u64 {
        match self {
            IuSpecialReg::Y => 0x40_0000,
            IuSpecialReg::PSR => 0x40_0004,
            IuSpecialReg::WIM => 0x40_0008,
            IuSpecialReg::TBR => 0x40_000C,
            IuSpecialReg::PC => 0x40_0010,
            IuSpecialReg::NPC => 0x40_0014,
            IuSpecialReg::FSR => 0x40_0018,
            IuSpecialReg::CPSR => 0x40_001C,
            IuSpecialReg::ASR(n) => 0x40_0040 + u64::from(*n - 16) * 4,
        }
    }
}

pub trait DsuRegister: MemoryMappedRegister<u32> {
    /// Set true for DSU registers that only exist at the core 0 offset
    /// but have bits for each core.
    const USES_CORE0_OFFSET: bool = false;
}

impl DsuRegister for DsuCtrl {}
impl DsuRegister for DsuBrss {
    const USES_CORE0_OFFSET: bool = true;
}
impl DsuRegister for DsuDbgm {}
impl DsuRegister for DsuDtr {}
impl DsuRegister for Psr {}
impl DsuRegister for Asr17 {}
impl DsuRegister for Asr24 {}
impl DsuRegister for Asr25 {}

memory_mapped_bitfield_register! {
    /// DSU Control Register (GRLIB IP Core User's Manual 32.6.1)
    ///
    /// The DSU is controlled by the DSU control register.
    pub struct DsuCtrl(u32);
    0x00, "dsu_ctrl",
    impl From;
    /// Power down (PW) - Returns ‘1’ when processor is in power-down mode.
    pub pw, _: 11;
    /// Processor halt (HL) - Returns ‘1’ on read when processor is halted. If the processor is in debug
    /// mode, setting this bit will put the processor in halt mode.
    pub hl, set_hl: 10;
    /// Processor error mode (PE) - returns ‘1’ on read when processor is in error mode, else ‘0’. If written
    /// with ‘1’, it will clear the error and halt mode.
    pub pe, set_pe: 9;
    /// External Break (EB) - Value of the external DSUBRE signal (read-only)
    pub eb, _: 8;
    /// External Enable (EE) - Value of the external DSUEN signal (read-only)
    pub ee, _: 7;
    /// Debug mode (DM) - Indicates when the processor has entered debug mode (read-only).
    pub dm, _: 6;
    /// Break on error traps (BZ) - if set, will force the processor into debug mode on all except the
    /// following traps: priviledged_instruction, fpu_disabled, window_overflow, window_underflow,
    /// asynchronous_interrupt, ticc_trap.
    pub bz, set_bz: 5;
    /// Break on trap (BX) - if set, will force the processor into debug mode when any trap occurs.
    pub bx, set_bx: 4;
    /// Break on S/W breakpoint (BS) - if set, debug mode will be forced when an breakpoint instruction
    /// (ta 1) is executed.
    pub bs, set_bs: 3;
    /// Break on IU watchpoint (BW) - if set, debug mode will be forced on a IU watchpoint (trap 0xb).
    pub bw, set_bw: 2;
    /// Break on error (BE) - if set, will force the processor to debug mode when the processor would have
    /// entered error condition (trap in trap).
    pub be, set_be: 1;
    /// Trace enable (TE) - Enables instruction tracing. If set the instructions will be stored in the trace
    /// buffer. Remains set when then processor enters debug or error mode
    pub te, set_te: 0;
}

memory_mapped_bitfield_register! {
    /// DSU Break and Single Step Register (GRLIB IP Core User's Manual 32.6.2)
    ///
    /// This register is used to break or single step the processor(s). This register
    /// controls all processors in a multi-processor system, and is only accessible
    /// in the DSU memory map of processor 0.
    pub struct DsuBrss(u32);
    0x20, "dsu_brss",
    impl From;
    /// Single step (SSx) - if set, the processor x will execute one instruction and return to debug mode. The
    /// bit remains set after the processor goes into the debug mode. As an exception, if the instruction is a
    /// branch with the annul bit set, and if the delay instruction is effectively annulled, the processor will
    /// execute the branch, the annulled delay instruction and the instruction thereafter before returning to
    /// debug mode.
    pub bool, ss, set_ss: 16, 16, 16;
    /// Break now (BNx) - Force processor x into debug mode if the Break on watchpoint (BW) bit in the
    /// processors DSU control register is set. If cleared, the processor x will resume execution.
    pub bool, bn, set_bn: 0, 0, 16;
}

memory_mapped_bitfield_register! {
    /// DSU Debug Mode Mask Register (GRLIB IP Core User's Manual 32.6.3)
    ///
    /// When one of the processors in a multiprocessor LEON3 system enters the debug mode the value of
    /// the DSU Debug Mode Mask register determines if the other processors are forced in the debug mode.
    /// This register controls all processors in a multi-processor system, and is only accessible in the DSU
    /// memory map of processor 0.
    struct DsuDbgm(u32);
    0x24, "dsu_dbgm",
    impl From;
    /// Debug mode mask (DMx) - If set, the corresponding processor will not be able to force running
    /// processors into debug mode even if it enters debug mode.
    bool, dm, set_dm: 16, 16, 16;
    /// Enter debug mode (EDx) - Force processor x into debug mode if any of processors in a
    /// multiprocessor system enters the debug mode. If 0, the processor x will not enter the debug mode.
    bool, ed, set_ed: 0, 0, 16;
}

memory_mapped_bitfield_register! {
    /// DSU Trap Register (GRLIB IP Core User's Manual 32.6.4)
    ///
    /// The DSU trap register is a read-only register that indicates which SPARC trap type that caused the
    /// processor to enter debug mode. When debug mode is force by setting the BN bit in the DSU control
    /// register, the trap type will be 0xb (hardware watchpoint trap).
    pub struct DsuDtr(u32);
    0x40_0020, "dsu_dtr",
    impl From;
    /// Error mode (EM) - Set if the trap would have cause the processor to enter error mode.
    pub em, _: 12;
    /// Trap type (TRAPTYPE) - 8-bit SPARC trap type
    pub u8, traptype, _: 11, 4;
}

memory_mapped_bitfield_register! {
    /// PSR - Processor State Register (Sparc Architecture Manual Version 8, Section 4.2)
    ///
    /// The 32-bit PSR contains various fields that control the processor and hold status
    /// information. It can be modified by the SAVE, RESTORE, Ticc, and RETT
    /// instructions, and by all instructions that modify the condition codes. The
    /// privileged RDPSR and WRPSR instructions read and write the PSR directly.
    pub struct Psr(u32);
    0x40_0004, "psr",
    impl From;
    /// Implementation (impl) - Hardwired to identify an implementation or class of implementations
    /// of the architecture. The hardware should not change this field in
    /// response to a WRPSR instruction. Together, the PSR.impl and PSR.ver fields
    /// define a unique implementation or class of implementations of the architecture.
    /// See Appendix L, “Implementation Characteristics.”
    pub impl_, _: 31, 28;
    /// Version (ver) - Implementation-dependent. The ver field is either
    /// hardwired to identify one or more particular implementations or is a readable and
    /// writable state field whose properties are implementation-dependent.
    /// See Appendix L, “Implementation Characteristics.”
    pub ver, _: 27, 24;
    /// Integer Condition Codes (icc) - The IU’s condition codes. These bits are modified by the
    /// arithmetic and logical instructions whose names end with the letters cc (e.g.,
    /// ANDcc), and by the WRPSR instruction. The Bicc and Ticc instructions cause a
    /// transfer of control based on the value of these bits.
    pub icc, _: 23, 20;
    /// Negative (n) - An ICC bit that indicates whether the 32-bit 2’s complement ALU result was negative for
    /// the last instruction that modified the icc field. 1 = negative, 0 = not negative.
    pub n, _: 23;
    /// Zero (z) - An ICC bit that indicates whether the 32-bit ALU result was zero for the last instruction
    /// that modified the icc field. 1 = zero, 0 = nonzero.
    pub z, _: 22;
    /// Overflow (v) - An ICC bit that indicates whether the ALU result was within the range of (was represent-
    /// able in) 32-bit 2’s complement notation for the last instruction that modified the
    /// icc field. 1 = overflow, 0 = no overflow.
    pub v, _: 21;
    /// Carry (c) - An ICC bit that indicates whether a 2’s complement carry out (or borrow) occurred for the
    /// last instruction that modified the icc field. Carry is set on addition if there is a
    /// carry out of bit 31. Carry is set on subtraction if there is borrow into bit 31. 1 =
    /// carry, 0 = no carry.
    pub c, _: 20;
    /// Enable Coprocessor (EC) Determines whether the implementation-dependent coprocessor is enabled.
    /// If disabled, a coprocessor instruction will trap. 1 = enabled, 0 = disabled. If an
    /// implementation does not support a coprocessor in hardware, PSR.EC should
    /// always read as 0 and writes to it should be ignored.
    ///
    /// Programming Note Software can use the EF and EC bits to determine whether a particular process uses the FPU or CP.
    /// If a process does not use the FPU/CP, its registers do not need to be saved across a context switch.
    pub ec, _: 13;
    /// Enable Floating-point (EF) - Determines whether the FPU is enabled. If disabled, a floating-point
    /// instruction will trap. 1 = enabled, 0 = disabled. If an implementation does not
    /// support a hardware FPU, PSR.EF should always read as 0 and writes to it should
    /// be ignored.
    ///
    /// Programming Note: Software can use the EF and EC bits to determine whether a particular process uses the FPU or CP.
    /// If a process does not use the FPU/CP, its registers do not need to be saved across a context switch.
    pub ef, _: 12;
    /// Processor Interrupt Level (PIL) - Identify the interrupt level above which the processor
    /// will accept an interrupt. See Chapter 7, “Traps.”
    pub pil, _: 11, 8;
    /// Supervisor (S) - Determines whether the processor is in supervisor or user mode. 1 = super-
    /// visor mode, 0 = user mode.
    pub s, set_s: 7;
    /// Previous Supervisor (PS) - The value of the S bit at the time of the most recent trap.
    pub ps, set_ps: 6;
    /// Enable Traps (ET) - Determines whether traps are enabled. A trap automatically resets ET to 0.
    /// When ET=0, an interrupt request is ignored and an exception trap causes the IU
    /// to halt execution, which typically results in a reset trap that resumes execution at
    /// address 0. 1 = traps enabled, 0 = traps disabled. See Chapter 7, “Traps.”
    pub et, set_et: 5;
    /// Current Window Pointer (CWP) - A counter that identifies the current window into the r registers.
    /// The hardware decrements the CWP on traps and SAVE instructions, and increments it on
    /// RESTORE and RETT instructions (modulo NWINDOWS).
    pub cwp, _: 4, 0;
}

impl Default for Psr {
    // reset value
    fn default() -> Self {
        let mut this = Self::from(0);
        this.set_et(true);
        this.set_ps(true);
        this.set_s(true);
        this
    }
}

memory_mapped_bitfield_register! {
    /// ASR17 - LEON3 Configuration Register (GRLIB IP Core User's Manual, LEON3/FT)
    ///
    /// The ancillary state register 17 (%asr17) provides information on how various configuration options
    /// were set during synthesis. This can be used to enhance the performance of software, or to support enu-
    /// meration in multi-processor systems. There are also a few bits that are writable to configure certain
    /// aspects of the processor.
    pub struct Asr17(u32);
    0x40_0044, "asr17",
    impl From;
    /// Processor index (INDEX) - In multi-processor systems, each LEON core gets a unique index to
    /// support enumeration. The value in this field is identical to the hindex VHDL generic parameter in
    /// the VHDL model.
    pub index, _: 31, 28;
    /// Disable Branch Prediction (DBP) - Disables branch prediction when set to ‘1’. Field is only avail-
    /// able if the VHDL generic bp is set to the value 2.
    pub dbp, set_dbp: 27;
    /// Tagged arithmetic (NOTAG) - If this read-only field is ‘1’ then the processor supports tagged arith-
    /// metic and the compare-and-swap (CASA) instruction. The current version if the LEON3 always
    /// supports tagged arithmetic and CASA.
    pub notag, _: 26;
    /// Disable Branch Prediction on instruction cache misses (DBPM) - When set to ‘1’ this avoids
    /// instruction cache fetches (and possible MMU table walk) for predicted instructions that may be
    /// annulled. This feature is on by default (reset value ‘1’), if branch prediction is programmable then
    /// this is also programmable.
    pub dbpm, set_dpbm: 25;
    /// REX version (REXV) - read-only field that is set to ‘00’ if REX is not implemented, ‘01’ if REX is
    /// implemented, ‘10’ and ‘11’ values are reserved for future implementations
    pub rexv, _: 24, 23;
    /// REX mode (REXM) - set to ‘00’ for REX enabled, ‘01’ for REX illegal and ‘10’ for REX
    /// transparent mode. Writable with reset value ‘01’ when REX support has been enabled
    pub rexm, set_rexm: 22, 21;
    /// Clock switching enabled (CS). If set, switching between AHB and CPU frequency is available.
    pub cs, _: 17;
    /// CPU clock frequency (CF). CPU core runs at (CF+1) times AHB frequency.
    pub cf, _: 16, 15;
    /// Disable write error trap (DWT). When set, a write error trap (tt = 0x2b) will be ignored. Set to zero
    /// after reset.
    pub dwt, set_dwt: 14;
    /// Single-vector trapping (SVT) enable. If set, will enable single-vector trapping. Fixed to zero if SVT
    /// is not implemented. Set to zero after reset.
    pub svt, set_stv: 13;
    /// Load delay (LDDEL) - If set, the pipeline uses a 2-cycle load delay. Otherwise, a 1-cycle load
    /// delay i s used. Generated from the lddel VHDL generic parameter in the VHDL model.
    pub lddel, _: 12;
    /// FPU option. “00” = no FPU; “01” = GRFPU; “10” = Meiko FPU, “11” = GRFPU-Lite
    pub fpu, _: 11, 10;
    /// If set, the optional multiply-accumulate (MAC) instruction is available
    pub mac, _: 9;
    /// If set, the SPARC V8 multiply and divide instructions are available
    pub v8, _: 8;
    /// Number of implemented watchpoints (NWP) (0 - 4)
    pub nwp, _: 7, 5;
    /// Number of implemented registers windows corresponds to NWIN+1.
    pub nwin, _: 4, 0;
}

memory_mapped_bitfield_register! {
    /// ASR24 - Hardware Watchpoint/Breakpoint Address Register (GRLIB IP Core User's Manual, LEON3/FT)
    ///
    /// Each breakpoint consists of a pair of ancillary state registers (%asr24/25, %asr26/27, %asr28/29 and
    /// %asr30/31) registers; one with the break address and one with a mask.
    struct Asr24(u32);
    0x40_0060, "asr24",
    impl From;
    /// WADDR - Address to compare against
    waddr, set_waddr: 31, 2;
    /// IF - If set, break on instruction fetch from the specified address/mask combination
    ifetch, set_ifetch: 0;
}

memory_mapped_bitfield_register! {
    /// ASR25 - Hardware Watchpoint/Breakpoint Mask Register (GRLIB IP Core User's Manual, LEON3/FT)
    ///
    /// Each breakpoint consists of a pair of ancillary state registers (%asr24/25, %asr26/27, %asr28/29 and
    /// %asr30/31) registers; one with the break address and one with a mask.
    struct Asr25(u32);
    0x40_0064, "asr25",
    impl From;
    /// WMASK - Bit mask controlling which bits to check (1) or ignore (0) for match
    wmask, set_wmask: 31, 2;
    /// DL - If set, break on data load from the specified address/mask combination
    dl, set_dl: 1;
    /// DS - If set, break on data store to the specified address/mask combination
    ds, set_ds: 0;
}
