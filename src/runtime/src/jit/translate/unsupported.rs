//! JIT-untranslatable opcode table (single source for prescan + bail arms).
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

/// Return `Some(reason)` if `func` contains an opcode the JIT cannot yet
/// translate, otherwise `None`. Keep this list in lock-step with the `bail!`
/// arms in `translate_instr` below — every opcode that bails there must be
/// reported here so `compile_module` can skip the function up front rather than
/// abort mid-translation.
///
/// fix-jit-cross-zpkg-transitive-eager (2026-06-20): with `--mode jit` now the
/// default and eager loading pulling the *whole* transitive dep closure into
/// the module, the merged module routinely contains stdlib functions that use
/// `out`/`ref` params (`LoadLocalAddr`) or native interop (`CallNative`). Those
/// must degrade per-function to interp (jit_call misses `fn_entries` → the
/// `cross_zpkg_via_interp` fallback runs the bytecode), not fail the program.
pub(crate) fn jit_unsupported_reason(func: &Function) -> Option<&'static str> {
    for block in &func.blocks {
        for instr in &block.instructions {
            if let Some(reason) = unsupported_reason(instr) {
                return Some(reason);
            }
        }
    }
    None
}

/// Single source of truth for "which opcodes the JIT cannot translate, and why".
///
/// converge-vm-arith-semantics (H3): this collapses the two hand-maintained
/// lists that previously had to be kept in lock-step — the prescan (above) and
/// the `bail!` arms in `translate_instr`. The prescan loops over this; each
/// `bail!` arm sources its message from here (`unsupported_reason(instr).expect(..)`),
/// so the two checkpoints can never drift.
///
/// Must take `&Instruction` (not a flat opcode set): the two generic cases are
/// *conditional* on `method_type_args` — a non-generic `Call`/`VCall` DOES JIT.
pub(crate) fn unsupported_reason(instr: &Instruction) -> Option<&'static str> {
    Some(match instr {
        Instruction::CallNative(_)           => "CallNative",
        Instruction::CallNativeVtable { .. } => "CallNativeVtable",
        Instruction::PinPtr { .. }           => "PinPtr",
        Instruction::UnpinPtr { .. }         => "UnpinPtr",
        Instruction::LoadLocalAddr { .. }    => "LoadLocalAddr",
        Instruction::LoadElemAddr { .. }     => "LoadElemAddr",
        Instruction::LoadFieldAddr(_)        => "LoadFieldAddr",
        // add-generic-methods: method-level generics run on the interpreter for
        // now (the JIT frame has no method_type_args carrier, and the JIT call
        // paths don't thread it). A function that *reads* a method type param,
        // or *makes* a generic call, stays interp — everything else JITs.
        Instruction::MethodTypeArg { .. }    => "MethodTypeArg (generic method body)",
        Instruction::MethodDefault { .. }    => "MethodDefault (generic method body)",
        Instruction::Call(insn)  if !insn.method_type_args.is_empty() => "generic Call",
        Instruction::VCall(insn) if !insn.method_type_args.is_empty() => "generic VCall",
        // fix-generic-array-value-zero-init (方案 C): `new T[n]` on a generic type
        // parameter needs runtime method_type_args / receiver.type_args to zero-init
        // value-type slots — the carrier the JIT frame lacks — so it runs interp
        // (consistent with MethodDefault above). Non-generic ArrayNew (kind==0) JITs.
        Instruction::ArrayNew(insn) if insn.type_param_kind != 0 => "generic-type-param ArrayNew",
        _ => return None,
    })
}
