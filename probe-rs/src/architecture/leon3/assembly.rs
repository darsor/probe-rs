#![expect(clippy::unusual_byte_groupings)]

/// SPARC software breakpoint instruction
pub const TA1: u32 = 0b10_0_1000_111010_00000_1_000000_0000001;

