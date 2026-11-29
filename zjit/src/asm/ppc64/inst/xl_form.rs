/// The struct that represents an XL-form branch instruction that can be encoded.
///
/// +------+-------+-------+-------+-------+------------+----+
/// | 0  5 | 6  10 | 11 15 | 16 18 | 19 20 | 21      30 | 31 |
/// |   19 | bo    | bi    |       | bh    | xo         | lk |
/// +------+-------+-------+-------+-------+------------+----+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct XLForm {
    /// How to decide whether to branch.
    bo: u8,

    /// The CR bit to test.
    bi: u8,

    /// A hint about what kind of branch this is.
    bh: u8,

    /// The extended opcode.
    xo: u16,

    /// Whether to put the address of the next instruction in LR.
    lk: bool,
}

impl XLForm {
    /// Branch always, regardless of the CR.
    const BO_ALWAYS: u8 = 0b10100;

    /// BLR: branch to the address in LR
    /// Power ISA 2.07B, Book I, Branch Instructions
    pub fn blr() -> Self {
        Self { bo: Self::BO_ALWAYS, bi: 0, bh: 0, xo: 16, lk: false }
    }

    /// BCTR: branch to the address in CTR
    /// Power ISA 2.07B, Book I, Branch Instructions
    pub fn bctr() -> Self {
        Self { bo: Self::BO_ALWAYS, bi: 0, bh: 0, xo: 528, lk: false }
    }

    /// BCTRL: branch to the address in CTR, put the return address in LR
    /// Power ISA 2.07B, Book I, Branch Instructions
    pub fn bctrl() -> Self {
        Self { bo: Self::BO_ALWAYS, bi: 0, bh: 0, xo: 528, lk: true }
    }
}

impl From<XLForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: XLForm) -> Self {
        assert!(inst.bo < 32 && inst.bi < 32 && inst.bh < 4 && inst.xo < 1024);

        0
        | (19 << 26)
        | ((inst.bo as u32) << 21)
        | ((inst.bi as u32) << 16)
        | ((inst.bh as u32) << 11)
        | ((inst.xo as u32) << 1)
        | (inst.lk as u32)
    }
}

impl From<XLForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: XLForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_ne_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blr() {
        let result: u32 = XLForm::blr().into();
        assert_eq!(0x4e800020, result);
    }

    #[test]
    fn test_bctr() {
        let result: u32 = XLForm::bctr().into();
        assert_eq!(0x4e800420, result);
    }

    #[test]
    fn test_bctrl() {
        let result: u32 = XLForm::bctrl().into();
        assert_eq!(0x4e800421, result);
    }
}
