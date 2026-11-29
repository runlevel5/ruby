/// The struct that represents an XFX-form instruction that can be encoded.
///
/// +------+-------+----------------+------------+----+
/// | 0  5 | 6  10 | 11          20 | 21      30 | 31 |
/// |   31 | rt/rs | spr            | xo         |    |
/// +------+-------+----------------+------------+----+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
/// The spr field holds the low 5 bits of the register number followed by its high 5 bits.
pub struct XFXForm {
    /// The number of the target register for mfspr, or of the source register for mtspr.
    rt: u8,

    /// The number of the special purpose register.
    spr: u16,

    /// The extended opcode.
    xo: u16,
}

impl XFXForm {
    /// The number of the fixed-point exception register.
    pub const XER: u16 = 1;

    /// The number of the link register.
    pub const LR: u16 = 8;

    /// The number of the count register.
    pub const CTR: u16 = 9;

    /// MFSPR: rt = spr
    /// Power ISA 2.07B, Book I, Move To/From System Register Instructions
    pub fn mfspr(rt: u8, spr: u16) -> Self {
        Self { rt, spr, xo: 339 }
    }

    /// MTSPR: spr = rs
    /// Power ISA 2.07B, Book I, Move To/From System Register Instructions
    pub fn mtspr(spr: u16, rs: u8) -> Self {
        Self { rt: rs, spr, xo: 467 }
    }
}

impl From<XFXForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: XFXForm) -> Self {
        assert!(inst.rt < 32 && inst.spr < 1024 && inst.xo < 1024);

        0
        | (31 << 26)
        | ((inst.rt as u32) << 21)
        | (((inst.spr & 0x1f) as u32) << 16)
        | (((inst.spr >> 5) as u32) << 11)
        | ((inst.xo as u32) << 1)
    }
}

impl From<XFXForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: XFXForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_ne_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mfspr_lr() {
        let result: u32 = XFXForm::mfspr(0, XFXForm::LR).into();
        assert_eq!(0x7c0802a6, result);
    }

    #[test]
    fn test_mtspr_lr() {
        let result: u32 = XFXForm::mtspr(XFXForm::LR, 0).into();
        assert_eq!(0x7c0803a6, result);
    }

    #[test]
    fn test_mtspr_ctr() {
        let result: u32 = XFXForm::mtspr(XFXForm::CTR, 12).into();
        assert_eq!(0x7d8903a6, result);
    }

    #[test]
    fn test_mtspr_xer() {
        let result: u32 = XFXForm::mtspr(XFXForm::XER, 0).into();
        assert_eq!(0x7c0103a6, result);
    }
}
