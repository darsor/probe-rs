//! LEON3 register descriptions.

use std::sync::LazyLock;

use crate::{
    CoreRegisters,
    architecture::leon3::communication_interface::Leon3Error,
    core::{CoreRegister, RegisterDataType, RegisterId, RegisterRole, UnwindRule},
};

#[derive(Clone, Copy)]
pub enum Leon3RegisterId {
    IuCore(IuCoreReg),
    IuSpecial(IuSpecialReg),
    Fpu(FpuReg),
}

impl Leon3RegisterId {
    const fn to_u16(self) -> u16 {
        match self {
            Leon3RegisterId::IuCore(iu_core_reg) => {
                0x0000
                    | match iu_core_reg {
                        IuCoreReg::G(n) => 0x0000 | (n as u16),
                        IuCoreReg::O(n) => 0x0100 | (n as u16),
                        IuCoreReg::L(n) => 0x0200 | (n as u16),
                        IuCoreReg::I(n) => 0x0300 | (n as u16),
                    }
            }
            Leon3RegisterId::IuSpecial(iu_special_reg) => {
                0x1000
                    | match iu_special_reg {
                        IuSpecialReg::Y => 0,
                        IuSpecialReg::PSR => 1,
                        IuSpecialReg::WIM => 2,
                        IuSpecialReg::TBR => 3,
                        IuSpecialReg::PC => 4,
                        IuSpecialReg::NPC => 5,
                        IuSpecialReg::FSR => 6,
                        IuSpecialReg::CPSR => 7,
                        IuSpecialReg::ASR(n) => 16 + n as u16,
                    }
            }
            Leon3RegisterId::Fpu(fpu_reg) => {
                0x2000
                    | match fpu_reg {
                        FpuReg::F(n) => n as u16,
                    }
            }
        }
    }
}

impl From<Leon3RegisterId> for RegisterId {
    fn from(value: Leon3RegisterId) -> Self {
        RegisterId(value.to_u16())
    }
}

impl TryFrom<RegisterId> for Leon3RegisterId {
    type Error = Leon3Error;

    fn try_from(value: RegisterId) -> Result<Self, Self::Error> {
        match value.0 >> 12 {
            0 => {
                // iu core
                let n = (value.0 & 0xFF) as u8;
                Ok(Leon3RegisterId::IuCore(if n > 7 {
                    Err(Leon3Error::InvalidRegisterId(value))?
                } else {
                    match value.0 >> 8 {
                        0 => IuCoreReg::G(n),
                        1 => IuCoreReg::O(n),
                        2 => IuCoreReg::L(n),
                        3 => IuCoreReg::I(n),
                        _ => Err(Leon3Error::InvalidRegisterId(value))?,
                    }
                }))
            }
            1 => {
                // iu special
                Ok(Leon3RegisterId::IuSpecial(match value.0 & 0xFF {
                    0 => IuSpecialReg::Y,
                    1 => IuSpecialReg::PSR,
                    2 => IuSpecialReg::WIM,
                    3 => IuSpecialReg::TBR,
                    4 => IuSpecialReg::PC,
                    5 => IuSpecialReg::NPC,
                    6 => IuSpecialReg::FSR,
                    7 => IuSpecialReg::CPSR,
                    n @ 32..48 => IuSpecialReg::ASR((n - 16) as u8),
                    _ => Err(Leon3Error::InvalidRegisterId(value))?,
                }))
            }
            2 => {
                // fpu
                let n = (value.0 & 0xFF) as u8;
                Ok(Leon3RegisterId::Fpu(FpuReg::F(n)))
            }
            _ => Err(Leon3Error::InvalidRegisterId(value)),
        }
    }
}

#[derive(Clone, Copy)]
pub enum IuCoreReg {
    G(u8),
    O(u8),
    L(u8),
    I(u8),
}

#[derive(Clone, Copy)]
pub enum IuSpecialReg {
    Y,
    PSR,
    WIM,
    TBR,
    PC,
    NPC,
    FSR,
    CPSR,
    ASR(u8),
}

#[derive(Clone, Copy)]
pub enum FpuReg {
    F(u8),
}

/// The program counter register.
pub const PC: CoreRegister = CoreRegister {
    roles: &[RegisterRole::Core("pc"), RegisterRole::ProgramCounter],
    id: RegisterId(Leon3RegisterId::IuSpecial(IuSpecialReg::PC).to_u16()),
    data_type: RegisterDataType::UnsignedInteger(32),
    unwind_rule: UnwindRule::Clear,
};

/// The stack pointer register.
pub const SP: CoreRegister = CoreRegister {
    roles: &[
        RegisterRole::Core("r14"),
        RegisterRole::Argument("o6"),
        RegisterRole::Return("o6"),
        RegisterRole::StackPointer,
    ],
    id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::O(6)).to_u16()),
    data_type: RegisterDataType::UnsignedInteger(32),
    unwind_rule: UnwindRule::Clear,
};

/// The frame pointer register.
pub const FP: CoreRegister = CoreRegister {
    roles: &[
        RegisterRole::Core("r30"),
        RegisterRole::Argument("i6"),
        RegisterRole::Return("i6"),
        RegisterRole::FramePointer,
    ],
    id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::I(6)).to_u16()),
    data_type: RegisterDataType::UnsignedInteger(32),
    unwind_rule: UnwindRule::Clear,
};

/// The return address register.
pub const RA: CoreRegister = CoreRegister {
    roles: &[
        RegisterRole::Core("r31"),
        RegisterRole::Argument("i7"),
        RegisterRole::Return("i7"),
        RegisterRole::ReturnAddress,
    ],
    id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::I(7)).to_u16()),
    data_type: RegisterDataType::UnsignedInteger(32),
    unwind_rule: UnwindRule::Clear,
};

/// The LEON3 core registers.
pub static LEON3_CORE_REGISTERS: LazyLock<CoreRegisters> =
    LazyLock::new(|| CoreRegisters::new(LEON3_REGISTERS_SET.iter().collect::<Vec<_>>()));

// TODO(darsor): these register IDs assume 8 windows
static LEON3_REGISTERS_SET: &[CoreRegister] = &[
    PC,
    CoreRegister {
        roles: &[RegisterRole::Core("r0"), RegisterRole::Core("g0")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::G(0)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r1"), RegisterRole::Core("g1")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::G(1)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r2"), RegisterRole::Core("g2")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::G(2)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r3"), RegisterRole::Core("g3")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::G(3)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r4"), RegisterRole::Core("g4")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::G(4)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r5"), RegisterRole::Core("g5")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::G(5)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r6"), RegisterRole::Core("g6")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::G(6)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r7"), RegisterRole::Core("g7")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::G(7)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r8"),
            RegisterRole::Argument("o0"),
            RegisterRole::Return("o0"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::O(0)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r9"),
            RegisterRole::Argument("o1"),
            RegisterRole::Return("o1"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::O(1)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r10"),
            RegisterRole::Argument("o2"),
            RegisterRole::Return("o2"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::O(2)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r11"),
            RegisterRole::Argument("o3"),
            RegisterRole::Return("o3"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::O(3)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r12"),
            RegisterRole::Argument("o4"),
            RegisterRole::Return("o4"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::O(4)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r13"),
            RegisterRole::Argument("o5"),
            RegisterRole::Return("o5"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::O(5)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    SP,
    CoreRegister {
        roles: &[
            RegisterRole::Core("r15"),
            RegisterRole::Argument("o7"),
            RegisterRole::Return("o7"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::O(7)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r16"), RegisterRole::Core("l0")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::L(0)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r17"), RegisterRole::Core("l1")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::L(1)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r18"), RegisterRole::Core("l2")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::L(2)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r19"), RegisterRole::Core("l3")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::L(3)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r20"), RegisterRole::Core("l4")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::L(4)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r21"), RegisterRole::Core("l5")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::L(5)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r22"), RegisterRole::Core("l6")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::L(6)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[RegisterRole::Core("r23"), RegisterRole::Core("l7")],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::L(7)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r24"),
            RegisterRole::Argument("i0"),
            RegisterRole::Return("i0"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::I(0)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r25"),
            RegisterRole::Argument("i1"),
            RegisterRole::Return("i1"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::I(1)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r26"),
            RegisterRole::Argument("i2"),
            RegisterRole::Return("i2"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::I(2)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r27"),
            RegisterRole::Argument("i3"),
            RegisterRole::Return("i3"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::I(3)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r28"),
            RegisterRole::Argument("i4"),
            RegisterRole::Return("i4"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::I(4)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    CoreRegister {
        roles: &[
            RegisterRole::Core("r29"),
            RegisterRole::Argument("i5"),
            RegisterRole::Return("i5"),
        ],
        id: RegisterId(Leon3RegisterId::IuCore(IuCoreReg::I(5)).to_u16()),
        data_type: RegisterDataType::UnsignedInteger(32),
        unwind_rule: UnwindRule::Clear,
    },
    FP,
    RA,
];

// TODO(darsor):
// include  "y", "psr", "wim", "tbr", "pc", "npc", "fsr", and "csr"
