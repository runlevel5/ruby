#![allow(dead_code)] // For instructions and operands we're not currently using.
#![allow(clippy::identity_op)]

use crate::asm::CodeBlock;

mod inst;
mod opnd;

use inst::*;

// We're going to make these public to make using these things easier in the
// backend (so they don't have to have knowledge about the submodule).
pub use opnd::*;

/// Unwrap the number of a register operand.
fn reg_no(opnd: PPC64Opnd) -> u8 {
    match opnd {
        PPC64Opnd::Reg(reg) => reg.reg_no,
        _ => panic!("Expected register operand, got {opnd:?}"),
    }
}

/// Unwrap an immediate operand that fits in a signed 16-bit field.
fn simm16(opnd: PPC64Opnd) -> i16 {
    let imm = match opnd {
        PPC64Opnd::Imm(imm) => i16::try_from(imm).ok(),
        PPC64Opnd::UImm(uimm) => i16::try_from(uimm).ok(),
        _ => None,
    };
    imm.unwrap_or_else(|| panic!("Expected signed 16-bit immediate, got {opnd:?}"))
}

/// Unwrap an immediate operand that fits in an unsigned 16-bit field.
fn uimm16(opnd: PPC64Opnd) -> u16 {
    let uimm = match opnd {
        PPC64Opnd::Imm(imm) => u16::try_from(imm).ok(),
        PPC64Opnd::UImm(uimm) => u16::try_from(uimm).ok(),
        _ => None,
    };
    uimm.unwrap_or_else(|| panic!("Expected unsigned 16-bit immediate, got {opnd:?}"))
}

/// ADDI - add a signed 16-bit immediate to ra, put the result in rt
pub fn addi(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, imm: PPC64Opnd) {
    assert!(ra != R0, "addi reads r0 as zero, use li");
    let bytes: [u8; 4] = DForm::addi(reg_no(rt), reg_no(ra), simm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// ADDIS - add a signed 16-bit immediate shifted left by 16 bits to ra, put the result in rt
pub fn addis(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, imm: PPC64Opnd) {
    assert!(ra != R0, "addis reads r0 as zero, use lis");
    let bytes: [u8; 4] = DForm::addis(reg_no(rt), reg_no(ra), simm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// LI - put a sign-extended 16-bit immediate in rt
pub fn li(cb: &mut CodeBlock, rt: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::addi(reg_no(rt), 0, simm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// LIS - put a sign-extended 16-bit immediate shifted left by 16 bits in rt
pub fn lis(cb: &mut CodeBlock, rt: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::addis(reg_no(rt), 0, simm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// ORI - or rs with an unsigned 16-bit immediate, put the result in ra
pub fn ori(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::ori(reg_no(ra), reg_no(rs), uimm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// ORIS - or rs with an unsigned 16-bit immediate shifted left by 16 bits, put the result in ra
pub fn oris(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::oris(reg_no(ra), reg_no(rs), uimm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// Unwrap a memory operand into its base register number and a displacement
/// that fits in a signed 16-bit field.
fn mem_d(opnd: PPC64Opnd) -> (u8, i16) {
    match opnd {
        PPC64Opnd::Mem(mem) => {
            assert!(mem.base_reg_no != 0, "r0 reads as zero when used as a base register");
            let disp = i16::try_from(mem.disp)
                .unwrap_or_else(|_| panic!("Expected signed 16-bit displacement, got {}", mem.disp));
            (mem.base_reg_no, disp)
        }
        _ => panic!("Expected memory operand, got {opnd:?}"),
    }
}

/// ANDI. - and rs with an unsigned 16-bit immediate, put the result in ra, set CR0
pub fn andi_dot(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::andi_dot(reg_no(ra), reg_no(rs), uimm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// XORI - xor rs with an unsigned 16-bit immediate, put the result in ra
pub fn xori(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::xori(reg_no(ra), reg_no(rs), uimm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// NOP - do nothing
pub fn nop(cb: &mut CodeBlock) {
    let bytes: [u8; 4] = DForm::ori(0, 0, 0).into();
    cb.write_bytes(&bytes);
}

/// LBZ - load a byte from memory into rt, zero-extended
pub fn lbz(cb: &mut CodeBlock, rt: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_d(mem);
    let bytes: [u8; 4] = DForm::lbz(reg_no(rt), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// LHZ - load a halfword from memory into rt, zero-extended
pub fn lhz(cb: &mut CodeBlock, rt: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_d(mem);
    let bytes: [u8; 4] = DForm::lhz(reg_no(rt), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// LHA - load a halfword from memory into rt, sign-extended
pub fn lha(cb: &mut CodeBlock, rt: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_d(mem);
    let bytes: [u8; 4] = DForm::lha(reg_no(rt), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// LWZ - load a word from memory into rt, zero-extended
pub fn lwz(cb: &mut CodeBlock, rt: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_d(mem);
    let bytes: [u8; 4] = DForm::lwz(reg_no(rt), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// STB - store the low byte of rs into memory
pub fn stb(cb: &mut CodeBlock, rs: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_d(mem);
    let bytes: [u8; 4] = DForm::stb(reg_no(rs), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// STH - store the low halfword of rs into memory
pub fn sth(cb: &mut CodeBlock, rs: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_d(mem);
    let bytes: [u8; 4] = DForm::sth(reg_no(rs), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// STW - store the low word of rs into memory
pub fn stw(cb: &mut CodeBlock, rs: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_d(mem);
    let bytes: [u8; 4] = DForm::stw(reg_no(rs), ra, disp).into();
    cb.write_bytes(&bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use crate::assert_disasm_snapshot;

    fn compile<R>(run: R) -> CodeBlock where R: FnOnce(&mut super::CodeBlock) {
        let mut cb = super::CodeBlock::new_dummy();
        run(&mut cb);
        cb
    }

    #[test]
    fn test_addi() {
        let cb = compile(|cb| addi(cb, R3, R4, PPC64Opnd::new_imm(-8)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: addi r3, r4, -8");
        assert_snapshot!(cb.hexdump(), @"f8ff6438");
    }

    #[test]
    fn test_addi_uimm() {
        let cb = compile(|cb| addi(cb, R1, R1, PPC64Opnd::new_uimm(0x7fff)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: addi r1, r1, 0x7fff");
        assert_snapshot!(cb.hexdump(), @"ff7f2138");
    }

    #[test]
    #[should_panic(expected = "addi reads r0 as zero")]
    fn test_addi_r0_base() {
        compile(|cb| addi(cb, R3, R0, PPC64Opnd::new_imm(1)));
    }

    #[test]
    #[should_panic(expected = "Expected signed 16-bit immediate")]
    fn test_addi_imm_too_large() {
        compile(|cb| addi(cb, R3, R4, PPC64Opnd::new_imm(0x8000)));
    }

    #[test]
    fn test_addis() {
        let cb = compile(|cb| addis(cb, R3, R4, PPC64Opnd::new_imm(0x1234)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: addis r3, r4, 0x1234");
        assert_snapshot!(cb.hexdump(), @"3412643c");
    }

    #[test]
    fn test_li() {
        let cb = compile(|cb| li(cb, R0, PPC64Opnd::new_imm(-1)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: li r0, -1");
        assert_snapshot!(cb.hexdump(), @"ffff0038");
    }

    #[test]
    fn test_lis() {
        let cb = compile(|cb| lis(cb, R12, PPC64Opnd::new_imm(-32768)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lis r12, -0x8000");
        assert_snapshot!(cb.hexdump(), @"0080803d");
    }

    #[test]
    fn test_ori() {
        let cb = compile(|cb| ori(cb, R3, R4, PPC64Opnd::new_uimm(0xffff)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: ori r3, r4, 0xffff");
        assert_snapshot!(cb.hexdump(), @"ffff8360");
    }

    #[test]
    #[should_panic(expected = "Expected unsigned 16-bit immediate")]
    fn test_ori_negative_imm() {
        compile(|cb| ori(cb, R3, R4, PPC64Opnd::new_imm(-1)));
    }

    #[test]
    fn test_oris() {
        let cb = compile(|cb| oris(cb, R12, R11, PPC64Opnd::new_uimm(0x1234)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: oris r12, r11, 0x1234");
        assert_snapshot!(cb.hexdump(), @"34126c65");
    }

    #[test]
    fn test_andi_dot() {
        let cb = compile(|cb| andi_dot(cb, R3, R4, PPC64Opnd::new_uimm(0xff)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: andi. r3, r4, 0xff");
        assert_snapshot!(cb.hexdump(), @"ff008370");
    }

    #[test]
    fn test_xori() {
        let cb = compile(|cb| xori(cb, R3, R4, PPC64Opnd::new_uimm(1)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: xori r3, r4, 1");
        assert_snapshot!(cb.hexdump(), @"01008368");
    }

    #[test]
    fn test_nop() {
        let cb = compile(nop);
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: nop");
        assert_snapshot!(cb.hexdump(), @"00000060");
    }

    #[test]
    fn test_lbz() {
        let cb = compile(|cb| lbz(cb, R3, PPC64Opnd::new_mem(8, R4, 8)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lbz r3, 8(r4)");
        assert_snapshot!(cb.hexdump(), @"08006488");
    }

    #[test]
    fn test_lhz() {
        let cb = compile(|cb| lhz(cb, R3, PPC64Opnd::new_mem(16, R4, -2)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lhz r3, -2(r4)");
        assert_snapshot!(cb.hexdump(), @"feff64a0");
    }

    #[test]
    fn test_lha() {
        let cb = compile(|cb| lha(cb, R3, PPC64Opnd::new_mem(16, R4, 2)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lha r3, 2(r4)");
        assert_snapshot!(cb.hexdump(), @"020064a8");
    }

    #[test]
    fn test_lwz() {
        let cb = compile(|cb| lwz(cb, R3, PPC64Opnd::new_mem(32, R1, 0x7ffc)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lwz r3, 0x7ffc(r1)");
        assert_snapshot!(cb.hexdump(), @"fc7f6180");
    }

    #[test]
    #[should_panic(expected = "r0 reads as zero when used as a base register")]
    fn test_load_r0_base() {
        compile(|cb| lwz(cb, R3, PPC64Opnd::new_mem(32, R0, 0)));
    }

    #[test]
    #[should_panic(expected = "Expected signed 16-bit displacement")]
    fn test_load_disp_too_large() {
        compile(|cb| lwz(cb, R3, PPC64Opnd::new_mem(32, R1, 0x8000)));
    }

    #[test]
    fn test_stb() {
        let cb = compile(|cb| stb(cb, R3, PPC64Opnd::new_mem(8, R4, 1)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: stb r3, 1(r4)");
        assert_snapshot!(cb.hexdump(), @"01006498");
    }

    #[test]
    fn test_sth() {
        let cb = compile(|cb| sth(cb, R5, PPC64Opnd::new_mem(16, R31, -32768)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: sth r5, -0x8000(r31)");
        assert_snapshot!(cb.hexdump(), @"0080bfb0");
    }

    #[test]
    fn test_stw() {
        let cb = compile(|cb| stw(cb, R0, PPC64Opnd::new_mem(32, R1, 8)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: stw r0, 8(r1)");
        assert_snapshot!(cb.hexdump(), @"08000190");
    }
}
