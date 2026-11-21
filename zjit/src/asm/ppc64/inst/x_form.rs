/// The struct that represents an X-form instruction that can be encoded.
///
/// +------+-------+-------+-------+------------+----+
/// | 0  5 | 6  10 | 11 15 | 16 20 | 21      30 | 31 |
/// |   31 | rt/rs | ra    | rb    | xo         | rc |
/// +------+-------+-------+-------+------------+----+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct XForm {
    /// The number of the target register, or of the source register for logical instructions.
    rt: u8,

    /// The number of the first operand register, or of the target register for logical instructions.
    ra: u8,

    /// The number of the second operand register.
    rb: u8,

    /// The extended opcode.
    xo: u16,

    /// Whether to set CR0 by comparing the result with zero.
    rc: bool,
}

impl XForm {
    /// AND: ra = rs & rb
    /// Power ISA 2.07B, Book I, Fixed-Point Logical Instructions
    pub fn and(ra: u8, rs: u8, rb: u8, rc: bool) -> Self {
        Self { rt: rs, ra, rb, xo: 28, rc }
    }

    /// OR: ra = rs | rb
    /// Power ISA 2.07B, Book I, Fixed-Point Logical Instructions
    pub fn or(ra: u8, rs: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 444, rc: false }
    }

    /// XOR: ra = rs ^ rb
    /// Power ISA 2.07B, Book I, Fixed-Point Logical Instructions
    pub fn xor(ra: u8, rs: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 316, rc: false }
    }

    /// NOR: ra = !(rs | rb)
    /// Power ISA 2.07B, Book I, Fixed-Point Logical Instructions
    pub fn nor(ra: u8, rs: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 124, rc: false }
    }

    /// CMP: compare ra with rb as signed integers, set CR field bf.
    /// Compares all 64 bits when l is true and the low 32 bits otherwise.
    /// The bits where rt usually goes hold bf, a reserved bit, and l.
    /// Power ISA 2.07B, Book I, Fixed-Point Compare Instructions
    pub fn cmp(bf: u8, l: bool, ra: u8, rb: u8) -> Self {
        assert!(bf < 8);
        Self { rt: (bf << 2) | (l as u8), ra, rb, xo: 0, rc: false }
    }

    /// CMPL: compare ra with rb as unsigned integers, set CR field bf.
    /// Compares all 64 bits when l is true and the low 32 bits otherwise.
    /// Power ISA 2.07B, Book I, Fixed-Point Compare Instructions
    pub fn cmpl(bf: u8, l: bool, ra: u8, rb: u8) -> Self {
        assert!(bf < 8);
        Self { rt: (bf << 2) | (l as u8), ra, rb, xo: 32, rc: false }
    }
}

impl From<XForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: XForm) -> Self {
        assert!(inst.rt < 32 && inst.ra < 32 && inst.rb < 32 && inst.xo < 1024);

        0
        | (31 << 26)
        | ((inst.rt as u32) << 21)
        | ((inst.ra as u32) << 16)
        | ((inst.rb as u32) << 11)
        | ((inst.xo as u32) << 1)
        | (inst.rc as u32)
    }
}

impl From<XForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: XForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_and() {
        let result: u32 = XForm::and(3, 4, 5, false).into();
        assert_eq!(0x7c832838, result);
    }

    #[test]
    fn test_and_rc() {
        let result: u32 = XForm::and(0, 4, 5, true).into();
        assert_eq!(0x7c802839, result);
    }

    #[test]
    fn test_or() {
        let result: u32 = XForm::or(3, 4, 5).into();
        assert_eq!(0x7c832b78, result);
    }

    #[test]
    fn test_xor() {
        let result: u32 = XForm::xor(3, 4, 5).into();
        assert_eq!(0x7c832a78, result);
    }

    #[test]
    fn test_nor() {
        let result: u32 = XForm::nor(3, 4, 5).into();
        assert_eq!(0x7c8328f8, result);
    }

    #[test]
    fn test_cmp() {
        let result: u32 = XForm::cmp(0, true, 3, 4).into();
        assert_eq!(0x7c232000, result);
    }

    #[test]
    fn test_cmp_32_bits() {
        let result: u32 = XForm::cmp(0, false, 3, 4).into();
        assert_eq!(0x7c032000, result);
    }

    #[test]
    fn test_cmpl() {
        let result: u32 = XForm::cmpl(0, true, 3, 4).into();
        assert_eq!(0x7c232040, result);
    }

    #[test]
    fn test_cmpl_32_bits() {
        let result: u32 = XForm::cmpl(0, false, 3, 4).into();
        assert_eq!(0x7c032040, result);
    }
}
