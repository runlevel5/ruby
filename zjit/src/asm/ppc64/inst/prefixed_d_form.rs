/// The struct that represents a prefixed instruction with a D-form suffix that can be
/// encoded. The two words share a 34-bit immediate.
///
/// +------+------+------+----+-------+----------------+
/// | 0  5 | 6  7 | 8 10 | 11 | 12 13 | 14          31 |
/// |    1 | type |      | r  |       | imm0           |
/// +------+------+------+----+-------+----------------+
/// | 0  5 | 6          10 | 11    15 | 16          31 |
/// | opcd | rt            | ra       | imm1           |
/// +------+---------------+----------+----------------+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct PrefixedDForm {
    /// The type of the prefix: 0 for 8LS, 2 for MLS.
    prefix_type: u8,

    /// Whether the immediate is relative to the address of the instruction. ra must be 0 then.
    r: bool,

    /// The primary opcode of the suffix.
    opcd: u8,

    /// The number of the target register.
    rt: u8,

    /// The number of the base register, where 0 means the value zero.
    ra: u8,

    /// The 34-bit immediate, which is sign-extended.
    imm: i64,
}

impl PrefixedDForm {
    /// The number of bits of the immediate.
    pub const IMM_BITS: u32 = 34;

    /// PADDI: rt = (ra|0) + si, or the address of the instruction + si when r is set
    /// Power ISA 3.1, Book I, Fixed-Point Arithmetic Instructions
    pub fn paddi(rt: u8, ra: u8, si: i64, r: bool) -> Self {
        Self { prefix_type: 2, r, opcd: 14, rt, ra, imm: si }
    }

    /// PLD: rt = doubleword at (ra|0) + d, or at the address of the instruction + d when r is set
    /// Power ISA 3.1, Book I, Fixed-Point Load Instructions
    pub fn pld(rt: u8, ra: u8, d: i64, r: bool) -> Self {
        Self { prefix_type: 0, r, opcd: 57, rt, ra, imm: d }
    }
}

impl From<PrefixedDForm> for [u32; 2] {
    /// Convert an instruction into its prefix and its suffix.
    fn from(inst: PrefixedDForm) -> Self {
        assert!(inst.prefix_type < 4 && inst.opcd < 64 && inst.rt < 32 && inst.ra < 32);
        assert!(!inst.r || inst.ra == 0, "a PC-relative instruction takes no base register");
        let limit = 1 << (PrefixedDForm::IMM_BITS - 1);
        assert!((-limit..limit).contains(&inst.imm), "immediate must fit in 34 bits, got {}", inst.imm);

        let prefix = 0
            | (1 << 26)
            | ((inst.prefix_type as u32) << 24)
            | ((inst.r as u32) << 20)
            | ((inst.imm >> 16) as u32 & 0x3ffff);
        let suffix = 0
            | ((inst.opcd as u32) << 26)
            | ((inst.rt as u32) << 21)
            | ((inst.ra as u32) << 16)
            | (inst.imm as u16 as u32);
        [prefix, suffix]
    }
}

impl From<PrefixedDForm> for [u8; 8] {
    /// Convert an instruction into an 8 byte array.
    fn from(inst: PrefixedDForm) -> [u8; 8] {
        let [prefix, suffix]: [u32; 2] = inst.into();
        let mut bytes = [0; 8];
        bytes[..4].copy_from_slice(&prefix.to_ne_bytes());
        bytes[4..].copy_from_slice(&suffix.to_ne_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paddi() {
        let result: [u32; 2] = PrefixedDForm::paddi(3, 4, 0x12345, false).into();
        assert_eq!([0x06000001, 0x38642345], result);
    }

    #[test]
    fn test_paddi_34_bits() {
        let result: [u32; 2] = PrefixedDForm::paddi(3, 0, 0x106a20340, false).into();
        assert_eq!([0x060106a2, 0x38600340], result);
    }

    #[test]
    fn test_paddi_pc_relative() {
        let result: [u32; 2] = PrefixedDForm::paddi(12, 0, -0x23456, true).into();
        assert_eq!([0x0613fffd, 0x3980cbaa], result);
    }

    #[test]
    fn test_pld() {
        let result: [u32; 2] = PrefixedDForm::pld(5, 4, 0x12345, false).into();
        assert_eq!([0x04000001, 0xe4a42345], result);
    }

    #[test]
    fn test_pld_pc_relative() {
        let result: [u32; 2] = PrefixedDForm::pld(5, 0, 0x12345, true).into();
        assert_eq!([0x04100001, 0xe4a02345], result);
    }
}
