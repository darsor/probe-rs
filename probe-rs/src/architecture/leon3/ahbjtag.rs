use std::time::{Duration, Instant};

use bitvec::{field::BitField as _, slice::BitSlice};
use itertools::{Itertools as _, Position};
use scroll::Pread as _;

use crate::{
    MemoryInterface,
    architecture::leon3::communication_interface::Leon3Error,
    memory::{InvalidDataLengthError, MemoryNotAlignedError, valid_32bit_address},
    probe::{DebugProbeError, Probe},
};

const ADATA_LEN: u32 = 35;
const DDATA_LEN: u32 = 33;

// TODO(darsor): make this configurable
const JTAG_TIMEOUT: Duration = Duration::from_secs(2);

/// AHBJTAG driver used to access the AHB bus through JTAG.
#[derive(Debug)]
pub struct AhbJtag {
    probe: Probe,
    config: probe_rs_target::AhbJtag,
    state: AhbJtagState,
}

#[derive(Debug)]
pub struct AhbJtagState {
    current_transaction_size: Option<TransactionSize>,
    current_transaction_kind: Option<TransactionKind>,
}

impl AhbJtagState {
    pub fn new() -> Self {
        Self {
            current_transaction_size: None,
            current_transaction_kind: None,
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
enum Seq {
    LastTransaction,
    ContinuingTransaction,
}

impl Seq {
    fn encode(self) -> u8 {
        match self {
            Seq::LastTransaction => 0,
            Seq::ContinuingTransaction => 1,
        }
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
            TransactionData::U8(data) => result[0] = *data,
            TransactionData::U16(data) => result[0..2].copy_from_slice(&data.to_be_bytes()),
            TransactionData::U32(data) => result[0..4].copy_from_slice(&data.to_be_bytes()),
        }
        result
    }

    fn as_u8(&self) -> u8 {
        if let TransactionData::U8(data) = self {
            *data
        } else {
            panic!("Not a u8")
        }
    }

    fn as_u16(&self) -> u16 {
        if let TransactionData::U16(data) = self {
            *data
        } else {
            panic!("Not a u16")
        }
    }

    fn as_u32(&self) -> u32 {
        if let TransactionData::U32(data) = self {
            *data
        } else {
            panic!("Not a u32")
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum TransactionOutcome {
    ReadDone(TransactionData),
    WriteDone,
    Pending,
}

impl AhbJtag {
    pub fn new(probe: Probe, config: probe_rs_target::AhbJtag) -> Self {
        Self {
            probe,
            config,
            state: AhbJtagState::new(),
        }
    }

    pub fn as_probe(&mut self) -> &mut Probe {
        &mut self.probe
    }

    fn write_adata(
        &mut self,
        address: u32,
        kind: TransactionKind,
        size: TransactionSize,
    ) -> Result<(), DebugProbeError> {
        let mut cmd = [0u8; 5];
        cmd[0] = (kind.encode() << 2) | size.encode();
        cmd[1..].copy_from_slice(&address.to_be_bytes());
        self.probe
            .try_as_jtag_probe()
            .expect("Should be JTAG probe")
            .write_register(self.config.adata_addr, &cmd, ADATA_LEN)?;
        self.state.current_transaction_kind = Some(kind);
        self.state.current_transaction_size = Some(size);
        Ok(())
    }

    fn read_ddata_with_timeout(
        &mut self,
        seq: Seq,
        timeout: Duration,
    ) -> Result<TransactionData, Leon3Error> {
        if seq == Seq::ContinuingTransaction {
            assert_eq!(
                self.state.current_transaction_size,
                Some(TransactionSize::U32),
                "Sequential reads can only be performed with U32s"
            );
        }
        let shift_in = match seq {
            Seq::LastTransaction => [0; 5],
            Seq::ContinuingTransaction => [1, 0, 0, 0, 0],
        };

        let start_time = Instant::now();
        loop {
            // read DDATA
            let result = self
                .probe
                .try_as_jtag_probe()
                .expect("Should be JTAG probe")
                .write_register(self.config.ddata_addr, &shift_in, DDATA_LEN)?;

            // interpret the result
            match self.transform_ddata_result(&result) {
                TransactionOutcome::ReadDone(data) => {
                    if seq == Seq::LastTransaction {
                        self.state.current_transaction_kind = None;
                        self.state.current_transaction_size = None;
                    }
                    return Ok(data);
                }
                TransactionOutcome::Pending => {
                    if start_time.elapsed() > timeout {
                        return Err(Leon3Error::Timeout);
                    }
                }
                TransactionOutcome::WriteDone => unreachable!("Should be reading"),
            }
        }
    }

    fn write_ddata_with_timeout(
        &mut self,
        data: TransactionData,
        seq: Seq,
        timeout: Duration,
    ) -> Result<(), Leon3Error> {
        if seq == Seq::ContinuingTransaction {
            assert_eq!(
                self.state.current_transaction_size,
                Some(TransactionSize::U32),
                "Sequential writes can only be performed with U32s"
            );
        }
        assert_eq!(
            Some(data.size()),
            self.state.current_transaction_size,
            "DDATA write size doesn't match ADATA fields"
        );
        let mut shift_in = match seq {
            Seq::LastTransaction => [0; 5],
            Seq::ContinuingTransaction => [1, 0, 0, 0, 0],
        };
        shift_in[1..].copy_from_slice(&data.encode());

        let start_time = Instant::now();
        loop {
            // write DDATA
            let result = self
                .probe
                .try_as_jtag_probe()
                .expect("Should be JTAG probe")
                .write_register(self.config.ddata_addr, &shift_in, DDATA_LEN)?;

            // interpret the result
            match self.transform_ddata_result(&result) {
                TransactionOutcome::WriteDone => {
                    if seq == Seq::LastTransaction {
                        // last transaction
                        self.state.current_transaction_kind = None;
                        self.state.current_transaction_size = None;
                    }
                    return Ok(());
                }
                TransactionOutcome::Pending => {
                    if start_time.elapsed() > timeout {
                        return Err(Leon3Error::Timeout);
                    }
                }
                TransactionOutcome::ReadDone(_) => unreachable!("Should be writing"),
            }
        }
    }

    fn transform_ddata_result(&self, response_bits: &BitSlice) -> TransactionOutcome {
        let seq = response_bits
            .get(32)
            .expect("AHBJTAG DDATA reponses should have at least 33 bits");
        if !seq {
            // transfer not yet complete
            TransactionOutcome::Pending
        } else if self.state.current_transaction_kind == Some(TransactionKind::Write) {
            TransactionOutcome::WriteDone
        } else {
            TransactionOutcome::ReadDone(match self.state.current_transaction_size {
                Some(TransactionSize::U32) => TransactionData::U32(response_bits[0..32].load_be()),
                Some(TransactionSize::U16) => TransactionData::U16(response_bits[16..32].load_be()),
                Some(TransactionSize::U8) => TransactionData::U8(response_bits[24..32].load_be()),
                None => panic!("Must write ADATA before reading DDATA"),
            })
        }
    }

    /// Read a series of 32-bit words from the target at the given address.
    ///
    /// The address must be aligned to 4 bytes. The SEQ flag is used for efficient
    /// sequential reads. The timeout is for a single word transaction, not the
    /// full read.
    fn read32_with_timeout(
        &mut self,
        address: u32,
        data: &mut [u32],
        timeout: Duration,
    ) -> Result<(), Leon3Error> {
        check_out_of_bounds(address, data.len() * 4)?;

        // Sequential transfers should not cross a 1 kB boundary.
        // Process transfers in chunks within 1024-byte boundaries
        for (chunk_idx, chunk) in &data
            .iter_mut()
            .enumerate()
            .chunk_by(|(word_idx, _)| (address + *word_idx as u32 * 4) / 1024)
        {
            // write ADATA once for the chunk
            let start_address = std::cmp::max(address, chunk_idx * 1024);
            self.write_adata(start_address, TransactionKind::Read, TransactionSize::U32)?;

            // read DDATA for each word in the chunk
            for (position, (_idx, word)) in chunk.with_position() {
                let seq = match position {
                    Position::First | Position::Middle => Seq::ContinuingTransaction,
                    Position::Last | Position::Only => Seq::LastTransaction,
                };
                *word = self.read_ddata_with_timeout(seq, timeout)?.as_u32();
            }
        }
        Ok(())
    }

    /// Read a single 16-bit word from the target at the given address.
    ///
    /// The address must be aligned to 2 bytes.
    fn read16_with_timeout(&mut self, address: u32, timeout: Duration) -> Result<u16, Leon3Error> {
        self.write_adata(address, TransactionKind::Read, TransactionSize::U16)?;
        self.read_ddata_with_timeout(Seq::LastTransaction, timeout)
            .map(|data| data.as_u16())
    }

    /// Read a single byte from the target at the given address.
    fn read8_with_timeout(&mut self, address: u32, timeout: Duration) -> Result<u8, Leon3Error> {
        self.write_adata(address, TransactionKind::Read, TransactionSize::U8)?;
        self.read_ddata_with_timeout(Seq::LastTransaction, timeout)
            .map(|data| data.as_u8())
    }

    /// Write a series of 32-bit words to the target at the given address.
    ///
    /// The address must be aligned to 4 bytes. The SEQ flag is used for efficient
    /// sequential writes. The timeout is for a single word transaction, not the
    /// full read.
    fn write32_with_timeout(
        &mut self,
        address: u32,
        data: &[u32],
        timeout: Duration,
    ) -> Result<(), Leon3Error> {
        check_out_of_bounds(address, data.len() * 4)?;

        // Sequential transfers should not cross a 1 kB boundary.
        // Process transfers in chunks within 1024-byte boundaries
        for (chunk_idx, chunk) in &data
            .iter()
            .enumerate()
            .chunk_by(|(word_idx, _)| (address + *word_idx as u32 * 4) / 1024)
        {
            // write ADATA once for the chunk
            let start_address = std::cmp::max(address, chunk_idx * 1024);
            self.write_adata(start_address, TransactionKind::Write, TransactionSize::U32)?;

            // write DDATA for each word in the chunk
            for (position, (_idx, word)) in chunk.with_position() {
                let seq = match position {
                    Position::First | Position::Middle => Seq::ContinuingTransaction,
                    Position::Last | Position::Only => Seq::LastTransaction,
                };
                self.write_ddata_with_timeout(TransactionData::U32(*word), seq, timeout)?;
            }
        }
        Ok(())
    }

    /// Write a single 16-bit word to the target at the given address.
    ///
    /// The address must be aligned to 2 bytes.
    fn write16_with_timeout(
        &mut self,
        address: u32,
        data: u16,
        timeout: Duration,
    ) -> Result<(), Leon3Error> {
        self.write_adata(address, TransactionKind::Write, TransactionSize::U16)?;
        self.write_ddata_with_timeout(TransactionData::U16(data), Seq::LastTransaction, timeout)
    }

    /// Write a single byte to the target at the given address.
    fn write8_with_timeout(
        &mut self,
        address: u32,
        data: u8,
        timeout: Duration,
    ) -> Result<(), Leon3Error> {
        self.write_adata(address, TransactionKind::Write, TransactionSize::U8)?;
        self.write_ddata_with_timeout(TransactionData::U8(data), Seq::LastTransaction, timeout)
    }
}

fn check_out_of_bounds(address: u32, num_bytes: usize) -> Result<(), Leon3Error> {
    if num_bytes > 0 {
        let num_bytes =
            u32::try_from(num_bytes).expect("Number of bytes to read should fit in u32");
        address
            .checked_add(num_bytes - 1)
            .ok_or(Leon3Error::OutOfBounds)
            .map(|_| {})
    } else {
        Ok(())
    }
}

fn check_alignment(address: u64, alignment: u64) -> Result<(), crate::Error> {
    if !address.is_multiple_of(alignment) {
        return Err(crate::Error::MemoryNotAligned(MemoryNotAlignedError {
            address,
            alignment: usize::try_from(alignment).expect("Alignment should fit in a usize"),
        }));
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

        self.read32_with_timeout(address, data32, JTAG_TIMEOUT)?;

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
        let address = valid_32bit_address(address)?;
        self.read32_with_timeout(address, data, JTAG_TIMEOUT)?;
        Ok(())
    }

    fn read_16(&mut self, address: u64, data: &mut [u16]) -> Result<(), crate::Error> {
        check_alignment(address, 2)?;
        let address = valid_32bit_address(address)?;
        check_out_of_bounds(address, data.len() * 2)?;
        for (word_idx, word16) in data.iter_mut().enumerate() {
            *word16 = self.read16_with_timeout(address + 2 * word_idx as u32, JTAG_TIMEOUT)?;
        }
        Ok(())
    }

    fn read_8(&mut self, address: u64, data: &mut [u8]) -> Result<(), crate::Error> {
        let address = valid_32bit_address(address)?;
        check_out_of_bounds(address, data.len())?;
        for (byte_idx, byte) in data.iter_mut().enumerate() {
            *byte = self.read8_with_timeout(address + byte_idx as u32, JTAG_TIMEOUT)?;
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
            self.write32_with_timeout(address, words32, JTAG_TIMEOUT)?;
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
            self.write32_with_timeout(address, &buffer, JTAG_TIMEOUT)?;
        }
        Ok(())
    }

    fn write_32(&mut self, address: u64, data: &[u32]) -> Result<(), crate::Error> {
        check_alignment(address, 4)?;
        let address = valid_32bit_address(address)?;
        self.write32_with_timeout(address, data, JTAG_TIMEOUT)?;
        Ok(())
    }

    fn write_16(&mut self, address: u64, data: &[u16]) -> Result<(), crate::Error> {
        check_alignment(address, 2)?;
        let address = valid_32bit_address(address)?;
        check_out_of_bounds(address, data.len() * 2)?;
        for (word_idx, word16) in data.iter().enumerate() {
            self.write16_with_timeout(address + 2 * word_idx as u32, *word16, JTAG_TIMEOUT)?;
        }
        Ok(())
    }

    fn write_8(&mut self, address: u64, data: &[u8]) -> Result<(), crate::Error> {
        let address = valid_32bit_address(address)?;
        check_out_of_bounds(address, data.len())?;
        for (byte_idx, byte) in data.iter().enumerate() {
            self.write8_with_timeout(address + byte_idx as u32, *byte, JTAG_TIMEOUT)?;
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
}
