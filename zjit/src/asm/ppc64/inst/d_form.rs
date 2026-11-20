/// The struct that represents a D-form instruction that can be encoded.
///
/// +------+-------+-------+----------------+
/// | 0  5 | 6  10 | 11 15 | 16          31 |
/// | opcd | rt/rs | ra    | si/ui          |
/// +------+-------+-------+----------------+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct DForm {
    /// The primary opcode.
    opcd: u8,

    /// The number of the target register, or of the source register for logical instructions.
    rt: u8,

    /// The number of the other register. As the base of addi and addis, 0 means the value zero.
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
}
