/// The struct that represents a D-form instruction that can be encoded.
///
/// +------+-------+-------+----------------+
/// | 0  5 | 6  10 | 11 15 | 16          31 |
/// | opcd | rt/rs | ra    | si/ui/d        |
/// +------+-------+-------+----------------+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct DForm {
    /// The primary opcode.
    opcd: u8,

    /// The number of the target register, or of the source register for logical instructions.
    rt: u8,

    /// The number of the other register. As the base of addi, addis, loads and stores, 0 means the value zero.
    ra: u8,

    /// The 16-bit immediate. Whether it is sign-extended depends on the instruction.
    imm: u16,
}

impl DForm {
    /// ADDI: rt = (ra|0) + sign-extended si
    /// Power ISA 2.07B, Book I, Fixed-Point Arithmetic Instructions
    pub fn addi(rt: u8, ra: u8, si: i16) -> Self {
        Self { opcd: 14, rt, ra, imm: si as u16 }
    }

    /// ADDIS: rt = (ra|0) + (sign-extended si << 16)
    /// Power ISA 2.07B, Book I, Fixed-Point Arithmetic Instructions
    pub fn addis(rt: u8, ra: u8, si: i16) -> Self {
        Self { opcd: 15, rt, ra, imm: si as u16 }
    }

    /// ORI: ra = rs | zero-extended ui
    /// Power ISA 2.07B, Book I, Fixed-Point Logical Instructions
    pub fn ori(ra: u8, rs: u8, ui: u16) -> Self {
        Self { opcd: 24, rt: rs, ra, imm: ui }
    }

    /// ORIS: ra = rs | (zero-extended ui << 16)
    /// Power ISA 2.07B, Book I, Fixed-Point Logical Instructions
    pub fn oris(ra: u8, rs: u8, ui: u16) -> Self {
        Self { opcd: 25, rt: rs, ra, imm: ui }
    }

    /// ANDI.: ra = rs & zero-extended ui, and set CR0 by comparing the result with zero
    /// Power ISA 2.07B, Book I, Fixed-Point Logical Instructions
    pub fn andi_dot(ra: u8, rs: u8, ui: u16) -> Self {
        Self { opcd: 28, rt: rs, ra, imm: ui }
    }

    /// XORI: ra = rs ^ zero-extended ui
    /// Power ISA 2.07B, Book I, Fixed-Point Logical Instructions
    pub fn xori(ra: u8, rs: u8, ui: u16) -> Self {
        Self { opcd: 26, rt: rs, ra, imm: ui }
    }

    /// LBZ: rt = zero-extended byte at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lbz(rt: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 34, rt, ra, imm: d as u16 }
    }

    /// LHZ: rt = zero-extended halfword at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lhz(rt: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 40, rt, ra, imm: d as u16 }
    }

    /// LHA: rt = sign-extended halfword at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lha(rt: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 42, rt, ra, imm: d as u16 }
    }

    /// LWZ: rt = zero-extended word at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Load Instructions
    pub fn lwz(rt: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 32, rt, ra, imm: d as u16 }
    }

    /// STB: store the low byte of rs at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Store Instructions
    pub fn stb(rs: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 38, rt: rs, ra, imm: d as u16 }
    }

    /// STH: store the low halfword of rs at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Store Instructions
    pub fn sth(rs: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 44, rt: rs, ra, imm: d as u16 }
    }

    /// STW: store the low word of rs at (ra|0) + sign-extended d
    /// Power ISA 2.07B, Book I, Fixed-Point Store Instructions
    pub fn stw(rs: u8, ra: u8, d: i16) -> Self {
        Self { opcd: 36, rt: rs, ra, imm: d as u16 }
    }

    /// CMPI: compare ra with sign-extended si as signed integers, set CR field bf.
    /// Compares all 64 bits of ra when l is true and its low 32 bits otherwise.
    /// The bits where rt usually goes hold bf, a reserved bit, and l.
    /// Power ISA 2.07B, Book I, Fixed-Point Compare Instructions
    pub fn cmpi(bf: u8, l: bool, ra: u8, si: i16) -> Self {
        assert!(bf < 8);
        Self { opcd: 11, rt: (bf << 2) | (l as u8), ra, imm: si as u16 }
    }

    /// CMPLI: compare ra with zero-extended ui as unsigned integers, set CR field bf.
    /// Compares all 64 bits of ra when l is true and its low 32 bits otherwise.
    /// Power ISA 2.07B, Book I, Fixed-Point Compare Instructions
    pub fn cmpli(bf: u8, l: bool, ra: u8, ui: u16) -> Self {
        assert!(bf < 8);
        Self { opcd: 10, rt: (bf << 2) | (l as u8), ra, imm: ui }
    }
}

impl From<DForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: DForm) -> Self {
        assert!(inst.opcd < 64 && inst.rt < 32 && inst.ra < 32);

        0
        | ((inst.opcd as u32) << 26)
        | ((inst.rt as u32) << 21)
        | ((inst.ra as u32) << 16)
        | (inst.imm as u32)
    }
}

impl From<DForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: DForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addi() {
        let result: u32 = DForm::addi(3, 4, 1).into();
        assert_eq!(0x38640001, result);
    }

    #[test]
    fn test_addi_negative() {
        let result: u32 = DForm::addi(3, 4, -1).into();
        assert_eq!(0x3864ffff, result);
    }

    #[test]
    fn test_addis() {
        let result: u32 = DForm::addis(31, 1, -32768).into();
        assert_eq!(0x3fe18000, result);
    }

    #[test]
    fn test_ori() {
        let result: u32 = DForm::ori(3, 4, 0xffff).into();
        assert_eq!(0x6083ffff, result);
    }

    #[test]
    fn test_oris() {
        let result: u32 = DForm::oris(12, 11, 0x1234).into();
        assert_eq!(0x656c1234, result);
    }

    #[test]
    fn test_andi_dot() {
        let result: u32 = DForm::andi_dot(3, 4, 0xff).into();
        assert_eq!(0x708300ff, result);
    }

    #[test]
    fn test_xori() {
        let result: u32 = DForm::xori(3, 4, 1).into();
        assert_eq!(0x68830001, result);
    }

    #[test]
    fn test_lbz() {
        let result: u32 = DForm::lbz(3, 4, 8).into();
        assert_eq!(0x88640008, result);
    }

    #[test]
    fn test_lhz() {
        let result: u32 = DForm::lhz(3, 4, -2).into();
        assert_eq!(0xa064fffe, result);
    }

    #[test]
    fn test_lha() {
        let result: u32 = DForm::lha(3, 4, 2).into();
        assert_eq!(0xa8640002, result);
    }

    #[test]
    fn test_lwz() {
        let result: u32 = DForm::lwz(3, 1, 0x7ffc).into();
        assert_eq!(0x80617ffc, result);
    }

    #[test]
    fn test_stb() {
        let result: u32 = DForm::stb(3, 4, 1).into();
        assert_eq!(0x98640001, result);
    }

    #[test]
    fn test_sth() {
        let result: u32 = DForm::sth(5, 31, -32768).into();
        assert_eq!(0xb0bf8000, result);
    }

    #[test]
    fn test_stw() {
        let result: u32 = DForm::stw(0, 1, 8).into();
        assert_eq!(0x90010008, result);
    }

    #[test]
    fn test_cmpi() {
        let result: u32 = DForm::cmpi(0, true, 3, -1).into();
        assert_eq!(0x2c23ffff, result);
    }

    #[test]
    fn test_cmpi_32_bits() {
        let result: u32 = DForm::cmpi(0, false, 3, 100).into();
        assert_eq!(0x2c030064, result);
    }

    #[test]
    fn test_cmpi_cr7() {
        let result: u32 = DForm::cmpi(7, true, 3, 0).into();
        assert_eq!(0x2fa30000, result);
    }

    #[test]
    fn test_cmpli() {
        let result: u32 = DForm::cmpli(0, true, 3, 0xffff).into();
        assert_eq!(0x2823ffff, result);
    }

    #[test]
    fn test_cmpli_32_bits() {
        let result: u32 = DForm::cmpli(0, false, 4, 0).into();
        assert_eq!(0x28040000, result);
    }
}
