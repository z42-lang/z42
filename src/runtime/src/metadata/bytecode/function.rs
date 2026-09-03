//! Function / FunctionCold / 帧名 / LocalVar / LineEntry / ExceptionEntry / BasicBlock。refactor-split-bytecode（2026-09-03）：从 1334 行的 `bytecode.rs` 按职责拆出，
//! 对外路径不变（`metadata::bytecode::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{ExecMode, TypeDesc};
use crate::metadata::bytecode_serde::{typed_reg_serde, typed_reg_vec_serde, typed_reg_opt_serde};
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Format a function's stack-trace display name with parameter signature.
/// Returns `<name>(<t1>,<t2>,...)` (e.g. `Demo.Greeter.greet(str)`).
/// Empty signature is `<name>()`. Used by VM frame push sites so traces
/// disambiguate overloads (1.3 split-debug-symbols Phase 4).
pub fn format_frame_name(func: &Function) -> String {
    let mut out = String::with_capacity(func.name.len() + 2 + func.param_count * 4);
    out.push_str(&func.name);
    out.push('(');
    for (i, t) in func.param_types().iter().enumerate() {
        if i > 0 { out.push(','); }
        out.push_str(t);
    }
    // When SIGS lacks per-param types (older artifacts or null source), fall
    // back to "?" placeholders matching `param_count` so the shape is
    // recognizable.
    if func.param_types().is_empty() && func.param_count > 0 {
        for i in 0..func.param_count {
            if i > 0 { out.push(','); }
            out.push('?');
        }
    }
    out.push(')');
    out
}

/// Cold (rarely-accessed) slice fields on `Function`. Boxed behind an
/// `Option` on Function so functions with no debug info, no try/catch,
/// no params, and no generics carry only an 8-byte null pointer instead
/// of six `Box<[T]>` headers (96 B inline → 8 B Option<Box>).
///
/// review.md E2.P5 (2026-05-27). Mirror of CoreCLR's split between
/// `MethodDesc` (hot, 32 B base) and `MethodDescChunk` / cold side
/// tables.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FunctionCold {
    /// 1.3 split-debug-symbols: per-parameter type names for stack-trace
    /// signature decoration. Length always equals `param_count` (zbc writer
    /// pads unknowns with "?"). Empty when param_count == 0.
    #[serde(default)]
    pub param_types: Box<[String]>,
    /// Exception handler ranges. Populated only when the function body
    /// contains `try` / `catch` / `finally`.
    #[serde(default)]
    pub exception_table: Box<[ExceptionEntry]>,
    /// Source-line mapping table (run-length encoded). Populated only when
    /// the module is built with debug symbols (DBUG section / sidecar).
    #[serde(default)]
    pub line_table: Box<[LineEntry]>,
    /// Debug info: maps register IDs to source-level variable names.
    /// Populated only with debug symbols.
    #[serde(default)]
    pub local_vars: Box<[LocalVar]>,
    /// Generic type parameter names: ["T"], ["K", "V"]. Empty for non-generic functions.
    #[serde(default)]
    pub type_params: Box<[String]>,
    /// L3-G3a: constraint bundle per type parameter (aligned by index with `type_params`).
    #[serde(default)]
    pub type_param_constraints: Box<[ConstraintBundle]>,
    /// C3b add-attribute-reflection-methods: user attributes applied to this
    /// method / top-level function (from the zbc SIGS section). Each points at a
    /// synthesized factory the runtime calls for `MethodInfo.GetCustomAttributes()`.
    #[serde(default)]
    pub custom_attributes: Box<[AttributeRef]>,
    /// add-parameter-attribute-reflection (zbc 1.15): per-parameter user
    /// attributes, aligned by index with the SIGS parameter array (length ==
    /// `param_count`, including the implicit `this` slot at index 0 for instance
    /// methods, which is empty). `loader` re-indexes these by source position
    /// (excluding `this`) for `ParameterInfo.GetCustomAttributes()`.
    #[serde(default)]
    pub param_attributes: Box<[Box<[AttributeRef]>]>,
    /// add-param-metadata (unify P1-d): per-param source name (SIGS
    /// `name_str_idx`), aligned by index with the SIGS parameter array (this-slot
    /// = "this"). Backs `ParameterInfo.Name` (authoritative over DBUG guess).
    #[serde(default)]
    pub param_names: Box<[String]>,
    /// add-param-metadata (unify P1-d): per-param default value `(kind, i64, str)`
    /// (kind 0=none/1=null/2=i64/3=f64bits/4=bool/5=str). Backs
    /// `ParameterInfo.DefaultValue`. Aligned by SIGS param index.
    #[serde(default)]
    pub param_defaults: Box<[(u8, i64, String)]>,
}

/// A single function.
///
/// review.md E2.P5 (2026-05-27): six rarely-accessed slice fields moved
/// into [`FunctionCold`] behind `Option<Box>`. Reads go through accessor
/// methods that return `&[T]` (empty slice when cold is absent). Sidecar
/// mutations (`loader.rs` debug-symbol overlay) lazy-init via
/// [`Function::cold_mut`].
/// serde default for `Function::params_from` (add-param-metadata): 0xFF = no varargs.
fn default_params_from() -> u8 { 0xFF }

#[derive(Debug, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    /// Number of parameters — they occupy registers 0..param_count-1 on entry.
    pub param_count: usize,
    /// Return type tag: "void", "str", "i32", "i64", "f64", "bool".
    pub ret_type: String,
    pub exec_mode: ExecMode,
    pub blocks: Vec<BasicBlock>,
    /// True for static class methods (no implicit `this` receiver).
    /// Instance methods have `this` as reg 0 and should not be treated as
    /// static-only entries in the StdlibCallIndex.
    #[serde(default)]
    pub is_static: bool,
    /// Member visibility (add-member-visibility, unify P1-b): 0=public /
    /// 1=private / 2=protected. Populated from the SIGS entry's `visibility:u8`
    /// at module load (mirrors `is_static`), so `MethodInfo.IsPublic` can
    /// report it via reflection. Defaults to 0 (public) for synthesized funcs.
    #[serde(default)]
    pub visibility: u8,
    /// Method modifiers (add-method-modifiers, unify P1-c): bit0=virtual /
    /// bit1=abstract. Populated from the SIGS entry's `method_flags:u8` at
    /// module load (mirrors `visibility`), so `MethodInfo.IsVirtual`
    /// (authoritative) / `IsAbstract` can report it via reflection.
    /// Defaults to 0 (non-virtual) for synthesized funcs.
    #[serde(default)]
    pub method_flags: u8,
    /// Required (logical) param count (add-param-metadata, unify P1-d): from the
    /// SIGS `min_arg:u16`. `ParameterInfo.IsOptional` = (logical pos >= min_arg).
    #[serde(default)]
    pub min_arg: u16,
    /// Params-varargs logical index (add-param-metadata): SIGS `params_from:u8`,
    /// 0xFF = none. `ParameterInfo.IsParams` = (logical pos == params_from).
    #[serde(default = "default_params_from")]
    pub params_from: u8,
    /// Total number of registers used (0 = unknown; VM falls back to dynamic sizing).
    #[serde(default)]
    pub max_reg: u32,
    /// Cold side-table (param_types / exception_table / line_table /
    /// local_vars / type_params / type_param_constraints). `None` for
    /// the common case of a non-generic function with no try/catch and
    /// no debug symbols. Reads go through accessor methods on `Function`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cold: Option<Box<FunctionCold>>,
    /// review.md C2 step 0.2 (2026-05-27): per-register static type
    /// from the C# IR's `TypedReg.Type`. Indexed by register ID;
    /// length matches `max_reg`. Empty `Box<[]>` when no REGT section
    /// is present in the zbc (legacy fixtures + writer-not-yet-updated
    /// path). JIT translate.rs reads this to specialize arithmetic /
    /// comparison / logical ops on known primitives (`I64` → emit
    /// Cranelift `iadd` instead of `jit_add` helper call).
    #[serde(default, skip)]
    pub reg_types: Box<[crate::metadata::ir_type::IrType]>,
    /// Precomputed block label → index mapping. Not serialized; populated after module load.
    #[serde(skip)]
    pub block_index: std::collections::HashMap<String, usize>,
    /// perf-vm-iteration: per-block pre-resolved branch targets (indices),
    /// parallel to `blocks`. Lets `Br`/`BrCond` jump by index instead of
    /// SipHashing the label string every back-edge (~25% of interp loop time).
    /// Not serialized; populated by `loader::build_block_indices`. Empty ⇒
    /// runtime falls back to `block_index` (hand-built test functions).
    #[serde(skip)]
    pub branch_targets: Vec<BranchTargets>,
    /// interp-superinstr-fusion (2026-08-01): per-block fused-tail super-instruction
    /// (e.g. `cmp`+`BrCond` → `CmpBr`), parallel to `blocks`. Recognized once at
    /// load by `superinstr::compute_fused_tails`; the interp reads
    /// `fused_tails[block_idx]` (O(1)) to run the fused step and skip a dispatch on
    /// hot loops. `None`/empty ⇒ no fusion for that block (normal execution).
    /// Not serialized (pure runtime optimization; no zbc/format impact).
    #[serde(skip)]
    pub fused_tails: Vec<Option<crate::metadata::superinstr::SuperInstr>>,
    /// perf-frame-name-precompute: the stack-frame display name (`"Fn(params)"`)
    /// + source file as `Arc<str>`, precomputed once at load (like
    /// `branch_targets`). `exec_function_body` clones these O(1) per call instead
    /// of re-running `format_frame_name` (String alloc + format) + a file clone
    /// on **every** call — that was 40–60% of call-heavy interp time (measured).
    /// `None` for hand-built test functions the loader never post-processes → the
    /// interp falls back to formatting on the fly.
    #[serde(skip)]
    pub frame_meta: Option<(std::sync::Arc<str>, std::sync::Arc<str>)>,
    /// Per-function token cache (introduce-method-token, 2026-05-08).
    /// Lazy-init by `metadata::resolver::resolve_module` after module load.
    /// `OnceLock` so `Function: Sync` is preserved (single-thread today,
    /// future multi-thread ready). Not serialized — purely runtime metadata.
    #[serde(skip)]
    pub resolved: std::sync::OnceLock<crate::metadata::resolver::ResolvedTokens>,
}
impl Function {
    /// Borrow the cold side-table or return a static empty slice. Accessor
    /// methods below all delegate here.
    #[inline]
    fn cold_slice<T, F: FnOnce(&FunctionCold) -> &[T]>(&self, f: F) -> &[T] {
        match self.cold.as_ref() {
            Some(c) => f(c),
            None    => &[],
        }
    }

    #[inline] pub fn param_types(&self)             -> &[String]           { self.cold_slice(|c| &c.param_types) }
    #[inline] pub fn exception_table(&self)         -> &[ExceptionEntry]   { self.cold_slice(|c| &c.exception_table) }
    #[inline] pub fn line_table(&self)              -> &[LineEntry]        { self.cold_slice(|c| &c.line_table) }
    #[inline] pub fn local_vars(&self)              -> &[LocalVar]         { self.cold_slice(|c| &c.local_vars) }
    #[inline] pub fn type_params(&self)             -> &[String]           { self.cold_slice(|c| &c.type_params) }
    #[inline] pub fn type_param_constraints(&self)  -> &[ConstraintBundle] { self.cold_slice(|c| &c.type_param_constraints) }
    /// C3b add-attribute-reflection-methods: user attributes applied to this function.
    #[inline] pub fn custom_attributes(&self)       -> &[AttributeRef]     { self.cold_slice(|c| &c.custom_attributes) }
    /// add-parameter-attribute-reflection: per-parameter attributes (SIGS-aligned,
    /// incl. `this` slot). Empty slice when the cold side-table is absent.
    #[inline] pub fn param_attributes(&self)        -> &[Box<[AttributeRef]>] { self.cold_slice(|c| &c.param_attributes) }
    #[inline] pub fn param_names(&self)             -> &[String]           { self.cold_slice(|c| &c.param_names) }
    #[inline] pub fn param_defaults(&self)          -> &[(u8, i64, String)] { self.cold_slice(|c| &c.param_defaults) }

    /// Number of register slots this function's activation frame needs — the
    /// COUNT (max register index + 1) covering: params (they occupy the low
    /// registers), every instruction's `dst`, and exception-table catch
    /// registers. Catch registers are written by the runtime at catch-install
    /// and may not be referenced by any surviving instruction after IR
    /// DCE/copy-prop, so they are folded in explicitly — otherwise the frame
    /// under-sizes and OOB-panics on catch.
    ///
    /// The interp loader backfills `self.max_reg` with this once post-load
    /// (`loader::build_block_indices`) so `Frame::new*` pre-sizes the register
    /// file in a single `resize` instead of growing one slot at a time via the
    /// cold `set_grow` path (~3.7% on call-heavy interp workloads). The JIT's
    /// `translate::max_reg` (which wants the max index) is this minus one.
    ///
    /// **Pure** — does NOT read `self.max_reg` (which is 0 at backfill time and
    /// the value we are computing), so it is safe to call while filling it.
    pub fn reg_file_len(&self) -> u32 {
        let mut max_idx = self.param_count.saturating_sub(1) as u32;
        for e in self.exception_table() {
            if e.catch_reg > max_idx { max_idx = e.catch_reg; }
        }
        for block in &self.blocks {
            for instr in &block.instructions {
                if let Some(d) = instr.written_reg() {
                    if d > max_idx { max_idx = d; }
                }
            }
        }
        max_idx + 1
    }

    /// Lazy-init the cold side-table for mutation. Used by sidecar debug-
    /// symbol overlay in `metadata::loader`.
    #[inline]
    pub fn cold_mut(&mut self) -> &mut FunctionCold {
        self.cold.get_or_insert_with(|| Box::new(FunctionCold::default()))
    }

    // ── add-offline-symbolication: code-offset ↔ (block, instr) mapping ────────
    //
    // z42 IR is a block+intra-block-instruction model with no linear bytecode
    // address, but offline symbolication (and the stripped-frame stack format
    // `at <func> +0x<offset>`) needs one stable key per site. We pack the site
    // into a single u32:
    //
    //   offset(block, instr) = (block << 16) | (instr & 0xffff)
    //
    // This is **O(1)** — critical because `update_caller_line` computes it on
    // every Call/VCall (a prefix-sum linearization was measured ~5% slower on
    // dispatch-heavy loops). The key is opaque but stable and block-major
    // monotonic; the user only treats `+0x<offset>` as a token to feed
    // `z42d symbolicate`, which unpacks it back to `(block, instr)` and looks up
    // the archived `.zsym` line table. `block`/`instr` fit u16 in every real z42
    // function (a basic block never holds 2^16 instructions, nor a function 2^16
    // blocks); `debug_assert` guards the invariant. This is the SINGLE source of
    // truth — the z42-side `z42d symbolicate` mirrors the same pack/unpack.

    /// Packed code offset for a `(block, instr)` site — `(block << 16) | instr`.
    /// O(1). `block`/`instr` come from the interp/JIT loop state and are trusted
    /// in range; the u16 bound is a `debug_assert` (see the module note).
    #[inline]
    pub fn linear_offset(&self, block: u32, instr: u32) -> u32 {
        debug_assert!(block <= 0xffff && instr <= 0xffff,
            "code offset packing overflow: block={block} instr={instr}");
        (block << 16) | (instr & 0xffff)
    }

    /// Inverse of [`linear_offset`]: unpack `(block, instr)` from a code offset.
    /// Used by `z42d symbolicate` (and tests) to map a captured `+0x<offset>`
    /// back to a site, then to a `LineEntry`.
    #[inline]
    pub fn offset_to_site(&self, offset: u32) -> (u32, u32) {
        (offset >> 16, offset & 0xffff)
    }
}
/// An entry in a function's local variable table: register `reg` holds variable `name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalVar {
    pub name: String,
    pub reg:  u16,
}

/// An entry in a function's source-line mapping table.
///
/// 2026-05-10 span-column-propagate (zbc 1.1): `column` carries 1-based
/// source column from `Span.Column`. Value `0` means unknown (legacy
/// hand-rolled IR or pre-1.1 zbc never reach here — reader rejects).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineEntry {
    pub block:  u32,
    pub instr:  u32,
    pub line:   u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file:   Option<String>,
    #[serde(default)]
    pub column: u32,
}

/// One row in a function's exception table.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExceptionEntry {
    pub try_start:   String,
    pub try_end:     String,
    pub catch_label: String,
    pub catch_type:  Option<String>,
    #[serde(with = "typed_reg_serde")]
    pub catch_reg:   u32,
}

/// A basic block — straight-line instructions ending in exactly one terminator.
#[derive(Debug, Serialize, Deserialize)]
pub struct BasicBlock {
    pub label: String,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}
