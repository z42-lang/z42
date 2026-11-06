//! JIT-untranslatable opcode table (single source for prescan + bail arms).
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//!
//! Compiled frames carry bound method arguments for MethodTypeArg and MethodDefault.
//! The table below still rejects instructions that need method marker substitution
//! or method-parameter array allocation until those helpers can use the carrier.

use super::*;

/// Return `Some(reason)` if `func` contains an opcode the JIT cannot yet
/// translate, otherwise `None`. Keep this list in lock-step with the `bail!`
/// arms in `translate_instr` below — every opcode that bails there must be
/// reported here so `compile_module` can skip the function up front rather than
/// abort mid-translation.
///
/// fix-jit-cross-zpkg-transitive-eager : with `--mode jit` now the
/// default and eager loading pulling the *whole* transitive dep closure into
/// the module, the merged module routinely contains stdlib functions that use
/// `out`/`ref` params (`LoadLocalAddr`) or native interop (`CallNative`). Those
/// must degrade per-function to interp (the slot is cached as Rejected and the
/// calling helper runs the bytecode on the interpreter), not fail the program.
pub(crate) fn jit_unsupported_reason(func: &Function) -> Option<&'static str> {
    // `catch (Err<T>)` on a method-level `T` binds the frame's method_type_args.
    if func.exception_table().iter().any(|e| e.catch_type.as_deref().is_some_and(has_method_markers)) {
        return Some("typed catch on a method type parameter ($mta:)");
    }
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
        // Concrete generic call sites compile. Forwarding markers still need
        // substitution against the caller's activation before the call.
        Instruction::Call(insn)  if forwards_caller_mta(&insn.method_type_args) => "generic Call forwarding the caller's type args ($mta:)",
        Instruction::VCall(insn) if forwards_caller_mta(&insn.method_type_args) => "generic VCall forwarding the caller's type args ($mta:)",
        // Method-parameter array allocation still needs the shared default/backing
        // selection path. Concrete and class-parameter ArrayNew already compile.
        Instruction::ArrayNew(insn) if insn.type_param_kind != 0 && insn.type_param_kind != 2
            => "method-type-param ArrayNew",
        // `o is T` / `o as T` / `new List<T>()` on a method-level `T` in a shared generic body:
        // the target / arguments carry `$mta:N` markers bound from method_type_args. Class-level
        // markers (`$cta:`) JIT — they bind from the receiver in reg 0.
        Instruction::IsInstance(insn) if has_method_markers(&insn.class_name) => "type test on a method type parameter ($mta:)",
        Instruction::AsCast(insn) if has_method_markers(&insn.class_name) => "type test on a method type parameter ($mta:)",
        Instruction::ObjNew(insn) if insn.type_args.iter().any(|a| has_method_markers(a))
            => "new of a generic over a method type parameter ($mta:)",
        _ => return None,
    })
}

use crate::metadata::index::type_param_markers::has_method_markers;

/// Whether a call site's `method_type_args` contain a `$mta:N` forwarding marker, bare or inside
/// a constructed argument (resolved against the caller frame at run time —
/// `interp::resolve_forwarded_mta`). Class-level `$cta:` markers bind from reg 0 and JIT
/// (`jit_call_mta` / `jit_vcall_mta`).
pub(crate) fn forwards_caller_mta(mta: &[String]) -> bool {
    mta.iter().any(|s| has_method_markers(s))
}
