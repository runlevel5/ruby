/// The struct that represents an I-form instruction that can be encoded.
///
/// +------+--------------------------+----+----+
/// | 0  5 | 6                     29 | 30 | 31 |
/// |   18 | li                       | aa | lk |
/// +------+--------------------------+----+----+
///
/// Bits are numbered as in the Power ISA, where bit 0 is the most significant.
pub struct IForm {
    /// The offset in bytes from this instruction to the target. Only its upper
    /// 24 bits are encoded, so it must be a multiple of 4.
    offset: i32,

    /// Whether the target is an absolute address rather than an offset.
    aa: bool,

    /// Whether to put the address of the next instruction in LR.
    lk: bool,
}

impl IForm {
    /// The number of bits the offset in bytes may take up, as a signed integer.
    pub const OFFSET_BITS: u32 = 26;

    /// B: branch to this instruction's address + offset
    /// Power ISA 2.07B, Book I, Branch Instructions
    pub fn b(offset: i32) -> Self {
        Self { offset, aa: false, lk: false }
    }

    /// BL: branch to this instruction's address + offset, put the return address in LR
    /// Power ISA 2.07B, Book I, Branch Instructions
    pub fn bl(offset: i32) -> Self {
        Self { offset, aa: false, lk: true }
    }
}

impl From<IForm> for u32 {
    /// Convert an instruction into a 32-bit value.
    fn from(inst: IForm) -> Self {
        let limit = 1 << (IForm::OFFSET_BITS - 1);
        assert!(inst.offset % 4 == 0, "I-form offset must be a multiple of 4");
        assert!((-limit..limit).contains(&inst.offset), "I-form offset must fit in 26 bits");

        0
        | (18 << 26)
        | ((inst.offset as u32) & 0x03ff_fffc)
        | ((inst.aa as u32) << 1)
        | (inst.lk as u32)
    }
}

impl From<IForm> for [u8; 4] {
    /// Convert an instruction into a 4 byte array.
    fn from(inst: IForm) -> [u8; 4] {
        let result: u32 = inst.into();
        result.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_b() {
        let result: u32 = IForm::b(256).into();
        assert_eq!(0x48000100, result);
    }

    #[test]
    fn test_b_backward() {
        let result: u32 = IForm::b(-4).into();
        assert_eq!(0x4bfffffc, result);
    }

    #[test]
    fn test_b_max() {
        let result: u32 = IForm::b(0x1fffffc).into();
        assert_eq!(0x49fffffc, result);
    }

    #[test]
    fn test_b_min() {
        let result: u32 = IForm::b(-0x2000000).into();
        assert_eq!(0x4a000000, result);
    }

    #[test]
    fn test_bl() {
        let result: u32 = IForm::bl(8).into();
        assert_eq!(0x48000009, result);
    }

    #[test]
    #[should_panic(expected = "I-form offset must fit in 26 bits")]
    fn test_b_too_far() {
        let _: u32 = IForm::b(0x2000000).into();
    }

    #[test]
    #[should_panic(expected = "I-form offset must be a multiple of 4")]
    fn test_b_unaligned() {
        let _: u32 = IForm::b(2).into();
    }
}
