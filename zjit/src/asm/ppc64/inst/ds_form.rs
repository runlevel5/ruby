/// The struct that represents a DS-form instruction that can be encoded.
///
/// +------+-------+-------+--------------+-------+
/// | 0  5 | 6  10 | 11 15 | 16        29 | 30 31 |
/// | opcd | rt/rs | ra    | ds           | xo    |
/// +------+-------+-------+--------------+-------+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct DSForm {
    /// The primary opcode.
    opcd: u8,

    /// The number of the target register for loads, or of the source register for stores.
    rt: u8,

    /// The number of the base register, where 0 means the value zero.
    ra: u8,

    /// The displacement in bytes. Only its upper 14 bits are encoded, so it must be a multiple of 4.
    d: i16,

    /// The extended opcode.
    xo: u8,
}

impl DSForm {
    /// LD: rt = doubleword at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn ld(rt: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 58, rt, ra, d, xo: 0 }
    }

    /// LWA: rt = sign-extended word at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lwa(rt: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 58, rt, ra, d, xo: 2 }
    }

    /// STD: store rs at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Store Instructions
    pub fn std(rs: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 62, rt: rs, ra, d, xo: 0 }
    }

    /// STDU: store rs at ra + sign-extended d, then put that address in ra
    /// Power ISA 2.07B, Book I, Fixed-Point Store Instructions
    pub fn stdu(rs: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 62, rt: rs, ra, d, xo: 1 }
    }
}

impl From<DSForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: DSForm) -> Self {
        assert!(inst.opcd < 64 && inst.rt < 32 && inst.ra < 32 && inst.xo < 4);
        assert!(inst.d % 4 == 0, "DS-form displacement must be a multiple of 4");

        0
        | ((inst.opcd as u32) << 26)
        | ((inst.rt as u32) << 21)
        | ((inst.ra as u32) << 16)
        | (inst.d as u16 as u32)
        | (inst.xo as u32)
    }
}

impl From<DSForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: DSForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_ne_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ld() {
        let result: u32 = DSForm::ld(3, 4, 8).into();
        assert_eq!(0xe8640008, result);
    }

    #[test]
    fn test_ld_negative() {
        let result: u32 = DSForm::ld(3, 1, -8).into();
        assert_eq!(0xe861fff8, result);
    }

    #[test]
    fn test_lwa() {
        let result: u32 = DSForm::lwa(3, 4, 4).into();
        assert_eq!(0xe8640006, result);
    }

    #[test]
    fn test_std() {
        let result: u32 = DSForm::std(0, 1, 16).into();
        assert_eq!(0xf8010010, result);
    }

    #[test]
    fn test_stdu() {
        let result: u32 = DSForm::stdu(1, 1, -112).into();
        assert_eq!(0xf821ff91, result);
    }

    #[test]
    #[should_panic(expected = "DS-form displacement must be a multiple of 4")]
    fn test_ld_unaligned() {
        let _: u32 = DSForm::ld(3, 4, 6).into();
    }
}
