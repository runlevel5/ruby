use crate::asm::CodeBlock;
use crate::asm::ppc64::*;
use crate::backend::lir::*;
use crate::stats::CompileError;
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

/// Size of the area every caller keeps at the bottom of its frame in the ELFv2 ABI:
/// back chain, CR save, LR save and TOC save doublewords
const LINKAGE_AREA_SIZE: i32 = 32;

/// Size of the area a caller reserves above the linkage area for a callee to
/// save the eight argument registers. Stack-passed arguments come after it.
const PARAM_SAVE_AREA_SIZE: i32 = 64;

/// Offset from the native SP at function entry to the first stack-passed C argument
pub const C_STACK_ARGS_ENTRY_OFFSET: i32 = LINKAGE_AREA_SIZE + PARAM_SAVE_AREA_SIZE;

/// Whether a lone [`Insn::CPush`] leaves the native SP misaligned for C calls
pub const CPUSH_MISALIGNS_STACK: bool = false;

/// Filler for new code pages. All zeros is an illegal instruction, so it's already what we want.
pub const NEW_PAGE_FILL: Option<u8> = None;

/// List of registers that can be used for register allocation.
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

/// A scratch register available for use by resolve_ssa to break register copy cycles.
/// Must not overlap with ALLOC_REGS or other preserved registers.
pub const SCRATCH_REG: Reg = R11_REG;
const SCRATCH0_OPND: Opnd = Opnd::Reg(SCRATCH_REG);

impl Assembler {
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
        todo!("ppc64 frame layout")
    }

    pub fn compile_with_regs(self, _cb: &mut CodeBlock, _regs: Vec<Reg>) -> Result<(CodePtr, Vec<CodePtr>), CompileError> {
        todo!("ppc64 backend")
    }
}
