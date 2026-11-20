/// The struct that represents a B-form instruction that can be encoded.
///
/// +------+-------+-------+----------------+----+----+
/// | 0  5 | 6  10 | 11 15 | 16          29 | 30 | 31 |
/// |   16 | bo    | bi    | bd             | aa | lk |
/// +------+-------+-------+----------------+----+----+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct BForm {
    /// How to decide whether to branch.
    bo: u8,

    /// The CR bit to test.
    bi: u8,

    /// The offset in bytes from this instruction to the target. Only its upper
    /// 14 bits are encoded, so it must be a multiple of 4.
    offset: i16,

    /// Whether the target is an absolute address rather than an offset.
    aa: bool,

    /// Whether to put the address of the next instruction in LR.
    lk: bool,
}

impl BForm {
    /// The number of bits the offset in bytes may take up, as a signed integer.
    pub const OFFSET_BITS: u32 = 16;

    /// BC: branch to this instruction's address + offset if CR bit bi satisfies bo
    /// Power ISA 2.07B, Book I, Branch Instructions
    pub fn bc(bo: u8, bi: u8, offset: i16) -> Self {
        Self { bo, bi, offset, aa: false, lk: false }
    }
}

impl From<BForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: BForm) -> Self {
        assert!(inst.bo < 32 && inst.bi < 32);
        assert!(inst.offset % 4 == 0, "B-form offset must be a multiple of 4");

        0
        | (16 << 26)
        | ((inst.bo as u32) << 21)
        | ((inst.bi as u32) << 16)
        | (inst.offset as u16 as u32)
        | ((inst.aa as u32) << 1)
        | (inst.lk as u32)
    }
}

impl From<BForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: BForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bc_true() {
        let result: u32 = BForm::bc(12, 2, 8).into();
        assert_eq!(0x41820008, result);
    }

    #[test]
    fn test_bc_false_backward() {
        let result: u32 = BForm::bc(4, 2, -8).into();
        assert_eq!(0x4082fff8, result);
    }

    #[test]
    fn test_bc_max() {
        let result: u32 = BForm::bc(12, 0, 32764).into();
        assert_eq!(0x41807ffc, result);
    }

    #[test]
    fn test_bc_min() {
        let result: u32 = BForm::bc(4, 0, -32768).into();
        assert_eq!(0x40808000, result);
    }

    #[test]
    #[should_panic(expected = "B-form offset must be a multiple of 4")]
    fn test_bc_unaligned() {
        let _: u32 = BForm::bc(12, 2, 6).into();
    }
}
