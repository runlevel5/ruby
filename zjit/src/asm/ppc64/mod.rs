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

/// CMPDI - compare ra with a signed 16-bit immediate as signed 64-bit integers, set CR0
pub fn cmpdi(cb: &mut CodeBlock, ra: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::cmpi(0, true, reg_no(ra), simm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// CMPWI - compare ra with a signed 16-bit immediate as signed 32-bit integers, set CR0
pub fn cmpwi(cb: &mut CodeBlock, ra: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::cmpi(0, false, reg_no(ra), simm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// CMPLDI - compare ra with an unsigned 16-bit immediate as unsigned 64-bit integers, set CR0
pub fn cmpldi(cb: &mut CodeBlock, ra: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::cmpli(0, true, reg_no(ra), uimm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// CMPLWI - compare ra with an unsigned 16-bit immediate as unsigned 32-bit integers, set CR0
pub fn cmplwi(cb: &mut CodeBlock, ra: PPC64Opnd, imm: PPC64Opnd) {
    let bytes: [u8; 4] = DForm::cmpli(0, false, reg_no(ra), uimm16(imm)).into();
    cb.write_bytes(&bytes);
}

/// Unwrap a memory operand like [`mem_d`], for instructions that also need
/// the displacement to be a multiple of 4.
fn mem_ds(opnd: PPC64Opnd) -> (u8, i16) {
    let (ra, disp) = mem_d(opnd);
    assert!(disp % 4 == 0, "Expected displacement to be a multiple of 4, got {disp}");
    (ra, disp)
}

/// LD - load a doubleword from memory into rt
pub fn ld(cb: &mut CodeBlock, rt: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_ds(mem);
    let bytes: [u8; 4] = DSForm::ld(reg_no(rt), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// LWA - load a word from memory into rt, sign-extended
pub fn lwa(cb: &mut CodeBlock, rt: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_ds(mem);
    let bytes: [u8; 4] = DSForm::lwa(reg_no(rt), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// STD - store rs into memory
pub fn std(cb: &mut CodeBlock, rs: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_ds(mem);
    let bytes: [u8; 4] = DSForm::std(reg_no(rs), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// STDU - store rs into memory, then move the base register to the address stored to
pub fn stdu(cb: &mut CodeBlock, rs: PPC64Opnd, mem: PPC64Opnd) {
    let (ra, disp) = mem_ds(mem);
    let bytes: [u8; 4] = DSForm::stdu(reg_no(rs), ra, disp).into();
    cb.write_bytes(&bytes);
}

/// ADD - add ra and rb, put the result in rt
pub fn add(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::add(reg_no(rt), reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// SUBF - subtract ra from rb, put the result in rt
pub fn subf(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::subf(reg_no(rt), reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// NEG - negate ra, put the result in rt
pub fn neg(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::neg(reg_no(rt), reg_no(ra)).into();
    cb.write_bytes(&bytes);
}

/// MULLD - multiply ra and rb, put the low 64 bits of the result in rt
pub fn mulld(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::mulld(reg_no(rt), reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// MULHD - multiply ra and rb as signed integers, put the high 64 bits of the result in rt
pub fn mulhd(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::mulhd(reg_no(rt), reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// AND - and rs with rb, put the result in ra
pub fn and(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::and(reg_no(ra), reg_no(rs), reg_no(rb), false).into();
    cb.write_bytes(&bytes);
}

/// AND. - and rs with rb, put the result in ra, set CR0
pub fn and_dot(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::and(reg_no(ra), reg_no(rs), reg_no(rb), true).into();
    cb.write_bytes(&bytes);
}

/// OR - or rs with rb, put the result in ra
pub fn or(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::or(reg_no(ra), reg_no(rs), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// MR - copy rs to ra
pub fn mr(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd) {
    or(cb, ra, rs, rs);
}

/// XOR - xor rs with rb, put the result in ra
pub fn xor(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::xor(reg_no(ra), reg_no(rs), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// NOR - or rs with rb, put the complement of the result in ra
pub fn nor(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::nor(reg_no(ra), reg_no(rs), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// NOT - put the complement of rs in ra
pub fn not(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd) {
    nor(cb, ra, rs, rs);
}

/// CMPD - compare ra with rb as signed 64-bit integers, set CR0
pub fn cmpd(cb: &mut CodeBlock, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::cmp(0, true, reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// CMPW - compare ra with rb as signed 32-bit integers, set CR0
pub fn cmpw(cb: &mut CodeBlock, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::cmp(0, false, reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// CMPLD - compare ra with rb as unsigned 64-bit integers, set CR0
pub fn cmpld(cb: &mut CodeBlock, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::cmpl(0, true, reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// CMPLW - compare ra with rb as unsigned 32-bit integers, set CR0
pub fn cmplw(cb: &mut CodeBlock, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::cmpl(0, false, reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// Unwrap the number of a register operand used as the base of an indexed load or store.
fn base_reg_no(opnd: PPC64Opnd) -> u8 {
    assert!(opnd != R0, "r0 reads as zero when used as a base register");
    reg_no(opnd)
}

/// LDX - load a doubleword from memory at ra + rb into rt
pub fn ldx(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::ldx(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// LBZX - load a byte from memory at ra + rb into rt, zero-extended
pub fn lbzx(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::lbzx(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// LHZX - load a halfword from memory at ra + rb into rt, zero-extended
pub fn lhzx(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::lhzx(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// LHAX - load a halfword from memory at ra + rb into rt, sign-extended
pub fn lhax(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::lhax(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// LWZX - load a word from memory at ra + rb into rt, zero-extended
pub fn lwzx(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::lwzx(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// LWAX - load a word from memory at ra + rb into rt, sign-extended
pub fn lwax(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::lwax(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// STDX - store rs into memory at ra + rb
pub fn stdx(cb: &mut CodeBlock, rs: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::stdx(reg_no(rs), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// STBX - store the low byte of rs into memory at ra + rb
pub fn stbx(cb: &mut CodeBlock, rs: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::stbx(reg_no(rs), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// STHX - store the low halfword of rs into memory at ra + rb
pub fn sthx(cb: &mut CodeBlock, rs: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::sthx(reg_no(rs), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// STWX - store the low word of rs into memory at ra + rb
pub fn stwx(cb: &mut CodeBlock, rs: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::stwx(reg_no(rs), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// LDBRX - load a doubleword from memory at ra + rb into rt with its bytes reversed
pub fn ldbrx(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::ldbrx(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// LWBRX - load a word from memory at ra + rb into rt with its bytes reversed, zero-extended
pub fn lwbrx(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::lwbrx(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// LHBRX - load a halfword from memory at ra + rb into rt with its bytes reversed, zero-extended
pub fn lhbrx(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::lhbrx(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
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

    #[test]
    fn test_cmpdi() {
        let cb = compile(|cb| cmpdi(cb, R3, PPC64Opnd::new_imm(-1)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: cmpdi r3, -1");
        assert_snapshot!(cb.hexdump(), @"ffff232c");
    }

    #[test]
    fn test_cmpwi() {
        let cb = compile(|cb| cmpwi(cb, R3, PPC64Opnd::new_imm(100)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: cmpwi r3, 0x64");
        assert_snapshot!(cb.hexdump(), @"6400032c");
    }

    #[test]
    fn test_cmpldi() {
        let cb = compile(|cb| cmpldi(cb, R3, PPC64Opnd::new_uimm(0xffff)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: cmpldi r3, 0xffff");
        assert_snapshot!(cb.hexdump(), @"ffff2328");
    }

    #[test]
    fn test_cmplwi() {
        let cb = compile(|cb| cmplwi(cb, R4, PPC64Opnd::new_uimm(0)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: cmplwi r4, 0");
        assert_snapshot!(cb.hexdump(), @"00000428");
    }

    #[test]
    #[should_panic(expected = "Expected unsigned 16-bit immediate")]
    fn test_cmpldi_negative_imm() {
        compile(|cb| cmpldi(cb, R3, PPC64Opnd::new_imm(-1)));
    }

    #[test]
    fn test_ld() {
        let cb = compile(|cb| ld(cb, R3, PPC64Opnd::new_mem(64, R4, 8)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: ld r3, 8(r4)");
        assert_snapshot!(cb.hexdump(), @"080064e8");
    }

    #[test]
    fn test_ld_negative_disp() {
        let cb = compile(|cb| ld(cb, R3, PPC64Opnd::new_mem(64, R1, -8)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: ld r3, -8(r1)");
        assert_snapshot!(cb.hexdump(), @"f8ff61e8");
    }

    #[test]
    #[should_panic(expected = "Expected displacement to be a multiple of 4")]
    fn test_ld_unaligned_disp() {
        compile(|cb| ld(cb, R3, PPC64Opnd::new_mem(64, R4, 6)));
    }

    #[test]
    fn test_lwa() {
        let cb = compile(|cb| lwa(cb, R3, PPC64Opnd::new_mem(32, R4, 4)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lwa r3, 4(r4)");
        assert_snapshot!(cb.hexdump(), @"060064e8");
    }

    #[test]
    fn test_std() {
        let cb = compile(|cb| std(cb, R0, PPC64Opnd::new_mem(64, R1, 16)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: std r0, 0x10(r1)");
        assert_snapshot!(cb.hexdump(), @"100001f8");
    }

    #[test]
    fn test_stdu() {
        let cb = compile(|cb| stdu(cb, R1, PPC64Opnd::new_mem(64, R1, -112)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: stdu r1, -0x70(r1)");
        assert_snapshot!(cb.hexdump(), @"91ff21f8");
    }

    #[test]
    fn test_add() {
        let cb = compile(|cb| add(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: add r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"142a647c");
    }

    #[test]
    fn test_subf() {
        let cb = compile(|cb| subf(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: subf r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"5028647c");
    }

    #[test]
    fn test_neg() {
        let cb = compile(|cb| neg(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: neg r3, r4");
        assert_snapshot!(cb.hexdump(), @"d000647c");
    }

    #[test]
    fn test_mulld() {
        let cb = compile(|cb| mulld(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: mulld r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"d229647c");
    }

    #[test]
    fn test_mulhd() {
        let cb = compile(|cb| mulhd(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: mulhd r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"9228647c");
    }

    #[test]
    fn test_and() {
        let cb = compile(|cb| and(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: and r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"3828837c");
    }

    #[test]
    fn test_and_dot() {
        let cb = compile(|cb| and_dot(cb, R0, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: and. r0, r4, r5");
        assert_snapshot!(cb.hexdump(), @"3928807c");
    }

    #[test]
    fn test_or() {
        let cb = compile(|cb| or(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: or r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"782b837c");
    }

    #[test]
    fn test_mr() {
        let cb = compile(|cb| mr(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: mr r3, r4");
        assert_snapshot!(cb.hexdump(), @"7823837c");
    }

    #[test]
    fn test_xor() {
        let cb = compile(|cb| xor(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: xor r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"782a837c");
    }

    #[test]
    fn test_nor() {
        let cb = compile(|cb| nor(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: nor r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"f828837c");
    }

    #[test]
    fn test_not() {
        let cb = compile(|cb| not(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: nor r3, r4, r4");
        assert_snapshot!(cb.hexdump(), @"f820837c");
    }

    #[test]
    fn test_cmpd() {
        let cb = compile(|cb| cmpd(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: cmpd r3, r4");
        assert_snapshot!(cb.hexdump(), @"0020237c");
    }

    #[test]
    fn test_cmpw() {
        let cb = compile(|cb| cmpw(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: cmpw r3, r4");
        assert_snapshot!(cb.hexdump(), @"0020037c");
    }

    #[test]
    fn test_cmpld() {
        let cb = compile(|cb| cmpld(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: cmpld r3, r4");
        assert_snapshot!(cb.hexdump(), @"4020237c");
    }

    #[test]
    fn test_cmplw() {
        let cb = compile(|cb| cmplw(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: cmplw r3, r4");
        assert_snapshot!(cb.hexdump(), @"4020037c");
    }

    #[test]
    fn test_ldx() {
        let cb = compile(|cb| ldx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: ldx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"2a28647c");
    }

    #[test]
    fn test_lbzx() {
        let cb = compile(|cb| lbzx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lbzx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"ae28647c");
    }

    #[test]
    fn test_lhzx() {
        let cb = compile(|cb| lhzx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lhzx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"2e2a647c");
    }

    #[test]
    fn test_lhax() {
        let cb = compile(|cb| lhax(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lhax r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"ae2a647c");
    }

    #[test]
    fn test_lwzx() {
        let cb = compile(|cb| lwzx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lwzx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"2e28647c");
    }

    #[test]
    fn test_lwax() {
        let cb = compile(|cb| lwax(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lwax r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"aa2a647c");
    }

    #[test]
    fn test_stdx() {
        let cb = compile(|cb| stdx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: stdx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"2a29647c");
    }

    #[test]
    fn test_stbx() {
        let cb = compile(|cb| stbx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: stbx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"ae29647c");
    }

    #[test]
    fn test_sthx() {
        let cb = compile(|cb| sthx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: sthx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"2e2b647c");
    }

    #[test]
    fn test_stwx() {
        let cb = compile(|cb| stwx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: stwx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"2e29647c");
    }

    #[test]
    fn test_ldbrx() {
        let cb = compile(|cb| ldbrx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: ldbrx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"282c647c");
    }

    #[test]
    fn test_lwbrx() {
        let cb = compile(|cb| lwbrx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lwbrx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"2c2c647c");
    }

    #[test]
    fn test_lhbrx() {
        let cb = compile(|cb| lhbrx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: lhbrx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"2c2e647c");
    }

    #[test]
    #[should_panic(expected = "r0 reads as zero when used as a base register")]
    fn test_ldx_r0_base() {
        compile(|cb| ldx(cb, R3, R0, R5));
    }
}
