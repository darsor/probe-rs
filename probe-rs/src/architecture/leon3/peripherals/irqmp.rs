use crate::{
    MemoryMappedRegister,
    architecture::leon3::{
        peripherals::{Leon3Peripheral, ResetablePeripheral},
        plugnplay::Record,
    },
    memory_mapped_bitfield_register,
};

/// Interrupt Pending Register offset
const IPEND_OFFSET: u64 = 0x004;
/// Interrupt Force Register offset
const IFORCE0_OFFSET: u64 = 0x008;
/// Interrupt Clear Register offset
const ICLEAR_OFFSET: u64 = 0x00C;
/// Processor Interrupt Mask Register offset
const PIMASK_OFFSET: u64 = 0x040;
/// Processor Interrupt Force Register offset
const PCFORCE_OFFSET: u64 = 0x080;

pub(crate) struct Irqmp {
    pnp_record: Record,
}

impl Irqmp {
    pub(crate) fn new(pnp_record: Record) -> Self {
        Self { pnp_record }
    }
}

impl Leon3Peripheral for Irqmp {
    fn device_id(&self) -> crate::architecture::leon3::plugnplay::Device {
        self.pnp_record.device
    }
}

impl ResetablePeripheral for Irqmp {
    fn reset(&self, probe: &mut dyn crate::MemoryInterface) -> Result<(), crate::Error> {
        let base_addr = self
            .pnp_record
            .address_spaces
            .first()
            .expect("IRQMP should have a PNP address space defined")
            .addresses
            .start;

        // mask all interrupts
        probe.write_word_32(base_addr + PIMASK_OFFSET, 0)?;

        // clear forced interrupts
        let mpstat: MPStat = probe
            .read_word_32(MPStat::get_mmio_address_from_base(base_addr)?)?
            .into();
        let ncpu = mpstat.ncpu();
        match ncpu {
            0 => probe.write_word_32(base_addr + IFORCE0_OFFSET, 0)?,
            1 => probe.write_word_32(base_addr + PCFORCE_OFFSET, 0)?,
            _ => {
                return Err(crate::Error::NotImplemented(
                    "IRQMP support for more than one core",
                ));
            }
        }

        // clear pending interrupts
        probe.write_word_32(base_addr + IPEND_OFFSET, 0)?;
        probe.write_word_32(base_addr + ICLEAR_OFFSET, u32::MAX)?;

        Ok(())
    }
}

memory_mapped_bitfield_register! {
    /// MPSTAT - Multiprocessor Status Reigster (GRLIB IP Core User's Manual 96.3.5)
    struct MPStat(u32);
    0x010, "mpstat",
    impl From;
    /// Number of CPUs (NCPU) - Number of CPUs in the system - 1
    ncpu, _: 31, 28;
    /// Broadcast Available (BA) - Set to ‘1’ if NCPU > 0.
    ba, _: 27;
    /// Extended boot registers available (ER). Set to ‘1’ if bootreg generic is 1.
    er, _: 26;
    /// Extended IRQ (EIRQ) - Interrupt number (1 - 15) used for extended interrupts. Fixed to 0 if
    /// extended interrupts are disabled.
    eirq, _: 19, 16;
    /// Power-down status of CPU[n] (STATUS[n]) - '1' = power-down, '0' = running. Write STATUS[n]
    /// with '1' to start processor n;
    status, set_status: 0, 0, 16;
}
