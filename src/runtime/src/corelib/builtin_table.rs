//! `BUILTINS` —— 旧表第 1 段（`PART1`），以及整张表的编译期拼接：
//! `PART1 ++ PART2（builtin_table_ext.rs）++ TYPED（builtin ABI v2 的 #[builtins] 模块）`。
//!
//! 下标就是 `BuiltinId`。zbc 里存的是**名字**（`BuiltinInsn { dst, name, args }`），`BuiltinId` 由
//! resolver 在**加载期**按名字填进 `Function.resolved.builtin_tokens`，AOT 也不烤它 ⇒ 它是单次运行内的
//! 派发令牌，不跨进程持久化：旧表项随领域迁到 ABI v2 时直接删掉，下标平移对既有产物没有影响。
//!
//! 新 builtin 不再加进旧表：写进所在领域的 `#[builtins]` 模块（`corelib/README.md`「加一个 builtin」）。

use super::*;

use super::builtin_table_ext::PART2;

/// The old-style entries (first half). Each entry's index in the joined table is its
/// `BuiltinId` for the lifetime of the process — the resolver assigns ids by name.
const PART1: &[(&str, Native)] = &[
    // ── I/O ────────────────────────────────────────────────────────────────────
    // `__println` / `__print` / `__eprintln` / `__eprint` / `__readline` ── 已迁 builtin ABI v2：见 `io::typed`。
    ("__concat",   Native::Val(io::builtin_concat)),
    ("__len",      Native::Val(io::builtin_len)),
    ("__contains", Native::Val(io::builtin_contains)),
    // ── TestIO sinks (R2 完整版) ──────────────────────────────────────────────
    ("__test_io_install_stdout_sink", Native::Void(io::builtin_test_io_install_stdout_sink)),
    ("__test_io_take_stdout_buffer",  Native::Val(io::builtin_test_io_take_stdout_buffer)),
    ("__test_io_install_stderr_sink", Native::Void(io::builtin_test_io_install_stderr_sink)),
    ("__test_io_take_stderr_buffer",  Native::Val(io::builtin_test_io_take_stderr_buffer)),
    // ── Time + bencher helpers ──────────────────────────────────────────────
    ("__time_now_mono_ns", Native::Val(bench::builtin_time_now_mono_ns)),
    ("__bench_black_box",  Native::Val(bench::builtin_bench_black_box)),
    // ── String (minimal intrinsic core; most methods are script-side now) ────
    ("__str_byte_length", Native::Val(string::builtin_str_byte_length)),
    ("__str_to_chars",    Native::Val(string::builtin_str_to_chars)),
    ("__str_from_chars", Native::Val(string::builtin_str_from_chars)),
    // shrink-primitive-native-interop (2026-08-27): __str_to_string removed —
    // Std.String.ToString 现在是脚本 `return this;`。
    ("__str_equals",     Native::Val(string::builtin_str_equals)),
    ("__str_hash_code",  Native::Val(string::builtin_str_hash_code)),
    // ── Char ──────────────────────────────────────────────────────────────────
    // shrink-primitive-native-interop (2026-08-27): __char_to_lower/__char_to_upper
    // removed — ASCII-only casing 迁纯脚本。IsWhiteSpace 保留（真 Unicode）。
    ("__char_is_whitespace", Native::Val(char::builtin_char_is_whitespace)),
    // ── Parse / convert / primitive ToString / BitConverter ── 已迁 builtin ABI v2：
    //    见 `convert::typed`（`#[builtins]` 模块，经下方 `TYPED` 登记）。

    // ── Math ──────────────────────────────────────────────────────────────────
    ("__math_pow",     Native::Val(math::builtin_math_pow)),
    ("__math_sqrt",    Native::Val(math::builtin_math_sqrt)),
    ("__math_floor",   Native::Val(math::builtin_math_floor)),
    ("__math_ceiling", Native::Val(math::builtin_math_ceiling)),
    ("__math_round",   Native::Val(math::builtin_math_round)),
    ("__math_log",     Native::Val(math::builtin_math_log)),
    ("__math_log10",   Native::Val(math::builtin_math_log10)),
    ("__math_sin",     Native::Val(math::builtin_math_sin)),
    ("__math_cos",     Native::Val(math::builtin_math_cos)),
    ("__math_tan",     Native::Val(math::builtin_math_tan)),
    ("__math_atan2",   Native::Val(math::builtin_math_atan2)),
    ("__math_exp",     Native::Val(math::builtin_math_exp)),
    ("__math_asin",    Native::Val(math::builtin_math_asin)),
    ("__math_acos",    Native::Val(math::builtin_math_acos)),
    ("__math_atan",    Native::Val(math::builtin_math_atan)),
    ("__math_sinh",    Native::Val(math::builtin_math_sinh)),
    ("__math_cosh",    Native::Val(math::builtin_math_cosh)),
    ("__math_tanh",    Native::Val(math::builtin_math_tanh)),
    ("__math_cbrt",    Native::Val(math::builtin_math_cbrt)),
    ("__math_log2",    Native::Val(math::builtin_math_log2)),
    // ── File / Directory / Glob / Temp / Console tty / Environment / clock ── 已迁 builtin ABI v2：
    //    见 `fs::typed`（FileStream 句柄、`__env_*`、`__process_exit`、`__time_now_ms` 也在那里）。

    // ── Object protocol ───────────────────────────────────────────────────────
    ("__obj_get_type",  Native::Val(object::builtin_obj_get_type)),
    ("__obj_ref_eq",    Native::Val(object::builtin_obj_ref_eq)),
    ("__obj_hash_code", Native::Val(object::builtin_obj_hash_code)),
    ("__obj_equals",    Native::Val(object::builtin_obj_equals)),
    ("__obj_to_str",    Native::Val(object::builtin_obj_to_str)),
    ("__delegate_eq",   Native::Val(object::builtin_delegate_eq)),
    ("__delegate_target", Native::Val(object::builtin_delegate_target)),
    ("__delegate_fn_name", Native::Val(object::builtin_delegate_fn_name)),
    ("__make_closure", Native::Val(object::builtin_make_closure)),
    ("__obj_make_weak", Native::Val(object::builtin_obj_make_weak)),
    ("__obj_upgrade_weak", Native::Val(object::builtin_obj_upgrade_weak)),
    // ── Reflection (add-reflection-mvp, 2026-06-08) ─────────────────────────────
    // align-type-memberinfo-hierarchy: `__type_name` removed — Type.Name inherits
    // from MemberInfo (build_type populates the field), no native getter.
    // add-reflective-invoke: constructor reflection (ConstructorInfo enumeration).
    // plan-generic-reflection G1: runtime generic instantiation (MakeGenericType,
    // constraint-validated). Constructed CreateInstance is handled in the existing
    // __activator_create builtin (reifies __typeArgs onto the new instance).
    // add-enum-type-metadata (unify-type-metadata P1-a): enum reflection.
    // add-json-serde: property-attribute reflection (reads the auto-property backing
    // field `__prop_<Name>`'s field_attributes — no wire-format bump).
    // add-method-invoke-non-generic (0.3.12): reflective invocation primitives.
    // add-reflective-invoke (G2): generic-method reflection — MakeGenericMethod
    // produces a constructed MethodInfo; GetGenericArguments reads type params/args;
    // Invoke on the constructed MethodInfo threads __typeArgs into the callee frame.
    // add-property-getvalue-setvalue: reflective property read/write (reuses the
    // non-generic invoke path via get_<X>/set_<X> accessors).
    // plan-generic-reflection (serde-driven): reflective field read/write (direct
    // slot access, powers reflective deserialization onto plain public fields).
    // retire-test-runner: no-arg reflective construction (test-class instantiation).
    // add-reflective-invoke: ConstructorInfo.Invoke — parameterised construction.
    // `Std.Runtime.ModuleLoader.Load`: merge a compiled artifact into the live VM (pure load).
    // retire-test-runner: invoke a free/static [Test]/[Benchmark] function by FQN
    // (zero-arg) — stdlib tests are free functions, not class instance methods.
    // add-reflection-generic-type-definition: `typeof` now lowers to the Typeof
    // opcode (interp/jit), not a builtin — the former `__typeof` is removed.

    // ── Array protocol（add-array-base-class，2026-05-07）─────────────────────
    ("__array_clone", Native::Val(array::builtin_array_clone)),
    // add-json-serde: reflective array create/get/set/length (build/read/write T[] whose
    // element type is only known as a runtime Type — serde `T[]` support).
    ("__array_create", Native::Val(array::builtin_array_create)),
    ("__array_get",    Native::Val(array::builtin_array_get)),
    ("__array_set",    Native::Void(array::builtin_array_set)),
    // ── GC control（Phase 3d.2 expose-gc-to-scripts） ────────────────────────
    ("__gc_collect",       Native::Void(gc::builtin_gc_collect)),
    ("__gc_used_bytes",    Native::Val(gc::builtin_gc_used_bytes)),
    ("__gc_force_collect", Native::Val(gc::builtin_gc_force_collect)),
    // ── add-custom-allocator P2 (2026-05-22) ─────────────────────────────
    ("__gc_finalize",      Native::Val(gc::builtin_gc_finalize)),
    // ── GCHandle struct + HeapStats（reorganize-gc-stdlib，2026-05-07）───────
    ("__gc_handle_alloc",    Native::Val(gc::builtin_gc_handle_alloc)),
    ("__gc_handle_target",   Native::Val(gc::builtin_gc_handle_target)),
    ("__gc_handle_is_alloc", Native::Val(gc::builtin_gc_handle_is_alloc)),
    ("__gc_handle_kind",     Native::Val(gc::builtin_gc_handle_kind)),
    ("__gc_handle_free",     Native::Void(gc::builtin_gc_handle_free)),
    ("__gc_stats",           Native::Val(gc::builtin_gc_stats)),
    // ── add-std-process (2026-05-13) ── seed compatibility only: the stdlib calls the typed
    //    `process::typed` builtins; these old shapes stay for the published seed (`process_legacy.rs`).
    ("__process_run",                 Native::Val(process_legacy::builtin_process_run)),
    ("__process_spawn",               Native::Val(process_legacy::builtin_process_spawn)),
    ("__process_handle_wait",         Native::Val(process_legacy::builtin_process_handle_wait)),
    ("__process_handle_try_wait",     Native::Val(process_legacy::builtin_process_handle_try_wait)),
    ("__process_handle_kill",         Native::Val(process_legacy::builtin_process_handle_kill)),
    ("__process_handle_write_stdin",  Native::Val(process_legacy::builtin_process_handle_write_stdin)),
    ("__process_handle_close_stdin",  Native::Val(process_legacy::builtin_process_handle_close_stdin)),
    ("__process_handle_pid",          Native::Val(process_legacy::builtin_process_handle_pid)),
    ("__process_handle_drop",         Native::Val(process_legacy::builtin_process_handle_drop)),
];

/// 完整表 = PART1 ++ PART2，**编译期**拼接：BuiltinId 就是最终下标，运行期再拼会
/// 让热路径 `BUILTINS[id].1(..)` 多一次间接。分两个文件纯粹是行数硬限所迫
/// （单文件 515 行 > 500）——切点选在历史首个「appended to preserve BuiltinIds」
/// 边界，语义上就是「原始表 ++ 追加日志」，不是按行数腰斩。
/// Builtin ABI v2（runtime-audit P1-8）：`#[z42_macros::builtins]` 模块各自的 `DESCS`，排在
/// PART1 ++ PART2 之后。**一个模块一行**——模块里写了 `#[builtin]` 即登记，不再逐函数列表。
/// 迁过来的 builtin 同时从 PART1 / PART2 删掉（名字唯一，`native_decl_tests` 核对）。
const TYPED: &[&[super::abi::BuiltinDesc]] = &[
    super::convert::typed::DESCS,
    super::string::typed::DESCS,
    super::reflection::enums::typed::DESCS,
    super::reflection::type_query::typed::DESCS,
    super::reflection::fields::typed::DESCS,
    super::reflection::methods::typed::DESCS,
    super::reflection::properties::typed::DESCS,
    super::reflection::attributes::typed::DESCS,
    super::reflection::generics::typed::DESCS,
    super::reflection::methodof::typed::DESCS,
    super::reflection::invoke::typed::DESCS,
    super::reflection::accessors::typed::DESCS,
    super::reflection::module_load::typed::DESCS,
    super::threading::typed::DESCS,
    super::fs::typed::DESCS,
    super::fs_backend::memory::typed::DESCS,
    super::process::typed::DESCS,
    super::io::typed::DESCS,
    super::repl::typed::DESCS,
    super::monitor::typed::DESCS,
    #[cfg(not(target_arch = "wasm32"))]
    super::network::tcp::typed::DESCS,
    #[cfg(not(target_arch = "wasm32"))]
    super::network::udp::typed::DESCS,
    #[cfg(not(target_arch = "wasm32"))]
    super::tls::typed::DESCS,
    #[cfg(target_arch = "wasm32")]
    super::network::wasm::typed::DESCS,
    #[cfg(test)]
    super::abi::test_builtins::DESCS,
];

/// Old-table builtins that may run z42 code on the calling thread (reflection invoke /
/// property access / attribute construction / type and module loading that runs
/// initializers, the REPL's callbacks): a folded call of their `[Native]` wrapper keeps a
/// light frame for the wrapper, so whatever that code throws shows the same stack as the
/// unfolded call (`interp::forward`). Conservative — every reflection / load-context / REPL
/// builtin. Typed builtins say it themselves (`#[builtin(reenters)]`); this list only
/// shrinks, as those areas migrate.
pub(crate) const LEGACY_REENTERS: &[&str] = &[
    "__type_is_class",
    "__repl_complete_probe", "__repl_set_completer", "__repl_set_key_editor", "__repl_set_keywords",
    "__repl_member_names",
    "__lctx_default", "__lctx_create_collectible", "__lctx_load",
    "__lctx_name", "__lctx_is_collectible", "__lctx_assemblies", "__asm_name",
    "__asm_is_collectible", "__asm_loadcontext", "__asm_get_types", "__type_is_collectible",
    "__type_assembly", "__lctx_unload",
    "__test_module_load", "__asm_get_functions",
];

const TYPED_N: usize = super::abi::descs_len(TYPED);

const TOTAL: usize = PART1.len() + PART2.len() + TYPED_N;

const fn joined() -> [(&'static str, Native); TOTAL] {
    // 填充值只为让数组可 const 初始化；下面的循环会覆盖每一格（长度即 TOTAL）。
    let mut out = [("", Native::Val(nop as NativeFn)); TOTAL];
    let mut i = 0;
    while i < PART1.len() { out[i] = PART1[i]; i += 1; }
    let mut j = 0;
    while j < PART2.len() { out[PART1.len() + j] = PART2[j]; j += 1; }
    let mut k = PART1.len() + PART2.len();
    let mut m = 0;
    while m < TYPED.len() {
        let mut d = 0;
        while d < TYPED[m].len() {
            let desc: &'static super::abi::BuiltinDesc = &TYPED[m][d];
            out[k] = (desc.name, Native::Typed(desc));
            k += 1;
            d += 1;
        }
        m += 1;
    }
    out
}

fn nop(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    unreachable!("builtin table padding slot is always overwritten by joined()")
}

static JOINED: [(&str, Native); TOTAL] = joined();

pub(crate) const BUILTINS: &[(&str, Native)] = &JOINED;
