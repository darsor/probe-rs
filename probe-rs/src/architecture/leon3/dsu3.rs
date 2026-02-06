use crate::{MemoryInterface, MemoryMappedRegister, memory_mapped_bitfield_register};

#[derive(Debug)]
pub(crate) struct Dsu3<'state> {
    state: &'state mut Dsu3State,
}

impl<'state> Dsu3<'state> {
    pub fn try_attach(
        state: &'state mut Dsu3State,
        ahb: &mut dyn MemoryInterface,
    ) -> Result<Self, crate::Error> {
        let this = Self { state };
        if !this.state.initialized {
            this.modify_dsu_reg::<DsuCtrl, _>(ahb, |ctrl| {
                ctrl.set_bw(true);
            })?;
            this.state.initialized = true;
        }
        Ok(this)
    }

    pub fn read_dsu_reg<R: MemoryMappedRegister<u32>>(
        &self,
        ahb: &mut dyn MemoryInterface,
    ) -> Result<R, crate::Error> {
        let addr = R::get_mmio_address_from_base(self.state.base_addr)?;
        Ok(R::from(ahb.read_word_32(addr)?))
    }

    pub fn write_dsu_reg<R: MemoryMappedRegister<u32>>(
        &self,
        value: R,
        ahb: &mut dyn MemoryInterface,
    ) -> Result<(), crate::Error> {
        let addr = R::get_mmio_address_from_base(self.state.base_addr)?;
        ahb.write_word_32(addr, value.into())
    }

    pub fn modify_dsu_reg<R: MemoryMappedRegister<u32>, T>(
        &self,
        ahb: &mut dyn MemoryInterface,
        f: impl Fn(&mut R) -> T,
    ) -> Result<T, crate::Error> {
        let mut value = self.read_dsu_reg::<R>(ahb)?;
        let result = f(&mut value);
        self.write_dsu_reg(value, ahb)?;
        Ok(result)
    }
}

#[derive(Debug)]
pub(crate) struct Dsu3State {
    base_addr: u64,
    initialized: bool,
}

impl Dsu3State {
    pub(crate) fn new(base_addr: u32) -> Self {
        Self {
            base_addr: base_addr as u64,
            initialized: false,
        }
    }
}

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
    struct DsuDtr(u32);
    0x400020, "dsu_dtr",
    impl From;
    /// Error mode (EM) - Set if the trap would have cause the processor to enter error mode.
    em, _: 12;
    /// Trap type (TRAPTYPE) - 8-bit SPARC trap type
    u8, traptype, _: 11, 4;
}

memory_mapped_bitfield_register! {
    /// PSR - Processor State Register (Sparc Architecture Manual Version 8, Section 4.2)
    ///
    /// The 32-bit PSR contains various fields that control the processor and hold status
    /// information. It can be modified by the SAVE, RESTORE, Ticc, and RETT
    /// instructions, and by all instructions that modify the condition codes. The
    /// privileged RDPSR and WRPSR instructions read and write the PSR directly.
    struct Psr(u32);
    0x400004, "psr",
    impl From;
    /// Implementation (impl) - Hardwired to identify an implementation or class of implementations
    /// of the architecture. The hardware should not change this field in
    /// response to a WRPSR instruction. Together, the PSR.impl and PSR.ver fields
    /// define a unique implementation or class of implementations of the architecture.
    /// See Appendix L, “Implementation Characteristics.”
    impl_, _: 31, 28;
    /// Version (ver) - Implementation-dependent. The ver field is either
    /// hardwired to identify one or more particular implementations or is a readable and
    /// writable state field whose properties are implementation-dependent.
    /// See Appendix L, “Implementation Characteristics.”
    ver, _: 27, 24;
    /// Integer Condition Codes (icc) - The IU’s condition codes. These bits are modified by the
    /// arithmetic and logical instructions whose names end with the letters cc (e.g.,
    /// ANDcc), and by the WRPSR instruction. The Bicc and Ticc instructions cause a
    /// transfer of control based on the value of these bits.
    icc, _: 23, 20;
    /// Negative (n) - An ICC bit that indicates whether the 32-bit 2’s complement ALU result was negative for
    /// the last instruction that modified the icc field. 1 = negative, 0 = not negative.
    n, _: 23;
    /// Zero (z) - An ICC bit that indicates whether the 32-bit ALU result was zero for the last instruction
    /// that modified the icc field. 1 = zero, 0 = nonzero.
    z, _: 22;
    /// Overflow (v) - An ICC bit that indicates whether the ALU result was within the range of (was represent-
    /// able in) 32-bit 2’s complement notation for the last instruction that modified the
    /// icc field. 1 = overflow, 0 = no overflow.
    v, _: 21;
    /// Carry (c) - An ICC bit that indicates whether a 2’s complement carry out (or borrow) occurred for the
    /// last instruction that modified the icc field. Carry is set on addition if there is a
    /// carry out of bit 31. Carry is set on subtraction if there is borrow into bit 31. 1 =
    /// carry, 0 = no carry.
    c, _: 20;
    /// Enable Coprocessor (EC) Determines whether the implementation-dependent coprocessor is enabled.
    /// If disabled, a coprocessor instruction will trap. 1 = enabled, 0 = disabled. If an
    /// implementation does not support a coprocessor in hardware, PSR.EC should
    /// always read as 0 and writes to it should be ignored.
    ///
    /// Programming Note Software can use the EF and EC bits to determine whether a particular process uses the FPU or CP.
    /// If a process does not use the FPU/CP, its registers do not need to be saved across a context switch.
    ec, _: 13;
    /// Enable Floating-point (EF) - Determines whether the FPU is enabled. If disabled, a floating-point
    /// instruction will trap. 1 = enabled, 0 = disabled. If an implementation does not
    /// support a hardware FPU, PSR.EF should always read as 0 and writes to it should
    /// be ignored.
    ///
    /// Programming Note: Software can use the EF and EC bits to determine whether a particular process uses the FPU or CP.
    /// If a process does not use the FPU/CP, its registers do not need to be saved across a context switch.
    ef, _: 12;
    /// Processor Interrupt Level (PIL) - Identify the interrupt level above which the processor
    /// will accept an interrupt. See Chapter 7, “Traps.”
    pil, _: 11, 8;
    /// Supervisor (S) - Determines whether the processor is in supervisor or user mode. 1 = super-
    /// visor mode, 0 = user mode.
    s, _: 7;
    /// Previous Supervisor (PS) - The value of the S bit at the time of the most recent trap.
    ps, _: 6;
    /// Enable Traps (ET) - Determines whether traps are enabled. A trap automatically resets ET to 0.
    /// When ET=0, an interrupt request is ignored and an exception trap causes the IU
    /// to halt execution, which typically results in a reset trap that resumes execution at
    /// address 0. 1 = traps enabled, 0 = traps disabled. See Chapter 7, “Traps.”
    et, _: 5;
    /// Current Window Pointer (CWP) - A counter that identifies the current window into the r registers.
    /// The hardware decrements the CWP on traps and SAVE instructions, and increments it on
    /// RESTORE and RETT instructions (modulo NWINDOWS).
    cwp, _: 4, 0;
}
