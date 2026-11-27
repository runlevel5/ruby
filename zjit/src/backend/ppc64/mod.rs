use crate::asm::{CodeBlock, Label};
use crate::asm::ppc64::*;
use crate::codegen::split_patch_point;
use crate::cruby::*;
use crate::backend::lir::*;
use crate::options::asm_dump;
use crate::stats::{CompileError, trace_compile_phase};
use crate::virtualmem::CodePtr;

// Use the PPC64 register type for this platform
pub type Reg = PPC64Reg;

/// Convert reg_no for MemBase::Reg into Reg, assuming it's a 64-bit register
pub fn mem_base_reg(reg_no: u8) -> Reg {
    Reg { num_bits: 64, reg_no }
}

// Callee-saved registers
pub const CFP: Opnd = Opnd::Reg(R28_REG);
pub const EC: Opnd = Opnd::Reg(R29_REG);
pub const SP: Opnd = Opnd::Reg(R30_REG);

// C argument registers on this platform
pub const C_ARG_OPNDS: [Opnd; 8] = [
    Opnd::Reg(R3_REG),
    Opnd::Reg(R4_REG),
    Opnd::Reg(R5_REG),
    Opnd::Reg(R6_REG),
    Opnd::Reg(R7_REG),
    Opnd::Reg(R8_REG),
    Opnd::Reg(R9_REG),
    Opnd::Reg(R10_REG)
];
const _: () = ::core::assert!(C_ARG_OPNDS.len() == C_ARG_REGS.len());
const _: () = ::core::assert!(C_ARG_OPNDS.len() == C_ARG_REGREGS.len());

// C return value register on this platform
pub const C_RET_REG: Reg = R3_REG;
pub const C_RET_OPND: Opnd = Opnd::Reg(R3_REG);
pub const NATIVE_STACK_PTR: Opnd = Opnd::Reg(R1_REG);
pub const NATIVE_BASE_PTR: Opnd = Opnd::Reg(R31_REG);

/// A version of the Power ISA, oldest first
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PowerIsa {
    /// What POWER8 implements, the oldest CPU this backend generates code for
    V2_07,
    /// What POWER9 implements
    V3_0,
    /// What POWER10 implements
    V3_1,
}

/// The newest version of the Power ISA that the CPU we are running on implements
#[cfg(not(test))]
fn power_isa() -> PowerIsa {
    use std::ffi::c_ulong;
    use std::sync::OnceLock;

    unsafe extern "C" {
        fn getauxval(type_: c_ulong) -> c_ulong;
    }
    const AT_HWCAP2: c_ulong = 26;
    const PPC_FEATURE2_ARCH_3_00: c_ulong = 0x0080_0000;
    const PPC_FEATURE2_ARCH_3_1: c_ulong = 0x0004_0000;

    static ISA: OnceLock<PowerIsa> = OnceLock::new();
    *ISA.get_or_init(|| {
        let hwcap2 = unsafe { getauxval(AT_HWCAP2) };
        if hwcap2 & PPC_FEATURE2_ARCH_3_1 != 0 {
            PowerIsa::V3_1
        } else if hwcap2 & PPC_FEATURE2_ARCH_3_00 != 0 {
            PowerIsa::V3_0
        } else {
            PowerIsa::V2_07
        }
    })
}

#[cfg(test)]
thread_local! {
    /// Tests generate the same code on every CPU. One that is about a newer ISA sets this.
    static TEST_POWER_ISA: std::cell::Cell<PowerIsa> = const { std::cell::Cell::new(PowerIsa::V2_07) };
}

#[cfg(test)]
fn power_isa() -> PowerIsa {
    TEST_POWER_ISA.get()
}

/// How far a push or a pop moves the native SP, which always needs to be
/// aligned to a 16-byte boundary
const C_SP_STEP: i32 = 16;

/// Size of the area every caller keeps at the bottom of its frame in the ELFv2 ABI:
/// back chain, CR save, LR save and TOC save doublewords
const LINKAGE_AREA_SIZE: i32 = 32;

/// Offset of the LR save doubleword in the linkage area. A callee saves its return address
/// in the one of its caller.
const LR_SAVE_OFFSET: i32 = 16;

/// Size of the header at the bottom of a frame. Its first doubleword is the back chain,
/// the address of the caller's frame, which stack walkers such as backtrace(3) and
/// debuggers follow. The second one is padding for alignment.
const FRAME_HEADER_SIZE: i32 = 16;

/// Size of the area a caller reserves above the linkage area for a callee to
/// save the eight argument registers. Stack-passed arguments come after it.
const PARAM_SAVE_AREA_SIZE: i32 = 64;

/// Offset from the native SP at function entry to the first stack-passed C argument
pub const C_STACK_ARGS_ENTRY_OFFSET: i32 = LINKAGE_AREA_SIZE + PARAM_SAVE_AREA_SIZE;

/// How far a call moves the frame header down for it to become the linkage area of the
/// callee, with what was above the header right after the parameter save area
const CALL_AREA_SIZE: i32 = C_STACK_ARGS_ENTRY_OFFSET - FRAME_HEADER_SIZE;

/// Whether a lone [`Insn::CPush`] leaves the native SP misaligned for C calls
pub const CPUSH_MISALIGNS_STACK: bool = false;

/// Filler for new code pages. All zeros is an illegal instruction, so it's already what we want.
pub const NEW_PAGE_FILL: Option<u8> = None;

impl CodeBlock {
    /// The number of bytes that a patch point keeps for the jump that invalidation writes
    /// over it. The jump goes to a side exit of the function that the patch point is in,
    /// so it is a single b. [`Assembler::ppc64_emit`] gives up on a function that is too
    /// large for one to reach across it.
    pub fn jmp_ptr_bytes(&self) -> usize {
        4
    }

    /// Whether a b or a bl reaches any address of the code region from any other.
    /// Its offset is a signed 26-bit number of bytes. It can jump to +/-32MiB,
    /// so this is the case when --zjit-exec-mem-size <= 32.
    fn b_reaches_code_region(&self) -> bool {
        // The widest in-region branch spans the region minus one instruction.
        b_offset_fits(self.virtual_region_size() as i64 - 4)
    }
}

/// Map Opnd to PPC64Opnd
impl From<Opnd> for PPC64Opnd {
    fn from(opnd: Opnd) -> Self {
        match opnd {
            Opnd::UImm(value) => PPC64Opnd::new_uimm(value),
            Opnd::Imm(value) => PPC64Opnd::new_imm(value),
            Opnd::Reg(reg) => PPC64Opnd::Reg(reg),
            Opnd::Mem(Mem { base: MemBase::Reg(reg_no), num_bits, disp }) => {
                PPC64Opnd::new_mem(num_bits, PPC64Opnd::Reg(mem_base_reg(reg_no)), disp)
            },
            Opnd::Mem(Mem { base: MemBase::VReg(_), .. }) => {
                panic!("attempted to lower an Opnd::Mem with a MemBase::VReg base")
            },
            Opnd::Mem(Mem { base: MemBase::Stack { .. } | MemBase::StackIndirect { .. }, .. }) => {
                panic!("attempted to lower an Opnd::Mem with a MemBase::Stack/StackIndirect base")
            },
            Opnd::VReg { .. } => panic!("attempted to lower an Opnd::VReg"),
            Opnd::Value(_) => panic!("attempted to lower an Opnd::Value"),
            Opnd::None => panic!(
                "Attempted to lower an Opnd::None. This often happens when an out operand was not allocated for an instruction because the output of the instruction was not used. Please ensure you are using the output."
            ),
        }
    }
}

/// Also implement going from a reference to an operand for convenience.
impl From<&Opnd> for PPC64Opnd {
    fn from(opnd: &Opnd) -> Self {
        PPC64Opnd::from(*opnd)
    }
}

fn emit_jmp_ptr(cb: &mut CodeBlock, dst_ptr: CodePtr) {
    let offset = dst_ptr.as_offset() - cb.get_write_ptr().as_offset();

    // If the offset is short enough, then we'll use the
    // branch instruction. Otherwise, we'll move the
    // destination into CTR and branch to that.
    if b_offset_fits(offset) {
        b(cb, offset);
    } else {
        emit_load_code_ptr(cb, Assembler::EMIT_OPND, dst_ptr);
        mtctr(cb, Assembler::EMIT_OPND);
        bctr(cb);
    }
}

/// Emit the instructions to put the address of some place in the code region into the
/// given register. Returns how many words they take, like [`emit_load_value`].
fn emit_load_code_ptr(cb: &mut CodeBlock, rd: PPC64Opnd, dst_ptr: CodePtr) -> usize {
    if power_isa() >= PowerIsa::V3_1 {
        // The offset of pla reaches +/-8GiB, which is more than the code region can be
        let start_pos = cb.get_write_pos();
        align_prefixed(cb);
        pla(cb, rd, dst_ptr.as_offset() - cb.get_write_ptr().as_offset());
        (cb.get_write_pos() - start_pos) / 4
    } else {
        emit_load_value(cb, rd, dst_ptr.raw_addr(cb) as u64)
    }
}

/// Emit the required instructions to load the given value into the
/// given register. Our goal here is to use as few instructions as
/// possible to get this value into the register.
/// Returns how many words they take.
fn emit_load_value(cb: &mut CodeBlock, rd: PPC64Opnd, value: u64) -> usize {
    let imm = value as i64;

    if let Ok(imm) = i16::try_from(imm) {
        // If the value fits into the sign-extended
        // immediate of li, then we'll use that.
        li(cb, rd, PPC64Opnd::new_imm(imm.into()));
        1
    } else if let Ok(imm) = i32::try_from(imm) {
        // lis sign-extends too, and takes the upper 16 bits
        // of a 32-bit value. ori fills in the lower 16 bits.
        lis(cb, rd, PPC64Opnd::new_imm((imm >> 16).into()));
        let lower = imm as u16;
        if lower == 0 {
            1
        } else {
            ori(cb, rd, rd, PPC64Opnd::new_uimm(lower.into()));
            2
        }
    } else if power_isa() >= PowerIsa::V3_1 && prefixed_imm_fits(imm) {
        // ISA 3.1 has an instruction for an immediate of 34 bits,
        // which takes at least 3 instructions below.
        let start_pos = cb.get_write_pos();
        align_prefixed(cb);
        pli(cb, rd, imm);
        (cb.get_write_pos() - start_pos) / 4
    } else {
        // Finally we'll fall back to loading the upper 32 bits
        // like above, shifting them into place, and filling in
        // the lower 32 bits 16 at a time as long as they are necessary.
        let mut num_insns = match imm >> 32 {
            // (0 only gets here for a value with bit 31 set, which lis would sign-extend.)
            0 => {
                li(cb, rd, PPC64Opnd::new_imm(0));
                1
            }
            upper => {
                let num_insns = emit_load_value(cb, rd, upper as u64);
                sldi(cb, rd, rd, PPC64Opnd::new_uimm(32));
                num_insns + 1
            }
        };

        let upper = (value >> 16) as u16;
        if upper != 0 {
            oris(cb, rd, rd, PPC64Opnd::new_uimm(upper.into()));
            num_insns += 1;
        }

        let lower = value as u16;
        if lower != 0 {
            ori(cb, rd, rd, PPC64Opnd::new_uimm(lower.into()));
            num_insns += 1;
        }
        num_insns
    }
}

/// The value of an immediate operand, if it is one
fn imm_opnd_value(opnd: &Opnd) -> Option<i64> {
    match *opnd {
        Opnd::Imm(imm) => Some(imm),
        Opnd::UImm(uimm) => Some(uimm as i64),
        _ => None,
    }
}

/// Whether a load or a store can encode the displacement of a memory operand
fn mem_disp_fits(num_bits: u8, disp: i32) -> bool {
    // ld and std, which move 64 bits, only take a multiple of 4
    i16::try_from(disp).is_ok() && (num_bits != 64 || disp % 4 == 0)
}

/// Split a displacement into the immediates of an addis and an addi that add it to a
/// register. None if the displacement is so large that addis can't encode its half.
fn split_disp(disp: i64) -> Option<(i16, i16)> {
    // addi sign-extends its half, so the half that addis takes makes up for a negative one
    let lower = disp as i16;
    let upper = i16::try_from((disp - i64::from(lower)) >> 16).ok()?;
    Some((upper, lower))
}

/// Whether a Jo or a JoMul reads the outcome of the arithmetic instruction at insn_idx.
/// Only instructions that move the result around come in between the two.
fn overflow_checked(insns: &[Insn], insn_idx: usize) -> bool {
    insns[insn_idx + 1..].iter()
        .find(|insn| !matches!(insn, Insn::Store { .. } | Insn::Mov { .. } | Insn::Load { .. } | Insn::LoadInto { .. } | Insn::Comment(_) | Insn::PosMarker(_)))
        .is_some_and(|insn| matches!(insn, Insn::Jo(_) | Insn::JoMul(_)))
}

/// Whether the instruction that reads the outcome of the Cmp at insn_idx is for unsigned
/// integers. The two kinds of comparison set the same CR bits, so unlike on the other
/// platforms, it's the Cmp that has to know which one it is.
fn compared_unsigned(insns: &[Insn], insn_idx: usize) -> bool {
    for insn in &insns[insn_idx + 1..] {
        match insn {
            Insn::Jb(_) | Insn::Jbe(_) => return true,
            Insn::Je(_) | Insn::Jne(_) | Insn::Jz(_) | Insn::Jnz(_) |
            Insn::Jl(_) | Insn::Jg(_) | Insn::Jge(_) |
            Insn::CSelZ { .. } | Insn::CSelNZ { .. } | Insn::CSelE { .. } | Insn::CSelNE { .. } |
            Insn::CSelL { .. } | Insn::CSelLE { .. } | Insn::CSelG { .. } | Insn::CSelGE { .. } |
            Insn::Cmp { .. } | Insn::Test { .. } => return false,
            _ => {}
        }
    }
    false
}

/// List of registers that can be used for register allocation.
/// SCRATCH0_OPND, SCRATCH1_OPND, and EMIT_OPND are excluded.
pub const ALLOC_REGS: &[Reg] = &[
    R3_REG,
    R4_REG,
    R5_REG,
    R6_REG,
    R7_REG,
    R8_REG,
    R9_REG,
    R10_REG,
];

/// Special scratch registers for intermediate processing. They should be used only by
/// [`Assembler::ppc64_scratch_split`] or [`Assembler::new_with_scratch_reg`].
/// They are the two volatile registers that aren't for arguments, other than r0.
/// A call puts the address of the callee in r12, so nothing in them outlives a call.
const SCRATCH0_OPND: Opnd = Opnd::Reg(R11_REG);
const SCRATCH1_OPND: Opnd = Opnd::Reg(R12_REG);

/// A scratch register available for use by resolve_ssa to break register copy cycles.
/// Must not overlap with ALLOC_REGS or other preserved registers.
pub const SCRATCH_REG: Reg = R11_REG;

impl Assembler {
    const MAX_FRAME_STACK_SLOTS: usize = 2048;

    /// Special register for intermediate processing in ppc64_emit. It should be used only by ppc64_emit.
    /// r0 reads as zero where an instruction takes the base of an address, which rules it out for
    /// operands. That leaves loading immediates, which is what ppc64_emit needs a register for.
    const EMIT_REG: Reg = R0_REG;
    const EMIT_OPND: PPC64Opnd = PPC64Opnd::Reg(Self::EMIT_REG);

    /// Return an Assembler with scratch registers disabled in the backend, and a scratch register.
    pub fn new_with_scratch_reg() -> (Self, Opnd) {
        (Self::new_with_accept_scratch_reg(true), SCRATCH0_OPND)
    }

    /// Return true if opnd contains a scratch reg
    pub fn has_scratch_reg(opnd: Opnd) -> bool {
        Self::has_reg(opnd, SCRATCH0_OPND.unwrap_reg())
    }

    /// Get the list of registers from which we will allocate on this platform
    pub fn get_alloc_regs() -> Vec<Reg> {
        ALLOC_REGS.to_vec()
    }

    /// How many bytes a call and a [Self::frame_setup] would change native SP
    pub fn frame_size() -> i32 {
        // A call leaves room for the linkage and parameter save areas of the callee,
        // and frame_setup puts the base pointer right below them
        C_STACK_ARGS_ENTRY_OFFSET + C_SP_STEP
    }

    /// Split platform-specific instructions
    /// The transformations done here are meant to make our lives simpler in later
    /// stages of the compilation pipeline.
    /// Here we may want to make sure that all instructions (except load and store)
    /// have no memory operands.
    fn ppc64_split(mut self) -> Assembler
    {
        /// When you're storing a register into a memory location or loading a
        /// memory location into a register, the displacement from the base
        /// register of the memory location must fit into 16 bits. If it doesn't,
        /// then we need to load that memory address into a register first.
        fn split_memory_address(asm: &mut Assembler, opnd: Opnd) -> Opnd {
            match opnd {
                Opnd::Mem(mem) => {
                    if mem_disp_fits(mem.num_bits, mem.disp) {
                        opnd
                    } else if asm.accept_scratch_reg {
                        asm.lea_into(SCRATCH1_OPND, Opnd::Mem(Mem { num_bits: 64, ..mem }));
                        Opnd::mem(mem.num_bits, SCRATCH1_OPND, 0)
                    } else {
                        let base = asm.lea(Opnd::Mem(Mem { num_bits: 64, ..mem }));
                        Opnd::mem(mem.num_bits, base, 0)
                    }
                },
                _ => unreachable!("Can only split memory addresses.")
            }
        }

        /// Any memory operands you're sending into an Op::Load instruction need
        /// to be split in case their displacement doesn't fit into 16 bits.
        fn split_load_operand(asm: &mut Assembler, opnd: Opnd) -> Opnd {
            match opnd {
                Opnd::Reg(_) | Opnd::VReg { .. } => opnd,
                Opnd::Mem(_) => {
                    let split_opnd = split_memory_address(asm, opnd);
                    let out_opnd = asm.load(split_opnd);
                    // Only cmp tells 32-bit operands from 64-bit ones. asm.load with fewer
                    // bits zero-extends the value, so it's safe to recognize it as a 32-bit value.
                    if out_opnd.rm_num_bits() < 32 {
                        out_opnd.with_num_bits(32)
                    } else {
                        out_opnd
                    }
                },
                _ => asm.load(opnd)
            }
        }

        /// The second operand of an instruction can stay an immediate of any size.
        /// ppc64_emit loads one that the instruction can't encode into EMIT_OPND,
        /// so it only needs a register when it comes from memory.
        fn split_reg_or_imm(asm: &mut Assembler, opnd: Opnd) -> Opnd {
            match opnd {
                Opnd::Mem(_) => split_load_operand(asm, opnd),
                _ => opnd
            }
        }

        /// Returns the operands that should be used for an isel instruction.
        fn split_csel_operands(asm: &mut Assembler, opnd0: Opnd, opnd1: Opnd) -> (Opnd, Opnd) {
            let opnd0 = match opnd0 {
                Opnd::Reg(_) | Opnd::VReg { .. } => opnd0,
                _ => split_load_operand(asm, opnd0)
            };

            let opnd1 = match opnd1 {
                Opnd::Reg(_) | Opnd::VReg { .. } => opnd1,
                _ => split_load_operand(asm, opnd1)
            };

            (opnd0, opnd1)
        }

        fn split_less_than_32_cmp(asm: &mut Assembler, opnd0: Opnd) -> Opnd {
            match opnd0 {
                Opnd::Reg(_) | Opnd::VReg { .. } => {
                    match opnd0.rm_num_bits() {
                        8 => asm.and(opnd0.with_num_bits(64), Opnd::UImm(0xff)),
                        16 => asm.and(opnd0.with_num_bits(64), Opnd::UImm(0xffff)),
                        32 | 64 => opnd0,
                        bits => unreachable!("Invalid number of bits. {}", bits)
                    }
                }
                _ => opnd0
            }
        }

        let mut asm_local = Assembler::new_with_asm(&self);
        let mut iterator = self.instruction_iterator();
        let asm = &mut asm_local;

        while let Some((_index, mut insn)) = iterator.next(asm) {
            // Here we're going to map the operands of the instruction to load
            // any Opnd::Value operands into registers if they are heap objects
            // such that only the Op::Load instruction needs to handle that
            // case. If the values aren't heap objects then we'll treat them as
            // if they were just unsigned integer.
            let is_load = matches!(insn, Insn::Load { .. } | Insn::LoadInto { .. });
            let is_jump = insn.is_jump();

            insn.for_each_operand_mut(|opnd| {
                if let Opnd::Value(value) = opnd {
                    if value.special_const_p() {
                        *opnd = Opnd::UImm(value.as_u64());
                    } else if !is_load && !is_jump {
                        *opnd = asm.load(*opnd);
                    }
                };
            });

            // We are replacing instructions here so we know they are already
            // being used. It is okay not to use their output here.
            #[allow(unused_must_use)]
            match &mut insn {
                Insn::Add { left, right, .. } |
                Insn::And { left, right, .. } |
                Insn::Or { left, right, .. } |
                Insn::Xor { left, right, .. } => {
                    match (*left, *right) {
                        // When one operand is a register, legalize the other operand
                        // into possibly an immediate and swap the order if necessary.
                        // Only the rhs can be an immediate, but these are commutative.
                        (reg_opnd @ (Opnd::Reg(_) | Opnd::VReg { .. }), other_opnd) |
                        (other_opnd, reg_opnd @ (Opnd::Reg(_) | Opnd::VReg { .. })) => {
                            *left = reg_opnd;
                            *right = split_reg_or_imm(asm, other_opnd);
                        }
                        _ => {
                            *left = split_load_operand(asm, *left);
                            *right = split_reg_or_imm(asm, *right);
                        }
                    }
                    asm.push_insn(insn);
                }
                Insn::Sub { left, right, .. } => {
                    *left = split_load_operand(asm, *left);
                    *right = split_reg_or_imm(asm, *right);
                    asm.push_insn(insn);
                }
                Insn::Cmp { left, right } => {
                    let opnd0 = split_load_operand(asm, *left);
                    let opnd0 = split_less_than_32_cmp(asm, opnd0);
                    let split_right = split_reg_or_imm(asm, *right);
                    let opnd1 = match split_right {
                        Opnd::VReg { .. } if opnd0.num_bits() != split_right.num_bits() => {
                            split_right.with_num_bits(opnd0.num_bits().unwrap())
                        },
                        _ => split_right
                    };

                    asm.cmp(opnd0, opnd1);
                },
                Insn::CRet(opnd) => {
                    match opnd {
                        // If the value is already in the return register, then
                        // we don't need to do anything.
                        Opnd::Reg(C_RET_REG) => {},

                        // If the value is a memory address, we need to first
                        // make sure the displacement isn't too large and then
                        // load it into the return register.
                        Opnd::Mem(_) => {
                            let split = split_memory_address(asm, *opnd);
                            asm.load_into(C_RET_OPND, split);
                        },

                        // Otherwise we just need to load the value into the
                        // return register.
                        _ => {
                            asm.load_into(C_RET_OPND, *opnd);
                        }
                    }
                    asm.cret(C_RET_OPND);
                },
                Insn::CSelZ { truthy, falsy, .. } |
                Insn::CSelNZ { truthy, falsy, .. } |
                Insn::CSelE { truthy, falsy, .. } |
                Insn::CSelNE { truthy, falsy, .. } |
                Insn::CSelL { truthy, falsy, .. } |
                Insn::CSelLE { truthy, falsy, .. } |
                Insn::CSelG { truthy, falsy, .. } |
                Insn::CSelGE { truthy, falsy, .. } => {
                    let (opnd0, opnd1) = split_csel_operands(asm, *truthy, *falsy);
                    *truthy = opnd0;
                    *falsy = opnd1;
                    asm.push_insn(insn);
                },
                Insn::JmpOpnd(opnd) => {
                    if let Opnd::Mem(_) = opnd {
                        let opnd0 = split_load_operand(asm, *opnd);
                        asm.jmp_opnd(opnd0);
                    } else {
                        asm.jmp_opnd(*opnd);
                    }
                },
                Insn::Load { opnd, .. } |
                Insn::LoadInto { opnd, .. } => {
                    *opnd = match opnd {
                        Opnd::Mem(_) => split_memory_address(asm, *opnd),
                        _ => *opnd
                    };
                    asm.push_insn(insn);
                },
                Insn::LoadSExt { opnd, out } => {
                    match opnd {
                        // We only want to sign extend if the operand is a
                        // register, instruction output, or memory address that
                        // is 32 bits. Otherwise we'll just load the value
                        // directly since there's no need to sign extend.
                        Opnd::Reg(Reg { num_bits: 32, .. }) |
                        Opnd::VReg { num_bits: 32, .. } |
                        Opnd::Mem(Mem { num_bits: 32, .. }) => {
                            asm.push_insn(insn);
                        },
                        _ => {
                            asm.push_insn(Insn::Load { opnd: *opnd, out: *out });
                        }
                    };
                },
                Insn::Mov { dest, src } => {
                    match (&dest, &src) {
                        // If we're attempting to load into a memory operand, then
                        // we'll switch over to the store instruction.
                        (Opnd::Mem(_), _) => {
                            let opnd0 = split_memory_address(asm, *dest);
                            let value = split_reg_or_imm(asm, *src);
                            asm.store(opnd0, value);
                        },
                        // If we're loading a memory operand into a register, then
                        // we'll switch over to the load instruction.
                        (Opnd::Reg(_) | Opnd::VReg { .. }, Opnd::Mem(_)) => {
                            let value = split_memory_address(asm, *src);
                            asm.load_into(*dest, value);
                        },
                        // Otherwise we'll use the normal mov instruction.
                        (Opnd::Reg(_), _) => {
                            asm.mov(*dest, *src);
                        },
                        _ => unreachable!("unexpected combination of operands in Insn::Mov: {dest:?}, {src:?}")
                    };
                },
                Insn::Not { opnd, .. } => {
                    // The value that is being negated must be in a register, so
                    // if we get anything else we need to load it first.
                    *opnd = match opnd {
                        Opnd::Mem(_) => split_load_operand(asm, *opnd),
                        _ => *opnd
                    };
                    asm.push_insn(insn);
                },
                Insn::LShift { opnd, .. } |
                Insn::RShift { opnd, .. } |
                Insn::URShift { opnd, .. } |
                Insn::ByteSwap { opnd, .. } => {
                    // The operand must be in a register, so
                    // if we get anything else we need to load it first.
                    *opnd = split_load_operand(asm, *opnd);
                    asm.push_insn(insn);
                },
                Insn::Store { dest, .. } => {
                    if asm.accept_scratch_reg && matches!(dest, Opnd::Mem(_)) {
                        *dest = split_memory_address(asm, *dest);
                    }
                    asm.push_insn(insn);
                },
                Insn::Mul { left, right, .. } => {
                    *left = split_load_operand(asm, *left);
                    *right = split_load_operand(asm, *right);
                    asm.push_insn(insn);
                },
                Insn::Test { left, right } => {
                    // The value being tested must be in a register, so if it's
                    // not already one we'll load it first.
                    let opnd0 = split_load_operand(asm, *left);
                    let opnd1 = split_reg_or_imm(asm, *right);
                    asm.test(opnd0, opnd1);
                },
                _ => {
                    asm.push_insn(insn);
                }
            }
        }

        asm_local
    }

    /// Split instructions using scratch registers. To maximize the use of the register pool for
    /// VRegs, most splits should happen in [`Self::ppc64_split`]. However, some instructions
    /// need to be split with registers after `alloc_regs`, e.g. for `compile_exits`, so this
    /// splits them and uses scratch registers for it.
    /// Linearizes all blocks into a single giant block.
    fn ppc64_scratch_split(self) -> Assembler {
        /// If opnd is Opnd::Mem with a too large disp, make the disp smaller using lea.
        fn split_large_disp(asm: &mut Assembler, opnd: Opnd, scratch_opnd: Opnd) -> Opnd {
            match opnd {
                Opnd::Mem(Mem { num_bits, disp, .. }) if !mem_disp_fits(num_bits, disp) => {
                    asm.lea_into(scratch_opnd, opnd);
                    Opnd::mem(num_bits, scratch_opnd, 0)
                }
                _ => opnd,
            }
        }

        /// If opnd is Opnd::Mem with MemBase::Stack, lower it to Opnd::Mem with MemBase::Reg, and split a large disp.
        fn split_stack_membase(asm: &mut Assembler, opnd: Opnd, scratch_opnd: Opnd) -> Opnd {
            let opnd = split_only_stack_membase(asm, opnd, scratch_opnd);
            split_large_disp(asm, opnd, scratch_opnd)
        }

        /// split_stack_membase but without split_large_disp. This should be used only by lea,
        /// whose lowering already handles large displacements in ppc64_emit.
        fn split_only_stack_membase(asm: &mut Assembler, opnd: Opnd, scratch_opnd: Opnd) -> Opnd {
            match opnd {
                Opnd::Mem(Mem { base: stack_membase @ MemBase::Stack { .. }, disp: opnd_disp, num_bits: opnd_num_bits }) => {
                    // Convert MemBase::Stack to MemBase::Reg(NATIVE_BASE_PTR) with the
                    // correct stack displacement. The stack slot value lives directly at
                    // [NATIVE_BASE_PTR + stack_disp], so we just adjust the base and
                    // combine displacements -- no indirection needed. Large
                    // displacements are handled by split_stack_membase().
                    let Mem { base, disp: stack_disp, .. } = asm.stack_state.stack_membase_to_mem(stack_membase);
                    Opnd::Mem(Mem { base, disp: stack_disp + opnd_disp, num_bits: opnd_num_bits })
                }
                Opnd::Mem(Mem { base: MemBase::StackIndirect { stack_idx }, disp: opnd_disp, num_bits: opnd_num_bits }) => {
                    // The spilled value (a pointer) lives at a stack slot. Load it
                    // into a scratch register, then use the register as the base.
                    let stack_mem = asm.stack_state.stack_membase_to_mem(MemBase::Stack { stack_idx, num_bits: 64 });
                    let stack_opnd = split_large_disp(asm, Opnd::Mem(stack_mem), scratch_opnd);
                    asm.load_into(scratch_opnd, stack_opnd);
                    Opnd::Mem(Mem {
                        base: MemBase::Reg(scratch_opnd.unwrap_reg().reg_no),
                        disp: opnd_disp,
                        num_bits: opnd_num_bits,
                    })
                }
                _ => opnd,
            }
        }

        /// If opnd is Opnd::Mem, lower it to scratch_opnd. You should use this when `opnd` is read by the instruction, not written.
        fn split_memory_read(asm: &mut Assembler, opnd: Opnd, scratch_opnd: Opnd) -> Opnd {
            if let Opnd::Mem(_) = opnd {
                let opnd = split_stack_membase(asm, opnd, scratch_opnd);
                let scratch_opnd = opnd.num_bits().map(|num_bits| scratch_opnd.with_num_bits(num_bits)).unwrap_or(scratch_opnd);
                asm.load_into(scratch_opnd, opnd);
                scratch_opnd
            } else {
                opnd
            }
        }

        /// Lower a CPushPair operand into something a store can take: a register,
        /// or a zero immediate. Anything else is loaded into scratch_opnd.
        fn split_push_operand(asm: &mut Assembler, opnd: Opnd, scratch_opnd: Opnd) -> Opnd {
            match opnd {
                Opnd::Reg(_) | Opnd::UImm(0) | Opnd::Imm(0) => opnd,
                Opnd::Mem(_) => split_memory_read(asm, opnd, scratch_opnd),
                _ => {
                    asm.load_into(scratch_opnd, opnd);
                    scratch_opnd
                }
            }
        }

        /// Lower what a store writes into something ppc64_emit can put in EMIT_OPND, which
        /// can't be the base that the load of a heap object goes through.
        fn split_store_source(asm: &mut Assembler, opnd: Opnd, scratch_opnd: Opnd) -> Opnd {
            match opnd {
                Opnd::Value(value) if !value.special_const_p() => {
                    asm.load_into(scratch_opnd, opnd);
                    scratch_opnd
                }
                _ => split_stack_membase(asm, opnd, scratch_opnd),
            }
        }

        /// If opnd is Opnd::Mem, set scratch_reg to *opnd. Return Some(Opnd::Mem) if it needs to be written back from scratch_reg.
        fn split_memory_write(opnd: &mut Opnd, scratch_opnd: Opnd) -> Option<Opnd> {
            if let Opnd::Mem(_) = opnd {
                let mem_opnd = *opnd;
                *opnd = opnd.num_bits().map(|num_bits| scratch_opnd.with_num_bits(num_bits)).unwrap_or(scratch_opnd);
                Some(mem_opnd)
            } else {
                None
            }
        }

        let mut asm_local = Assembler::new_with_asm_without_blocks(&self);
        asm_local.accept_scratch_reg = true;

        // Create one giant block to linearize everything into
        asm_local.new_block_without_id("linearized");

        let asm = &mut asm_local;

        // Get linearized instructions with branch parameters expanded into ParallelMov
        let linearized_insns = self.linearize_instructions();

        // Process each linearized instruction
        for insn in linearized_insns.iter() {
            let mut insn = insn.clone();
            match &mut insn {
                Insn::Add { left, right, out } |
                Insn::Sub { left, right, out } |
                Insn::Mul { left, right, out } |
                Insn::And { left, right, out } |
                Insn::Or { left, right, out } |
                Insn::Xor { left, right, out } |
                Insn::CSelZ  { truthy: left, falsy: right, out } |
                Insn::CSelNZ { truthy: left, falsy: right, out } |
                Insn::CSelE  { truthy: left, falsy: right, out } |
                Insn::CSelNE { truthy: left, falsy: right, out } |
                Insn::CSelL  { truthy: left, falsy: right, out } |
                Insn::CSelLE { truthy: left, falsy: right, out } |
                Insn::CSelG  { truthy: left, falsy: right, out } |
                Insn::CSelGE { truthy: left, falsy: right, out } => {
                    *left = split_memory_read(asm, *left, SCRATCH0_OPND);
                    *right = split_memory_read(asm, *right, SCRATCH1_OPND);
                    let mem_out = split_memory_write(out, SCRATCH0_OPND);

                    asm.push_insn(insn);

                    if let Some(mem_out) = mem_out {
                        let mem_out = split_stack_membase(asm, mem_out, SCRATCH1_OPND);
                        asm.store(mem_out, SCRATCH0_OPND);
                    }
                }
                Insn::Not { opnd, out } |
                Insn::ByteSwap { opnd, out } |
                Insn::LShift { opnd, out, .. } |
                Insn::RShift { opnd, out, .. } |
                Insn::URShift { opnd, out, .. } => {
                    *opnd = split_memory_read(asm, *opnd, SCRATCH0_OPND);
                    let mem_out = split_memory_write(out, SCRATCH0_OPND);

                    asm.push_insn(insn);

                    if let Some(mem_out) = mem_out {
                        let mem_out = split_stack_membase(asm, mem_out, SCRATCH1_OPND);
                        asm.store(mem_out, SCRATCH0_OPND);
                    }
                }
                Insn::Cmp { left, right } |
                Insn::Test { left, right } => {
                    *left = split_memory_read(asm, *left, SCRATCH0_OPND);
                    *right = split_memory_read(asm, *right, SCRATCH1_OPND);
                    asm.push_insn(insn);
                }
                // For compile_exits, support splitting simple C arguments here
                Insn::CCall { data } if !data.opnds.is_empty() => {
                    for (i, opnd) in data.opnds.iter().enumerate() {
                        asm.load_into(C_ARG_OPNDS[i], *opnd);
                    }
                    data.opnds = vec![];
                    asm.push_insn(insn);
                }
                Insn::CPushPair(opnd0, opnd1) => {
                    if let Some(opnd0) = opnd0 {
                        *opnd0 = split_push_operand(asm, *opnd0, SCRATCH0_OPND);
                    }
                    *opnd1 = split_push_operand(asm, *opnd1, SCRATCH1_OPND);
                    asm.push_insn(insn);
                }
                // For compile_exits, support splitting simple return values here
                Insn::CRet(opnd) => {
                    match opnd {
                        Opnd::Reg(C_RET_REG) => {},
                        _ => asm.load_into(C_RET_OPND, *opnd),
                    }
                    asm.cret(C_RET_OPND);
                }
                Insn::Lea { opnd, out } => {
                    *opnd = split_only_stack_membase(asm, *opnd, SCRATCH0_OPND);
                    let mem_out = split_memory_write(out, SCRATCH0_OPND);

                    asm.push_insn(insn);

                    if let Some(mem_out) = mem_out {
                        let mem_out = split_stack_membase(asm, mem_out, SCRATCH1_OPND);
                        asm.store(mem_out, SCRATCH0_OPND);
                    }
                }
                Insn::Load { opnd, out } |
                Insn::LoadInto { opnd, dest: out } => {
                    *out = split_stack_membase(asm, *out, SCRATCH1_OPND);

                    if let Opnd::Mem(_) = out {
                        *opnd = split_store_source(asm, *opnd, SCRATCH0_OPND);
                        asm.store(*out, *opnd);
                    } else {
                        *opnd = split_stack_membase(asm, *opnd, SCRATCH0_OPND);
                        // If in and out are the same, this is a redundant mov
                        if opnd != out {
                            asm.push_insn(insn);
                        }
                    }
                }
                &mut Insn::IncrCounter { mem, value } => {
                    // It's split here to support IncrCounter in compile_exits.
                    assert!(matches!(mem, Opnd::UImm(_)));
                    asm.load_into(SCRATCH0_OPND, mem);
                    asm.incr_counter(SCRATCH0_OPND, value);
                }
                Insn::Store { dest, src } => {
                    *dest = split_stack_membase(asm, *dest, SCRATCH0_OPND);
                    *src = split_store_source(asm, *src, SCRATCH1_OPND);
                    asm.push_insn(insn);
                }
                Insn::Mov { dest, src } => {
                    *dest = split_stack_membase(asm, *dest, SCRATCH1_OPND);
                    match dest {
                        Opnd::Reg(_) => {
                            *src = split_stack_membase(asm, *src, SCRATCH0_OPND);
                            asm.load_into(*dest, *src);
                        }
                        Opnd::Mem(_) => {
                            *src = split_store_source(asm, *src, SCRATCH0_OPND);
                            asm.store(*dest, *src);
                        }
                        _ => asm.push_insn(insn),
                    }
                }
                &mut Insn::PatchPoint(ref data) => {
                    split_patch_point(asm, &data.target, data.invariant, data.version);
                }
                _ => {
                    asm.push_insn(insn);
                }
            }
        }

        asm_local
    }

    /// Emit platform-specific machine code
    /// Returns a list of GC offsets. Can return failure to signal caller to retry.
    fn ppc64_emit(&mut self, cb: &mut CodeBlock) -> Result<Vec<CodePtr>, CompileError> {
        /// Emit a conditional jump instruction to a specific target. This is
        /// called when lowering any of the conditional jump instructions.
        fn emit_conditional_jump(asm: &Assembler, cb: &mut CodeBlock, condition: Condition, target: Target) {
            fn generate_branch(cb: &mut CodeBlock, condition: Condition, dst_ptr: CodePtr) {
                let offset = dst_ptr.as_offset() - cb.get_write_ptr().as_offset();
                if bc_offset_fits(offset) {
                    // If the jump offset fits into the conditional jump as
                    // an immediate value, then we can use the bc
                    // instruction directly.
                    bc(cb, condition, offset);
                } else if b_offset_fits(offset - 4) { // - 4 for bc
                    // If the jump offset fits into the unconditional jump as
                    // an immediate value, we can use inverse bc + b.
                    //
                    // We're going to write out the inverse condition so
                    // that if it doesn't match it will skip over the
                    // instruction used for branching.
                    bc(cb, condition.inverse(), 2 * 4);
                    b(cb, offset - 4); // - 4 for bc
                } else {
                    // Otherwise, we need to load the address into CTR and
                    // branch to that. The load takes 5 instructions at most,
                    // and we pad it to that many to know what to skip over.
                    const LOAD_INSNS: usize = 5;

                    // We're going to write out the inverse condition so
                    // that if it doesn't match it will skip over the
                    // instructions used for branching.
                    bc(cb, condition.inverse(), (LOAD_INSNS as i64 + 3) * 4);
                    let load_insns = emit_load_code_ptr(cb, Assembler::EMIT_OPND, dst_ptr);
                    (load_insns..LOAD_INSNS).for_each(|_| nop(cb));
                    mtctr(cb, Assembler::EMIT_OPND);
                    bctr(cb);
                }
            }

            let label = match target {
                Target::CodePtr(dst_ptr) => {
                    generate_branch(cb, condition, dst_ptr);
                    return;
                },
                Target::Label(l) => l,
                Target::Block(ref edge) => asm.block_label(edge.target),
                Target::SideExit(..) => {
                    unreachable!("Target::SideExit should have been compiled by compile_exits")
                },
            };
            // bc only reaches +/-32KiB, which a label at the far end of a function, like
            // the one for a side exit, can be beyond. Reserve room for inverse bc + b.
            cb.label_ref(label, 2 * 4, move |cb, src_addr, dst_addr| {
                // src_addr is after the two instructions while bc
                // counts the offset relative to its own address.
                let offset = dst_addr - (src_addr - 2 * 4);
                if bc_offset_fits(offset) {
                    bc(cb, condition, offset);
                    nop(cb);
                    Ok(())
                } else if b_offset_fits(offset - 4) { // - 4 for bc
                    bc(cb, condition.inverse(), 2 * 4);
                    b(cb, offset - 4); // - 4 for bc
                    Ok(())
                } else {
                    Err(())
                }
            });
        }

        /// Emit a b that goes to a label
        fn emit_jmp_label(cb: &mut CodeBlock, label: Label) {
            // Reserve space for a single b instruction
            cb.label_ref(label, 4, |cb, src_addr, dst_addr| {
                // src_addr is after the instruction while b
                // counts the offset relative to its own address.
                let offset = dst_addr - (src_addr - 4);
                if b_offset_fits(offset) {
                    b(cb, offset);
                    Ok(())
                } else {
                    Err(())
                }
            });
        }

        /// Emit a load from memory into a register, which zero-extends
        /// anything narrower than the register
        fn emit_load_mem(cb: &mut CodeBlock, rt: PPC64Opnd, mem: PPC64Opnd) {
            let PPC64Opnd::Mem(PPC64Mem { num_bits, .. }) = mem else {
                panic!("Expected memory operand, got {mem:?}");
            };
            match num_bits {
                64 => ld(cb, rt, mem),
                32 => lwz(cb, rt, mem),
                16 => lhz(cb, rt, mem),
                8 => lbz(cb, rt, mem),
                num_bits => panic!("unexpected num_bits: {num_bits}"),
            }
        }

        /// Emit a store of the lower bits of a register to memory
        fn emit_store_mem(cb: &mut CodeBlock, rs: PPC64Opnd, mem: PPC64Opnd) {
            let PPC64Opnd::Mem(PPC64Mem { num_bits, .. }) = mem else {
                panic!("Expected memory operand, got {mem:?}");
            };
            match num_bits {
                64 => std(cb, rs, mem),
                32 => stw(cb, rs, mem),
                16 => sth(cb, rs, mem),
                8 => stb(cb, rs, mem),
                num_bits => panic!("unexpected num_bits: {num_bits}"),
            }
        }

        /// Emit a copy of one register to another. A copy between registers of fewer than
        /// 64 bits leaves zero in the bits above them, like on the other platforms.
        fn emit_mov_reg(cb: &mut CodeBlock, dest: &Opnd, src: &Opnd) {
            match dest.rm_num_bits().min(src.rm_num_bits()) {
                64 => mr(cb, dest.into(), src.into()),
                num_bits => {
                    let clear_bits = PPC64Opnd::new_uimm(64 - u64::from(num_bits));
                    rldicl(cb, dest.into(), src.into(), PPC64Opnd::new_uimm(0), clear_bits);
                }
            }
        }

        /// Do the address calculation of `out = base + disp`
        fn emit_add_disp(cb: &mut CodeBlock, out: PPC64Opnd, base: PPC64Opnd, disp: i32) {
            if let Ok(disp) = i16::try_from(disp) {
                addi(cb, out, base, PPC64Opnd::new_imm(disp.into()));
            } else if let Some((upper, lower)) = split_disp(disp.into()) {
                addis(cb, out, base, PPC64Opnd::new_imm(upper.into()));
                if lower != 0 {
                    addi(cb, out, out, PPC64Opnd::new_imm(lower.into()));
                }
            } else {
                emit_load_value(cb, Assembler::EMIT_OPND, disp as u64);
                add(cb, out, base, Assembler::EMIT_OPND);
            }
        }

        /// Load a VALUE to a register and remember it for GC marking and reference updating
        fn emit_load_gc_value(cb: &mut CodeBlock, gc_offsets: &mut Vec<CodePtr>, dest: PPC64Opnd, value: VALUE) {
            // We dont need to check if it's a special const
            // here because we only allow these operands to hit
            // this point if they're not a special const.
            assert!(!value.special_const_p());
            assert_ne!(Assembler::EMIT_OPND, dest, "the load goes through dest, which r0 can't be the base of");

            // This assumes only load instructions can contain
            // references to GC'd Value operands. If the value
            // being loaded is a heap object, we'll report that
            // back out to the gc_offsets list.
            //
            // The value lives in the code, where GC can update it, and we jump
            // over it. Before ISA 3.1, there is no PC-relative load, so load it
            // through its address.
            b(cb, 4 + (SIZEOF_VALUE as i64));
            let value_ptr = cb.get_write_ptr();
            cb.write_bytes(&value.as_u64().to_le_bytes());
            gc_offsets.push(value_ptr);

            if power_isa() >= PowerIsa::V3_1 {
                align_prefixed(cb);
                pld_pc_relative(cb, dest, value_ptr.as_offset() - cb.get_write_ptr().as_offset());
            } else {
                emit_load_value(cb, dest, value_ptr.raw_addr(cb) as u64);
                ld(cb, dest, PPC64Opnd::new_mem(64, dest, 0));
            }
        }

        /// Return the register that an operand is in. An immediate is loaded into EMIT_OPND first.
        fn emit_reg_or_imm(cb: &mut CodeBlock, opnd: &Opnd) -> PPC64Opnd {
            match imm_opnd_value(opnd) {
                Some(imm) => {
                    emit_load_value(cb, Assembler::EMIT_OPND, imm as u64);
                    Assembler::EMIT_OPND
                }
                None => opnd.into(),
            }
        }

        /// Clear SO of XER, which stays set once an operation has overflowed, so that
        /// it tells about the overflow-recording instruction that comes next.
        /// ISA 3.0 doesn't go through SO. See [`emit_overflow_recording`].
        fn emit_clear_overflow(cb: &mut CodeBlock) {
            if power_isa() < PowerIsa::V3_0 {
                li(cb, Assembler::EMIT_OPND, PPC64Opnd::new_imm(0));
                mtxer(cb, Assembler::EMIT_OPND);
            }
        }

        type EmitArith = fn(&mut CodeBlock, PPC64Opnd, PPC64Opnd, PPC64Opnd);

        /// Emit an overflow-recording instruction so that CR0 has a bit for Jo to branch on,
        /// which [`overflow_condition`] tells. ISA 3.0 has an instruction that copies OV of
        /// XER there, which is only about the last such instruction. Before it, the form
        /// that also sets CR0 copies SO, which [`emit_clear_overflow`] needs to clear first.
        fn emit_overflow_recording(cb: &mut CodeBlock, arith: EmitArith, arith_dot: EmitArith, rt: PPC64Opnd, ra: PPC64Opnd, rb: PPC64Opnd) {
            if power_isa() >= PowerIsa::V3_0 {
                arith(cb, rt, ra, rb);
                mcrxrx(cb);
            } else {
                arith_dot(cb, rt, ra, rb);
            }
        }

        /// The condition that CR0 meets after [`emit_overflow_recording`] on overflow
        fn overflow_condition() -> Condition {
            if power_isa() >= PowerIsa::V3_0 {
                Condition::LT
            } else {
                Condition::SO
            }
        }

        /// Clear the upper 32 bits of the result of a 32-bit operation, like
        /// writing to a 32-bit register does on the other platforms
        fn emit_truncate_result(cb: &mut CodeBlock, out: &Opnd) {
            if out.rm_num_bits() == 32 {
                rldicl(cb, out.into(), out.into(), PPC64Opnd::new_uimm(0), PPC64Opnd::new_uimm(32));
            }
        }

        /// Return the register to store for a push. No register reads as zero in
        /// a store, so a zero immediate is loaded into EMIT_OPND first.
        fn emit_push_operand(cb: &mut CodeBlock, opnd: &Opnd) -> PPC64Opnd {
            match opnd {
                Opnd::UImm(0) | Opnd::Imm(0) => {
                    li(cb, Assembler::EMIT_OPND, PPC64Opnd::new_imm(0));
                    Assembler::EMIT_OPND
                }
                _ => opnd.into(),
            }
        }

        /// Move SP, and the frame header it points at, by `disp` bytes. stdu stores the
        /// back chain where SP is about to point, so there is one at SP at any moment.
        fn emit_move_frame_header(cb: &mut CodeBlock, disp: i32) {
            ld(cb, Assembler::EMIT_OPND, PPC64Opnd::new_mem(64, R1, 0));
            stdu(cb, Assembler::EMIT_OPND, PPC64Opnd::new_mem(64, R1, disp));
        }

        /// Pop a value from the stack by loading the one above the frame header then
        /// moving the header up over it.
        fn emit_pop(cb: &mut CodeBlock, opnd: PPC64Opnd) {
            ld(cb, opnd, PPC64Opnd::new_mem(64, R1, FRAME_HEADER_SIZE));
            emit_move_frame_header(cb, C_SP_STEP);
        }

        /// Emit a call to a function at an absolute address
        fn emit_call_ptr(cb: &mut CodeBlock, dst_addr: u64) {
            let region_start = cb.get_ptr(0).raw_addr(cb) as u64;
            let region_end = region_start + cb.virtual_region_size() as u64;

            // A call into JIT code gets rewritten in place when the address of the callee
            // changes, so it takes up the same room no matter what the address is.
            let rewritable = (region_start..region_end).contains(&dst_addr);

            if rewritable && cb.b_reaches_code_region() {
                // Use bl when it reaches all of the code region
                bl(cb, dst_addr as i64 - cb.get_write_ptr().raw_addr(cb) as i64);
            } else {
                // A C function expects its own address in r12 when called through
                // its global entry point, to find its TOC with.
                let offset = dst_addr as i64 - cb.get_write_ptr().raw_addr(cb) as i64;
                // (Aligning a prefixed instruction may move it by a word.)
                if power_isa() >= PowerIsa::V3_1 && prefixed_imm_fits(offset) && prefixed_imm_fits(offset - 4) {
                    // The offset of pla reaches +/-8GiB, which is all of the code region
                    // and often the functions of the executable. Where it is, it takes up
                    // the same room no matter what the address is.
                    align_prefixed(cb);
                    pla(cb, R12, dst_addr as i64 - cb.get_write_ptr().raw_addr(cb) as i64);
                } else {
                    const LOAD_INSNS: usize = 5;
                    let load_insns = emit_load_value(cb, R12, dst_addr);
                    if rewritable {
                        (load_insns..LOAD_INSNS).for_each(|_| nop(cb));
                    }
                }
                mtctr(cb, R12);
                bctrl(cb);
            }
        }

        /// Fill nops until a jump written at `last_patch_pos` can no longer reach
        /// past the current write position.
        fn emit_pad_after_patch_point(cb: &mut CodeBlock, last_patch_pos: Option<usize>) {
            let Some(last_patch_pos) = last_patch_pos else { return };
            while cb.get_write_pos().saturating_sub(last_patch_pos) < cb.jmp_ptr_bytes() && !cb.has_dropped_bytes() {
                nop(cb);
            }
        }

        // List of GC offsets
        let mut gc_offsets: Vec<CodePtr> = Vec::new();

        // Buffered list of PosMarker callbacks to fire if codegen is successful
        let mut pos_markers: Vec<(usize, CodePtr)> = vec![];

        // The write_pos for the last Insn::PatchPoint, if any
        let mut last_patch_pos: Option<usize> = None;

        // Where the code of this function starts
        let start_pos = cb.get_write_pos();

        // For each instruction
        // NOTE: At this point, the assembler should have been linearized into a single giant block
        // by either resolve_parallel_mov_pass() or ppc64_scratch_split().
        let mut insn_idx: usize = 0;
        assert_eq!(self.basic_blocks.len(), 1, "Assembler should be linearized into a single block before ppc64_emit");
        let insns = &self.basic_blocks[0].insns;

        while let Some(insn) = insns.get(insn_idx) {
            match insn {
                Insn::Comment(text) => {
                    cb.add_comment(text);
                },
                Insn::Label(target) => {
                    cb.write_label(target.unwrap_label());
                },
                // Report back the current position in the generated code
                Insn::PosMarker(..) => {
                    pos_markers.push((insn_idx, cb.get_write_ptr()))
                }
                Insn::PosMarkerAtBlockEnd(..) => {
                    unreachable!("PosMarkerAtBlockEnd should have been lowered by linearize_instructions");
                },
                Insn::BakeString(text) => {
                    for byte in text.as_bytes() {
                        cb.write_byte(*byte);
                    }

                    // Add a null-terminator byte for safety (in case we pass
                    // this to C code)
                    cb.write_byte(0);

                    // Pad out the string to the next 4-byte boundary so that
                    // it's easy to jump past.
                    for _ in 0..(4 - ((text.len() + 1) % 4)) {
                        cb.write_byte(0);
                    }
                },
                &Insn::FrameSetup { preserved, slot_count } => {
                    const { assert!(SIZEOF_VALUE == 8, "alignment logic relies on SIZEOF_VALUE == 8"); }
                    // The return address, which is in LR, goes to the LR save doubleword that
                    // the caller has for us in its linkage area. r31 goes right below the
                    // caller's frame, and becomes the base pointer.
                    mflr(cb, Self::EMIT_OPND);
                    std(cb, Self::EMIT_OPND, PPC64Opnd::new_mem(64, R1, LR_SAVE_OFFSET));
                    std(cb, R31, PPC64Opnd::new_mem(64, R1, -SIZEOF_VALUE_I32));
                    addi(cb, R31, R1, PPC64Opnd::new_imm((-C_SP_STEP).into()));

                    // Preserved registers go below the base pointer, the slots below them, and
                    // the frame header at the bottom. stdu moves SP past all of them at once
                    // and stores the back chain at the new SP.
                    let mut slot_count = slot_count + preserved.len();
                    // Align slot_count
                    if slot_count % 2 == 1 {
                        slot_count += 1
                    }
                    let frame_size = C_SP_STEP + (slot_count * SIZEOF_VALUE) as i32 + FRAME_HEADER_SIZE;
                    stdu(cb, R1, PPC64Opnd::new_mem(64, R1, -frame_size));
                    for (idx, reg) in preserved.iter().enumerate() {
                        std(cb, reg.into(), PPC64Opnd::new_mem(64, R31, -(idx as i32 + 1) * SIZEOF_VALUE_I32));
                    }
                }
                Insn::FrameTeardown { preserved } => {
                    // Restore preserved registers below frame pointer.
                    for (idx, reg) in preserved.iter().enumerate() {
                        ld(cb, reg.into(), PPC64Opnd::new_mem(64, R31, -(idx as i32 + 1) * SIZEOF_VALUE_I32));
                    }

                    // SP = the caller's frame, which is right above where r31 was saved
                    ld(cb, Self::EMIT_OPND, PPC64Opnd::new_mem(64, R31, C_SP_STEP + LR_SAVE_OFFSET));
                    addi(cb, R1, R31, PPC64Opnd::new_imm(C_SP_STEP.into()));
                    ld(cb, R31, PPC64Opnd::new_mem(64, R31, SIZEOF_VALUE_I32));
                    mtlr(cb, Self::EMIT_OPND);
                }
                // Popping the stack arguments of a call, which were pushed above the frame header
                Insn::Add { left, right, out } if *out == NATIVE_STACK_PTR => {
                    assert_eq!(*left, NATIVE_STACK_PTR, "native SP should only be set from itself");
                    let bytes = imm_opnd_value(right).expect("native SP should move by an immediate");
                    emit_move_frame_header(cb, bytes.try_into().unwrap());
                },
                Insn::Add { left, right, out } => {
                    let checked = overflow_checked(insns, insn_idx);
                    match imm_opnd_value(right) {
                        Some(imm) if !checked && i16::try_from(imm).is_ok() => {
                            addi(cb, out.into(), left.into(), PPC64Opnd::new_imm(imm));
                        }
                        _ if checked => {
                            emit_clear_overflow(cb);
                            let right = emit_reg_or_imm(cb, right);
                            emit_overflow_recording(cb, addo, addo_dot, out.into(), left.into(), right);
                        }
                        _ => {
                            let right = emit_reg_or_imm(cb, right);
                            add(cb, out.into(), left.into(), right);
                        }
                    }
                    emit_truncate_result(cb, out);
                },
                Insn::Sub { left, right, out } => {
                    let checked = overflow_checked(insns, insn_idx);
                    // There is no instruction to subtract an immediate. Add its negation.
                    let negated = imm_opnd_value(right)
                        .and_then(i64::checked_neg)
                        .filter(|negated| i16::try_from(*negated).is_ok());
                    match negated {
                        Some(negated) if !checked => {
                            addi(cb, out.into(), left.into(), PPC64Opnd::new_imm(negated));
                        }
                        _ if checked => {
                            emit_clear_overflow(cb);
                            let right = emit_reg_or_imm(cb, right);
                            emit_overflow_recording(cb, subfo, subfo_dot, out.into(), right, left.into());
                        }
                        _ => {
                            let right = emit_reg_or_imm(cb, right);
                            subf(cb, out.into(), right, left.into());
                        }
                    }
                    emit_truncate_result(cb, out);
                },
                Insn::Mul { left, right, out } => {
                    // If the next instruction is JoMul
                    if overflow_checked(insns, insn_idx) {
                        emit_clear_overflow(cb);
                        emit_overflow_recording(cb, mulldo, mulldo_dot, out.into(), left.into(), right.into());
                    } else {
                        mulld(cb, out.into(), left.into(), right.into());
                    }
                    emit_truncate_result(cb, out);
                },
                Insn::And { left, right, out } => {
                    match imm_opnd_value(right) {
                        Some(imm) if u16::try_from(imm).is_ok() => {
                            andi_dot(cb, out.into(), left.into(), PPC64Opnd::new_imm(imm));
                        }
                        _ => {
                            let right = emit_reg_or_imm(cb, right);
                            and(cb, out.into(), left.into(), right);
                        }
                    }
                    emit_truncate_result(cb, out);
                },
                Insn::Or { left, right, out } => {
                    match imm_opnd_value(right) {
                        Some(imm) if u16::try_from(imm).is_ok() => {
                            ori(cb, out.into(), left.into(), PPC64Opnd::new_imm(imm));
                        }
                        _ => {
                            let right = emit_reg_or_imm(cb, right);
                            or(cb, out.into(), left.into(), right);
                        }
                    }
                    emit_truncate_result(cb, out);
                },
                Insn::Xor { left, right, out } => {
                    match imm_opnd_value(right) {
                        Some(imm) if u16::try_from(imm).is_ok() => {
                            xori(cb, out.into(), left.into(), PPC64Opnd::new_imm(imm));
                        }
                        _ => {
                            let right = emit_reg_or_imm(cb, right);
                            xor(cb, out.into(), left.into(), right);
                        }
                    }
                    emit_truncate_result(cb, out);
                },
                Insn::Not { opnd, out } => {
                    not(cb, out.into(), opnd.into());
                    emit_truncate_result(cb, out);
                },
                Insn::RShift { opnd, shift, out } => {
                    // A value of 32 bits has zero above them, so it needs its sign back first
                    let opnd = if opnd.rm_num_bits() == 32 {
                        extsw(cb, out.into(), opnd.into());
                        out
                    } else {
                        opnd
                    };
                    match imm_opnd_value(shift) {
                        Some(shift) => sradi(cb, out.into(), opnd.into(), PPC64Opnd::new_imm(shift)),
                        None => srad(cb, out.into(), opnd.into(), shift.into()),
                    }
                    emit_truncate_result(cb, out);
                },
                Insn::URShift { opnd, shift, out } => {
                    match imm_opnd_value(shift) {
                        Some(shift) => srdi(cb, out.into(), opnd.into(), PPC64Opnd::new_imm(shift)),
                        None => srd(cb, out.into(), opnd.into(), shift.into()),
                    }
                },
                Insn::LShift { opnd, shift, out } => {
                    match imm_opnd_value(shift) {
                        // No shift records overflow. Shifting by one bit is
                        // adding the value to itself, which can.
                        Some(1) if overflow_checked(insns, insn_idx) => {
                            emit_clear_overflow(cb);
                            emit_overflow_recording(cb, addo, addo_dot, out.into(), opnd.into(), opnd.into());
                        }
                        Some(shift) => sldi(cb, out.into(), opnd.into(), PPC64Opnd::new_imm(shift)),
                        None => sld(cb, out.into(), opnd.into(), shift.into()),
                    }
                    emit_truncate_result(cb, out);
                },
                Insn::ByteSwap { opnd, out } => {
                    // Before ISA 3.1, only a load can reverse bytes. Go through the doubleword
                    // below SP, which the ABI lets us use for as long as we don't make a call.
                    const SWAP_DISP: i32 = -SIZEOF_VALUE_I32;
                    std(cb, opnd.into(), PPC64Opnd::new_mem(64, R1, SWAP_DISP));
                    li(cb, Self::EMIT_OPND, PPC64Opnd::new_imm(SWAP_DISP.into()));
                    match opnd.rm_num_bits() {
                        64 => ldbrx(cb, out.into(), R1, Self::EMIT_OPND),
                        32 => lwbrx(cb, out.into(), R1, Self::EMIT_OPND),
                        16 => lhbrx(cb, out.into(), R1, Self::EMIT_OPND),
                        num_bits => panic!("unexpected num_bits: {num_bits}"),
                    }
                },
                Insn::Store { dest, src } => {
                    // Split src into EMIT_OPND if necessary
                    let src_reg: PPC64Opnd = match src {
                        Opnd::Reg(_) => src.into(),
                        // Immediates
                        &Opnd::Imm(imm) => {
                            emit_load_value(cb, Self::EMIT_OPND, imm as u64);
                            Self::EMIT_OPND
                        }
                        &Opnd::UImm(imm) => {
                            emit_load_value(cb, Self::EMIT_OPND, imm);
                            Self::EMIT_OPND
                        }
                        // For mem-to-mem store, load the source into EMIT_OPND
                        Opnd::Mem(Mem { base: MemBase::Reg(_), .. }) => {
                            emit_load_mem(cb, Self::EMIT_OPND, src.into());
                            Self::EMIT_OPND
                        }
                        src @ (Opnd::Mem(_) | Opnd::Value(_) | Opnd::None | Opnd::VReg { .. }) => panic!("Unexpected source operand during ppc64_emit: {src:?}")
                    };

                    emit_store_mem(cb, src_reg, dest.into());
                },
                Insn::Load { opnd, out } |
                Insn::LoadInto { opnd, dest: out } => {
                    match *opnd {
                        Opnd::Reg(_) | Opnd::VReg { .. } => {
                            emit_mov_reg(cb, out, opnd);
                        },
                        Opnd::UImm(uimm) => {
                            emit_load_value(cb, out.into(), uimm);
                        },
                        Opnd::Imm(imm) => {
                            emit_load_value(cb, out.into(), imm as u64);
                        },
                        Opnd::Mem(_) => {
                            emit_load_mem(cb, out.into(), opnd.into());
                        },
                        Opnd::Value(value) => {
                            emit_load_gc_value(cb, &mut gc_offsets, out.into(), value);
                        },
                        Opnd::None => {
                            unreachable!("Attempted to load from None operand");
                        }
                    };
                },
                Insn::LoadSExt { opnd, out } => {
                    match *opnd {
                        Opnd::Reg(Reg { num_bits: 32, .. }) |
                        Opnd::VReg { num_bits: 32, .. } => {
                            extsw(cb, out.into(), opnd.into());
                        },
                        // lwa only takes a displacement that is a multiple of 4
                        Opnd::Mem(Mem { num_bits: 32, disp, .. }) if disp % 4 == 0 => {
                            lwa(cb, out.into(), opnd.into());
                        },
                        Opnd::Mem(Mem { num_bits: 32, .. }) => {
                            lwz(cb, out.into(), opnd.into());
                            extsw(cb, out.into(), out.into());
                        },
                        _ => unreachable!()
                    };
                },
                Insn::Mov { dest, src } => {
                    // ppc64_split() and ppc64_scratch_split() should have turned
                    // moves from and to memory into loads and stores.
                    match imm_opnd_value(src) {
                        Some(imm) => {
                            emit_load_value(cb, dest.into(), imm as u64);
                        },
                        None => {
                            emit_mov_reg(cb, dest, src);
                        }
                    }
                },
                Insn::Lea { opnd, out } => {
                    let &Opnd::Mem(Mem { num_bits: _, base: MemBase::Reg(base_reg_no), disp }) = opnd else {
                        panic!("Unexpected Insn::Lea operand in ppc64_emit: {opnd:?}");
                    };
                    emit_add_disp(cb, out.into(), PPC64Opnd::Reg(mem_base_reg(base_reg_no)), disp);
                }
                Insn::LeaJumpTarget { out, target, .. } => {
                    if let Target::Label(label_idx) = target {
                        // Set output to the raw address of the label. There is no PC-relative
                        // address calculation, so start from the address we are at, and add
                        // the distance to the label once it's known.
                        let out: PPC64Opnd = out.into();
                        let base_addr = cb.get_write_pos() as i64;
                        emit_load_value(cb, out, cb.get_write_ptr().raw_addr(cb) as u64);
                        cb.label_ref(*label_idx, 2 * 4, move |cb, _, dst_addr| {
                            let (upper, lower) = split_disp(dst_addr - base_addr).ok_or(())?;
                            addis(cb, out, out, PPC64Opnd::new_imm(upper.into()));
                            addi(cb, out, out, PPC64Opnd::new_imm(lower.into()));
                            Ok(())
                        });
                    } else {
                        // Set output to the jump target's raw address
                        emit_load_code_ptr(cb, out.into(), target.unwrap_code_ptr());
                    }
                },
                Insn::CPush(opnd) => {
                    // Push a value to the stack by moving the frame header down and storing
                    // where it was, leaving an 8-byte gap for alignment.
                    emit_move_frame_header(cb, -C_SP_STEP);
                    std(cb, opnd.into(), PPC64Opnd::new_mem(64, R1, FRAME_HEADER_SIZE));
                },
                Insn::CPushPair(opnd0, opnd1) => {
                    emit_move_frame_header(cb, -C_SP_STEP);
                    // Second operand ends up at the lower stack address
                    if let Some(opnd0) = opnd0 {
                        let first_push = emit_push_operand(cb, opnd0);
                        std(cb, first_push, PPC64Opnd::new_mem(64, R1, FRAME_HEADER_SIZE + SIZEOF_VALUE_I32));
                    }
                    // With no first operand, write only the lower slot while
                    // still reserving the pair.
                    let second_push = emit_push_operand(cb, opnd1);
                    std(cb, second_push, PPC64Opnd::new_mem(64, R1, FRAME_HEADER_SIZE));
                },
                Insn::CPop { out } => {
                    emit_pop(cb, out.into());
                },
                Insn::CPopInto(opnd) => {
                    emit_pop(cb, opnd.into());
                },
                Insn::CPopPairInto(opnd0, opnd1) => {
                    // First operand is popped from the lower stack address.
                    // The second one wins when they are the same register.
                    if opnd0 != opnd1 {
                        ld(cb, opnd0.into(), PPC64Opnd::new_mem(64, R1, FRAME_HEADER_SIZE));
                    }
                    ld(cb, opnd1.into(), PPC64Opnd::new_mem(64, R1, FRAME_HEADER_SIZE + SIZEOF_VALUE_I32));
                    emit_move_frame_header(cb, C_SP_STEP);
                },
                Insn::CCall { data, .. } => {
                    // Whatever we call saves LR, and possibly more, in the linkage area that it
                    // expects at SP, and looks for its stack arguments above the parameter save
                    // area. Pushes leave them right above the frame header, so move the header
                    // down until it is the start of a linkage area that far from them.
                    emit_move_frame_header(cb, -CALL_AREA_SIZE);

                    let fptr = &data.fptr;
                    match fptr {
                        Opnd::UImm(fptr) => {
                            emit_call_ptr(cb, *fptr);
                        }
                        Opnd::Reg(_) => {
                            mr(cb, R12, fptr.into());
                            mtctr(cb, R12);
                            bctrl(cb);
                        }
                        _ => unreachable!("unsupported ccall fptr: {fptr:?}")
                    }

                    // The callee may have saved its arguments over where the header was
                    emit_move_frame_header(cb, CALL_AREA_SIZE);
                },
                Insn::CRet { .. } => {
                    blr(cb);
                },
                Insn::Cmp { left, right } => {
                    let unsigned = compared_unsigned(insns, insn_idx);
                    let doubleword = left.rm_num_bits() == 64;
                    match (imm_opnd_value(right), unsigned) {
                        (Some(imm), false) if i16::try_from(imm).is_ok() => {
                            let imm = PPC64Opnd::new_imm(imm);
                            if doubleword { cmpdi(cb, left.into(), imm) } else { cmpwi(cb, left.into(), imm) }
                        }
                        (Some(imm), true) if u16::try_from(imm).is_ok() => {
                            let imm = PPC64Opnd::new_imm(imm);
                            if doubleword { cmpldi(cb, left.into(), imm) } else { cmplwi(cb, left.into(), imm) }
                        }
                        _ => {
                            let right = emit_reg_or_imm(cb, right);
                            match (unsigned, doubleword) {
                                (false, true) => cmpd(cb, left.into(), right),
                                (false, false) => cmpw(cb, left.into(), right),
                                (true, true) => cmpld(cb, left.into(), right),
                                (true, false) => cmplw(cb, left.into(), right),
                            }
                        }
                    }
                },
                Insn::Test { left, right } => {
                    // Only CR0 is of interest, so the result goes to EMIT_OPND to be discarded
                    match imm_opnd_value(right) {
                        Some(imm) if u16::try_from(imm).is_ok() => {
                            andi_dot(cb, Self::EMIT_OPND, left.into(), PPC64Opnd::new_imm(imm));
                        }
                        _ => {
                            let right = emit_reg_or_imm(cb, right);
                            if left.rm_num_bits() == 32 {
                                // and. would compare all 64 bits of the result with zero
                                and(cb, Self::EMIT_OPND, left.into(), right);
                                cmpwi(cb, Self::EMIT_OPND, PPC64Opnd::new_imm(0));
                            } else {
                                and_dot(cb, Self::EMIT_OPND, left.into(), right);
                            }
                        }
                    }
                },
                Insn::JmpOpnd(opnd) => {
                    mtctr(cb, opnd.into());
                    bctr(cb);
                },
                Insn::Jmp(target) => {
                    match *target {
                        Target::CodePtr(dst_ptr) => {
                            emit_jmp_ptr(cb, dst_ptr);
                        },
                        Target::Label(label_idx) => {
                            emit_jmp_label(cb, label_idx);
                        },
                        Target::Block(ref edge) => {
                            let label = self.block_label(edge.target);
                            emit_jmp_label(cb, label);
                        },
                        Target::SideExit(..) => {
                            unreachable!("Target::SideExit should have been compiled by compile_exits")
                        },
                    };
                },
                Insn::Je(target) | Insn::Jz(target) => {
                    emit_conditional_jump(self, cb, Condition::EQ, target.clone());
                },
                Insn::Jne(target) | Insn::Jnz(target) => {
                    emit_conditional_jump(self, cb, Condition::NE, target.clone());
                },
                Insn::Jl(target) => {
                    emit_conditional_jump(self, cb, Condition::LT, target.clone());
                },
                Insn::Jg(target) => {
                    emit_conditional_jump(self, cb, Condition::GT, target.clone());
                },
                Insn::Jge(target) => {
                    emit_conditional_jump(self, cb, Condition::GE, target.clone());
                },
                // The Cmp before these two compares unsigned integers. See compared_unsigned().
                Insn::Jbe(target) => {
                    emit_conditional_jump(self, cb, Condition::LE, target.clone());
                },
                Insn::Jb(target) => {
                    emit_conditional_jump(self, cb, Condition::LT, target.clone());
                },
                Insn::Jo(target) | Insn::JoMul(target) => {
                    emit_conditional_jump(self, cb, overflow_condition(), target.clone());
                },
                Insn::Joz(opnd, target) => {
                    cmpdi(cb, opnd.into(), PPC64Opnd::new_imm(0));
                    emit_conditional_jump(self, cb, Condition::EQ, target.clone());
                },
                Insn::Jonz(opnd, target) => {
                    cmpdi(cb, opnd.into(), PPC64Opnd::new_imm(0));
                    emit_conditional_jump(self, cb, Condition::NE, target.clone());
                },
                Insn::PatchPoint(..) => unreachable!("PatchPoint should have been lowered to PatchPointPad in ppc64_scratch_split"),
                Insn::PatchPointPad => {
                    emit_pad_after_patch_point(cb, last_patch_pos);
                    // This position is itself where a jump gets written on invalidation, so it
                    // becomes what following code has to keep its distance from.
                    last_patch_pos = Some(cb.get_write_pos());
                },
                Insn::BoundaryPad => {
                    // A boundary is never patched, so it doesn't become a position to keep away
                    // from. The last patch point stays that, and the pad just gave it its room.
                    emit_pad_after_patch_point(cb, last_patch_pos);
                },
                Insn::IncrCounter { mem, value } => {
                    // Try to increment the counter until nothing else has written to it between
                    // the load and the store. Neither takes a displacement, and EMIT_OPND can't
                    // be the base of the address, so it's the index, and the sum is in SCRATCH1_OPND.
                    let sum: PPC64Opnd = SCRATCH1_OPND.into();
                    li(cb, Self::EMIT_OPND, PPC64Opnd::new_imm(0));
                    ldarx(cb, sum, mem.into(), Self::EMIT_OPND);
                    match imm_opnd_value(value) {
                        Some(imm) => addi(cb, sum, sum, PPC64Opnd::new_imm(imm)),
                        None => add(cb, sum, sum, value.into()),
                    }
                    stdcx_dot(cb, sum, mem.into(), Self::EMIT_OPND);
                    bc(cb, Condition::NE, -3 * 4);
                },
                Insn::Breakpoint => {
                    trap(cb);
                },
                Insn::Abort => {
                    // All zeros is an illegal instruction
                    cb.write_bytes(&[0; 4]);
                },
                Insn::CSelZ { truthy, falsy, out } |
                Insn::CSelE { truthy, falsy, out } => {
                    isel(cb, out.into(), truthy.into(), falsy.into(), Condition::EQ);
                },
                Insn::CSelNZ { truthy, falsy, out } |
                Insn::CSelNE { truthy, falsy, out } => {
                    isel(cb, out.into(), truthy.into(), falsy.into(), Condition::NE);
                },
                Insn::CSelL { truthy, falsy, out } => {
                    isel(cb, out.into(), truthy.into(), falsy.into(), Condition::LT);
                },
                Insn::CSelLE { truthy, falsy, out } => {
                    isel(cb, out.into(), truthy.into(), falsy.into(), Condition::LE);
                },
                Insn::CSelG { truthy, falsy, out } => {
                    isel(cb, out.into(), truthy.into(), falsy.into(), Condition::GT);
                },
                Insn::CSelGE { truthy, falsy, out } => {
                    isel(cb, out.into(), truthy.into(), falsy.into(), Condition::GE);
                }
            };

            insn_idx += 1;
        }

        // Error if we couldn't write out everything
        if cb.has_dropped_bytes() {
            Err(CompileError::OutOfMemory)
        } else if last_patch_pos.is_some() && !b_offset_fits((cb.get_write_pos() - start_pos) as i64) {
            // A patch point only has room for a b to get to its side exit
            Err(CompileError::LabelLinkingFailure)
        } else {
            // No bytes dropped, so the pos markers point to valid code
            for (insn_idx, pos) in pos_markers {
                if let Insn::PosMarker(callback) = insns.get(insn_idx).unwrap() {
                    callback(pos, cb);
                } else {
                    panic!("non-PosMarker in pos_markers insn_idx={insn_idx} {self:?}");
                }
            }

            Ok(gc_offsets)
        }
    }

    /// Optimize and compile the stored instructions
    pub fn compile_with_regs(self, cb: &mut CodeBlock, regs: Vec<Reg>) -> Result<(CodePtr, Vec<CodePtr>), CompileError> {
        // The backend is allowed to use scratch registers only if it has not accepted them so far.
        let use_scratch_reg = !self.accept_scratch_reg;
        let mut regs = RegPool::new(regs);
        asm_dump!(self, init);

        let mut asm = trace_compile_phase("split", || self.ppc64_split());

        asm_dump!(asm, split);

        trace_compile_phase("regalloc", || {
            trace_compile_phase("number_instructions", || asm.number_instructions(0));

            let live_in = trace_compile_phase("analyze_liveness", || asm.analyze_liveness());
            let mut intervals = trace_compile_phase("build_intervals", || asm.build_intervals(live_in));

            // Dump live intervals if requested
            if let Some(crate::options::Options { dump_lir: Some(dump_lirs), .. }) = unsafe { crate::options::OPTIONS.as_ref() } {
                if dump_lirs.contains(&crate::options::DumpLIR::live_intervals) {
                    println!("LIR live_intervals:\n{}", crate::backend::lir::debug_intervals(&asm, &intervals));
                }
            }

            trace_compile_phase("preferred_registers", || asm.preferred_register_assignments(&mut intervals, &mut regs));
            let num_stack_slots = trace_compile_phase("linear_scan", || asm.linear_scan(&intervals, &regs));

            asm.stack_state.num_spill_slots = num_stack_slots;
            asm.stack_state.num_side_exit_stack_map_slots = asm.side_exit_stack_map_slots(&intervals);
            let stack_slot_count = asm.stack_state.stack_slot_count();
            if stack_slot_count > Self::MAX_FRAME_STACK_SLOTS {
                return Err(CompileError::NativeStackTooLarge);
            }

            // Dump vreg-to-physical-register mapping if requested
            if let Some(crate::options::Options { dump_lir: Some(dump_lirs), .. }) = unsafe { crate::options::OPTIONS.as_ref() } {
                if dump_lirs.contains(&crate::options::DumpLIR::alloc_regs) {
                    println!("LIR live_intervals:\n{}", crate::backend::lir::debug_intervals(&asm, &intervals));

                    println!("VReg assignments:");
                    for (i, interval) in intervals.iter().enumerate() {
                        if let Some(alloc) = interval.assigned.get() {
                            let alloc_str = match alloc {
                                Allocation::Reg(n) => format!("{}", regs.reg_at(n)),
                                Allocation::Stack(n) => format!("Stack[{}]", n),
                            };
                            println!("  v{} => {} (ranges: {})", i, alloc_str, interval.ranges_string());
                        }
                    }
                }
            }

            // Update FrameSetup slot_count now that StackState knows the
            // register allocator spill and side-exit capture counts.
            trace_compile_phase("count_stack_slots", || {
                for block in asm.basic_blocks.iter_mut() {
                    for insn in block.insns.iter_mut() {
                        if let Insn::FrameSetup { slot_count, .. } = insn {
                            *slot_count = stack_slot_count;
                        }
                    }
                }
            });

            trace_compile_phase("resolve_ssa", || {
                asm.handle_caller_saved_regs(&intervals, &regs, &C_ARG_REGREGS);
                asm.resolve_ssa(&intervals, &regs);
            });

            Ok(())
        })?;
        asm_dump!(asm, alloc_regs);

        // We are moved out of SSA after resolve_ssa

        // We put compile_exits after alloc_regs to avoid extending live ranges for VRegs spilled on side exits.
        // Exit code is compiled into a separate list of instructions that we append
        // to the last reachable block before scratch_split, so it gets linearized and split.
        trace_compile_phase("compile_exits", || {
            let exit_insns = asm.compile_exits();

            // Append exit instructions to the last reachable block so they are
            // included in linearize_instructions and processed by scratch_split.
            if let Some(&last_block) = asm.block_order().last() {
                for insn in exit_insns {
                    asm.basic_blocks[last_block.0].insns.push(insn);
                    asm.basic_blocks[last_block.0].insn_ids.push(None);
                }
            }
        });
        asm_dump!(asm, compile_exits);

        if use_scratch_reg {
            asm = trace_compile_phase("scratch_split", || asm.ppc64_scratch_split());
            asm_dump!(asm, scratch_split);
        } else {
            // For trampolines that use scratch registers, resolve ParallelMov without scratch_reg.
            asm = trace_compile_phase("resolve_parallel_mov", || asm.resolve_parallel_mov_pass());
            asm_dump!(asm, resolve_parallel_mov);
        }

        trace_compile_phase("emit", || {
            // Create label instances in the code block
            for (idx, name) in asm.label_names.iter().enumerate() {
                let label = cb.new_label(*name);
                assert_eq!(label, Label(idx));
            }

            let start_ptr = cb.get_write_ptr();
            let gc_offsets = asm.ppc64_emit(cb).inspect_err(|_| cb.clear_labels())?;
            assert!(!cb.has_dropped_bytes(), "emit should not drop bytes without error");

            cb.link_labels().or(Err(CompileError::LabelLinkingFailure))?;

            trace_compile_phase("invalidate_icache", || {
                // Invalidate icache for newly written out region so we don't run stale code.
                unsafe { rb_jit_icache_invalidate(start_ptr.raw_ptr(cb) as _, cb.get_write_ptr().raw_ptr(cb) as _) };
            });

            if crate::state::ZJITState::has_instance() {
                crate::stats::incr_counter_by(crate::stats::Counter::total_native_stack_bytes, (asm.stack_state.stack_slot_count() * (VALUE_BITS as usize)).try_into().unwrap());
            }
            Ok((start_ptr, gc_offsets))
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::assert_disasm_snapshot;

    use super::*;
    use insta::assert_snapshot;

    fn setup_asm() -> (Assembler, CodeBlock) {
        crate::options::rb_zjit_prepare_options(); // Allow `get_option!` in Assembler
        let mut asm = Assembler::new();
        asm.new_block_without_id("test");
        (asm, CodeBlock::new_dummy())
    }

    fn setup_asm_with_scratch_reg() -> (Assembler, CodeBlock, Opnd) {
        crate::options::rb_zjit_prepare_options(); // Allow `get_option!` in Assembler
        let (mut asm, scratch_reg) = Assembler::new_with_scratch_reg();
        asm.new_block_without_id("test");
        (asm, CodeBlock::new_dummy(), scratch_reg)
    }

    /// Disassembly without what depends on where the code is: the offset of each
    /// instruction, and the instructions that load an absolute address into reg
    #[cfg(feature = "disasm")]
    fn stable_disasm(cb: &CodeBlock, reg: &str) -> String {
        let load_prefixes = ["li", "lis", "ori", "oris", "sldi"].map(|mnemonic| format!("{mnemonic} {reg},"));
        let mut lines: Vec<String> = vec![];
        for line in cb.disasm().lines() {
            let insn = line.split_once(": ").map_or(line, |(_, insn)| insn);
            if load_prefixes.iter().any(|prefix| insn.starts_with(prefix.as_str())) {
                let load = format!("load {reg}, address");
                if lines.last() != Some(&load) {
                    lines.push(load);
                }
            } else {
                lines.push(insn.to_string());
            }
        }
        lines.join("\n")
    }

    #[test]
    fn test_emit_bake_string() {
        let (mut asm, mut cb) = setup_asm();

        asm.bake_string("Hello, world!");
        asm.compile_with_num_regs(&mut cb, 0);

        // Testing that we pad the string to the nearest 4-byte boundary to make
        // it easier to jump over.
        assert_snapshot!(cb.hexdump(), @"48656c6c6f2c20776f726c6421000000");
    }

    #[test]
    fn test_mov_reg() {
        let (mut asm, mut cb) = setup_asm();

        asm.mov(Opnd::Reg(R3_REG), Opnd::Reg(R4_REG));
        // Moving 32 bits leaves zero in the upper 32 bits
        asm.mov(Opnd::Reg(R3_REG).with_num_bits(32), Opnd::Reg(R4_REG).with_num_bits(32));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: mr r3, r4
        0x4: clrldi r3, r4, 0x20
        ");
        assert_snapshot!(cb.hexdump(), @"7823837c20008378");
    }

    #[test]
    fn test_emit_load_value_16_bits() {
        let (mut asm, mut cb) = setup_asm();

        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0));
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x7fff));
        asm.mov(Opnd::Reg(R3_REG), Opnd::Imm(-0x8000));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r3, 0
        0x4: li r3, 0x7fff
        0x8: li r3, -0x8000
        ");
        assert_snapshot!(cb.hexdump(), @"00006038ff7f603800806038");
    }

    #[test]
    fn test_emit_load_value_32_bits() {
        let (mut asm, mut cb) = setup_asm();

        // ori isn't necessary when the lower 16 bits are zero
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x1234_0000));
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x1234_5678));
        // li sign-extends, so a value with bit 15 set takes lis
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x8000));
        asm.mov(Opnd::Reg(R3_REG), Opnd::Imm(-0x1234_5678));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: lis r3, 0x1234
        0x4: lis r3, 0x1234
        0x8: ori r3, r3, 0x5678
        0xc: lis r3, 0
        0x10: ori r3, r3, 0x8000
        0x14: lis r3, -0x1235
        0x18: ori r3, r3, 0xa988
        ");
        assert_snapshot!(cb.hexdump(), @"3412603c3412603c785663600000603c00806360cbed603c88a96360");
    }

    #[test]
    fn test_emit_load_value_unsigned_32_bits() {
        let (mut asm, mut cb) = setup_asm();

        // lis sign-extends, so a value with bit 31 set starts from zero
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x8000_0000));
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0xffff_ffff));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r3, 0
        0x4: oris r3, r3, 0x8000
        0x8: li r3, 0
        0xc: oris r3, r3, 0xffff
        0x10: ori r3, r3, 0xffff
        ");
        assert_snapshot!(cb.hexdump(), @"000060380080636400006038ffff6364ffff6360");
    }

    #[test]
    fn test_emit_load_value_64_bits() {
        let (mut asm, mut cb) = setup_asm();

        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x1_0000_0000));
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x7fff_1234_5678));
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x1234_5678_9abc_def0));
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0xffff_ffff_0000_0001));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r3, 1
        0x4: sldi r3, r3, 0x20
        0x8: li r3, 0x7fff
        0xc: sldi r3, r3, 0x20
        0x10: oris r3, r3, 0x1234
        0x14: ori r3, r3, 0x5678
        0x18: lis r3, 0x1234
        0x1c: ori r3, r3, 0x5678
        0x20: sldi r3, r3, 0x20
        0x24: oris r3, r3, 0x9abc
        0x28: ori r3, r3, 0xdef0
        0x2c: li r3, -1
        0x30: sldi r3, r3, 0x20
        0x34: ori r3, r3, 1
        ");
        assert_snapshot!(cb.hexdump(), @"01006038c6076378ff7f6038c607637834126364785663603412603c78566360c6076378bc9a6364f0de6360ffff6038c607637801006360");
    }

    #[test]
    fn test_emit_load_value_isa_3_1() {
        TEST_POWER_ISA.set(PowerIsa::V3_1);
        let (mut asm, mut cb) = setup_asm();

        // pli is for what takes more than 2 instructions and fits in 34 bits.
        // (Capstone doesn't decode prefixed instructions. .byte is the prefix, and
        // what follows is the suffix, which has the low 16 bits of the immediate.)
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x1234_5678));
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0xffff_ffff));
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x1_ffff_ffff));
        asm.mov(Opnd::Reg(R3_REG), Opnd::Imm(-0x2_0000_0000));
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x2_0000_0000));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: lis r3, 0x1234
        0x4: ori r3, r3, 0x5678
        0x8: .byte 0xff, 0xff, 0x00, 0x06
        0xc: li r3, -1
        0x10: .byte 0xff, 0xff, 0x01, 0x06
        0x14: li r3, -1
        0x18: .byte 0x00, 0x00, 0x02, 0x06
        0x1c: li r3, 0
        0x20: li r3, 2
        0x24: sldi r3, r3, 0x20
        ");
        assert_snapshot!(cb.hexdump(), @"3412603c78566360ffff0006ffff6038ffff0106ffff6038000002060000603802006038c6076378");
    }

    #[test]
    fn test_emit_prefixed_at_64_byte_boundary() {
        TEST_POWER_ISA.set(PowerIsa::V3_1);
        let (mut asm, mut cb) = setup_asm();

        // A prefixed instruction must not cross a 64-byte boundary, so a nop goes
        // in the last word before one
        for _ in 0..15 {
            asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0));
        }
        asm.mov(Opnd::Reg(R3_REG), Opnd::UImm(0x1_0000_0000));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm().lines().skip(15).collect::<Vec<_>>().join("\n"), @"
        0x3c: nop
        0x40: .byte 0x00, 0x00, 0x01, 0x06
        0x44: li r3, 0
        ");
        assert_eq!(18 * 4, cb.get_write_pos());
    }

    #[test]
    fn test_emit_load_gc_value_isa_3_1() {
        TEST_POWER_ISA.set(PowerIsa::V3_1);
        let (mut asm, mut cb) = setup_asm();

        // The value is in the code for GC to update, and pld reads it from there.
        // (The two .byte after it are pld.)
        let opnd = asm.load(Opnd::Value(VALUE(0xCAFECAFECAFE0000)));
        asm.store(Opnd::mem(64, SP, 0), opnd);
        asm.compile_with_num_regs(&mut cb, 1);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: b 0xc
        0x4: lfd f23, 0(r30)
        0x8: lfd f23, -0x3502(r30)
        0xc: .byte 0xff, 0xff, 0x13, 0x04
        0x10: .byte 0xf8, 0xff, 0x60, 0xe4
        0x14: std r3, 0(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"0c0000480000fecafecafecaffff1304f8ff60e400007ef8");
    }

    #[test]
    fn test_emit_load_value_num_insns() {
        let mut cb = CodeBlock::new_dummy();

        for (num_insns, value) in [(1, 0), (1, u64::MAX), (2, 0x1234_5678), (3, 0xffff_ffff), (5, 0x1234_5678_9abc_def0)] {
            let start = cb.get_write_pos();
            assert_eq!(num_insns, emit_load_value(&mut cb, R3, value));
            assert_eq!(num_insns * 4, cb.get_write_pos() - start);
        }
    }

    #[test]
    fn test_emit_load_mem() {
        let (mut asm, mut cb) = setup_asm();

        asm.load_into(Opnd::Reg(R3_REG), Opnd::mem(64, SP, 8));
        asm.load_into(Opnd::Reg(R3_REG), Opnd::mem(32, SP, -4));
        asm.load_into(Opnd::Reg(R3_REG), Opnd::mem(16, SP, 2));
        asm.load_into(Opnd::Reg(R3_REG), Opnd::mem(8, SP, 1));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r3, 8(r30)
        0x4: lwz r3, -4(r30)
        0x8: lhz r3, 2(r30)
        0xc: lbz r3, 1(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"08007ee8fcff7e8002007ea001007e88");
    }

    #[test]
    fn test_emit_load_sext() {
        let (mut asm, mut cb) = setup_asm();

        asm.push_insn(Insn::LoadSExt { opnd: Opnd::Reg(R4_REG).with_num_bits(32), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::LoadSExt { opnd: Opnd::mem(32, SP, 4), out: Opnd::Reg(R3_REG) });
        // lwa can't encode a displacement that isn't a multiple of 4
        asm.push_insn(Insn::LoadSExt { opnd: Opnd::mem(32, SP, 2), out: Opnd::Reg(R3_REG) });
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: extsw r3, r4
        0x4: lwa r3, 4(r30)
        0x8: lwz r3, 2(r30)
        0xc: extsw r3, r3
        ");
        assert_snapshot!(cb.hexdump(), @"b407837c06007ee802007e80b407637c");
    }

    #[test]
    fn test_store() {
        let (mut asm, mut cb) = setup_asm();

        asm.store(Opnd::mem(64, SP, 8), Opnd::Reg(R3_REG));
        asm.store(Opnd::mem(32, SP, 4), Opnd::Reg(R3_REG));
        asm.store(Opnd::mem(16, SP, 2), Opnd::Reg(R3_REG));
        asm.store(Opnd::mem(8, SP, 1), Opnd::Reg(R3_REG));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: std r3, 8(r30)
        0x4: stw r3, 4(r30)
        0x8: sth r3, 2(r30)
        0xc: stb r3, 1(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"08007ef804007e9002007eb001007e98");
    }

    #[test]
    fn test_store_immediate() {
        let (mut asm, mut cb) = setup_asm();

        // Immediates go through r0, even zero
        asm.store(Opnd::mem(64, SP, 0), Opnd::UImm(0));
        asm.store(Opnd::mem(64, SP, 8), Opnd::UImm(0x1234_5678));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r0, 0
        0x4: std r0, 0(r30)
        0x8: lis r0, 0x1234
        0xc: ori r0, r0, 0x5678
        0x10: std r0, 8(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"0000003800001ef83412003c7856006008001ef8");
    }

    #[test]
    fn test_store_mem_to_mem() {
        let (mut asm, mut cb) = setup_asm();

        asm.store(Opnd::mem(64, SP, 0), Opnd::mem(64, CFP, 8));
        asm.store(Opnd::mem(32, SP, 8), Opnd::mem(32, CFP, 16));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r0, 8(r28)
        0x4: std r0, 0(r30)
        0x8: lwz r0, 0x10(r28)
        0xc: stw r0, 8(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"08001ce800001ef810001c8008001e90");
    }

    #[test]
    fn test_emit_lea() {
        let (mut asm, mut cb) = setup_asm();

        // Fits in the immediate of addi
        asm.lea_into(Opnd::Reg(R3_REG), Opnd::mem(64, SP, 0x7ff8));
        // addis for the upper half, addi for the lower half
        asm.lea_into(Opnd::Reg(R3_REG), Opnd::mem(64, SP, 0x12_3458));
        // The lower half is negative, so the upper half is one more
        asm.lea_into(Opnd::Reg(R3_REG), Opnd::mem(64, SP, 0x8000));
        // The lower half is zero
        asm.lea_into(Opnd::Reg(R3_REG), Opnd::mem(64, SP, -0x10000));
        // The output register can be the base
        asm.lea_into(SP, Opnd::mem(64, SP, -0x12_3458));
        // Too large for addis
        asm.lea_into(Opnd::Reg(R3_REG), Opnd::mem(64, SP, i32::MAX));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: addi r3, r30, 0x7ff8
        0x4: addis r3, r30, 0x12
        0x8: addi r3, r3, 0x3458
        0xc: addis r3, r30, 1
        0x10: addi r3, r3, -0x8000
        0x14: addis r3, r30, -1
        0x18: addis r30, r30, -0x12
        0x1c: addi r30, r30, -0x3458
        0x20: lis r0, 0x7fff
        0x24: ori r0, r0, 0xffff
        0x28: add r3, r30, r0
        ");
        assert_snapshot!(cb.hexdump(), @"f87f7e3812007e3c5834633801007e3c00806338ffff7e3ceeffde3fa8cbde3bff7f003cffff006014027e7c");
    }

    #[test]
    fn test_patch_point_pad() {
        let (mut asm, mut cb) = setup_asm();

        // The second patch point leaves room for a jump written at the first one
        asm.push_insn(Insn::PatchPointPad);
        asm.push_insn(Insn::PatchPointPad);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: nop
        0x4: nop
        ");
        assert_snapshot!(cb.hexdump(), @"0000006000000060");
    }

    #[test]
    fn test_b_reaches_code_region_at_range_boundary() {
        // The widest branch in a 32MiB region spans 32MiB - 4 bytes, which is
        // still the largest offset a b can encode.
        let cb = CodeBlock::new_dummy_sized(32 * 1024 * 1024);
        assert!(cb.b_reaches_code_region());

        // One instruction further and a b can no longer span the region
        let cb = CodeBlock::new_dummy_sized(32 * 1024 * 1024 + 4);
        assert!(!cb.b_reaches_code_region());
    }

    #[test]
    fn test_conditional_branch_to_label() {
        let (mut asm, mut cb) = setup_asm();
        let start = asm.new_label("start");
        let forward = asm.new_label("forward");

        asm.write_label(start.clone());
        asm.jg(forward.clone());
        asm.push_insn(Insn::Jl(start.clone()));
        asm.write_label(forward);
        asm.compile_with_num_regs(&mut cb, 0);

        // Each has room for a second instruction, in case bc can't reach the label
        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: bgt 0x10
        0x4: nop
        0x8: blt 0
        0xc: nop
        ");
        assert_snapshot!(cb.hexdump(), @"1000814100000060f8ff804100000060");
    }

    #[test]
    #[cfg(feature = "disasm")]
    fn test_conditional_branch_to_label_out_of_bc_range() {
        let (mut asm, mut cb) = setup_asm();
        let start = asm.new_label("start");
        let forward = asm.new_label("forward");

        asm.write_label(start.clone());
        asm.push_insn(Insn::Je(forward.clone()));
        // 32KiB of instructions, which bc can't jump over
        for _ in 0..(1 << 13) {
            asm.mov(Opnd::Reg(R3_REG), Opnd::Reg(R4_REG));
        }
        asm.push_insn(Insn::Jne(start));
        asm.write_label(forward);
        asm.compile_with_num_regs(&mut cb, 0);

        // bne skips over a b, which can
        let disasm = cb.disasm();
        let lines: Vec<&str> = disasm.lines().collect();
        assert_disasm_snapshot!(lines[..2].join("\n"), @"
        0x0: bne 8
        0x4: b 0x8010
        ");
        // Jumping backwards is the same
        assert_disasm_snapshot!(lines[lines.len() - 2..].join("\n"), @"
        0x8008: beq 0x8010
        0x800c: b 0
        ");
    }

    #[test]
    fn test_emit_je_fits_into_bc() {
        let (mut asm, mut cb) = setup_asm();

        let target: CodePtr = cb.get_write_ptr().add_bytes(80);

        asm.push_insn(Insn::Je(Target::CodePtr(target)));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"  0x0: beq 0x50");
        assert_snapshot!(cb.hexdump(), @"50008241");
    }

    #[test]
    fn test_emit_je_does_not_fit_into_bc() {
        let (mut asm, mut cb) = setup_asm();

        let offset = 1 << 15;
        let target: CodePtr = cb.get_write_ptr().add_bytes(offset);

        asm.push_insn(Insn::Je(Target::CodePtr(target)));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: bne 8
        0x4: b 0x8000
        ");
        assert_snapshot!(cb.hexdump(), @"08008240fc7f0048");
    }

    #[test]
    #[cfg(feature = "disasm")]
    fn test_emit_je_out_of_b_range() {
        let (mut asm, _) = setup_asm();
        let mut cb = CodeBlock::new_dummy_sized(128 * 1024 * 1024);

        let target = cb.get_write_ptr().add_bytes(1 << 26);
        asm.push_insn(Insn::Je(Target::CodePtr(target)));
        asm.compile_with_num_regs(&mut cb, 0);

        // A b can't reach 64MiB away, so the absolute address must be loaded for bctr.
        // The load is padded so that bne knows how far to skip.
        assert_eq!(8 * 4, cb.get_write_pos());
        let disasm = stable_disasm(&cb, "r0").replace("nop\n", "");
        assert_disasm_snapshot!(disasm, @"
        bne 0x20
        load r0, address
        mtctr r0
        bctr
        ");
    }

    #[test]
    fn test_emit_jumps_out_of_b_range_isa_3_1() {
        TEST_POWER_ISA.set(PowerIsa::V3_1);
        let (mut asm, _) = setup_asm();
        let mut cb = CodeBlock::new_dummy_sized(128 * 1024 * 1024);

        // pla, which is .byte and li, gets the address from how far away it is.
        // The one for the conditional jump is still padded for bne to know how
        // far to skip.
        let target = cb.get_write_ptr().add_bytes(1 << 26);
        asm.jmp(Target::CodePtr(target));
        asm.push_insn(Insn::Je(Target::CodePtr(target)));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: .byte 0x00, 0x04, 0x10, 0x06
        0x4: li r0, 0
        0x8: mtctr r0
        0xc: bctr
        0x10: bne 0x30
        0x14: .byte 0xff, 0x03, 0x10, 0x06
        0x18: li r0, -0x14
        0x1c: nop
        0x20: nop
        0x24: nop
        0x28: mtctr r0
        0x2c: bctr
        ");
        assert_snapshot!(cb.hexdump(), @"0004100600000038a603097c2004804e20008240ff031006ecff0038000000600000006000000060a603097c2004804e");
    }

    #[test]
    fn test_emit_jmp_ptr() {
        let (mut asm, mut cb) = setup_asm();

        let target = cb.get_write_ptr().add_bytes(80);
        asm.jmp(Target::CodePtr(target));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"  0x0: b 0x50");
        assert_snapshot!(cb.hexdump(), @"50000048");
    }

    #[test]
    #[cfg(feature = "disasm")]
    fn test_emit_jmp_ptr_out_of_b_range() {
        let (mut asm, _) = setup_asm();
        let mut cb = CodeBlock::new_dummy_sized(128 * 1024 * 1024);

        let target = cb.get_write_ptr().add_bytes(1 << 26);
        asm.jmp(Target::CodePtr(target));
        asm.compile_with_num_regs(&mut cb, 0);

        // A b can't reach 64MiB away, so the absolute address must be loaded for bctr.
        assert_disasm_snapshot!(stable_disasm(&cb, "r0"), @"
        load r0, address
        mtctr r0
        bctr
        ");
    }

    #[test]
    fn test_emit_jmp_opnd() {
        let (mut asm, mut cb, scratch_reg) = setup_asm_with_scratch_reg();

        asm.jmp_opnd(scratch_reg);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: mtctr r11
        0x4: bctr
        ");
        assert_snapshot!(cb.hexdump(), @"a603697d2004804e");
    }

    #[test]
    fn test_emit_joz_jonz() {
        let (mut asm, mut cb) = setup_asm();

        let target: CodePtr = cb.get_write_ptr().add_bytes(80);

        // There is no compare-and-branch, so these compare with zero first
        asm.push_insn(Insn::Joz(Opnd::Reg(R3_REG), Target::CodePtr(target)));
        asm.push_insn(Insn::Jonz(Opnd::Reg(R3_REG), Target::CodePtr(target)));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: cmpdi r3, 0
        0x4: beq 0x50
        0x8: cmpdi r3, 0
        0xc: bne 0x50
        ");
        assert_snapshot!(cb.hexdump(), @"0000232c4c0082410000232c44008240");
    }

    #[test]
    #[cfg(feature = "disasm")]
    fn test_emit_lea_label() {
        let (mut asm, mut cb) = setup_asm();

        let label = asm.new_label("label");
        asm.push_insn(Insn::LeaJumpTarget { out: Opnd::Reg(R3_REG), target: label.clone() });
        asm.write_label(label);
        asm.bake_string("Hello, world!");
        asm.compile_with_num_regs(&mut cb, 0);

        // The address we are at, plus the distance to the label
        let disasm = stable_disasm(&cb, "r3");
        let lea: Vec<&str> = disasm.lines().take(3).collect();
        assert!(lea[2].starts_with("addi r3, r3, "), "the distance depends on what the load takes: {}", lea[2]);
        assert_disasm_snapshot!(lea[..2].join("\n"), @"
        load r3, address
        addis r3, r3, 0
        ");
    }

    #[test]
    fn test_emit_add() {
        let (mut asm, mut cb) = setup_asm();

        asm.push_insn(Insn::Add { left: Opnd::Reg(R4_REG), right: Opnd::Reg(R5_REG), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::Add { left: Opnd::Reg(R4_REG), right: Opnd::Imm(-8), out: Opnd::Reg(R3_REG) });
        // An immediate that addi can't encode goes through r0
        asm.push_insn(Insn::Add { left: Opnd::Reg(R4_REG), right: Opnd::UImm(0x8000), out: Opnd::Reg(R3_REG) });
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: add r3, r4, r5
        0x4: addi r3, r4, -8
        0x8: lis r0, 0
        0xc: ori r0, r0, 0x8000
        0x10: add r3, r4, r0
        ");
        assert_snapshot!(cb.hexdump(), @"142a647cf8ff64380000003c008000601402647c");
    }

    #[test]
    fn test_emit_sub() {
        let (mut asm, mut cb) = setup_asm();

        // subf takes its operands the other way around
        asm.push_insn(Insn::Sub { left: Opnd::Reg(R4_REG), right: Opnd::Reg(R5_REG), out: Opnd::Reg(R3_REG) });
        // Subtracting an immediate is adding its negation
        asm.push_insn(Insn::Sub { left: Opnd::Reg(R4_REG), right: Opnd::UImm(1), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::Sub { left: Opnd::Reg(R4_REG), right: Opnd::Imm(-0x8000), out: Opnd::Reg(R3_REG) });
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: subf r3, r5, r4
        0x4: addi r3, r4, -1
        0x8: li r0, -0x8000
        0xc: subf r3, r0, r4
        ");
        assert_snapshot!(cb.hexdump(), @"5020657cffff6438008000385020607c");
    }

    #[test]
    fn test_emit_add_32_bits() {
        let (mut asm, mut cb) = setup_asm();

        let (out, left, right) = (Opnd::Reg(R3_REG).with_num_bits(32), Opnd::Reg(R4_REG).with_num_bits(32), Opnd::Reg(R5_REG).with_num_bits(32));
        asm.push_insn(Insn::Add { left, right, out });
        asm.compile_with_num_regs(&mut cb, 0);

        // The upper 32 bits of the sum are cleared
        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: add r3, r4, r5
        0x4: clrldi r3, r3, 0x20
        ");
        assert_snapshot!(cb.hexdump(), @"142a647c20006378");
    }

    #[test]
    fn test_sp_movements_are_single_instruction() {
        let (mut asm, mut cb) = setup_asm();

        asm.add_into(NATIVE_STACK_PTR, 0x20.into());
        asm.sub_into(NATIVE_STACK_PTR, 0x20.into());
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r0, 0(r1)
        0x4: stdu r0, 0x20(r1)
        0x8: addi r1, r1, -0x20
        ");
        assert_snapshot!(cb.hexdump(), @"000001e8210001f8e0ff2138");
    }

    #[test]
    fn test_emit_add_with_overflow_check() {
        let (mut asm, mut cb) = setup_asm();
        let overflow = asm.new_label("overflow");

        // XER is cleared first, since its overflow bit is sticky. (.byte is addo.)
        asm.push_insn(Insn::Add { left: Opnd::Reg(R4_REG), right: Opnd::Reg(R5_REG), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::Jo(overflow.clone()));
        // An immediate goes through r0 after r0 has cleared XER
        asm.push_insn(Insn::Add { left: Opnd::Reg(R4_REG), right: Opnd::Imm(1), out: Opnd::Reg(R3_REG) });
        // The result may get stored before the check
        asm.store(Opnd::mem(64, SP, 0), Opnd::Reg(R3_REG));
        asm.push_insn(Insn::Jo(overflow.clone()));
        asm.write_label(overflow);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r0, 0
        0x4: mtxer r0
        0x8: .byte 0x15, 0x2e, 0x64, 0x7c
        0xc: bso 0x30
        0x10: nop
        0x14: li r0, 0
        0x18: mtxer r0
        0x1c: li r0, 1
        0x20: .byte 0x15, 0x06, 0x64, 0x7c
        0x24: std r3, 0(r30)
        0x28: bso 0x30
        0x2c: nop
        ");
        assert_snapshot!(cb.hexdump(), @"00000038a603017c152e647c240083410000006000000038a603017c010000381506647c00007ef80800834100000060");
    }

    #[test]
    fn test_emit_overflow_check_isa_3_0() {
        TEST_POWER_ISA.set(PowerIsa::V3_0);
        let (mut asm, mut cb) = setup_asm();
        let overflow = asm.new_label("overflow");

        // OV of XER is not sticky, and mcrxrx copies it to LT of CR0.
        // (.byte is addo, subfo and mulldo.)
        asm.push_insn(Insn::Add { left: Opnd::Reg(R4_REG), right: Opnd::Reg(R5_REG), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::Jo(overflow.clone()));
        asm.push_insn(Insn::Add { left: Opnd::Reg(R4_REG), right: Opnd::Imm(1), out: Opnd::Reg(R3_REG) });
        asm.store(Opnd::mem(64, SP, 0), Opnd::Reg(R3_REG));
        asm.push_insn(Insn::Jo(overflow.clone()));
        asm.push_insn(Insn::Sub { left: Opnd::Reg(R4_REG), right: Opnd::Reg(R5_REG), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::Jo(overflow.clone()));
        asm.push_insn(Insn::Mul { left: Opnd::Reg(R4_REG), right: Opnd::Reg(R5_REG), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::JoMul(overflow.clone()));
        asm.push_insn(Insn::LShift { opnd: Opnd::Reg(R4_REG), shift: Opnd::UImm(1), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::Jo(overflow.clone()));
        asm.write_label(overflow);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: .byte 0x14, 0x2e, 0x64, 0x7c
        0x4: mcrxrx cr0
        0x8: blt 0x58
        0xc: nop
        0x10: li r0, 1
        0x14: .byte 0x14, 0x06, 0x64, 0x7c
        0x18: mcrxrx cr0
        0x1c: std r3, 0(r30)
        0x20: blt 0x58
        0x24: nop
        0x28: .byte 0x50, 0x24, 0x65, 0x7c
        0x2c: mcrxrx cr0
        0x30: blt 0x58
        0x34: nop
        0x38: .byte 0xd2, 0x2d, 0x64, 0x7c
        0x3c: mcrxrx cr0
        0x40: blt 0x58
        0x44: nop
        0x48: .byte 0x14, 0x26, 0x64, 0x7c
        0x4c: mcrxrx cr0
        0x50: blt 0x58
        0x54: nop
        ");
        assert_snapshot!(cb.hexdump(), @"142e647c8004007c5000804100000060010000381406647c8004007c00007ef838008041000000605024657c8004007c2800804100000060d22d647c8004007c18008041000000601426647c8004007c0800804100000060");
    }

    #[test]
    fn test_emit_sub_with_overflow_check() {
        let (mut asm, mut cb) = setup_asm();
        let overflow = asm.new_label("overflow");

        // (.byte is subfo.)
        asm.push_insn(Insn::Sub { left: Opnd::Reg(R4_REG), right: Opnd::Reg(R5_REG), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::Jo(overflow.clone()));
        asm.push_insn(Insn::Sub { left: Opnd::Reg(R4_REG), right: Opnd::Imm(1), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::Jo(overflow.clone()));
        asm.write_label(overflow);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r0, 0
        0x4: mtxer r0
        0x8: .byte 0x51, 0x24, 0x65, 0x7c
        0xc: bso 0x2c
        0x10: nop
        0x14: li r0, 0
        0x18: mtxer r0
        0x1c: li r0, 1
        0x20: .byte 0x51, 0x24, 0x60, 0x7c
        0x24: bso 0x2c
        0x28: nop
        ");
        assert_snapshot!(cb.hexdump(), @"00000038a603017c5124657c200083410000006000000038a603017c010000385124607c0800834100000060");
    }

    #[test]
    fn test_emit_mul() {
        let (mut asm, mut cb) = setup_asm();
        let overflow = asm.new_label("overflow");

        asm.push_insn(Insn::Mul { left: Opnd::Reg(R4_REG), right: Opnd::Reg(R5_REG), out: Opnd::Reg(R3_REG) });
        // (.byte is mulldo.)
        asm.push_insn(Insn::Mul { left: Opnd::Reg(R4_REG), right: Opnd::Reg(R5_REG), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::JoMul(overflow.clone()));
        asm.write_label(overflow);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: mulld r3, r4, r5
        0x4: li r0, 0
        0x8: mtxer r0
        0xc: .byte 0xd3, 0x2d, 0x64, 0x7c
        0x10: bso 0x18
        0x14: nop
        ");
        assert_snapshot!(cb.hexdump(), @"d229647c00000038a603017cd32d647c0800834100000060");
    }

    #[test]
    fn test_emit_and_or_xor() {
        let (mut asm, mut cb) = setup_asm();

        let (out, left, right) = (Opnd::Reg(R3_REG), Opnd::Reg(R4_REG), Opnd::Reg(R5_REG));
        asm.push_insn(Insn::And { left, right, out });
        asm.push_insn(Insn::Or { left, right, out });
        asm.push_insn(Insn::Xor { left, right, out });
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: and r3, r4, r5
        0x4: or r3, r4, r5
        0x8: xor r3, r4, r5
        ");
        assert_snapshot!(cb.hexdump(), @"3828837c782b837c782a837c");
    }

    #[test]
    fn test_emit_and_or_xor_with_immediate() {
        let (mut asm, mut cb) = setup_asm();

        let (out, left) = (Opnd::Reg(R3_REG), Opnd::Reg(R4_REG));
        asm.push_insn(Insn::And { left, right: Opnd::UImm(0xffff), out });
        asm.push_insn(Insn::Or { left, right: Opnd::UImm(1), out });
        asm.push_insn(Insn::Xor { left, right: Opnd::UImm(0x14), out });
        // These take unsigned 16-bit immediates. Anything else goes through r0.
        asm.push_insn(Insn::And { left, right: Opnd::Imm(-8), out });
        asm.push_insn(Insn::Or { left, right: Opnd::UImm(0x1_0000), out });
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: andi. r3, r4, 0xffff
        0x4: ori r3, r4, 1
        0x8: xori r3, r4, 0x14
        0xc: li r0, -8
        0x10: and r3, r4, r0
        0x14: lis r0, 1
        0x18: or r3, r4, r0
        ");
        assert_snapshot!(cb.hexdump(), @"ffff83700100836014008368f8ff00383800837c0100003c7803837c");
    }

    #[test]
    fn test_emit_not() {
        let (mut asm, mut cb) = setup_asm();

        asm.push_insn(Insn::Not { opnd: Opnd::Reg(R4_REG), out: Opnd::Reg(R3_REG) });
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"  0x0: nor r3, r4, r4");
        assert_snapshot!(cb.hexdump(), @"f820837c");
    }

    #[test]
    fn test_emit_shifts() {
        let (mut asm, mut cb) = setup_asm();

        let (out, opnd) = (Opnd::Reg(R3_REG), Opnd::Reg(R4_REG));
        asm.push_insn(Insn::LShift { opnd, shift: Opnd::UImm(3), out });
        asm.push_insn(Insn::RShift { opnd, shift: Opnd::UImm(3), out });
        asm.push_insn(Insn::URShift { opnd, shift: Opnd::UImm(3), out });
        asm.push_insn(Insn::LShift { opnd, shift: Opnd::Reg(R5_REG), out });
        asm.push_insn(Insn::RShift { opnd, shift: Opnd::Reg(R5_REG), out });
        asm.push_insn(Insn::URShift { opnd, shift: Opnd::Reg(R5_REG), out });
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: sldi r3, r4, 3
        0x4: sradi r3, r4, 3
        0x8: rldicl r3, r4, 0x3d, 3
        0xc: sld r3, r4, r5
        0x10: srad r3, r4, r5
        0x14: srd r3, r4, r5
        ");
        assert_snapshot!(cb.hexdump(), @"241f8378741e837cc2e883783628837c342e837c362c837c");
    }

    #[test]
    fn test_emit_lshift_with_overflow_check() {
        let (mut asm, mut cb) = setup_asm();
        let overflow = asm.new_label("overflow");

        // Shifting left by one bit is adding the value to itself. (.byte is addo.)
        asm.push_insn(Insn::LShift { opnd: Opnd::Reg(R4_REG), shift: Opnd::UImm(1), out: Opnd::Reg(R3_REG) });
        asm.push_insn(Insn::Jo(overflow.clone()));
        asm.write_label(overflow);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r0, 0
        0x4: mtxer r0
        0x8: .byte 0x15, 0x26, 0x64, 0x7c
        0xc: bso 0x14
        0x10: nop
        ");
        assert_snapshot!(cb.hexdump(), @"00000038a603017c1526647c0800834100000060");
    }

    #[test]
    fn test_byteswap() {
        let (mut asm, mut cb) = setup_asm();

        // Stored below SP and loaded back with the bytes reversed
        for num_bits in [64, 32, 16] {
            let opnd = Opnd::Reg(R4_REG).with_num_bits(num_bits);
            asm.push_insn(Insn::ByteSwap { opnd, out: Opnd::Reg(R3_REG).with_num_bits(num_bits) });
        }
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: std r4, -8(r1)
        0x4: li r0, -8
        0x8: ldbrx r3, r1, r0
        0xc: std r4, -8(r1)
        0x10: li r0, -8
        0x14: lwbrx r3, r1, r0
        0x18: std r4, -8(r1)
        0x1c: li r0, -8
        0x20: lhbrx r3, r1, r0
        ");
        assert_snapshot!(cb.hexdump(), @"f8ff81f8f8ff00382804617cf8ff81f8f8ff00382c04617cf8ff81f8f8ff00382c06617c");
    }

    #[test]
    fn test_emit_cmp_signed() {
        let (mut asm, mut cb) = setup_asm();
        let label = asm.new_label("label");

        asm.write_label(label.clone());
        asm.cmp(Opnd::Reg(R3_REG), Opnd::Reg(R4_REG));
        asm.push_insn(Insn::Jl(label.clone()));
        asm.cmp(Opnd::Reg(R3_REG), Opnd::Imm(-1));
        asm.push_insn(Insn::Jge(label.clone()));
        // An immediate that cmpdi can't encode goes through r0
        asm.cmp(Opnd::Reg(R3_REG), Opnd::UImm(0x8000));
        asm.jg(label.clone());
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: cmpd r3, r4
        0x4: blt 0
        0x8: nop
        0xc: cmpdi r3, -1
        0x10: bge 0
        0x14: nop
        0x18: lis r0, 0
        0x1c: ori r0, r0, 0x8000
        0x20: cmpd r3, r0
        0x24: bgt 0
        0x28: nop
        ");
        assert_snapshot!(cb.hexdump(), @"0020237cfcff804100000060ffff232cf0ff8040000000600000003c008000600000237cdcff814100000060");
    }

    #[test]
    fn test_emit_cmp_unsigned() {
        let (mut asm, mut cb) = setup_asm();
        let label = asm.new_label("label");

        // The jump after the cmp tells that the comparison is unsigned
        asm.write_label(label.clone());
        asm.cmp(Opnd::Reg(R3_REG), Opnd::Reg(R4_REG));
        asm.jb(label.clone());
        asm.cmp(Opnd::Reg(R3_REG), Opnd::UImm(0xffff));
        asm.push_insn(Insn::Jbe(label.clone()));
        // An immediate that cmpldi can't encode goes through r0
        asm.cmp(Opnd::Reg(R3_REG), Opnd::Imm(-1));
        asm.jb(label.clone());
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: cmpld r3, r4
        0x4: blt 0
        0x8: nop
        0xc: cmpldi r3, 0xffff
        0x10: ble 0
        0x14: nop
        0x18: li r0, -1
        0x1c: cmpld r3, r0
        0x20: blt 0
        0x24: nop
        ");
        assert_snapshot!(cb.hexdump(), @"4020237cfcff804100000060ffff2328f0ff814000000060ffff00384000237ce0ff804100000060");
    }

    #[test]
    fn test_emit_cmp_32_bits() {
        let (mut asm, mut cb) = setup_asm();
        let label = asm.new_label("label");

        let (left, right) = (Opnd::Reg(R3_REG).with_num_bits(32), Opnd::Reg(R4_REG).with_num_bits(32));
        asm.write_label(label.clone());
        asm.cmp(left, right);
        asm.push_insn(Insn::Jl(label.clone()));
        asm.cmp(left, Opnd::UImm(1));
        asm.jb(label.clone());
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: cmpw r3, r4
        0x4: blt 0
        0x8: nop
        0xc: cmplwi r3, 1
        0x10: blt 0
        0x14: nop
        ");
        assert_snapshot!(cb.hexdump(), @"0020037cfcff80410000006001000328f0ff804100000060");
    }

    #[test]
    fn test_emit_test() {
        let (mut asm, mut cb) = setup_asm();

        asm.test(Opnd::Reg(R3_REG), Opnd::Reg(R4_REG));
        asm.test(Opnd::Reg(R3_REG), Opnd::UImm(7));
        // An immediate that andi. can't encode goes through r0
        asm.test(Opnd::Reg(R3_REG), Opnd::Imm(-8));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: and. r0, r3, r4
        0x4: andi. r0, r3, 7
        0x8: li r0, -8
        0xc: and. r0, r3, r0
        ");
        assert_snapshot!(cb.hexdump(), @"3920607c07006070f8ff00383900607c");
    }

    #[test]
    fn test_emit_test_32_bits() {
        let (mut asm, mut cb) = setup_asm();

        // and. would set CR0 from all 64 bits of the result
        asm.test(Opnd::Reg(R3_REG).with_num_bits(32), Opnd::Reg(R4_REG).with_num_bits(32));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: and r0, r3, r4
        0x4: cmpwi r0, 0
        ");
        assert_snapshot!(cb.hexdump(), @"3820607c0000002c");
    }

    #[test]
    fn test_emit_csel() {
        let (mut asm, mut cb) = setup_asm();

        let (truthy, falsy, out) = (Opnd::Reg(R4_REG), Opnd::Reg(R5_REG), Opnd::Reg(R3_REG));
        asm.push_insn(Insn::CSelE { truthy, falsy, out });
        asm.push_insn(Insn::CSelL { truthy, falsy, out });
        asm.push_insn(Insn::CSelG { truthy, falsy, out });
        // isel only picks its first operand when a CR bit is set, so these swap them
        asm.push_insn(Insn::CSelNE { truthy, falsy, out });
        asm.push_insn(Insn::CSelGE { truthy, falsy, out });
        asm.push_insn(Insn::CSelLE { truthy, falsy, out });
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: isel r3, r4, r5, cr0eq
        0x4: isel r3, r4, r5, cr0lt
        0x8: isel r3, r4, r5, cr0gt
        0xc: isel r3, r5, r4, cr0eq
        0x10: isel r3, r5, r4, cr0lt
        0x14: isel r3, r5, r4, cr0gt
        ");
        assert_snapshot!(cb.hexdump(), @"9e28647c1e28647c5e28647c9e20657c1e20657c5e20657c");
    }

    #[test]
    fn test_emit_frame() {
        let (mut asm, mut cb) = setup_asm();

        asm.frame_setup(&[]);
        asm.frame_teardown(&[]);
        asm.cret(C_RET_OPND);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: mflr r0
        0x4: std r0, 0x10(r1)
        0x8: std r31, -8(r1)
        0xc: addi r31, r1, -0x10
        0x10: stdu r1, -0x20(r1)
        0x14: ld r0, 0x20(r31)
        0x18: addi r1, r31, 0x10
        0x1c: ld r31, 8(r31)
        0x20: mtlr r0
        0x24: blr
        ");
        assert_snapshot!(cb.hexdump(), @"a602087c100001f8f8ffe1fbf0ffe13be1ff21f820001fe810003f380800ffeba603087c2000804e");
    }

    #[test]
    fn test_emit_frame_with_preserved_regs() {
        let (mut asm, mut cb) = setup_asm();

        // Three registers take up four slots to keep SP aligned to 16 bytes
        asm.frame_setup(JIT_PRESERVED_REGS);
        asm.frame_teardown(JIT_PRESERVED_REGS);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: mflr r0
        0x4: std r0, 0x10(r1)
        0x8: std r31, -8(r1)
        0xc: addi r31, r1, -0x10
        0x10: stdu r1, -0x40(r1)
        0x14: std r28, -8(r31)
        0x18: std r30, -0x10(r31)
        0x1c: std r29, -0x18(r31)
        0x20: ld r28, -8(r31)
        0x24: ld r30, -0x10(r31)
        0x28: ld r29, -0x18(r31)
        0x2c: ld r0, 0x20(r31)
        0x30: addi r1, r31, 0x10
        0x34: ld r31, 8(r31)
        0x38: mtlr r0
        ");
        assert_snapshot!(cb.hexdump(), @"a602087c100001f8f8ffe1fbf0ffe13bc1ff21f8f8ff9ffbf0ffdffbe8ffbffbf8ff9febf0ffdfebe8ffbfeb20001fe810003f380800ffeba603087c");
    }

    #[test]
    fn test_frame_setup() {
        let (mut asm, mut cb) = setup_asm();

        asm.stack_state.stack_base_idx = 16;
        asm.frame_setup(&[]);
        asm.compile(&mut cb).unwrap();

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: mflr r0
        0x4: std r0, 0x10(r1)
        0x8: std r31, -8(r1)
        0xc: addi r31, r1, -0x10
        0x10: stdu r1, -0xa0(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"a602087c100001f8f8ffe1fbf0ffe13b61ff21f8");
    }

    #[test]
    fn test_frame_setup_with_largest_frame() {
        let (mut asm, mut cb) = setup_asm();

        // The largest frame is still within what addi can subtract
        asm.stack_state.stack_base_idx = Assembler::MAX_FRAME_STACK_SLOTS;
        asm.frame_setup(&[]);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: mflr r0
        0x4: std r0, 0x10(r1)
        0x8: std r31, -8(r1)
        0xc: addi r31, r1, -0x10
        0x10: stdu r1, -0x4020(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"a602087c100001f8f8ffe1fbf0ffe13be1bf21f8");
    }

    #[test]
    fn test_frame_setup_with_too_large_frame() {
        let (mut asm, mut cb) = setup_asm();

        asm.stack_state.stack_base_idx = Assembler::MAX_FRAME_STACK_SLOTS + 1;
        asm.frame_setup(&[]);

        assert_eq!(Some(CompileError::NativeStackTooLarge), asm.compile(&mut cb).err());
    }

    #[test]
    fn test_cpush_cpop() {
        let (mut asm, mut cb) = setup_asm();

        asm.cpush(Opnd::Reg(R3_REG));
        asm.cpop_into(Opnd::Reg(R3_REG));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r0, 0(r1)
        0x4: stdu r0, -0x10(r1)
        0x8: std r3, 0x10(r1)
        0xc: ld r3, 0x10(r1)
        0x10: ld r0, 0(r1)
        0x14: stdu r0, 0x10(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"000001e8f1ff01f8100061f8100061e8000001e8110001f8");
    }

    #[test]
    fn test_cpush_pair() {
        let (mut asm, mut cb) = setup_asm();

        asm.cpush_pair(Opnd::Reg(R3_REG), Opnd::Reg(R4_REG));
        // No register reads as zero in a store, so zero goes through r0
        asm.push_insn(Insn::CPushPair(Some(Opnd::Reg(R3_REG)), 0.into()));
        // Only the lower slot is written
        asm.push_insn(Insn::CPushPair(None, Opnd::Reg(R4_REG)));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r0, 0(r1)
        0x4: stdu r0, -0x10(r1)
        0x8: std r3, 0x18(r1)
        0xc: std r4, 0x10(r1)
        0x10: ld r0, 0(r1)
        0x14: stdu r0, -0x10(r1)
        0x18: std r3, 0x18(r1)
        0x1c: li r0, 0
        0x20: std r0, 0x10(r1)
        0x24: ld r0, 0(r1)
        0x28: stdu r0, -0x10(r1)
        0x2c: std r4, 0x10(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"000001e8f1ff01f8180061f8100081f8000001e8f1ff01f8180061f800000038100001f8000001e8f1ff01f8100081f8");
    }

    #[test]
    fn test_cpop_pair_into() {
        let (mut asm, mut cb) = setup_asm();

        asm.cpop_pair_into(Opnd::Reg(R4_REG), Opnd::Reg(R3_REG));
        // Popping a pair into one register leaves the upper slot in it
        asm.cpop_pair_into(Opnd::Reg(R3_REG), Opnd::Reg(R3_REG));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r4, 0x10(r1)
        0x4: ld r3, 0x18(r1)
        0x8: ld r0, 0(r1)
        0xc: stdu r0, 0x10(r1)
        0x10: ld r3, 0x18(r1)
        0x14: ld r0, 0(r1)
        0x18: stdu r0, 0x10(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"100081e8180061e8000001e8110001f8180061e8000001e8110001f8");
    }

    #[test]
    fn test_ccall() {
        let (mut asm, mut cb) = setup_asm();

        // SP moves past the linkage and parameter save areas for the callee,
        // and r12 has the address of the callee
        asm.ccall(0x1234_5678 as _, vec![]);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r0, 0(r1)
        0x4: stdu r0, -0x50(r1)
        0x8: lis r12, 0x1234
        0xc: ori r12, r12, 0x5678
        0x10: mtctr r12
        0x14: bctrl
        0x18: ld r0, 0(r1)
        0x1c: stdu r0, 0x50(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"000001e8b1ff01f83412803d78568c61a603897d2104804e000001e8510001f8");
    }

    #[test]
    fn test_ccall_reg() {
        let (mut asm, mut cb) = setup_asm();

        let _ = asm.ccall_reg(C_ARG_OPNDS[2], VALUE_BITS);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r0, 0(r1)
        0x4: stdu r0, -0x50(r1)
        0x8: mr r12, r5
        0xc: mtctr r12
        0x10: bctrl
        0x14: ld r0, 0(r1)
        0x18: stdu r0, 0x50(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"000001e8b1ff01f8782bac7ca603897d2104804e000001e8510001f8");
    }

    #[test]
    fn test_ccall_jit_code() {
        let (mut asm, mut cb) = setup_asm();

        // bl reaches anywhere in a code region this small, and JIT code has no use for r12
        let target = cb.get_write_ptr().add_bytes(0x100);
        asm.ccall(target.raw_ptr(&cb), vec![]);
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r0, 0(r1)
        0x4: stdu r0, -0x50(r1)
        0x8: bl 0x100
        0xc: ld r0, 0(r1)
        0x10: stdu r0, 0x50(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"000001e8b1ff01f8f9000048000001e8510001f8");
    }

    #[test]
    fn test_ccall_jit_code_in_large_region_isa_3_1() {
        TEST_POWER_ISA.set(PowerIsa::V3_1);

        // pla, which is .byte and li, takes up the same room no matter the address
        let compile = |target_offset: usize| {
            let (mut asm, _) = setup_asm();
            let mut cb = CodeBlock::new_dummy_sized(128 * 1024 * 1024);
            let target = cb.get_write_ptr().add_bytes(target_offset);
            asm.ccall(target.raw_ptr(&cb), vec![]);
            asm.compile_with_num_regs(&mut cb, 0);
            cb
        };

        let near = compile(0);
        let far = compile(1 << 26);
        assert_eq!(near.get_write_pos(), far.get_write_pos());
        assert_disasm_snapshot!(far.disasm(), @"
        0x0: ld r0, 0(r1)
        0x4: stdu r0, -0x50(r1)
        0x8: .byte 0xff, 0x03, 0x10, 0x06
        0xc: li r12, -8
        0x10: mtctr r12
        0x14: bctrl
        0x18: ld r0, 0(r1)
        0x1c: stdu r0, 0x50(r1)
        ");
        assert_snapshot!(far.hexdump(), @"000001e8b1ff01f8ff031006f8ff8039a603897d2104804e000001e8510001f8");
    }

    #[test]
    #[cfg(feature = "disasm")]
    fn test_ccall_jit_code_in_large_region() {
        // A call into JIT code takes up the same room no matter the address,
        // so that rewriting it with another address fits
        let code_size = |target_offset: usize| {
            let (mut asm, _) = setup_asm();
            let mut cb = CodeBlock::new_dummy_sized(128 * 1024 * 1024);
            let target = cb.get_write_ptr().add_bytes(target_offset);
            asm.ccall(target.raw_ptr(&cb), vec![]);
            asm.compile_with_num_regs(&mut cb, 0);
            (cb.get_write_pos(), stable_disasm(&cb, "r12").replace("nop\n", ""))
        };

        let (near_size, near_disasm) = code_size(0);
        let (far_size, far_disasm) = code_size(1 << 26);
        assert_eq!(near_size, far_size);
        // 2 instructions to move the frame header down, 5 for the address, mtctr, bctrl,
        // 2 to move the header back up
        assert_eq!(11 * 4, near_size);
        assert_eq!(near_disasm, far_disasm);
        assert_disasm_snapshot!(near_disasm, @"
        ld r0, 0(r1)
        stdu r0, -0x50(r1)
        load r12, address
        mtctr r12
        bctrl
        ld r0, 0(r1)
        stdu r0, 0x50(r1)
        ");
    }

    #[test]
    fn test_incr_counter() {
        let (mut asm, mut cb, scratch_reg) = setup_asm_with_scratch_reg();

        asm.incr_counter(scratch_reg, 1.into());
        asm.incr_counter(scratch_reg, Opnd::Reg(R3_REG));
        asm.compile_with_num_regs(&mut cb, 0);

        // Retries when the store finds that something else wrote to the counter
        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r0, 0
        0x4: ldarx r12, r11, r0
        0x8: addi r12, r12, 1
        0xc: stdcx. r12, r11, r0
        0x10: bne 4
        0x14: li r0, 0
        0x18: ldarx r12, r11, r0
        0x1c: add r12, r12, r3
        0x20: stdcx. r12, r11, r0
        0x24: bne 0x18
        ");
        assert_snapshot!(cb.hexdump(), @"00000038a8008b7d01008c39ad018b7df4ff824000000038a8008b7d141a8c7dad018b7df4ff8240");
    }

    #[test]
    fn test_breakpoint_and_abort() {
        let (mut asm, mut cb) = setup_asm();

        asm.breakpoint();
        asm.abort();
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: trap
        0x4: .byte 0x00, 0x00, 0x00, 0x00
        ");
        assert_snapshot!(cb.hexdump(), @"0800e07f00000000");
    }

    #[test]
    fn test_split_memory_operands() {
        let (mut asm, mut cb) = setup_asm();

        // Both operands are loaded into registers first
        let sum = asm.add(Opnd::mem(64, SP, 0), Opnd::mem(64, SP, 8));
        asm.store(Opnd::mem(64, SP, 16), sum);
        asm.compile_with_num_regs(&mut cb, 2);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r3, 0(r30)
        0x4: ld r4, 8(r30)
        0x8: add r3, r3, r4
        0xc: std r3, 0x10(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"00007ee808009ee81422637c10007ef8");
    }

    #[test]
    fn test_split_immediate_on_the_left() {
        let (mut asm, mut cb) = setup_asm();

        // Addition is commutative, so the immediate becomes the second operand
        let sum = asm.add(Opnd::UImm(8), Opnd::Reg(R4_REG));
        asm.store(Opnd::mem(64, SP, 0), sum);
        // Subtraction isn't, so the immediate is loaded
        let difference = asm.sub(Opnd::UImm(8), Opnd::Reg(R4_REG));
        asm.store(Opnd::mem(64, SP, 8), difference);
        asm.compile_with_num_regs(&mut cb, 1);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: addi r3, r4, 8
        0x4: std r3, 0(r30)
        0x8: li r3, 8
        0xc: subf r3, r4, r3
        0x10: std r3, 8(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"0800643800007ef8080060385018647c08007ef8");
    }

    #[test]
    fn test_split_mov_to_and_from_memory() {
        let (mut asm, mut cb) = setup_asm();

        asm.mov(Opnd::Reg(R3_REG), Opnd::mem(64, SP, 8));
        asm.mov(Opnd::mem(64, SP, 16), Opnd::Reg(R3_REG));
        asm.mov(Opnd::mem(64, SP, 24), Opnd::UImm(1));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r3, 8(r30)
        0x4: std r3, 0x10(r30)
        0x8: li r0, 1
        0xc: std r0, 0x18(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"08007ee810007ef80100003818001ef8");
    }

    #[test]
    fn test_split_load_with_large_displacement() {
        let (mut asm, mut cb) = setup_asm();

        // The displacement doesn't fit in 16 bits, so the address is calculated first
        let value = asm.load(Opnd::mem(64, SP, 0x12_3458));
        asm.store(Opnd::mem(64, CFP, 0), value);
        // ld can't encode a displacement that isn't a multiple of 4
        let value = asm.load(Opnd::mem(64, SP, 6));
        asm.store(Opnd::mem(64, CFP, 0), value);
        // lbz can
        let value = asm.load(Opnd::mem(8, SP, 6));
        asm.store(Opnd::mem(8, CFP, 0), value);
        asm.compile_with_num_regs(&mut cb, 1);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: addis r3, r30, 0x12
        0x4: addi r3, r3, 0x3458
        0x8: ld r3, 0(r3)
        0xc: std r3, 0(r28)
        0x10: addi r3, r30, 6
        0x14: ld r3, 0(r3)
        0x18: std r3, 0(r28)
        0x1c: lbz r3, 6(r30)
        0x20: stb r3, 0(r28)
        ");
        assert_snapshot!(cb.hexdump(), @"12007e3c58346338000063e800007cf806007e38000063e800007cf806007e8800007c98");
    }

    #[test]
    fn test_split_cmp_8_bits() {
        let (mut asm, mut cb) = setup_asm();

        // Only the lower 8 bits of the register are compared
        asm.cmp(Opnd::Reg(R5_REG).with_num_bits(8), Opnd::UImm(0xc));
        asm.compile_with_num_regs(&mut cb, 1);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: andi. r3, r5, 0xff
        0x4: cmpdi r3, 0xc
        ");
        assert_snapshot!(cb.hexdump(), @"ff00a3700c00232c");
    }

    #[test]
    fn test_split_csel_immediates() {
        let (mut asm, mut cb) = setup_asm();

        asm.cmp(Opnd::Reg(R5_REG), Opnd::UImm(0));
        let out = asm.csel_e(Opnd::UImm(0x14), Opnd::UImm(0));
        asm.store(Opnd::mem(64, SP, 0), out);
        asm.compile_with_num_regs(&mut cb, 2);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: cmpdi r5, 0
        0x4: li r3, 0x14
        0x8: li r4, 0
        0xc: isel r3, r3, r4, cr0eq
        0x10: std r3, 0(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"0000252c14006038000080389e20637c00007ef8");
    }

    #[test]
    fn test_store_with_scratch_reg_and_large_displacement() {
        let (mut asm, mut cb, scratch_reg) = setup_asm_with_scratch_reg();

        // With no scratch split to come, the address goes to the other scratch register
        asm.store(Opnd::mem(64, scratch_reg, 0x12_3458), Opnd::Reg(R3_REG));
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: addis r12, r11, 0x12
        0x4: addi r12, r12, 0x3458
        0x8: std r3, 0(r12)
        ");
        assert_snapshot!(cb.hexdump(), @"12008b3d58348c3900006cf8");
    }

    #[test]
    fn test_ccall_resolve_parallel_moves_single_cycle() {
        let (mut asm, mut cb) = setup_asm();

        // r3 and r4 swap, which takes the scratch register
        asm.ccall(0 as _, vec![C_ARG_OPNDS[1], C_ARG_OPNDS[0]]);
        asm.compile_with_num_regs(&mut cb, ALLOC_REGS.len());

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: mr r11, r3
        0x4: mr r3, r4
        0x8: mr r4, r11
        0xc: ld r0, 0(r1)
        0x10: stdu r0, -0x50(r1)
        0x14: li r12, 0
        0x18: mtctr r12
        0x1c: bctrl
        0x20: ld r0, 0(r1)
        0x24: stdu r0, 0x50(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"781b6b7c7823837c785b647d000001e8b1ff01f800008039a603897d2104804e000001e8510001f8");
    }

    #[test]
    fn test_ccall_register_preservation() {
        let (mut asm, mut cb) = setup_asm();

        // Three values live across the call, which are pushed in pairs
        let v0 = asm.load(1.into());
        let v1 = asm.load(2.into());
        let v2 = asm.load(3.into());
        asm.ccall(0 as _, vec![]);
        asm.store(Opnd::mem(64, SP, 0), v0);
        asm.store(Opnd::mem(64, SP, 8), v1);
        asm.store(Opnd::mem(64, SP, 16), v2);
        asm.compile_with_num_regs(&mut cb, ALLOC_REGS.len());

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r3, 1
        0x4: li r4, 2
        0x8: li r5, 3
        0xc: ld r0, 0(r1)
        0x10: stdu r0, -0x10(r1)
        0x14: std r3, 0x18(r1)
        0x18: std r4, 0x10(r1)
        0x1c: ld r0, 0(r1)
        0x20: stdu r0, -0x10(r1)
        0x24: std r5, 0x18(r1)
        0x28: li r0, 0
        0x2c: std r0, 0x10(r1)
        0x30: ld r0, 0(r1)
        0x34: stdu r0, -0x50(r1)
        0x38: li r12, 0
        0x3c: mtctr r12
        0x40: bctrl
        0x44: ld r0, 0(r1)
        0x48: stdu r0, 0x50(r1)
        0x4c: ld r5, 0x18(r1)
        0x50: ld r0, 0(r1)
        0x54: stdu r0, 0x10(r1)
        0x58: ld r4, 0x10(r1)
        0x5c: ld r3, 0x18(r1)
        0x60: ld r0, 0(r1)
        0x64: stdu r0, 0x10(r1)
        0x68: std r3, 0(r30)
        0x6c: std r4, 8(r30)
        0x70: std r5, 0x10(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"01006038020080380300a038000001e8f1ff01f8180061f8100081f8000001e8f1ff01f81800a1f800000038100001f8000001e8b1ff01f800008039a603897d2104804e000001e8510001f81800a1e8000001e8110001f8100081e8180061e8000001e8110001f800007ef808009ef81000bef8");
    }

    #[test]
    fn test_ccall_stack_args() {
        let (mut asm, mut cb) = setup_asm();

        // 9 arguments: 8 in registers, 1 on the stack. It is pushed with padding to keep SP
        // aligned to 16 bytes, and ends up right above the parameter save area of the callee.
        let a0 = asm.load(Opnd::UImm(1));
        let mut args: Vec<Opnd> = (2..=8).map(|i| asm.load(Opnd::UImm(i))).collect();
        args.insert(0, a0);
        args.push(a0);
        asm.ccall(0 as _, args);
        asm.compile_with_num_regs(&mut cb, ALLOC_REGS.len());

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: li r3, 1
        0x4: li r4, 2
        0x8: li r5, 3
        0xc: li r6, 4
        0x10: li r7, 5
        0x14: li r8, 6
        0x18: li r9, 7
        0x1c: li r10, 8
        0x20: ld r0, 0(r1)
        0x24: stdu r0, -0x10(r1)
        0x28: std r3, 0x10(r1)
        0x2c: ld r0, 0(r1)
        0x30: stdu r0, -0x50(r1)
        0x34: li r12, 0
        0x38: mtctr r12
        0x3c: bctrl
        0x40: ld r0, 0(r1)
        0x44: stdu r0, 0x50(r1)
        0x48: ld r0, 0(r1)
        0x4c: stdu r0, 0x10(r1)
        ");
        assert_snapshot!(cb.hexdump(), @"01006038020080380300a0380400c0380500e038060000390700203908004039000001e8f1ff01f8100061f8000001e8b1ff01f800008039a603897d2104804e000001e8510001f8000001e8110001f8");
    }

    #[test]
    fn test_scratch_split_spilled_values() {
        let (mut asm, mut cb) = setup_asm();

        // With one register for two values, one of them lives on the stack.
        // Storing it is a load into r0 and a store.
        let v0 = asm.load(Opnd::mem(64, SP, 0));
        let v1 = asm.load(Opnd::mem(64, SP, 8));
        asm.store(Opnd::mem(64, SP, 16), v0);
        asm.store(Opnd::mem(64, SP, 24), v1);
        asm.compile_with_num_regs(&mut cb, 1);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r3, 0(r30)
        0x4: ld r0, 8(r30)
        0x8: std r0, -8(r31)
        0xc: std r3, 0x10(r30)
        0x10: ld r0, -8(r31)
        0x14: std r0, 0x18(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"00007ee808001ee8f8ff1ff810007ef8f8ff1fe818001ef8");
    }

    #[test]
    fn test_scratch_split_spilled_operands() {
        let (mut asm, mut cb) = setup_asm();

        // With one register for three values, two of them live on the stack
        // and go through the scratch registers
        let v0 = asm.load(Opnd::mem(64, SP, 0));
        let v1 = asm.load(Opnd::mem(64, SP, 8));
        let v2 = asm.load(Opnd::mem(64, SP, 16));
        let sum = asm.add(v0, v1);
        let sum = asm.add(sum, v2);
        asm.store(Opnd::mem(64, SP, 24), sum);
        asm.compile_with_num_regs(&mut cb, 1);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r3, 0(r30)
        0x4: ld r0, 8(r30)
        0x8: std r0, -8(r31)
        0xc: ld r0, 0x10(r30)
        0x10: std r0, -0x10(r31)
        0x14: ld r12, -8(r31)
        0x18: add r3, r3, r12
        0x1c: ld r12, -0x10(r31)
        0x20: add r3, r3, r12
        0x24: std r3, 0x18(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"00007ee808001ee8f8ff1ff810001ee8f0ff1ff8f8ff9fe91462637cf0ff9fe91462637c18007ef8");
    }

    #[test]
    fn test_scratch_split_spilled_lshift() {
        let (mut asm, mut cb) = setup_asm();

        let v0 = asm.load(Opnd::mem(64, SP, 0));
        let v1 = asm.load(Opnd::mem(64, SP, 8));
        let shifted = asm.lshift(v1, Opnd::UImm(3));
        asm.store(Opnd::mem(64, SP, 16), v0);
        asm.store(Opnd::mem(64, SP, 24), shifted);
        asm.compile_with_num_regs(&mut cb, 1);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r0, 0(r30)
        0x4: std r0, -8(r31)
        0x8: ld r3, 8(r30)
        0xc: sldi r3, r3, 3
        0x10: ld r0, -8(r31)
        0x14: std r0, 0x10(r30)
        0x18: std r3, 0x18(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"00001ee8f8ff1ff808007ee8241f6378f8ff1fe810001ef818007ef8");
    }

    #[test]
    fn test_scratch_split_spilled_overflow_check() {
        let (mut asm, mut cb) = setup_asm();
        let overflow = asm.new_label("overflow");

        // The sum is stored to its stack slot between the addition and the jump.
        // (.byte is addo.)
        let v0 = asm.load(Opnd::mem(64, SP, 0));
        let v1 = asm.load(Opnd::mem(64, SP, 8));
        let sum = asm.add(v0, v1);
        asm.push_insn(Insn::Jo(overflow.clone()));
        asm.store(Opnd::mem(64, SP, 16), v0);
        asm.store(Opnd::mem(64, SP, 24), sum);
        asm.write_label(overflow);
        asm.compile_with_num_regs(&mut cb, 1);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: ld r0, 0(r30)
        0x4: std r0, -8(r31)
        0x8: ld r3, 8(r30)
        0xc: ld r11, -8(r31)
        0x10: li r0, 0
        0x14: mtxer r0
        0x18: .byte 0x15, 0x1e, 0x6b, 0x7c
        0x1c: bso 0x30
        0x20: nop
        0x24: ld r0, -8(r31)
        0x28: std r0, 0x10(r30)
        0x2c: std r3, 0x18(r30)
        ");
        assert_snapshot!(cb.hexdump(), @"00001ee8f8ff1ff808007ee8f8ff7fe900000038a603017c151e6b7c1400834100000060f8ff1fe810001ef818007ef8");
    }

    #[test]
    fn test_scratch_split_incr_counter() {
        let (mut asm, mut cb) = setup_asm();

        asm.incr_counter(Opnd::const_ptr(0x1234_5678 as *const u8), 1.into());
        asm.compile_with_num_regs(&mut cb, 0);

        assert_disasm_snapshot!(cb.disasm(), @"
        0x0: lis r11, 0x1234
        0x4: ori r11, r11, 0x5678
        0x8: li r0, 0
        0xc: ldarx r12, r11, r0
        0x10: addi r12, r12, 1
        0x14: stdcx. r12, r11, r0
        0x18: bne 0xc
        ");
        assert_snapshot!(cb.hexdump(), @"3412603d78566b6100000038a8008b7d01008c39ad018b7df4ff8240");
    }
}
