/// The struct that represents an XS-form instruction that can be encoded.
///
/// +------+-------+-------+-------+----------+----+----+
/// | 0  5 | 6  10 | 11 15 | 16 20 | 21    29 | 30 | 31 |
/// |   31 | rs    | ra    | sh    | xo       | sh | rc |
/// +------+-------+-------+-------+----------+----+----+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
/// Bits 16 to 20 hold the low 5 bits of the 6-bit sh and bit 30 holds its high bit.
pub struct XSForm {
    /// The number of the source register.
    rs: u8,

    /// The number of the target register.
    ra: u8,

    /// How many bits to shift by.
    sh: u8,

    /// The extended opcode.
    xo: u16,
}

impl XSForm {
    /// SRADI: ra = rs shifted right by sh, filling with copies of the sign bit
    /// Power ISA 2.07B, Book I, Fixed-Point Rotate and Shift Instructions
    pub fn sradi(ra: u8, rs: u8, sh: u8) -> Self {
        Self { rs, ra, sh, xo: 413 }
    }
}

impl From<XSForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: XSForm) -> Self {
        assert!(inst.rs < 32 && inst.ra < 32 && inst.sh < 64 && inst.xo < 512);

        0
        | (31 << 26)
        | ((inst.rs as u32) << 21)
        | ((inst.ra as u32) << 16)
        | (((inst.sh & 0x1f) as u32) << 11)
        | ((inst.xo as u32) << 2)
        | (((inst.sh >> 5) as u32) << 1)
    }
}

impl From<XSForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: XSForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sradi() {
        let result: u32 = XSForm::sradi(3, 4, 1).into();
        assert_eq!(0x7c830e74, result);
    }

    #[test]
    fn test_sradi_high_sh() {
        let result: u32 = XSForm::sradi(3, 4, 63).into();
        assert_eq!(0x7c83fe76, result);
    }
}
