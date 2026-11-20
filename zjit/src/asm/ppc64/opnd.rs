use std::fmt;

/// This operand represents a general-purpose register.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct PPC64Reg
{
    // Size in bits
    pub num_bits: u8,

    // Register index number
    pub reg_no: u8,
}

impl PPC64Reg {
    pub fn with_num_bits(&self, num_bits: u8) -> Self {
        assert!(num_bits == 8 || num_bits == 16 || num_bits == 32 || num_bits == 64);
        Self { num_bits, reg_no: self.reg_no }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PPC64Mem
{
    // Size in bits
    pub num_bits: u8,

    /// Base register number
    pub base_reg_no: u8,

    /// Constant displacement from the base, not scaled
    pub disp: i32,
}

impl PPC64Mem {
    pub fn new(num_bits: u8, reg: PPC64Opnd, disp: i32) -> Self {
        match reg {
            PPC64Opnd::Reg(reg) => {
                Self { num_bits, base_reg_no: reg.reg_no, disp }
            },
            _ => panic!("Expected register operand")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PPC64Opnd
{
    // Dummy operand
    None,

    // Immediate value
    Imm(i64),

    // Unsigned immediate
    UImm(u64),

    // Register
    Reg(PPC64Reg),

    // Memory
    Mem(PPC64Mem)
}

impl PPC64Opnd {
    /// Create a new immediate value operand.
    pub fn new_imm(value: i64) -> Self {
        PPC64Opnd::Imm(value)
    }

    /// Create a new unsigned immediate value operand.
    pub fn new_uimm(value: u64) -> Self {
        PPC64Opnd::UImm(value)
    }

    /// Creates a new memory operand.
    pub fn new_mem(num_bits: u8, reg: PPC64Opnd, disp: i32) -> Self {
        PPC64Opnd::Mem(PPC64Mem::new(num_bits, reg, disp))
    }

    /// Convenience function to check if this operand is a register.
    pub fn is_reg(&self) -> bool {
        matches!(self, PPC64Opnd::Reg(_))
    }

    /// Unwrap a register from an operand.
    pub fn unwrap_reg(&self) -> PPC64Reg {
        match self {
            PPC64Opnd::Reg(reg) => *reg,
            _ => panic!("Expected register operand")
        }
    }
}

// Register roles below follow the 64-bit ELFv2 ABI.

// volatile, reads as zero when used as a base register or as RA of addi/addis
pub const R0_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 0 };

// stack pointer
pub const R1_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 1 };

// TOC pointer
pub const R2_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 2 };

// argument registers, R3 is also the return value
pub const R3_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 3 };
pub const R4_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 4 };
pub const R5_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 5 };
pub const R6_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 6 };
pub const R7_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 7 };
pub const R8_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 8 };
pub const R9_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 9 };
pub const R10_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 10 };

// volatile
pub const R11_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 11 };

// volatile, holds the callee's address at its global entry point
pub const R12_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 12 };

// thread pointer
pub const R13_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 13 };

// callee-save registers
pub const R14_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 14 };
pub const R15_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 15 };
pub const R16_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 16 };
pub const R17_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 17 };
pub const R18_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 18 };
pub const R19_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 19 };
pub const R20_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 20 };
pub const R21_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 21 };
pub const R22_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 22 };
pub const R23_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 23 };
pub const R24_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 24 };
pub const R25_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 25 };
pub const R26_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 26 };
pub const R27_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 27 };
pub const R28_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 28 };
pub const R29_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 29 };
pub const R30_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 30 };
pub const R31_REG: PPC64Reg = PPC64Reg { num_bits: 64, reg_no: 31 };

pub const R0: PPC64Opnd = PPC64Opnd::Reg(R0_REG);
pub const R1: PPC64Opnd = PPC64Opnd::Reg(R1_REG);
pub const R2: PPC64Opnd = PPC64Opnd::Reg(R2_REG);
pub const R3: PPC64Opnd = PPC64Opnd::Reg(R3_REG);
pub const R4: PPC64Opnd = PPC64Opnd::Reg(R4_REG);
pub const R5: PPC64Opnd = PPC64Opnd::Reg(R5_REG);
pub const R6: PPC64Opnd = PPC64Opnd::Reg(R6_REG);
pub const R7: PPC64Opnd = PPC64Opnd::Reg(R7_REG);
pub const R8: PPC64Opnd = PPC64Opnd::Reg(R8_REG);
pub const R9: PPC64Opnd = PPC64Opnd::Reg(R9_REG);
pub const R10: PPC64Opnd = PPC64Opnd::Reg(R10_REG);
pub const R11: PPC64Opnd = PPC64Opnd::Reg(R11_REG);
pub const R12: PPC64Opnd = PPC64Opnd::Reg(R12_REG);
pub const R13: PPC64Opnd = PPC64Opnd::Reg(R13_REG);
pub const R14: PPC64Opnd = PPC64Opnd::Reg(R14_REG);
pub const R15: PPC64Opnd = PPC64Opnd::Reg(R15_REG);
pub const R16: PPC64Opnd = PPC64Opnd::Reg(R16_REG);
pub const R17: PPC64Opnd = PPC64Opnd::Reg(R17_REG);
pub const R18: PPC64Opnd = PPC64Opnd::Reg(R18_REG);
pub const R19: PPC64Opnd = PPC64Opnd::Reg(R19_REG);
pub const R20: PPC64Opnd = PPC64Opnd::Reg(R20_REG);
pub const R21: PPC64Opnd = PPC64Opnd::Reg(R21_REG);
pub const R22: PPC64Opnd = PPC64Opnd::Reg(R22_REG);
pub const R23: PPC64Opnd = PPC64Opnd::Reg(R23_REG);
pub const R24: PPC64Opnd = PPC64Opnd::Reg(R24_REG);
pub const R25: PPC64Opnd = PPC64Opnd::Reg(R25_REG);
pub const R26: PPC64Opnd = PPC64Opnd::Reg(R26_REG);
pub const R27: PPC64Opnd = PPC64Opnd::Reg(R27_REG);
pub const R28: PPC64Opnd = PPC64Opnd::Reg(R28_REG);
pub const R29: PPC64Opnd = PPC64Opnd::Reg(R29_REG);
pub const R30: PPC64Opnd = PPC64Opnd::Reg(R30_REG);
pub const R31: PPC64Opnd = PPC64Opnd::Reg(R31_REG);

// C argument registers
pub const C_ARG_REGS: [PPC64Opnd; 8] = [R3, R4, R5, R6, R7, R8, R9, R10];
pub const C_ARG_REGREGS: [PPC64Reg; 8] = [R3_REG, R4_REG, R5_REG, R6_REG, R7_REG, R8_REG, R9_REG, R10_REG];

impl fmt::Display for PPC64Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.num_bits {
            64 => write!(f, "r{}", self.reg_no),
            _ => write!(f, "{self:?}"),
        }
    }
}
