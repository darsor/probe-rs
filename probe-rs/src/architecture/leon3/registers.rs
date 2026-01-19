//! LEON3 register descriptions.

use std::sync::LazyLock;

use crate::{
    CoreRegisters,
    core::{CoreRegister, RegisterDataType, RegisterId, RegisterRole, UnwindRule},
};

/// The LEON3 core registers.
pub static LEON3_CORE_REGISTERS: LazyLock<CoreRegisters> =
    LazyLock::new(|| CoreRegisters::new(LEON3_REGISTERS_SET.iter().collect::<Vec<_>>()));

// TODO(darsor): these register IDs assume 8 windows
static LEON3_REGISTERS_SET: &[CoreRegister] = &[
    // The program counter register.
    CoreRegister {
        roles: &[RegisterRole::Core("pc"), RegisterRole::ProgramCounter],
        id: RegisterId(0x1010),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r0"), RegisterRole::Core("g0")],
        id: RegisterId(0x0200),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r1"), RegisterRole::Core("g1")],
        id: RegisterId(0x0204),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r2"), RegisterRole::Core("g2")],
        id: RegisterId(0x0208),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r3"), RegisterRole::Core("g3")],
        id: RegisterId(0x020C),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r4"), RegisterRole::Core("g4")],
        id: RegisterId(0x0210),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r5"), RegisterRole::Core("g5")],
        id: RegisterId(0x0214),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r6"), RegisterRole::Core("g6")],
        id: RegisterId(0x0218),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r7"), RegisterRole::Core("g7")],
        id: RegisterId(0x021C),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r8"),
            RegisterRole::Argument("o0"),
            RegisterRole::Return("o0"),
        ],
        // for %on registers encode bits [4:3] = 1 (offset 32) and bits [2:0] = n
        id: RegisterId(0x8),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r9"),
            RegisterRole::Argument("o1"),
            RegisterRole::Return("o1"),
        ],
        id: RegisterId(0x9),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r10"),
            RegisterRole::Argument("o2"),
            RegisterRole::Return("o2"),
        ],
        id: RegisterId(0xA),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r11"),
            RegisterRole::Argument("o3"),
            RegisterRole::Return("o3"),
        ],
        id: RegisterId(0xB),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r12"),
            RegisterRole::Argument("o4"),
            RegisterRole::Return("o4"),
        ],
        id: RegisterId(0xC),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r13"),
            RegisterRole::Argument("o5"),
            RegisterRole::Return("o5"),
        ],
        id: RegisterId(0xD),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r14"),
            RegisterRole::Argument("o6"),
            RegisterRole::Return("o6"),
        ],
        id: RegisterId(0xE),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r15"),
            RegisterRole::Argument("o7"),
            RegisterRole::Return("o7"),
        ],
        id: RegisterId(0xF),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r16"), RegisterRole::Core("l0")],
        // for %ln registers encode bits [4:3] = 2 (offset 64) and bits [2:0] = n
        id: RegisterId(0x10),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r17"), RegisterRole::Core("l1")],
        id: RegisterId(0x11),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r18"), RegisterRole::Core("l2")],
        id: RegisterId(0x12),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r19"), RegisterRole::Core("l3")],
        id: RegisterId(0x13),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r20"), RegisterRole::Core("l4")],
        id: RegisterId(0x14),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r21"), RegisterRole::Core("l5")],
        id: RegisterId(0x15),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r22"), RegisterRole::Core("l6")],
        id: RegisterId(0x16),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r23"), RegisterRole::Core("l7")],
        id: RegisterId(0x17),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r24"),
            RegisterRole::Argument("i0"),
            RegisterRole::Return("i0"),
        ],
        // for %in registers encode bits [4:3] = 3 (offset 64) and bits [2:0] = n
        id: RegisterId(0x18),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r25"),
            RegisterRole::Argument("i1"),
            RegisterRole::Return("i1"),
        ],
        id: RegisterId(0x19),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r26"),
            RegisterRole::Argument("i2"),
            RegisterRole::Return("i2"),
        ],
        id: RegisterId(0x1A),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r27"),
            RegisterRole::Argument("i3"),
            RegisterRole::Return("i3"),
        ],
        id: RegisterId(0x1B),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r28"),
            RegisterRole::Argument("i4"),
            RegisterRole::Return("i4"),
        ],
        id: RegisterId(0x1C),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r29"),
            RegisterRole::Argument("i5"),
            RegisterRole::Return("i5"),
        ],
        id: RegisterId(0x1D),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r30"),
            RegisterRole::Argument("i6"),
            RegisterRole::Return("i6"),
        ],
        id: RegisterId(0x1E),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r31"),
            RegisterRole::Argument("i7"),
            RegisterRole::Return("i7"),
        ],
        id: RegisterId(0x1F),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
];

// TODO(darsor):
// include  "y", "psr", "wim", "tbr", "pc", "npc", "fsr", and "csr"
