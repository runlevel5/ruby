/// The struct that represents an A-form instruction that can be encoded.
///
/// +------+-------+-------+-------+-------+-------+----+
/// | 0  5 | 6  10 | 11 15 | 16 20 | 21 25 | 26 30 | 31 |
/// |   31 | rt    | ra    | rb    | bc    | xo    |    |
/// +------+-------+-------+-------+-------+-------+----+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct AForm {
    /// The number of the target register.
    rt: u8,

    /// The number of the register to pick when the CR bit is set, where 0 means the value zero.
    ra: u8,

    /// The number of the register to pick when the CR bit is clear.
    rb: u8,

    /// The CR bit to test.
    bc: u8,

    /// The extended opcode.
    xo: u8,
}

impl AForm {
    /// ISEL: rt = if CR bit bc is set { (ra|0) } else { rb }
    /// Power ISA 2.07B, Book I, Fixed-Point Select Instruction
    pub fn isel(rt: u8, ra: u8, rb: u8, bc: u8) -> Self {
        Self { rt, ra, rb, bc, xo: 15 }
    }
}

impl From<AForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: AForm) -> Self {
        assert!(inst.rt < 32 && inst.ra < 32 && inst.rb < 32 && inst.bc < 32 && inst.xo < 32);

        0
        | (31 << 26)
        | ((inst.rt as u32) << 21)
        | ((inst.ra as u32) << 16)
        | ((inst.rb as u32) << 11)
        | ((inst.bc as u32) << 6)
        | ((inst.xo as u32) << 1)
    }
}

impl From<AForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: AForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_ne_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isel_eq() {
        let result: u32 = AForm::isel(3, 4, 5, 2).into();
        assert_eq!(0x7c64289e, result);
    }

    #[test]
    fn test_isel_lt() {
        let result: u32 = AForm::isel(3, 4, 5, 0).into();
        assert_eq!(0x7c64281e, result);
    }

    #[test]
    fn test_isel_so() {
        let result: u32 = AForm::isel(31, 30, 29, 3).into();
        assert_eq!(0x7ffee8de, result);
    }
}
