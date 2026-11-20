/// The struct that represents an MD-form instruction that can be encoded.
///
/// +------+-------+-------+-------+---------+-------+-----+----+
/// | 0  5 | 6  10 | 11 15 | 16 20 | 21   26 | 27 29 |  30 | 31 |
/// |   30 | rs    | ra    | sh    | mb/me   | xo    | sh  | rc |
/// +------+-------+-------+-------+---------+-------+-----+----+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
/// Bits 16 to 20 hold the low 5 bits of the 6-bit sh and bit 30 holds its high bit.
/// The mb/me field holds the low 5 bits of the 6-bit mask bound followed by its high bit.
pub struct MDForm {
    /// The number of the source register.
    rs: u8,

    /// The number of the target register.
    ra: u8,

    /// How many bits to rotate left by.
    sh: u8,

    /// The first bit of the mask for rldicl, or the last bit of the mask for rldicr.
    mask: u8,

    /// The extended opcode.
    xo: u8,
}

impl MDForm {
    /// RLDICL: ra = rs rotated left by sh, with bits 0 to mb - 1 cleared
    /// Power ISA 2.07B, Book I, Fixed-Point Rotate and Shift Instructions
    pub fn rldicl(ra: u8, rs: u8, sh: u8, mb: u8) -> Self {
        Self { rs, ra, sh, mask: mb, xo: 0 }
    }

    /// RLDICR: ra = rs rotated left by sh, with bits me + 1 to 63 cleared
    /// Power ISA 2.07B, Book I, Fixed-Point Rotate and Shift Instructions
    pub fn rldicr(ra: u8, rs: u8, sh: u8, me: u8) -> Self {
        Self { rs, ra, sh, mask: me, xo: 1 }
    }
}

impl From<MDForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: MDForm) -> Self {
        assert!(inst.rs < 32 && inst.ra < 32 && inst.sh < 64 && inst.mask < 64 && inst.xo < 8);

        0
        | (30 << 26)
        | ((inst.rs as u32) << 21)
        | ((inst.ra as u32) << 16)
        | (((inst.sh & 0x1f) as u32) << 11)
        | (((inst.mask & 0x1f) as u32) << 6)
        | (((inst.mask >> 5) as u32) << 5)
        | ((inst.xo as u32) << 2)
        | (((inst.sh >> 5) as u32) << 1)
    }
}

impl From<MDForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: MDForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rldicl() {
        let result: u32 = MDForm::rldicl(3, 4, 0, 32).into();
        assert_eq!(0x78830020, result);
    }

    #[test]
    fn test_rldicl_high_sh() {
        let result: u32 = MDForm::rldicl(3, 4, 63, 1).into();
        assert_eq!(0x7883f842, result);
    }

    #[test]
    fn test_rldicr() {
        let result: u32 = MDForm::rldicr(3, 4, 32, 31).into();
        assert_eq!(0x788307c6, result);
    }

    #[test]
    fn test_rldicr_high_me() {
        let result: u32 = MDForm::rldicr(3, 4, 1, 62).into();
        assert_eq!(0x78830fa4, result);
    }
}
