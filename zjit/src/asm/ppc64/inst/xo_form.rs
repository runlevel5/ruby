/// The struct that represents an XO-form instruction that can be encoded.
///
/// +------+-------+-------+-------+----+----------+----+
/// | 0  5 | 6  10 | 11 15 | 16 20 | 21 | 22    30 | 31 |
/// |   31 | rt    | ra    | rb    | oe | xo       | rc |
/// +------+-------+-------+-------+----+----------+----+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct XOForm {
    /// The number of the target register.
    rt: u8,

    /// The number of the first operand register.
    ra: u8,

    /// The number of the second operand register.
    rb: u8,

    /// Whether to record overflow in XER.
    oe: bool,

    /// The extended opcode.
    xo: u16,

    /// Whether to set CR0 by comparing the result with zero.
    rc: bool,
}

impl XOForm {
    /// ADD: rt = ra + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Arithmetic Instructions
    pub fn add(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, oe: false, xo: 266, rc: false }
    }

    /// SUBF: rt = rb - ra
    /// Power ISA 2.07B, Book I, Fixed-Point Arithmetic Instructions
    pub fn subf(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, oe: false, xo: 40, rc: false }
    }

    /// NEG: rt = -ra
    /// Power ISA 2.07B, Book I, Fixed-Point Arithmetic Instructions
    pub fn neg(rt: u8, ra: u8) -> Self {
        Self { rt, ra, rb: 0, oe: false, xo: 104, rc: false }
    }

    /// MULLD: rt = low 64 bits of ra * rb
    /// Power ISA 2.07B, Book I, Fixed-Point Arithmetic Instructions
    pub fn mulld(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, oe: false, xo: 233, rc: false }
    }

    /// MULHD: rt = high 64 bits of ra * rb as signed integers
    /// Power ISA 2.07B, Book I, Fixed-Point Arithmetic Instructions
    pub fn mulhd(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, oe: false, xo: 73, rc: false }
    }
}

impl From<XOForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: XOForm) -> Self {
        assert!(inst.rt < 32 && inst.ra < 32 && inst.rb < 32 && inst.xo < 512);

        0
        | (31 << 26)
        | ((inst.rt as u32) << 21)
        | ((inst.ra as u32) << 16)
        | ((inst.rb as u32) << 11)
        | ((inst.oe as u32) << 10)
        | ((inst.xo as u32) << 1)
        | (inst.rc as u32)
    }
}

impl From<XOForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: XOForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let result: u32 = XOForm::add(3, 4, 5).into();
        assert_eq!(0x7c642a14, result);
    }

    #[test]
    fn test_subf() {
        let result: u32 = XOForm::subf(3, 4, 5).into();
        assert_eq!(0x7c642850, result);
    }

    #[test]
    fn test_neg() {
        let result: u32 = XOForm::neg(3, 4).into();
        assert_eq!(0x7c6400d0, result);
    }

    #[test]
    fn test_mulld() {
        let result: u32 = XOForm::mulld(3, 4, 5).into();
        assert_eq!(0x7c6429d2, result);
    }

    #[test]
    fn test_mulhd() {
        let result: u32 = XOForm::mulhd(3, 4, 5).into();
        assert_eq!(0x7c642892, result);
    }
}
