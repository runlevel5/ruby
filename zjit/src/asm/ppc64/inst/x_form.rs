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
    /// As the base of loads and stores, 0 means the value zero.
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

    /// SLD: ra = rs shifted left by the low 7 bits of rb, which gives zero for 64 and above
    /// Power ISA 2.07B, Book I, Fixed-Point Rotate and Shift Instructions
    pub fn sld(ra: u8, rs: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 27, rc: false }
    }

    /// SRD: ra = rs shifted right by the low 7 bits of rb, which gives zero for 64 and above
    /// Power ISA 2.07B, Book I, Fixed-Point Rotate and Shift Instructions
    pub fn srd(ra: u8, rs: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 539, rc: false }
    }

    /// SRAD: ra = rs shifted right by the low 7 bits of rb, filling with copies of the sign bit
    /// Power ISA 2.07B, Book I, Fixed-Point Rotate and Shift Instructions
    pub fn srad(ra: u8, rs: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 794, rc: false }
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

    /// LDX: rt = doubleword at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn ldx(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, xo: 21, rc: false }
    }

    /// LBZX: rt = zero-extended byte at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lbzx(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, xo: 87, rc: false }
    }

    /// LHZX: rt = zero-extended halfword at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lhzx(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, xo: 279, rc: false }
    }

    /// LHAX: rt = sign-extended halfword at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lhax(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, xo: 343, rc: false }
    }

    /// LWZX: rt = zero-extended word at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lwzx(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, xo: 23, rc: false }
    }

    /// LWAX: rt = sign-extended word at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lwax(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, xo: 341, rc: false }
    }

    /// STDX: store rs at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Store Instructions
    pub fn stdx(rs: u8, ra: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 149, rc: false }
    }

    /// STBX: store the low byte of rs at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Store Instructions
    pub fn stbx(rs: u8, ra: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 215, rc: false }
    }

    /// STHX: store the low halfword of rs at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Store Instructions
    pub fn sthx(rs: u8, ra: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 407, rc: false }
    }

    /// STWX: store the low word of rs at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Store Instructions
    pub fn stwx(rs: u8, ra: u8, rb: u8) -> Self {
        Self { rt: rs, ra, rb, xo: 151, rc: false }
    }

    /// LDBRX: rt = byte-reversed doubleword at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Load and Store with Byte Reversal Instructions
    pub fn ldbrx(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, xo: 532, rc: false }
    }

    /// LWBRX: rt = zero-extended byte-reversed word at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Load and Store with Byte Reversal Instructions
    pub fn lwbrx(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, xo: 534, rc: false }
    }

    /// LHBRX: rt = zero-extended byte-reversed halfword at (ra|0) + rb
    /// Power ISA 2.07B, Book I, Fixed-Point Load and Store with Byte Reversal Instructions
    pub fn lhbrx(rt: u8, ra: u8, rb: u8) -> Self {
        Self { rt, ra, rb, xo: 790, rc: false }
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

    #[test]
    fn test_ldx() {
        let result: u32 = XForm::ldx(3, 4, 5).into();
        assert_eq!(0x7c64282a, result);
    }

    #[test]
    fn test_lbzx() {
        let result: u32 = XForm::lbzx(3, 4, 5).into();
        assert_eq!(0x7c6428ae, result);
    }

    #[test]
    fn test_lhzx() {
        let result: u32 = XForm::lhzx(3, 4, 5).into();
        assert_eq!(0x7c642a2e, result);
    }

    #[test]
    fn test_lhax() {
        let result: u32 = XForm::lhax(3, 4, 5).into();
        assert_eq!(0x7c642aae, result);
    }

    #[test]
    fn test_lwzx() {
        let result: u32 = XForm::lwzx(3, 4, 5).into();
        assert_eq!(0x7c64282e, result);
    }

    #[test]
    fn test_lwax() {
        let result: u32 = XForm::lwax(3, 4, 5).into();
        assert_eq!(0x7c642aaa, result);
    }

    #[test]
    fn test_stdx() {
        let result: u32 = XForm::stdx(3, 4, 5).into();
        assert_eq!(0x7c64292a, result);
    }

    #[test]
    fn test_stbx() {
        let result: u32 = XForm::stbx(3, 4, 5).into();
        assert_eq!(0x7c6429ae, result);
    }

    #[test]
    fn test_sthx() {
        let result: u32 = XForm::sthx(3, 4, 5).into();
        assert_eq!(0x7c642b2e, result);
    }

    #[test]
    fn test_stwx() {
        let result: u32 = XForm::stwx(3, 4, 5).into();
        assert_eq!(0x7c64292e, result);
    }

    #[test]
    fn test_ldbrx() {
        let result: u32 = XForm::ldbrx(3, 4, 5).into();
        assert_eq!(0x7c642c28, result);
    }

    #[test]
    fn test_lwbrx() {
        let result: u32 = XForm::lwbrx(3, 4, 5).into();
        assert_eq!(0x7c642c2c, result);
    }

    #[test]
    fn test_lhbrx() {
        let result: u32 = XForm::lhbrx(3, 4, 5).into();
        assert_eq!(0x7c642e2c, result);
    }

    #[test]
    fn test_sld() {
        let result: u32 = XForm::sld(3, 4, 5).into();
        assert_eq!(0x7c832836, result);
    }

    #[test]
    fn test_srd() {
        let result: u32 = XForm::srd(3, 4, 5).into();
        assert_eq!(0x7c832c36, result);
    }

    #[test]
    fn test_srad() {
        let result: u32 = XForm::srad(3, 4, 5).into();
        assert_eq!(0x7c832e34, result);
    }
}
