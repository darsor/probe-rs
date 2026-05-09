//! AHBJTAG implementation for accessing the AHb system bus through a JTAG interface.

use std::iter;

use bitvec::{field::BitField as _, slice::BitSlice};
use itertools::{Itertools as _, Position};
use scroll::Pread as _;

use crate::{
    MemoryInterface,
    memory::{InvalidDataLengthError, MemoryNotAlignedError},
    probe::{
        CommandResult, DebugProbeError, JtagWriteCommand, JtagWriteData, Probe, ShiftDrCommand,
        ShiftDrData,
        queue::{BatchError, DeferredResultIndex, DeferredResultSet, Queue},
    },
};

const ADATA_LEN: u32 = 35;
const DDATA_LEN: u32 = 33;

/// Some error occurred when working with the AHBJTAG interface.
#[derive(thiserror::Error, Debug)]
pub enum AhbJtagError {
    /// An error with the usage of the probe occurred
    #[error("An error occured when using the debug probe.")]
    Probe(#[from] DebugProbeError),
    /// A sequential transaction was attempted before the previous one finished.
    #[error("Previous AHB transaction did not complete.")]
    TransactionNotFinished,
    /// The result index of a batched command is not available.
    #[error("The requested data is not available due to a previous error.")]
    BatchedResultNotAvailable,
    /// Address is out of range for a 32-bit address space.
    #[error("Address {0:#08X} is out of range for a 32-bit address space.")]
    AddressOutOfRange(u64),
    /// Memory access to address {0.address:#X?} was not aligned to {0.alignment} bytes.
    #[error(transparent)]
    MemoryNotAligned(#[from] MemoryNotAlignedError),
    /// The data buffer had an invalid length.
    #[error(transparent)]
    InvalidDataLength(#[from] InvalidDataLengthError),
}

/// AHBJTAG driver used to access the AHB bus through JTAG.
#[derive(Debug)]
pub struct AhbJtag {
    probe: Probe,
    config: probe_rs_target::AhbJtag,
    state: AhbJtagState,
}

#[derive(Debug)]
struct AhbJtagState {
    current_transaction: Option<TransactionState>,
    queued_commands: Queue<crate::Error>,
    jtag_results: DeferredResultSet<CommandResult>,
}

#[derive(Debug, Clone, Copy)]
struct TransactionState {
    size: TransactionSize,
    kind: TransactionKind,
    address: u32,
    first_access: bool,
}

impl AhbJtagState {
    pub fn new() -> Self {
        Self {
            current_transaction: None,
            queued_commands: Queue::new(),
            jtag_results: DeferredResultSet::new(),
        }
    }
}

/// AHB transaction sizes supported by AHBJTAG
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransactionSize {
    U8,
    U16,
    U32,
}

impl TransactionSize {
    fn encode(self) -> u8 {
        match self {
            TransactionSize::U8 => 0b00,
            TransactionSize::U16 => 0b01,
            TransactionSize::U32 => 0b10,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransactionKind {
    Read,
    Write,
}

impl TransactionKind {
    fn encode(self) -> u8 {
        match self {
            TransactionKind::Read => 0,
            TransactionKind::Write => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Seq {
    LastTransaction = 0,
    ContinuingTransaction = 1,
}

impl Seq {
    fn encode(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug)]
enum TransactionData {
    U8(u8),
    U16(u16),
    U32(u32),
}

impl TransactionData {
    fn size(&self) -> TransactionSize {
        match self {
            TransactionData::U8(_) => TransactionSize::U8,
            TransactionData::U16(_) => TransactionSize::U16,
            TransactionData::U32(_) => TransactionSize::U32,
        }
    }

    fn encode(&self) -> [u8; 4] {
        let mut result = [0u8; 4];
        match self {
            TransactionData::U8(data) => {
                return [*data; 4];
            }
            TransactionData::U16(data) => {
                result[0..2].copy_from_slice(&data.to_le_bytes());
                result[2..4].copy_from_slice(&data.to_le_bytes());
            }
            TransactionData::U32(data) => result[0..4].copy_from_slice(&data.to_le_bytes()),
        }
        result
    }
}

trait JtagData {}
impl JtagData for JtagWriteData {}
impl JtagData for ShiftDrData {}

impl AhbJtag {
    /// Construct a new AHBJTAG interface.
    pub fn new(probe: Probe, config: probe_rs_target::AhbJtag) -> Self {
        Self {
            probe,
            config,
            state: AhbJtagState::new(),
        }
    }

    /// Access the probe owned by the AHBJTAG interface.
    pub fn as_probe(&mut self) -> &mut Probe {
        &mut self.probe
    }

    fn schedule_write_adata(&mut self, address: u32, kind: TransactionKind, size: TransactionSize) {
        self.state.current_transaction = Some(TransactionState {
            size,
            kind,
            address,
            first_access: true,
        });
        let mut data = vec![0u8; 5];
        data[0..4].copy_from_slice(&address.to_le_bytes());
        data[4] = (kind.encode() << 2) | size.encode();
        tracing::debug!("Scheduled ADATA: 0x{address:08X} ({kind:?}, {size:?})");
        self.state.queued_commands.schedule(JtagWriteCommand {
            data: JtagWriteData {
                address: self.config.adata_addr,
                data,
                len: ADATA_LEN,
            },
            transform: |_, _| -> Result<CommandResult, crate::Error> { Ok(CommandResult::None) },
        });
    }

    fn schedule_read_ddata(&mut self, seq: Seq) -> DeferredResultIndex {
        let mut data = vec![0u8; 5];
        data[4] = seq.encode();

        self.schedule_ddata(data, seq)
    }

    fn schedule_write_ddata(&mut self, data: TransactionData, seq: Seq) -> DeferredResultIndex {
        if let Some(transaction) = &self.state.current_transaction {
            assert_eq!(
                transaction.size,
                data.size(),
                "DDATA write size doesn't match ADATA fields"
            );
        }

        let mut cmd_data = vec![0u8; 5];
        cmd_data[..4].copy_from_slice(&data.encode());
        cmd_data[4] = seq.encode();

        self.schedule_ddata(cmd_data, seq)
    }

    fn schedule_ddata(&mut self, data: Vec<u8>, seq: Seq) -> DeferredResultIndex {
        let Some(transaction) = &mut self.state.current_transaction else {
            unreachable!("writing DDATA before writing ADATA");
        };

        if seq == Seq::ContinuingTransaction {
            assert_eq!(
                transaction.size,
                TransactionSize::U32,
                "Sequential transactions can only be performed with U32s"
            );
            assert_ne!(
                transaction.address % 1024,
                1024 - 4,
                "Sequential transactions shall not cross 1024-byte boundaries"
            );
        }

        let index = if transaction.first_access {
            self.state.queued_commands.schedule(JtagWriteCommand {
                data: JtagWriteData {
                    address: self.config.ddata_addr,
                    data,
                    len: DDATA_LEN,
                },
                transform: Self::get_transform(transaction),
            })
        } else {
            self.state.queued_commands.schedule(ShiftDrCommand {
                inner: ShiftDrData {
                    data,
                    len: DDATA_LEN,
                },
                transform: Self::get_transform(transaction),
            })
        };

        // update next transaction
        transaction.first_access = false;
        if seq == Seq::LastTransaction {
            self.state.current_transaction = None;
        }

        index
    }

    fn execute(&mut self) -> Result<(), AhbJtagError> {
        let probe = self
            .probe
            .try_as_jtag_probe()
            .expect("Should be JTAG probe");

        // TODO(darsor): handle automatically
        probe.set_idle_cycles(4)?;
        let cmds = std::mem::take(&mut self.state.queued_commands);

        while !cmds.is_empty() {
            match cmds.execute(|queue| probe.write_register_batch(queue)) {
                Ok(r) => {
                    self.state.jtag_results.merge_from(r);
                    return Ok(());
                }
                Err(e) => match e.error {
                    BatchError::Specific(error) => match error {
                        crate::Error::AhbJtag(e) => return Err(e),
                        crate::Error::Probe(error) => return Err(error.into()),
                        _other => unreachable!("All error cases should be handled"),
                    },
                    BatchError::Probe(debug_probe_error) => {
                        return Err(debug_probe_error.into());
                    }
                },
            }
        }

        Ok(())
    }

    fn read_deferred_result(
        &mut self,
        index: DeferredResultIndex,
    ) -> Result<CommandResult, AhbJtagError> {
        match self.state.jtag_results.take(index) {
            Ok(result) => Ok(result),
            Err(index) => {
                self.execute()?;
                // We can lose data if `execute` fails.
                self.state
                    .jtag_results
                    .take(index)
                    .map_err(|_| AhbJtagError::BatchedResultNotAvailable)
            }
        }
    }

    fn get_transform<T: JtagData>(
        transaction: &TransactionState,
    ) -> fn(&T, &BitSlice) -> Result<CommandResult, crate::Error> {
        match transaction.kind {
            TransactionKind::Read => match transaction.size {
                TransactionSize::U8 => match transaction.address % 4 {
                    0 => Self::transform_read_ddata_8_offset_0,
                    1 => Self::transform_read_ddata_8_offset_1,
                    2 => Self::transform_read_ddata_8_offset_2,
                    3 => Self::transform_read_ddata_8_offset_3,
                    _ => unreachable!(),
                },
                TransactionSize::U16 => match transaction.address % 4 {
                    0 => Self::transform_read_ddata_16_offset_0,
                    2 => Self::transform_read_ddata_16_offset_2,
                    _ => unreachable!("U16 transaction should be U16 aligned"),
                },
                TransactionSize::U32 => Self::transform_read_ddata_32,
            },
            TransactionKind::Write => match transaction.first_access {
                true => Self::transform_first_write_ddata,
                false => Self::transform_seq_write_ddata,
            },
        }
    }

    fn transform_first_write_ddata(
        _command: &impl JtagData,
        _response_bits: &BitSlice,
    ) -> Result<CommandResult, crate::Error> {
        // seq is always 0 for first write, can't fail
        Ok(CommandResult::None)
    }

    fn transform_seq_write_ddata(
        _command: &impl JtagData,
        response_bits: &BitSlice,
    ) -> Result<CommandResult, crate::Error> {
        Self::check_seq(response_bits)?;
        Ok(CommandResult::None)
    }

    fn transform_read_ddata_32(
        _command: &impl JtagData,
        response_bits: &BitSlice,
    ) -> Result<CommandResult, crate::Error> {
        Self::check_seq(response_bits)?;
        Ok(CommandResult::U32(response_bits[0..32].load_le()))
    }

    fn transform_read_ddata_16_offset_0(
        _command: &impl JtagData,
        response_bits: &BitSlice,
    ) -> Result<CommandResult, crate::Error> {
        Self::check_seq(response_bits)?;
        Ok(CommandResult::U16(response_bits[16..32].load_le()))
    }

    fn transform_read_ddata_16_offset_2(
        _command: &impl JtagData,
        response_bits: &BitSlice,
    ) -> Result<CommandResult, crate::Error> {
        Self::check_seq(response_bits)?;
        Ok(CommandResult::U16(response_bits[0..16].load_le()))
    }

    fn transform_read_ddata_8_offset_0(
        _command: &impl JtagData,
        response_bits: &BitSlice,
    ) -> Result<CommandResult, crate::Error> {
        Self::check_seq(response_bits)?;
        Ok(CommandResult::U8(response_bits[24..32].load_le()))
    }

    fn transform_read_ddata_8_offset_1(
        _command: &impl JtagData,
        response_bits: &BitSlice,
    ) -> Result<CommandResult, crate::Error> {
        Self::check_seq(response_bits)?;
        Ok(CommandResult::U8(response_bits[16..24].load_le()))
    }

    fn transform_read_ddata_8_offset_2(
        _command: &impl JtagData,
        response_bits: &BitSlice,
    ) -> Result<CommandResult, crate::Error> {
        Self::check_seq(response_bits)?;
        Ok(CommandResult::U8(response_bits[8..16].load_le()))
    }

    fn transform_read_ddata_8_offset_3(
        _command: &impl JtagData,
        response_bits: &BitSlice,
    ) -> Result<CommandResult, crate::Error> {
        Self::check_seq(response_bits)?;
        Ok(CommandResult::U8(response_bits[0..8].load_le()))
    }

    fn check_seq(response_bits: &BitSlice) -> Result<(), crate::Error> {
        let seq = response_bits
            .get(32)
            .expect("AHBJTAG DDATA reponses should 33 bits");

        if !seq {
            // transfer not yet complete
            Err(crate::Error::AhbJtag(AhbJtagError::TransactionNotFinished))
        } else {
            Ok(())
        }
    }

    /// Read a series of 32-bit words from the target at the given address.
    ///
    /// The address must be aligned to 4 bytes. The SEQ flag is used for efficient
    /// sequential reads. The timeout is for a single word transaction, not the
    /// full read.
    fn read32(&mut self, address: u32, data: &mut [u32]) -> Result<(), AhbJtagError> {
        check_out_of_bounds(address as u64, data.len() * 4)?;
        let mut results = Vec::with_capacity(1024 / 4);
        let max_address = address + 4 * (data.len()) as u32;

        // Sequential transfers should not cross a 1 kB boundary.
        // Process transfers in chunks within 1024-byte boundaries
        for (chunk_idx, chunk) in &data
            .iter_mut()
            .enumerate()
            .chunk_by(|(word_idx, _)| (address + *word_idx as u32 * 4) / 1024)
        {
            let start_address = std::cmp::max(address, chunk_idx * 1024);
            let end_address = std::cmp::min((chunk_idx + 1) * 1024, max_address);

            // schedule write ADATA once for the chunk
            self.schedule_write_adata(start_address, TransactionKind::Read, TransactionSize::U32);

            // schedule read DDATA for each word in the chunk
            let num_transactions = (end_address - start_address) / 4;
            for (position, _) in (0..num_transactions).with_position() {
                let seq = match position {
                    Position::First | Position::Middle => Seq::ContinuingTransaction,
                    Position::Last | Position::Only => Seq::LastTransaction,
                };
                results.push(self.schedule_read_ddata(seq));
            }

            // execute and get the results
            for (result, (_, data)) in iter::zip(results.drain(..), chunk) {
                *data = self.read_deferred_result(result)?.into_u32();
            }
        }
        Ok(())
    }

    /// Read a single 16-bit word from the target at the given address.
    ///
    /// The address must be aligned to 2 bytes.
    fn read16(&mut self, address: u32) -> Result<u16, AhbJtagError> {
        self.schedule_write_adata(address, TransactionKind::Read, TransactionSize::U16);
        let result = self.schedule_read_ddata(Seq::LastTransaction);
        self.read_deferred_result(result)
            .map(|data| data.into_u16())
    }

    /// Read a single byte from the target at the given address.
    fn read8(&mut self, address: u32) -> Result<u8, AhbJtagError> {
        self.schedule_write_adata(address, TransactionKind::Read, TransactionSize::U8);
        let result = self.schedule_read_ddata(Seq::LastTransaction);
        self.read_deferred_result(result).map(|data| data.into_u8())
    }

    /// Write a series of 32-bit words to the target at the given address.
    ///
    /// The address must be aligned to 4 bytes. The SEQ flag is used for efficient
    /// sequential writes.
    fn write32(&mut self, address: u32, data: &[u32]) -> Result<(), AhbJtagError> {
        check_out_of_bounds(address as u64, data.len() * 4)?;

        // Sequential transfers should not cross a 1 kB boundary.
        // Process transfers in chunks within 1024-byte boundaries
        for (chunk_idx, chunk) in &data
            .iter()
            .enumerate()
            .chunk_by(|(word_idx, _)| (address + *word_idx as u32 * 4) / 1024)
        {
            // schedule write ADATA once for the chunk
            let start_address = std::cmp::max(address, chunk_idx * 1024);
            self.schedule_write_adata(start_address, TransactionKind::Write, TransactionSize::U32);

            // schedule write DDATA for each word in the chunk
            for (position, (_idx, word)) in chunk.with_position() {
                let seq = match position {
                    Position::First | Position::Middle => Seq::ContinuingTransaction,
                    Position::Last | Position::Only => Seq::LastTransaction,
                };
                self.schedule_write_ddata(TransactionData::U32(*word), seq);
            }
            self.execute()?;
        }
        Ok(())
    }

    /// Write a single 16-bit word to the target at the given address.
    ///
    /// The address must be aligned to 2 bytes.
    fn write16(&mut self, address: u32, data: u16) -> Result<(), AhbJtagError> {
        self.schedule_write_adata(address, TransactionKind::Write, TransactionSize::U16);
        self.schedule_write_ddata(TransactionData::U16(data), Seq::LastTransaction);
        self.execute()
    }

    /// Write a single byte to the target at the given address.
    fn write8(&mut self, address: u32, data: u8) -> Result<(), AhbJtagError> {
        self.schedule_write_adata(address, TransactionKind::Write, TransactionSize::U8);
        self.schedule_write_ddata(TransactionData::U8(data), Seq::LastTransaction);
        self.execute()
    }
}

fn valid_32bit_address(address: u64) -> Result<u32, AhbJtagError> {
    crate::memory::valid_32bit_address(address)
        .map_err(|_| AhbJtagError::AddressOutOfRange(address))
}

fn check_out_of_bounds(address: u64, num_bytes: usize) -> Result<(), AhbJtagError> {
    let max_address = address + num_bytes as u64 - 1;
    valid_32bit_address(max_address)?;
    Ok(())
}

fn check_alignment(address: u64, alignment: u64) -> Result<(), MemoryNotAlignedError> {
    if !address.is_multiple_of(alignment) {
        return Err(MemoryNotAlignedError {
            address,
            alignment: usize::try_from(alignment).expect("Alignment should fit in a usize"),
        });
    }
    Ok(())
}

impl MemoryInterface for AhbJtag {
    fn supports_native_64bit_access(&mut self) -> bool {
        false
    }

    fn read_64(&mut self, address: u64, data: &mut [u64]) -> Result<(), crate::Error> {
        check_alignment(address, 8)?;
        let address = valid_32bit_address(address)?;
        // SAFETY: Alignment transmute is sound between the u64 and u32 types
        let (prefix, data32, suffix) = unsafe { data.align_to_mut::<u32>() };
        assert_eq!(prefix.len(), 0);
        assert_eq!(suffix.len(), 0);

        self.read32(address, data32)?;

        // For a big-endian host, data[0] has
        //   host address offset:  0   1   2   3   4   5   6   7
        //   data bytes:          d0  d1  d2  d3  d4  d5  d6  d7
        // Where d0 is the data at address offset 0 of the target.
        // The target is big-endian, so d0 is the MSB and the u64 is
        // stored correctly.
        //
        // For a little-endian host, data[0] has
        //   host address offset:  0   1   2   3   4   5   6   7
        //   data bytes:          d3  d2  d1  d0  d7  d6  d5  d4
        // But we want
        //   data bytes:          d7  d6  d5  d4  d3  d2  d1  d0
        // so we need to swap the word order.
        #[cfg(target_endian = "little")]
        for word32_pair in data32.chunks_exact_mut(2) {
            word32_pair.swap(0, 1);
        }
        Ok(())
    }

    fn read_32(&mut self, address: u64, data: &mut [u32]) -> Result<(), crate::Error> {
        check_alignment(address, 4)?;
        let address =
            valid_32bit_address(address).map_err(|_| AhbJtagError::AddressOutOfRange(address))?;
        self.read32(address, data)?;
        Ok(())
    }

    fn read_16(&mut self, address: u64, data: &mut [u16]) -> Result<(), crate::Error> {
        check_alignment(address, 2)?;
        let address = valid_32bit_address(address)?;
        check_out_of_bounds(address as u64, data.len() * 2)?;
        for (word_idx, word16) in data.iter_mut().enumerate() {
            *word16 = self.read16(address + 2 * word_idx as u32)?;
        }
        Ok(())
    }

    fn read_8(&mut self, address: u64, data: &mut [u8]) -> Result<(), crate::Error> {
        let address = valid_32bit_address(address)?;
        check_out_of_bounds(address as u64, data.len())?;
        for (byte_idx, byte) in data.iter_mut().enumerate() {
            *byte = self.read8(address + byte_idx as u32)?;
        }
        Ok(())
    }

    fn write_64(&mut self, address: u64, data: &[u64]) -> Result<(), crate::Error> {
        check_alignment(address, 8)?;
        let address = valid_32bit_address(address)?;
        // SAFETY: Alignment transmute is sound between the u64 and u32 types
        let (prefix, words32, suffix) = unsafe { data.align_to::<u32>() };
        assert_eq!(prefix.len(), 0);
        assert_eq!(suffix.len(), 0);
        #[cfg(target_endian = "big")]
        {
            self.write32(address, words32)?;
        }
        #[cfg(target_endian = "little")]
        {
            let mut buffer = vec![0u32; data.len() * 2];
            for (buffer32_pair, word32_pair) in
                buffer.chunks_exact_mut(2).zip(words32.chunks_exact(2))
            {
                buffer32_pair[0] = word32_pair[1];
                buffer32_pair[1] = word32_pair[0];
            }
            self.write32(address, &buffer)?;
        }
        Ok(())
    }

    fn write_32(&mut self, address: u64, data: &[u32]) -> Result<(), crate::Error> {
        check_alignment(address, 4)?;
        let address = valid_32bit_address(address)?;
        self.write32(address, data)?;
        Ok(())
    }

    fn write_16(&mut self, address: u64, data: &[u16]) -> Result<(), crate::Error> {
        check_alignment(address, 2)?;
        let address = valid_32bit_address(address)?;
        check_out_of_bounds(address as u64, data.len() * 2)?;
        for (word_idx, word16) in data.iter().enumerate() {
            self.write16(address + 2 * word_idx as u32, *word16)?;
        }
        Ok(())
    }

    fn write_8(&mut self, address: u64, data: &[u8]) -> Result<(), crate::Error> {
        let address = valid_32bit_address(address)?;
        check_out_of_bounds(address as u64, data.len())?;
        for (byte_idx, byte) in data.iter().enumerate() {
            self.write8(address + byte_idx as u32, *byte)?;
        }
        Ok(())
    }

    fn supports_8bit_transfers(&self) -> Result<bool, crate::Error> {
        Ok(true)
    }

    fn flush(&mut self) -> Result<(), crate::Error> {
        Ok(())
    }

    fn read_mem_64bit(&mut self, address: u64, data: &mut [u8]) -> Result<(), crate::Error> {
        if !data.len().is_multiple_of(8) {
            return Err(InvalidDataLengthError::new("read_mem_64bit", 8).into());
        }
        let mut buffer = vec![0u64; data.len() / 8];
        self.read_64(address, &mut buffer)?;
        for (bytes, value) in data.chunks_exact_mut(8).zip(buffer.iter()) {
            bytes.copy_from_slice(&u64::to_be_bytes(*value));
        }
        Ok(())
    }

    fn read_mem_32bit(&mut self, address: u64, data: &mut [u8]) -> Result<(), crate::Error> {
        if !data.len().is_multiple_of(4) {
            return Err(InvalidDataLengthError::new("read_mem_32bit", 4).into());
        }
        let mut buffer = vec![0u32; data.len() / 4];
        self.read_32(address, &mut buffer)?;
        for (bytes, value) in data.chunks_exact_mut(4).zip(buffer.iter()) {
            bytes.copy_from_slice(&u32::to_be_bytes(*value));
        }
        Ok(())
    }

    fn write_mem_64bit(&mut self, address: u64, data: &[u8]) -> Result<(), crate::Error> {
        if !data.len().is_multiple_of(8) {
            return Err(InvalidDataLengthError::new("write_mem_64bit", 8).into());
        }
        let mut buffer = std::vec![0u64; data.len() / 8];
        for (bytes, value) in data.chunks_exact(8).zip(buffer.iter_mut()) {
            *value = bytes
                .pread_with(0, scroll::BE)
                .expect("an u64 - this is a bug, please report it");
        }

        self.write_64(address, &buffer)?;
        Ok(())
    }

    fn write_mem_32bit(&mut self, address: u64, data: &[u8]) -> Result<(), crate::Error> {
        if !data.len().is_multiple_of(4) {
            return Err(InvalidDataLengthError::new("write_mem_32bit", 4).into());
        }
        let mut buffer = std::vec![0u32; data.len() / 4];
        for (bytes, value) in data.chunks_exact(4).zip(buffer.iter_mut()) {
            *value = bytes
                .pread_with(0, scroll::BE)
                .expect("an u32 - this is a bug, please report it");
        }

        self.write_32(address, &buffer)?;
        Ok(())
    }

    fn write(&mut self, mut address: u64, mut data: &[u8]) -> Result<(), crate::Error> {
        let len = data.len();
        let start_extra_count = ((4 - (address % 4) as usize) % 4).min(len);
        let end_extra_count = (len - start_extra_count) % 4;
        let inbetween_count = len - start_extra_count - end_extra_count;
        assert!(start_extra_count < 4);
        assert!(end_extra_count < 4);
        assert!(inbetween_count.is_multiple_of(4));

        if start_extra_count != 0 {
            // We first do an 8 bit write of the first < 4 bytes up until the 4 byte aligned boundary.
            self.write_8(address, &data[..start_extra_count])?;

            address += start_extra_count as u64;
            data = &data[start_extra_count..];
        }

        // Make sure we don't try to do an empty but potentially unaligned write
        if inbetween_count > 0 {
            // We do a 32 bit write of the remaining bytes that are 4 byte aligned.
            let mut buffer = vec![0u32; inbetween_count / 4];
            for (bytes, value) in data.chunks_exact(4).zip(buffer.iter_mut()) {
                *value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            }
            self.write_32(address, &buffer)?;

            address += inbetween_count as u64;
            data = &data[inbetween_count..];
        }

        // We write the remaining bytes that we did not write yet which is always n < 4.
        if end_extra_count > 0 {
            self.write_8(address, &data[..end_extra_count])?;
        }

        Ok(())
    }
}
