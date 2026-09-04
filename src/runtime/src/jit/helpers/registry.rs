//! Central JIT helper registry.
//!
//! Single source of truth for the helper set. Two responsibilities:
//!
//! 1. **`register_symbols`** — called by `jit/mod.rs::compile_module` against
//!    the `JITBuilder` *before* JITModule construction; binds every helper's
//!    `#[unsafe(no_mangle)]` symbol name to its function pointer so Cranelift's
//!    linker can resolve calls.
//!
//! 2. **`declare_imports`** — called by `jit/translate.rs` against the
//!    `JITModule` to declare each helper's Cranelift signature, returning
//!    a `HelperIds` struct of `FuncId`s used during IR emission.
//!
//! Adding a new helper requires updates to **two** places:
//!   • the helper file under `jit/helpers/<category>.rs` (function definition)
//!   • this file (registration entry + declaration entry + `HelperIds` field)
//!
//! Compare to the prior 3-place split (helpers_*.rs definition + jit/mod.rs
//! `reg!()` block + translate.rs `declare_helpers`).

use anyhow::Result;
use cranelift_codegen::ir::{types, AbiParam};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module as CraneliftModule};

use super::{arith, array, call, closure, control, object, object_field, struct_ops, value, vcall};

// ─── HelperIds ──────────────────────────────────────────────────────────────
//
// One `FuncId` per helper, populated by `declare_imports` and consumed by
// translate.rs during IR emission.

#[allow(dead_code)] // not all helpers are referenced from every translate path
pub struct HelperIds {
    // value
    pub const_i32:      FuncId,
    pub const_i64:      FuncId,
    pub const_f64:      FuncId,
    pub const_bool:     FuncId,
    pub const_char:     FuncId,
    pub const_null:     FuncId,
    pub const_str:      FuncId,
    pub copy:           FuncId,
    pub str_concat:     FuncId,
    pub to_str:         FuncId,
    pub get_bool:       FuncId,
    pub set_ret:        FuncId,
    /// review.md C2 P1 step 1 (2026-05-28): return `frame.regs.as_mut_ptr()`.
    /// Called once at function entry; result cached for native slot access.
    pub regs_ptr:       FuncId,
    // control
    pub throw:          FuncId,
    pub install_catch:  FuncId,
    /// catch-by-generic-type (2026-05-06): peek at pending exception's class +
    /// subclass walk vs `target` string. Returns 1 on match, 0 otherwise.
    pub match_catch_type: FuncId,
    /// add-gc-safepoint-jit (2026-05-21): cooperative GC safepoint check
    /// trampoline. Emitted by translate.rs at function entry, backward
    /// branches, and post-Call sites.
    pub check_safepoint: FuncId,
    /// inline-jit-safepoint-check (2026-08-01): slow branch of the inlined
    /// safepoint fast path (counter reset + Mutex/phase/auto-collect drain).
    /// Called from native code only when the inlined decrement hits 0.
    pub check_safepoint_slow: FuncId,
    // arith
    pub add:            FuncId,
    pub sub:            FuncId,
    pub mul:            FuncId,
    pub div:            FuncId,
    pub rem:            FuncId,
    pub eq:             FuncId,
    pub ne:             FuncId,
    pub lt:             FuncId,
    pub le:             FuncId,
    pub gt:             FuncId,
    pub ge:             FuncId,
    pub and:            FuncId,
    pub or:             FuncId,
    pub not:            FuncId,
    pub neg:            FuncId,
    pub bit_and:        FuncId,
    pub bit_or:         FuncId,
    pub bit_xor:        FuncId,
    pub bit_not:        FuncId,
    pub shl:            FuncId,
    pub shr:            FuncId,
    // call
    pub call:           FuncId,
    pub builtin:        FuncId,
    // array
    pub array_new:      FuncId,
    pub array_new_lit:  FuncId,
    pub array_get:      FuncId,
    pub array_data:     FuncId,
    pub array_data_opt: FuncId,
    pub array_set:      FuncId,
    pub array_len:      FuncId,
    // object
    pub obj_new:        FuncId,
    pub typeof_op:      FuncId,
    pub field_get:      FuncId,
    pub obj_field_slot: FuncId,
    pub obj_ref_field_slot: FuncId,
    pub field_set:      FuncId,
    pub is_instance:    FuncId,
    pub as_cast:        FuncId,
    pub static_get:     FuncId,
    pub static_set:     FuncId,
    // D-8b-3 Phase 2: generic-T `default(T)` runtime resolution
    pub default_of:     FuncId,
    // vcall
    pub vcall:          FuncId,
    // closure (L3 / D1b)
    pub load_fn:        FuncId,
    pub mk_clos:        FuncId,
    pub call_indirect:  FuncId,
    pub load_fn_cached: FuncId,
    /// fix-numeric-cast-lowering (2026-05-13): explicit numeric cast.
    pub convert:        FuncId,
    // add-struct-jit-value-path (P5): blob value-type instruction helpers.
    pub struct_alloc:          FuncId,
    pub struct_copy:           FuncId,
    pub struct_field_get_prim: FuncId,
    pub struct_field_set_prim: FuncId,
}

// ─── register_symbols ───────────────────────────────────────────────────────

/// Bind every helper's `#[unsafe(no_mangle)]` symbol name to its function pointer
/// in the given `JITBuilder`. Must be called once during `compile_module`
/// before constructing the `JITModule`.
pub fn register_symbols(builder: &mut JITBuilder) {
    macro_rules! reg {
        ($name:expr, $fn:expr) => {
            builder.symbol($name, $fn as *const u8);
        };
    }
    // value
    reg!("jit_const_i32",     value::jit_const_i32);
    reg!("jit_const_i64",     value::jit_const_i64);
    reg!("jit_const_f64",     value::jit_const_f64);
    reg!("jit_const_bool",    value::jit_const_bool);
    reg!("jit_const_char",    value::jit_const_char);
    reg!("jit_const_null",    value::jit_const_null);
    reg!("jit_const_str",     value::jit_const_str);
    reg!("jit_copy",          value::jit_copy);
    reg!("jit_str_concat",    value::jit_str_concat);
    reg!("jit_to_str",        value::jit_to_str);
    reg!("jit_get_bool",      value::jit_get_bool);
    reg!("jit_set_ret",       value::jit_set_ret);
    reg!("jit_regs_ptr",      value::jit_regs_ptr);
    // control
    reg!("jit_throw",         control::jit_throw);
    reg!("jit_install_catch", control::jit_install_catch);
    reg!("jit_match_catch_type", control::jit_match_catch_type);
    reg!("jit_check_safepoint",  control::jit_check_safepoint);
    reg!("jit_check_safepoint_slow", control::jit_check_safepoint_slow);
    // arith
    reg!("jit_add",           arith::jit_add);
    reg!("jit_sub",           arith::jit_sub);
    reg!("jit_mul",           arith::jit_mul);
    reg!("jit_div",           arith::jit_div);
    reg!("jit_rem",           arith::jit_rem);
    reg!("jit_eq",            arith::jit_eq);
    reg!("jit_ne",            arith::jit_ne);
    reg!("jit_lt",            arith::jit_lt);
    reg!("jit_le",            arith::jit_le);
    reg!("jit_gt",            arith::jit_gt);
    reg!("jit_ge",            arith::jit_ge);
    reg!("jit_and",           arith::jit_and);
    reg!("jit_or",            arith::jit_or);
    reg!("jit_not",           arith::jit_not);
    reg!("jit_neg",           arith::jit_neg);
    reg!("jit_bit_and",       arith::jit_bit_and);
    reg!("jit_bit_or",        arith::jit_bit_or);
    reg!("jit_bit_xor",       arith::jit_bit_xor);
    reg!("jit_bit_not",       arith::jit_bit_not);
    reg!("jit_shl",           arith::jit_shl);
    reg!("jit_shr",           arith::jit_shr);
    // call
    reg!("jit_call",          call::jit_call);
    reg!("jit_builtin",       call::jit_builtin);
    // array
    reg!("jit_array_new",     array::jit_array_new);
    reg!("jit_array_new_lit", array::jit_array_new_lit);
    reg!("jit_array_get",     array::jit_array_get);
    reg!("jit_array_data",    array::jit_array_data);
    reg!("jit_array_data_opt", array::jit_array_data_opt);
    reg!("jit_array_set",     array::jit_array_set);
    reg!("jit_array_len",     array::jit_array_len);
    // object
    reg!("jit_obj_new",       object::jit_obj_new);
    reg!("jit_typeof",        object::jit_typeof);
    reg!("jit_default_of",    object::jit_default_of);
    reg!("jit_convert",       object::jit_convert);
    reg!("jit_field_get",     object_field::jit_field_get);
    reg!("jit_obj_field_slot", object_field::jit_obj_field_slot);
    reg!("jit_obj_ref_field_slot", object_field::jit_obj_ref_field_slot);
    reg!("jit_field_set",     object_field::jit_field_set);
    reg!("jit_is_instance",   object::jit_is_instance);
    reg!("jit_as_cast",       object::jit_as_cast);
    reg!("jit_static_get",    object::jit_static_get);
    reg!("jit_static_set",    object::jit_static_set);
    // vcall
    reg!("jit_vcall",         vcall::jit_vcall);
    // closure
    reg!("jit_load_fn",       closure::jit_load_fn);
    reg!("jit_load_fn_cached", closure::jit_load_fn_cached);
    reg!("jit_mk_clos",       closure::jit_mk_clos);
    reg!("jit_call_indirect", closure::jit_call_indirect);
    // add-struct-jit-value-path (P5): struct value-type instruction helpers
    reg!("jit_struct_alloc",          struct_ops::jit_struct_alloc);
    reg!("jit_struct_copy",           struct_ops::jit_struct_copy);
    reg!("jit_struct_field_get_prim", struct_ops::jit_struct_field_get_prim);
    reg!("jit_struct_field_set_prim", struct_ops::jit_struct_field_set_prim);
}

// ─── declare_imports ────────────────────────────────────────────────────────

/// Declare each helper's Cranelift `Signature` against the `JITModule` and
/// return a `HelperIds` of `FuncId`s. Called once per `JITModule` (currently
/// from `translate.rs::declare_helpers`).
pub fn declare_imports(jit: &mut JITModule) -> Result<HelperIds> {
    let ptr  = jit.target_config().pointer_type();
    let i8t  = types::I8;
    let i32t = types::I32;
    let i64t = types::I64;
    let f64t = types::F64;

    macro_rules! decl {
        ($name:expr, [$($p:expr),*], [$($r:expr),*]) => {{
            let mut sig = jit.make_signature();
            $(sig.params.push(AbiParam::new($p));)*
            $(sig.returns.push(AbiParam::new($r));)*
            jit.declare_function($name, Linkage::Import, &sig)?
        }};
    }

    // extend-jit-helper-abi (2026-04-28): every helper receives `ctx: *const
    // JitModuleCtx` as 2nd param (after `frame`). Helper bodies reach
    // VmContext via `(*ctx).vm_ctx`, replacing the previous `thread_local!`
    // slots in the old `helpers.rs`.
    Ok(HelperIds {
        const_i32:     decl!("jit_const_i32",  [ptr, ptr, i32t, i32t],                    []),
        const_i64:     decl!("jit_const_i64",  [ptr, ptr, i32t, i64t],                    []),
        const_f64:     decl!("jit_const_f64",  [ptr, ptr, i32t, f64t],                    []),
        const_bool:    decl!("jit_const_bool", [ptr, ptr, i32t, i8t],                     []),
        const_char:    decl!("jit_const_char", [ptr, ptr, i32t, i32t],                    []),
        const_null:    decl!("jit_const_null", [ptr, ptr, i32t],                           []),
        const_str:     decl!("jit_const_str",  [ptr, ptr, i32t, i32t],                    [i8t]),
        copy:          decl!("jit_copy",       [ptr, ptr, i32t, i32t],                    []),
        add:           decl!("jit_add",        [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        sub:           decl!("jit_sub",        [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        mul:           decl!("jit_mul",        [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        div:           decl!("jit_div",        [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        rem:           decl!("jit_rem",        [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        eq:            decl!("jit_eq",         [ptr, ptr, i32t, i32t, i32t],              []),
        ne:            decl!("jit_ne",         [ptr, ptr, i32t, i32t, i32t],              []),
        lt:            decl!("jit_lt",         [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        le:            decl!("jit_le",         [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        gt:            decl!("jit_gt",         [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        ge:            decl!("jit_ge",         [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        and:           decl!("jit_and",        [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        or:            decl!("jit_or",         [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        not:           decl!("jit_not",        [ptr, ptr, i32t, i32t],                    [i8t]),
        neg:           decl!("jit_neg",        [ptr, ptr, i32t, i32t],                    [i8t]),
        bit_and:       decl!("jit_bit_and",    [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        bit_or:        decl!("jit_bit_or",     [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        bit_xor:       decl!("jit_bit_xor",    [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        bit_not:       decl!("jit_bit_not",    [ptr, ptr, i32t, i32t],                    [i8t]),
        shl:           decl!("jit_shl",        [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        shr:           decl!("jit_shr",        [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        str_concat:    decl!("jit_str_concat", [ptr, ptr, i32t, i32t, i32t],              [i8t]),
        to_str:        decl!("jit_to_str",     [ptr, ptr, i32t, i32t],                    [i8t]),
        // jit_call(frame, ctx, dst, method_id, name_ptr, name_len, args_ptr, argc, ic_ptr, caller_line, caller_col) -> u8
        // formalize-jit-method-token Phase 2.C (2026-05-08): id-first dispatch
        // with name fallback for cross-zpkg UNRESOLVED targets.
        // span-column-propagate (2026-05-10): trailing `i32t` adds caller column.
        // make-vm-loading-lazy: `ic_ptr` (before line/col) caches the resolved
        // lazy/merged fn id per call site → lock-free by-id fast path.
        call:          decl!("jit_call",       [ptr, ptr, i32t, i32t, ptr, i64t, ptr, i64t, ptr, i32t, i32t, i32t], [i8t]),
        // jit_builtin(frame, ctx, dst, builtin_id, name_ptr, name_len, args_ptr, argc) -> u8
        // formalize-jit-method-token (2026-05-08): id-based dispatch (no hash).
        // fix-jit-builtin-ext-fallback: name_ptr/len added so an UNRESOLVED id (native-ext
        // facade unresolved at compile time) can resolve by name at call time.
        builtin:       decl!("jit_builtin",    [ptr, ptr, i32t, i32t, ptr, i64t, ptr, i64t], [i8t]),
        array_new:     decl!("jit_array_new",     [ptr, ptr, i32t, i32t, i8t, ptr, i64t], [i8t]),
        array_new_lit: decl!("jit_array_new_lit", [ptr, ptr, i32t, ptr, i64t, ptr, i64t], [i8t]),
        array_get:     decl!("jit_array_get",     [ptr, ptr, i32t, i32t, i32t],           [i8t]),
        // jit_array_data(frame, ctx, arr, out_ptr, out_len, out_width) -> u8 (0 ok / 1 exc)
        array_data:    decl!("jit_array_data",    [ptr, ptr, i32t, ptr, ptr, ptr],        [i8t]),
        // jit_array_data_opt(frame, ctx, arr, out_ptr, out_len, out_width) -> () — non-throwing hoist
        array_data_opt: decl!("jit_array_data_opt", [ptr, ptr, i32t, ptr, ptr, ptr],       []),
        array_set:     decl!("jit_array_set",     [ptr, ptr, i32t, i32t, i32t],           [i8t]),
        array_len:     decl!("jit_array_len",     [ptr, ptr, i32t, i32t],                 [i8t]),
        // jit_obj_new(frame, ctx, dst, cls_ptr, cls_len, ctor_ptr, ctor_len, args_ptr, argc,
        //             type_args_ptr, type_args_count, ctorless_mark_ptr)
        //             type_args_ptr, type_args_count) -> u8
        obj_new:       decl!("jit_obj_new",    [ptr, ptr, i32t, ptr, i64t, ptr, i64t, ptr, i64t, ptr, i64t, ptr], [i8t]),
        // jit_typeof(frame, ctx, dst, type_name_ptr, type_name_len, type_args_ptr, type_args_count)
        // add-reflection-generic-type-definition: type_args_ptr is `*const String`
        // into the IR `Instruction::Typeof { type_args }` storage (module lifetime).
        typeof_op:     decl!("jit_typeof",     [ptr, ptr, i32t, ptr, i64t, ptr, i64t], []),
        // formalize-jit-method-token Phase 2.E (2026-05-08): trailing `ptr`
        // arg = `*const FieldIC` (stable into Function.resolved.field_ic).
        field_get:     decl!("jit_field_get",  [ptr, ptr, i32t, i32t, ptr, i64t, ptr],    [i8t]),
        // P5-B: jit_obj_field_slot(frame, ctx, obj, name_ptr, name_len, exp_width, exp_tag, out_bytes_ptr, out_off) -> ()
        obj_field_slot: decl!("jit_obj_field_slot", [ptr, ptr, i32t, ptr, i64t, i32t, i32t, ptr, ptr], []),
        // T1-B: jit_obj_ref_field_slot(frame, ctx, obj, name_ptr, name_len, out_bytes_ptr, out_off, out_tag) -> ()
        obj_ref_field_slot: decl!("jit_obj_ref_field_slot", [ptr, ptr, i32t, ptr, i64t, ptr, ptr, ptr], []),
        field_set:     decl!("jit_field_set",  [ptr, ptr, i32t, ptr, i64t, i32t, ptr],    [i8t]),
        // jit_vcall(frame, ctx, dst, obj, method_ptr, method_len, args_ptr, argc, ic_ptr, caller_line, caller_col) -> u8
        // Phase 2.E: trailing `ptr` arg = `*const VCallIC`.
        // jit-stack-trace (2026-05-10): trailing two `i32t` = caller (line, col).
        vcall:         decl!("jit_vcall",      [ptr, ptr, i32t, i32t, ptr, i64t, ptr, i64t, ptr, i32t, i32t, i32t], [i8t]),
        is_instance:   decl!("jit_is_instance",[ptr, ptr, i32t, i32t, ptr, i64t],         []),
        as_cast:       decl!("jit_as_cast",    [ptr, ptr, i32t, i32t, ptr, i64t],         []),
        // formalize-jit-method-token Phase 2 (2026-05-08): id-based
        // make-vm-loading-lazy: trailing (field_ptr, field_len) for by-name
        // fallback when field_id is UNRESOLVED (lazily-loaded fn, no resolved table).
        // (jit_static_get(frame, ctx, dst, field_id, field_ptr, field_len),
        //  jit_static_set(frame, ctx, field_id, val, field_ptr, field_len))
        static_get:    decl!("jit_static_get", [ptr, ptr, i32t, i32t, ptr, i64t],          []),
        static_set:    decl!("jit_static_set", [ptr, ptr, i32t, i32t, ptr, i64t],          []),
        get_bool:      decl!("jit_get_bool",      [ptr, ptr, i32t],                       [i8t]),
        set_ret:       decl!("jit_set_ret",       [ptr, ptr, i32t],                       []),
        // C2 P1 step 1: jit_regs_ptr(frame) -> *mut Value. Note: NO ctx param.
        regs_ptr:      decl!("jit_regs_ptr",      [ptr],                                  [ptr]),
        // jit_throw(frame, ctx, reg, throw_line, throw_col, throw_offset) — jit-stack-trace + offline-symbolication
        throw:         decl!("jit_throw",         [ptr, ptr, i32t, i32t, i32t, i32t],     []),
        install_catch: decl!("jit_install_catch", [ptr, ptr, i32t],                       []),
        // jit_match_catch_type(frame, ctx, target_ptr, target_len) -> i8
        match_catch_type: decl!("jit_match_catch_type", [ptr, ptr, ptr, i64t],            [i8t]),
        // add-gc-safepoint-jit (2026-05-21): jit_check_safepoint(frame, ctx) -> void
        check_safepoint:  decl!("jit_check_safepoint",  [ptr, ptr],                       []),
        // inline-jit-safepoint-check (2026-08-01): jit_check_safepoint_slow(frame, ctx) -> void
        check_safepoint_slow: decl!("jit_check_safepoint_slow", [ptr, ptr],               []),
        // jit_load_fn(frame, ctx, dst, name_ptr, name_len) -> u8
        load_fn:        decl!("jit_load_fn",       [ptr, ptr, i32t, ptr, i64t],                  [i8t]),
        // jit_mk_clos(frame, ctx, dst, name_ptr, name_len, caps_ptr, caps_len, stack_alloc:u8) -> u8
        mk_clos:        decl!("jit_mk_clos",       [ptr, ptr, i32t, ptr, i64t, ptr, i64t, i8t], [i8t]),
        // jit_call_indirect(frame, ctx, dst, callee, args_ptr, args_len, caller_line, caller_col) -> u8
        call_indirect:  decl!("jit_call_indirect", [ptr, ptr, i32t, i32t, ptr, i64t, i32t, i32t, i32t], [i8t]),
        // jit_load_fn_cached(frame, ctx, dst, name_ptr, name_len, slot_id) -> u8
        load_fn_cached: decl!("jit_load_fn_cached", [ptr, ptr, i32t, ptr, i64t, i32t],           [i8t]),
        // jit_default_of(frame, ctx, dst, param_index) -> u8
        default_of:     decl!("jit_default_of",     [ptr, ptr, i32t, i32t],                      [i8t]),
        // jit_convert(frame, ctx, dst, src, to_tag) -> u8  (spec fix-numeric-cast-lowering)
        convert:        decl!("jit_convert",        [ptr, ptr, i32t, i32t, i32t],                [i8t]),
        // add-struct-jit-value-path (P5): blob value-type instruction helpers.
        // jit_struct_alloc(frame, ctx, dst, type_ptr, type_len, size) -> ()
        struct_alloc:          decl!("jit_struct_alloc",          [ptr, ptr, i32t, ptr, i64t, i32t], []),
        // jit_struct_copy(frame, ctx, dst, src, size) -> u8
        struct_copy:           decl!("jit_struct_copy",           [ptr, ptr, i32t, i32t, i32t],      [i8t]),
        // jit_struct_field_get_prim(frame, ctx, dst, base, byte_off, kind) -> u8
        struct_field_get_prim: decl!("jit_struct_field_get_prim", [ptr, ptr, i32t, i32t, i32t, i8t], [i8t]),
        // jit_struct_field_set_prim(frame, ctx, base, byte_off, kind, val) -> u8
        struct_field_set_prim: decl!("jit_struct_field_set_prim", [ptr, ptr, i32t, i32t, i8t, i32t], [i8t]),
    })
}
