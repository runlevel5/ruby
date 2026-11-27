#![allow(dead_code)] // For instructions and operands we're not currently using.
#![allow(clippy::identity_op)]

use crate::asm::CodeBlock;

mod arg;
mod inst;
mod opnd;

use inst::*;

// We're going to make these public to make using these things easier in the
// backend (so they don't have to have knowledge about the submodule).
pub use arg::*;
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

/// Whether a signed value fits in the immediate of a prefixed instruction
pub const fn prefixed_imm_fits(imm: i64) -> bool {
    let limit = 1 << (PrefixedDForm::IMM_BITS - 1);
    -limit <= imm && imm < limit
}

/// Emit a nop if a prefixed instruction would cross a 64-byte boundary at the write
/// position, which it must not. Code memory starts at a page boundary, so the write
/// position is as far into a 64-byte block as the address is.
pub fn align_prefixed(cb: &mut CodeBlock) {
    if cb.get_write_pos() % 64 == 60 {
        nop(cb);
    }
}

/// Write a prefixed instruction, which [`align_prefixed`] has to come before
fn write_prefixed(cb: &mut CodeBlock, bytes: [u8; 8]) {
    assert!(cb.get_write_pos() % 64 != 60, "a prefixed instruction must not cross a 64-byte boundary");
    cb.write_bytes(&bytes);
}

/// PLI - put a sign-extended 34-bit immediate in rt. Needs ISA 3.1.
pub fn pli(cb: &mut CodeBlock, rt: PPC64Opnd, imm: i64) {
    write_prefixed(cb, PrefixedDForm::paddi(reg_no(rt), 0, imm, false).into());
}

/// PLA - put the address that is offset bytes away from this instruction in rt. Needs ISA 3.1.
pub fn pla(cb: &mut CodeBlock, rt: PPC64Opnd, offset: i64) {
    write_prefixed(cb, PrefixedDForm::paddi(reg_no(rt), 0, offset, true).into());
}

/// PLD - load the doubleword that is offset bytes away from this instruction into rt. Needs ISA 3.1.
pub fn pld_pc_relative(cb: &mut CodeBlock, rt: PPC64Opnd, offset: i64) {
    write_prefixed(cb, PrefixedDForm::pld(reg_no(rt), 0, offset, true).into());
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

/// ADDO - add ra and rb, put the result in rt, set OV of XER on signed overflow
pub fn addo(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::addo(reg_no(rt), reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// SUBFO - subtract ra from rb, put the result in rt, set OV of XER on signed overflow
pub fn subfo(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::subfo(reg_no(rt), reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// MULLDO - multiply ra by rb, put the low 64 bits of the product in rt, set OV of XER
/// if the product does not fit in them
pub fn mulldo(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::mulldo(reg_no(rt), reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// ADDO. - add ra and rb, put the result in rt, set SO of CR0 on signed overflow.
/// SO is sticky, so it only tells about this addition if SO of XER was clear before it.
pub fn addo_dot(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::addo_dot(reg_no(rt), reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// SUBFO. - subtract ra from rb, put the result in rt, set SO of CR0 on signed overflow.
/// SO is sticky, so it only tells about this subtraction if SO of XER was clear before it.
pub fn subfo_dot(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::subfo_dot(reg_no(rt), reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// MULLDO. - multiply ra by rb, put the low 64 bits of the product in rt, set SO of CR0
/// if the product does not fit in them.
/// SO is sticky, so it only tells about this multiplication if SO of XER was clear before it.
pub fn mulldo_dot(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XOForm::mulldo_dot(reg_no(rt), reg_no(ra), reg_no(rb)).into();
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

/// Unwrap an immediate operand that fits in an unsigned 6-bit field.
fn uimm6(opnd: PPC64Opnd) -> u8 {
    let uimm = match opnd {
        PPC64Opnd::Imm(imm) => u8::try_from(imm).ok(),
        PPC64Opnd::UImm(uimm) => u8::try_from(uimm).ok(),
        _ => None,
    };
    uimm.filter(|&uimm| uimm < 64).unwrap_or_else(|| panic!("Expected unsigned 6-bit immediate, got {opnd:?}"))
}

/// SLD - shift rs left by rb, put the result in ra
pub fn sld(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::sld(reg_no(ra), reg_no(rs), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// SRD - shift rs right by rb, put the result in ra
pub fn srd(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::srd(reg_no(ra), reg_no(rs), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// SRAD - shift rs right by rb while keeping its sign, put the result in ra
pub fn srad(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::srad(reg_no(ra), reg_no(rs), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// SRADI - shift rs right by an immediate while keeping its sign, put the result in ra
pub fn sradi(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, sh: PPC64Opnd) {
    let bytes: [u8; 4] = XSForm::sradi(reg_no(ra), reg_no(rs), uimm6(sh)).into();
    cb.write_bytes(&bytes);
}

/// RLDICL - rotate rs left by sh, clear the bits to the left of bit mb, put the result in ra.
/// Bit 0 is the most significant.
pub fn rldicl(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, sh: PPC64Opnd, mb: PPC64Opnd) {
    let bytes: [u8; 4] = MDForm::rldicl(reg_no(ra), reg_no(rs), uimm6(sh), uimm6(mb)).into();
    cb.write_bytes(&bytes);
}

/// RLDICR - rotate rs left by sh, clear the bits to the right of bit me, put the result in ra.
/// Bit 0 is the most significant.
pub fn rldicr(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, sh: PPC64Opnd, me: PPC64Opnd) {
    let bytes: [u8; 4] = MDForm::rldicr(reg_no(ra), reg_no(rs), uimm6(sh), uimm6(me)).into();
    cb.write_bytes(&bytes);
}

/// SLDI - shift rs left by an immediate, put the result in ra
pub fn sldi(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, sh: PPC64Opnd) {
    let sh = uimm6(sh);
    let bytes: [u8; 4] = MDForm::rldicr(reg_no(ra), reg_no(rs), sh, 63 - sh).into();
    cb.write_bytes(&bytes);
}

/// SRDI - shift rs right by an immediate, put the result in ra
pub fn srdi(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd, sh: PPC64Opnd) {
    let sh = uimm6(sh);
    let bytes: [u8; 4] = MDForm::rldicl(reg_no(ra), reg_no(rs), (64 - sh) % 64, sh).into();
    cb.write_bytes(&bytes);
}

/// EXTSB - sign-extend the low byte of rs, put the result in ra
pub fn extsb(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::extsb(reg_no(ra), reg_no(rs)).into();
    cb.write_bytes(&bytes);
}

/// EXTSH - sign-extend the low halfword of rs, put the result in ra
pub fn extsh(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::extsh(reg_no(ra), reg_no(rs)).into();
    cb.write_bytes(&bytes);
}

/// EXTSW - sign-extend the low word of rs, put the result in ra
pub fn extsw(cb: &mut CodeBlock, ra: PPC64Opnd, rs: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::extsw(reg_no(ra), reg_no(rs)).into();
    cb.write_bytes(&bytes);
}

/// Whether a signed value that is a multiple of 4 fits within the specified number of bits.
const fn offset_fits_bits(offset: i64, num_bits: u32) -> bool {
    let limit = 1 << (num_bits - 1);
    offset % 4 == 0 && -limit <= offset && offset < limit
}

/// Whether b and bl can reach a target that is this many bytes away
pub const fn b_offset_fits(offset: i64) -> bool {
    offset_fits_bits(offset, IForm::OFFSET_BITS)
}

/// Whether bc can reach a target that is this many bytes away
pub const fn bc_offset_fits(offset: i64) -> bool {
    offset_fits_bits(offset, BForm::OFFSET_BITS)
}

/// B - branch to the instruction that is offset bytes away from this one
pub fn b(cb: &mut CodeBlock, offset: i64) {
    assert!(b_offset_fits(offset), "The offset must be a multiple of 4 that fits in 26 bits, got {offset}");
    let bytes: [u8; 4] = IForm::b(offset as i32).into();
    cb.write_bytes(&bytes);
}

/// BL - branch to the instruction that is offset bytes away from this one, put the return address in LR
pub fn bl(cb: &mut CodeBlock, offset: i64) {
    assert!(b_offset_fits(offset), "The offset must be a multiple of 4 that fits in 26 bits, got {offset}");
    let bytes: [u8; 4] = IForm::bl(offset as i32).into();
    cb.write_bytes(&bytes);
}

/// BC - branch to the instruction that is offset bytes away from this one if CR0 meets the condition
pub fn bc(cb: &mut CodeBlock, cond: Condition, offset: i64) {
    assert!(bc_offset_fits(offset), "The offset must be a multiple of 4 that fits in 16 bits, got {offset}");
    let bytes: [u8; 4] = BForm::bc(cond.bo, cond.bi, offset as i16).into();
    cb.write_bytes(&bytes);
}

/// BLR - branch to the address in LR
pub fn blr(cb: &mut CodeBlock) {
    let bytes: [u8; 4] = XLForm::blr().into();
    cb.write_bytes(&bytes);
}

/// BCTR - branch to the address in CTR
pub fn bctr(cb: &mut CodeBlock) {
    let bytes: [u8; 4] = XLForm::bctr().into();
    cb.write_bytes(&bytes);
}

/// BCTRL - branch to the address in CTR, put the return address in LR
pub fn bctrl(cb: &mut CodeBlock) {
    let bytes: [u8; 4] = XLForm::bctrl().into();
    cb.write_bytes(&bytes);
}

/// MFLR - copy LR to rt
pub fn mflr(cb: &mut CodeBlock, rt: PPC64Opnd) {
    let bytes: [u8; 4] = XFXForm::mfspr(reg_no(rt), XFXForm::LR).into();
    cb.write_bytes(&bytes);
}

/// MTLR - copy rs to LR
pub fn mtlr(cb: &mut CodeBlock, rs: PPC64Opnd) {
    let bytes: [u8; 4] = XFXForm::mtspr(XFXForm::LR, reg_no(rs)).into();
    cb.write_bytes(&bytes);
}

/// MTCTR - copy rs to CTR
pub fn mtctr(cb: &mut CodeBlock, rs: PPC64Opnd) {
    let bytes: [u8; 4] = XFXForm::mtspr(XFXForm::CTR, reg_no(rs)).into();
    cb.write_bytes(&bytes);
}

/// MTXER - copy rs to XER
pub fn mtxer(cb: &mut CodeBlock, rs: PPC64Opnd) {
    let bytes: [u8; 4] = XFXForm::mtspr(XFXForm::XER, reg_no(rs)).into();
    cb.write_bytes(&bytes);
}

/// MCRXRX - copy OV of XER to LT of CR0, which is where it is in XER now, unlike SO.
/// GT, EQ and SO of CR0 get OV32, CA and CA32. Needs ISA 3.0.
pub fn mcrxrx(cb: &mut CodeBlock) {
    let bytes: [u8; 4] = XForm::mcrxrx(0).into();
    cb.write_bytes(&bytes);
}

/// ISEL - put truthy in rt if CR0 meets the condition, and falsy otherwise
pub fn isel(cb: &mut CodeBlock, rt: PPC64Opnd, truthy: PPC64Opnd, falsy: PPC64Opnd, cond: Condition) {
    // The instruction only picks its first operand when a CR bit is set
    let (ra, rb) = if cond.expects_set() { (truthy, falsy) } else { (falsy, truthy) };
    assert!(ra != R0, "isel reads r0 as zero in its first operand");
    let bytes: [u8; 4] = AForm::isel(reg_no(rt), reg_no(ra), reg_no(rb), cond.bi).into();
    cb.write_bytes(&bytes);
}

/// LDARX - load a doubleword from memory at ra + rb into rt, and reserve the address
pub fn ldarx(cb: &mut CodeBlock, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::ldarx(reg_no(rt), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// STDCX. - store rs into memory at ra + rb if the address is still reserved, set EQ of CR0 on success
pub fn stdcx_dot(cb: &mut CodeBlock, rs: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
    let bytes: [u8; 4] = XForm::stdcx_dot(reg_no(rs), base_reg_no(ra), reg_no(rb)).into();
    cb.write_bytes(&bytes);
}

/// TRAP - raise a trap exception, which the kernel delivers as SIGTRAP
pub fn trap(cb: &mut CodeBlock) {
    let bytes: [u8; 4] = XForm::trap().into();
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
    fn test_addo() {
        let cb = compile(|cb| addo(cb, R3, R4, R5));
        // Capstone doesn't decode the overflow-recording forms
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: .byte 0x14, 0x2e, 0x64, 0x7c");
        assert_snapshot!(cb.hexdump(), @"142e647c");
    }

    #[test]
    fn test_subfo() {
        let cb = compile(|cb| subfo(cb, R3, R4, R5));
        // Capstone doesn't decode the overflow-recording forms
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: .byte 0x50, 0x2c, 0x64, 0x7c");
        assert_snapshot!(cb.hexdump(), @"502c647c");
    }

    #[test]
    fn test_mulldo() {
        let cb = compile(|cb| mulldo(cb, R3, R4, R5));
        // Capstone doesn't decode the overflow-recording forms
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: .byte 0xd2, 0x2d, 0x64, 0x7c");
        assert_snapshot!(cb.hexdump(), @"d22d647c");
    }

    #[test]
    fn test_addo_dot() {
        let cb = compile(|cb| addo_dot(cb, R3, R4, R5));
        // Capstone doesn't decode the overflow-recording forms
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: .byte 0x15, 0x2e, 0x64, 0x7c");
        assert_snapshot!(cb.hexdump(), @"152e647c");
    }

    #[test]
    fn test_subfo_dot() {
        let cb = compile(|cb| subfo_dot(cb, R3, R4, R5));
        // Capstone doesn't decode the overflow-recording forms
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: .byte 0x51, 0x2c, 0x64, 0x7c");
        assert_snapshot!(cb.hexdump(), @"512c647c");
    }

    #[test]
    fn test_mulldo_dot() {
        let cb = compile(|cb| mulldo_dot(cb, R3, R4, R5));
        // Capstone doesn't decode the overflow-recording forms
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: .byte 0xd3, 0x2d, 0x64, 0x7c");
        assert_snapshot!(cb.hexdump(), @"d32d647c");
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

    #[test]
    fn test_sld() {
        let cb = compile(|cb| sld(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: sld r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"3628837c");
    }

    #[test]
    fn test_srd() {
        let cb = compile(|cb| srd(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: srd r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"362c837c");
    }

    #[test]
    fn test_srad() {
        let cb = compile(|cb| srad(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: srad r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"342e837c");
    }

    #[test]
    fn test_sradi() {
        let cb = compile(|cb| sradi(cb, R3, R4, PPC64Opnd::new_uimm(1)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: sradi r3, r4, 1");
        assert_snapshot!(cb.hexdump(), @"740e837c");
    }

    #[test]
    fn test_sradi_63() {
        let cb = compile(|cb| sradi(cb, R3, R4, PPC64Opnd::new_uimm(63)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: sradi r3, r4, 0x3f");
        assert_snapshot!(cb.hexdump(), @"76fe837c");
    }

    #[test]
    fn test_rldicl() {
        let cb = compile(|cb| rldicl(cb, R3, R4, PPC64Opnd::new_uimm(0), PPC64Opnd::new_uimm(32)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: clrldi r3, r4, 0x20");
        assert_snapshot!(cb.hexdump(), @"20008378");
    }

    #[test]
    fn test_rldicr() {
        let cb = compile(|cb| rldicr(cb, R3, R4, PPC64Opnd::new_uimm(32), PPC64Opnd::new_uimm(31)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: sldi r3, r4, 0x20");
        assert_snapshot!(cb.hexdump(), @"c6078378");
    }

    #[test]
    fn test_sldi() {
        let cb = compile(|cb| sldi(cb, R3, R4, PPC64Opnd::new_uimm(1)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: sldi r3, r4, 1");
        assert_snapshot!(cb.hexdump(), @"a40f8378");
    }

    #[test]
    fn test_sldi_63() {
        let cb = compile(|cb| sldi(cb, R3, R4, PPC64Opnd::new_uimm(63)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: sldi r3, r4, 0x3f");
        assert_snapshot!(cb.hexdump(), @"06f88378");
    }

    #[test]
    fn test_srdi() {
        let cb = compile(|cb| srdi(cb, R3, R4, PPC64Opnd::new_uimm(1)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: rldicl r3, r4, 0x3f, 1");
        assert_snapshot!(cb.hexdump(), @"42f88378");
    }

    #[test]
    fn test_srdi_63() {
        let cb = compile(|cb| srdi(cb, R3, R4, PPC64Opnd::new_uimm(63)));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: rldicl r3, r4, 1, 0x3f");
        assert_snapshot!(cb.hexdump(), @"e00f8378");
    }

    #[test]
    #[should_panic(expected = "Expected unsigned 6-bit immediate")]
    fn test_sldi_64() {
        compile(|cb| sldi(cb, R3, R4, PPC64Opnd::new_uimm(64)));
    }

    #[test]
    fn test_extsb() {
        let cb = compile(|cb| extsb(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: extsb r3, r4");
        assert_snapshot!(cb.hexdump(), @"7407837c");
    }

    #[test]
    fn test_extsh() {
        let cb = compile(|cb| extsh(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: extsh r3, r4");
        assert_snapshot!(cb.hexdump(), @"3407837c");
    }

    #[test]
    fn test_extsw() {
        let cb = compile(|cb| extsw(cb, R3, R4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: extsw r3, r4");
        assert_snapshot!(cb.hexdump(), @"b407837c");
    }

    #[test]
    fn test_b_offset_fits() {
        assert!(b_offset_fits(0x1fffffc));
        assert!(b_offset_fits(-0x2000000));
        assert!(!b_offset_fits(0x2000000));
        assert!(!b_offset_fits(-0x2000004));
        assert!(!b_offset_fits(2));
    }

    #[test]
    fn test_bc_offset_fits() {
        assert!(bc_offset_fits(0x7ffc));
        assert!(bc_offset_fits(-0x8000));
        assert!(!bc_offset_fits(0x8000));
        assert!(!bc_offset_fits(-0x8004));
        assert!(!bc_offset_fits(2));
    }

    #[test]
    fn test_b() {
        let cb = compile(|cb| b(cb, 256));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: b 0x100");
        assert_snapshot!(cb.hexdump(), @"00010048");
    }

    #[test]
    fn test_b_backward() {
        let cb = compile(|cb| b(cb, -4));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: b 0xfffffffffffffffc");
        assert_snapshot!(cb.hexdump(), @"fcffff4b");
    }

    #[test]
    fn test_bl() {
        let cb = compile(|cb| bl(cb, 8));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: bl 8");
        assert_snapshot!(cb.hexdump(), @"09000048");
    }

    #[test]
    fn test_bc_eq() {
        let cb = compile(|cb| bc(cb, Condition::EQ, 8));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: beq 8");
        assert_snapshot!(cb.hexdump(), @"08008241");
    }

    #[test]
    fn test_bc_ne_backward() {
        let cb = compile(|cb| bc(cb, Condition::NE, -8));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: bne 0xfffffffffffffff8");
        assert_snapshot!(cb.hexdump(), @"f8ff8240");
    }

    #[test]
    fn test_bc_lt() {
        let cb = compile(|cb| bc(cb, Condition::LT, 32764));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: blt 0x7ffc");
        assert_snapshot!(cb.hexdump(), @"fc7f8041");
    }

    #[test]
    fn test_bc_ge() {
        let cb = compile(|cb| bc(cb, Condition::GE, -32768));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: bge 0xffffffffffff8000");
        assert_snapshot!(cb.hexdump(), @"00808040");
    }

    #[test]
    fn test_bc_gt() {
        let cb = compile(|cb| bc(cb, Condition::GT, 8));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: bgt 8");
        assert_snapshot!(cb.hexdump(), @"08008141");
    }

    #[test]
    fn test_bc_le() {
        let cb = compile(|cb| bc(cb, Condition::LE, 8));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: ble 8");
        assert_snapshot!(cb.hexdump(), @"08008140");
    }

    #[test]
    fn test_bc_so() {
        let cb = compile(|cb| bc(cb, Condition::SO, 8));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: bso 8");
        assert_snapshot!(cb.hexdump(), @"08008341");
    }

    #[test]
    fn test_bc_ns() {
        let cb = compile(|cb| bc(cb, Condition::NS, 8));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: bns 8");
        assert_snapshot!(cb.hexdump(), @"08008340");
    }

    #[test]
    fn test_blr() {
        let cb = compile(blr);
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: blr");
        assert_snapshot!(cb.hexdump(), @"2000804e");
    }

    #[test]
    fn test_bctr() {
        let cb = compile(bctr);
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: bctr");
        assert_snapshot!(cb.hexdump(), @"2004804e");
    }

    #[test]
    fn test_bctrl() {
        let cb = compile(bctrl);
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: bctrl");
        assert_snapshot!(cb.hexdump(), @"2104804e");
    }

    #[test]
    #[should_panic(expected = "The offset must be a multiple of 4 that fits in 26 bits")]
    fn test_b_too_far() {
        compile(|cb| b(cb, 0x2000000));
    }

    #[test]
    #[should_panic(expected = "The offset must be a multiple of 4 that fits in 16 bits")]
    fn test_bc_too_far() {
        compile(|cb| bc(cb, Condition::EQ, 0x8000));
    }

    #[test]
    fn test_mflr() {
        let cb = compile(|cb| mflr(cb, R0));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: mflr r0");
        assert_snapshot!(cb.hexdump(), @"a602087c");
    }

    #[test]
    fn test_mtlr() {
        let cb = compile(|cb| mtlr(cb, R0));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: mtlr r0");
        assert_snapshot!(cb.hexdump(), @"a603087c");
    }

    #[test]
    fn test_mtctr() {
        let cb = compile(|cb| mtctr(cb, R12));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: mtctr r12");
        assert_snapshot!(cb.hexdump(), @"a603897d");
    }

    #[test]
    fn test_mtxer() {
        let cb = compile(|cb| mtxer(cb, R0));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: mtxer r0");
        assert_snapshot!(cb.hexdump(), @"a603017c");
    }

    #[test]
    fn test_mcrxrx() {
        let cb = compile(mcrxrx);
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: mcrxrx cr0");
        assert_snapshot!(cb.hexdump(), @"8004007c");
    }

    #[test]
    fn test_prefixed_imm_fits() {
        assert!(prefixed_imm_fits((1 << 33) - 1));
        assert!(!prefixed_imm_fits(1 << 33));
        assert!(prefixed_imm_fits(-(1 << 33)));
        assert!(!prefixed_imm_fits(-(1 << 33) - 1));
    }

    #[test]
    fn test_align_prefixed() {
        let cb = compile(|cb| {
            // Nothing to do until the last word of a 64-byte block
            for _ in 0..15 {
                align_prefixed(cb);
                nop(cb);
            }
            assert_eq!(60, cb.get_write_pos());
            align_prefixed(cb);
            assert_eq!(64, cb.get_write_pos());
            pli(cb, R3, 0);
        });
        assert_eq!(72, cb.get_write_pos());
    }

    #[test]
    fn test_pli() {
        let cb = compile(|cb| pli(cb, R3, 0x106a20340));
        // Capstone doesn't decode prefixed instructions
        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: .byte 0xa2, 0x06, 0x01, 0x06
        0x4: li r3, 0x340
        ");
        assert_snapshot!(cb.hexdump(), @"a206010640036038");
    }

    #[test]
    fn test_pla() {
        let cb = compile(|cb| pla(cb, R12, -0x23456));
        // Capstone doesn't decode prefixed instructions
        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: .byte 0xfd, 0xff, 0x13, 0x06
        0x4: li r12, -0x3456
        ");
        assert_snapshot!(cb.hexdump(), @"fdff1306aacb8039");
    }

    #[test]
    fn test_pld_pc_relative() {
        let cb = compile(|cb| pld_pc_relative(cb, R5, 0x12345));
        // Capstone doesn't decode prefixed instructions
        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: .byte 0x01, 0x00, 0x10, 0x04
        0x4: .byte 0x45, 0x23, 0xa0, 0xe4
        ");
        assert_snapshot!(cb.hexdump(), @"010010044523a0e4");
    }

    #[test]
    fn test_isel_eq() {
        let cb = compile(|cb| isel(cb, R3, R4, R5, Condition::EQ));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: isel r3, r4, r5, cr0eq");
        assert_snapshot!(cb.hexdump(), @"9e28647c");
    }

    #[test]
    fn test_isel_ne() {
        let cb = compile(|cb| isel(cb, R3, R4, R5, Condition::NE));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: isel r3, r5, r4, cr0eq");
        assert_snapshot!(cb.hexdump(), @"9e20657c");
    }

    #[test]
    fn test_isel_lt() {
        let cb = compile(|cb| isel(cb, R3, R4, R5, Condition::LT));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: isel r3, r4, r5, cr0lt");
        assert_snapshot!(cb.hexdump(), @"1e28647c");
    }

    #[test]
    fn test_isel_le() {
        let cb = compile(|cb| isel(cb, R3, R4, R5, Condition::LE));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: isel r3, r5, r4, cr0gt");
        assert_snapshot!(cb.hexdump(), @"5e20657c");
    }

    #[test]
    fn test_isel_r0_falsy() {
        let cb = compile(|cb| isel(cb, R3, R4, R0, Condition::EQ));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: isel r3, r4, r0, cr0eq");
        assert_snapshot!(cb.hexdump(), @"9e00647c");
    }

    #[test]
    #[should_panic(expected = "isel reads r0 as zero in its first operand")]
    fn test_isel_r0_truthy() {
        compile(|cb| isel(cb, R3, R0, R5, Condition::EQ));
    }

    #[test]
    #[should_panic(expected = "isel reads r0 as zero in its first operand")]
    fn test_isel_r0_falsy_inverted() {
        compile(|cb| isel(cb, R3, R4, R0, Condition::NE));
    }

    #[test]
    fn test_ldarx() {
        let cb = compile(|cb| ldarx(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: ldarx r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"a828647c");
    }

    #[test]
    fn test_stdcx_dot() {
        let cb = compile(|cb| stdcx_dot(cb, R3, R4, R5));
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: stdcx. r3, r4, r5");
        assert_snapshot!(cb.hexdump(), @"ad29647c");
    }

    #[test]
    fn test_trap() {
        let cb = compile(trap);
        assert_disasm_snapshot!(cb.disasm(), @"  0x0: trap");
        assert_snapshot!(cb.hexdump(), @"0800e07f");
    }
}
