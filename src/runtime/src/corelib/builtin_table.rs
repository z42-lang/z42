//! `BUILTINS` 第 1 段 —— 原始登记表（2026-05-14 首次追加点之前）。
//!
//! **约定上只在表尾追加**：BuiltinId 就是本表的下标。表内多处 "appended to preserve
//! existing BuiltinIds" 注释记录了历次追加点。
//!
//! 🔴 **本注原先写着「BuiltinId 会被烤进 zbc；在中间插入会让既有产物里的调用全部错位」——
//! 那句话不成立**（2026-09-26 由 `builtin_table_ext.rs` 抬头读码核实，本处 2026-09-27 更正）：
//! zbc 里存的是**名字**（`BuiltinInsn { dst, name, args }`），`BuiltinId` 由 resolver 在
//! **加载期**按名字填进 `Function.resolved.builtin_tokens`，AOT 也不烤它 ⇒ 它是单次运行内的
//! 派发令牌，不跨进程持久化。append-only 的真实价值是让按 id 做的测试/遥测稳定、
//! 以及避免 review 时重核整张表。两份抬头此前互相矛盾，以核实过的那份为准。
//!
//! refactor-split-corelib-mod（complete-runtime-settings P5 前置，2026-09-05）：
//! 自 `corelib/mod.rs` 逐行搬出的**纯移动**（无逻辑改动）。mod.rs 长期 665 行、在
//! line-limit 棘轮基线上，再追加一条 builtin 就会让 `xtask test lines` 变红；而表
//! 本身是数据、按名字线性增长，与 mod.rs 里的分发逻辑变更频率完全不同。

use super::*;

use super::builtin_table_ext::PART2;

/// Single source of truth for all corelib builtins. Each entry's index
/// (position in this slice) is its **stable `BuiltinId`** for the lifetime
/// of the process — the resolver assigns IDs by walking this slice.
///
/// `introduce-method-token` (2026-05-08): replaces ad-hoc `HashMap` with an
/// indexed array so dispatch hot path can `BUILTINS[id.0 as usize].1(ctx, args)`
/// without hashing. The HashMap-based `exec_builtin(name, args)` entry point
/// remains as fallback for paths that haven't threaded `BuiltinId` yet (e.g.
/// JIT helpers Phase 1).
const PART1: &[(&str, Native)] = &[
    // ── I/O ────────────────────────────────────────────────────────────────────
    ("__println",  Native::Void(io::builtin_println)),
    ("__print",    Native::Void(io::builtin_print)),
    ("__eprintln", Native::Void(io::builtin_eprintln)),
    ("__eprint",   Native::Void(io::builtin_eprint)),
    ("__readline", Native::Val(io::builtin_readline)),
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
    ("__str_length",      Native::Val(string::builtin_str_length)),
    ("__str_byte_length", Native::Val(string::builtin_str_byte_length)),
    ("__str_char_at",     Native::Val(string::builtin_str_char_at)),
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

    // ── Parse / convert ───────────────────────────────────────────────────────
    //
    // rename-primitives-to-pascal-case (2026-05-24): builtin names now follow
    // BCL convention (Int32 / Int64 / SByte / Byte / Single / Boolean / ...).
    // BUILTINS array position is the stable BuiltinId — entry order preserved.
    ("__int64_parse",   Native::Val(convert::builtin_int64_parse)),
    ("__int32_parse",   Native::Val(convert::builtin_int32_parse)),
    // add-narrow-int-primitives (2026-05-15): per-type Parse with range
    // validation. Underlying Value is still I64; these only differ from
    // __int32_parse in the [min, max] check.
    ("__sbyte_parse",   Native::Val(convert::builtin_sbyte_parse)),
    ("__int16_parse",   Native::Val(convert::builtin_int16_parse)),
    ("__byte_parse",    Native::Val(convert::builtin_byte_parse)),
    ("__uint16_parse",  Native::Val(convert::builtin_uint16_parse)),
    ("__uint32_parse",  Native::Val(convert::builtin_uint32_parse)),
    ("__uint64_parse",  Native::Val(convert::builtin_uint64_parse)),
    ("__double_parse",  Native::Val(convert::builtin_double_parse)),
    ("__to_str",        Native::Val(convert::builtin_to_str)),
    ("__box_prim",      Native::Val(convert::builtin_box_prim)),
    ("__box_struct",    Native::Val(convert::builtin_box_struct)),
    ("__struct_hash_code", Native::Val(convert::builtin_struct_hash_code)),

    // ── Primitive IComparable / IEquatable (L3-G4b) ───────────────────────────
    // `__int32_*` underlying routines are shared by all narrow integer wrapper
    // types (Int16 / SByte / Byte / UInt16 / UInt32 / UInt64 / Int64) since
    // VM stores them all as Value::I64.
    // shrink-primitive-native-interop (2026-08-27): __int32_equals/__int32_hash_code,
    // __double_equals/__double_hash_code, __char_equals/__char_hash_code removed —
    // 迁纯脚本（对齐 wave1-bool-script）。ToString/Parse/compare 保留 native。
    ("__int32_to_string",   Native::Val(convert::builtin_int32_to_string)),
    ("__double_to_string",  Native::Val(convert::builtin_double_to_string)),
    // add-binary-float (2026-06-09): IEEE-754 bit reinterpret for BinaryReader/Writer
    ("__single_to_bits",    Native::Val(convert::builtin_single_to_bits)),
    ("__single_from_bits",  Native::Val(convert::builtin_single_from_bits)),
    ("__double_to_bits",    Native::Val(convert::builtin_double_to_bits)),
    ("__double_from_bits",  Native::Val(convert::builtin_double_from_bits)),
    ("__char_to_string",    Native::Val(convert::builtin_char_to_string)),
    ("__str_compare_to",    Native::Val(convert::builtin_str_compare_to)),

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

    // ── File I/O ──────────────────────────────────────────────────────────────
    ("__file_read_text",   Native::Val(fs::builtin_file_read_text)),
    ("__file_write_text",  Native::Void(fs::builtin_file_write_text)),
    ("__file_append_text", Native::Void(fs::builtin_file_append_text)),
    ("__file_exists",      Native::Val(fs::builtin_file_exists)),
    ("__file_delete",      Native::Void(fs::builtin_file_delete)),
    ("__file_last_write_time_ms", Native::Val(fs::builtin_file_last_write_time_ms)),

    // ── Directory（add-std-io-directory，2026-05-13）──────────────────────────
    ("__dir_exists",              Native::Val(fs::builtin_dir_exists)),
    ("__dir_create",              Native::Void(fs::builtin_dir_create)),
    ("__dir_delete",              Native::Void(fs::builtin_dir_delete)),
    ("__dir_enumerate",           Native::Val(fs::builtin_dir_enumerate)),
    ("__dir_enumerate_recursive", Native::Val(fs::builtin_dir_enumerate_recursive)),

    // ── Glob + Temp（extend-z42-io-glob-temp，2026-05-16）─────────────────────
    ("__path_glob",             Native::Val(fs::builtin_path_glob)),
    ("__file_create_temp_dir",  Native::Val(fs::builtin_file_create_temp_dir)),
    ("__file_create_temp_file", Native::Val(fs::builtin_file_create_temp_file)),

    // ── Script helpers（extend-z42-io-script-helpers, 2026-05-16）────────────
    ("__file_make_executable",      Native::Void(fs::builtin_file_make_executable)),
    ("__file_link",                 Native::Void(fs::builtin_file_link)),
    ("__file_symlink",              Native::Void(fs::builtin_file_symlink)),
    ("__file_get_size",             Native::Val(fs::builtin_file_get_size)),
    ("__console_is_terminal",       Native::Val(fs::builtin_console_is_terminal)),
    ("__console_error_is_terminal", Native::Val(fs::builtin_console_error_is_terminal)),
    ("__env_get_cwd",               Native::Val(fs::builtin_env_get_cwd)),
    ("__env_set_cwd",               Native::Void(fs::builtin_env_set_cwd)),

    // ── Environment / Process ─────────────────────────────────────────────────
    ("__env_get",      Native::Val(fs::builtin_env_get)),
    ("__env_args",     Native::Val(fs::builtin_env_args)),
    ("__process_exit", Native::Void(fs::builtin_process_exit)),
    ("__time_now_ms",  Native::Val(fs::builtin_time_now_ms)),

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
    ("__type_full_name",     Native::Val(reflection::builtin_type_full_name)),
    ("__type_element",       Native::Val(reflection::builtin_type_element)),
    ("__type_fields",        Native::Val(reflection::builtin_type_fields)),
    ("__type_methods",       Native::Val(reflection::builtin_type_methods)),
    // add-reflective-invoke: constructor reflection (ConstructorInfo enumeration).
    ("__type_constructors",  Native::Val(reflection::builtin_type_constructors)),
    ("__type_base",          Native::Val(reflection::builtin_type_base)),
    ("__type_generic_args",  Native::Val(reflection::builtin_type_generic_args)),
    ("__type_interfaces",    Native::Val(reflection::builtin_type_interfaces)),
    ("__type_members",       Native::Val(reflection::builtin_type_members)),
    ("__type_properties",    Native::Val(reflection::builtin_type_properties)),
    ("__type_is_abstract",   Native::Val(reflection::builtin_type_is_abstract)),
    ("__type_is_sealed",     Native::Val(reflection::builtin_type_is_sealed)),
    ("__type_is_value_type", Native::Val(reflection::builtin_type_is_value_type)),
    ("__type_is_record",     Native::Val(reflection::builtin_type_is_record)),
    ("__type_is_generic",    Native::Val(reflection::builtin_type_is_generic)),
    ("__type_is_primitive",  Native::Val(reflection::builtin_type_is_primitive)),
    ("__type_is_generic_definition", Native::Val(reflection::builtin_type_is_generic_definition)),
    ("__type_generic_definition",    Native::Val(reflection::builtin_type_generic_definition)),
    // plan-generic-reflection G1: runtime generic instantiation (MakeGenericType,
    // constraint-validated). Constructed CreateInstance is handled in the existing
    // __activator_create builtin (reifies __typeArgs onto the new instance).
    ("__type_make_generic",          Native::Val(reflection::builtin_type_make_generic)),
    ("__type_is_interface",  Native::Val(reflection::builtin_type_is_interface)),
    // add-enum-type-metadata (unify-type-metadata P1-a): enum reflection.
    ("__type_is_enum",       Native::Val(reflection::builtin_type_is_enum)),
    ("__type_is_delegate",   Native::Val(reflection::builtin_type_is_delegate)),
    ("__enum_names",         Native::Val(reflection::builtin_enum_names)),
    ("__enum_values",        Native::Val(reflection::builtin_enum_values)),
    ("__enum_name",          Native::Val(reflection::builtin_enum_name)),
    ("__type_is_class",      Native::Val(reflection::builtin_type_is_class)),
    ("__type_is_assignable_from", Native::Val(reflection::builtin_type_is_assignable_from)),
    ("__type_custom_attributes", Native::Val(reflection::builtin_type_custom_attributes)),
    ("__method_custom_attributes", Native::Val(reflection::builtin_method_custom_attributes)),
    ("__field_custom_attributes", Native::Val(reflection::builtin_field_custom_attributes)),
    ("__param_custom_attributes", Native::Val(reflection::builtin_param_custom_attributes)),
    // add-json-serde: property-attribute reflection (reads the auto-property backing
    // field `__prop_<Name>`'s field_attributes — no wire-format bump).
    ("__property_custom_attributes", Native::Val(reflection::builtin_property_custom_attributes)),
    // add-method-invoke-non-generic (0.3.12): reflective invocation primitives.
    ("__type_get_type",      Native::Val(reflection::builtin_type_get_type)),
    ("__method_invoke",      Native::Val(reflection::builtin_method_invoke)),
    // add-reflective-invoke (G2): generic-method reflection — MakeGenericMethod
    // produces a constructed MethodInfo; GetGenericArguments reads type params/args;
    // Invoke on the constructed MethodInfo threads __typeArgs into the callee frame.
    ("__method_make_generic",       Native::Val(reflection::builtin_method_make_generic)),
    ("__method_generic_arguments",  Native::Val(reflection::builtin_method_generic_arguments)),
    // add-property-getvalue-setvalue: reflective property read/write (reuses the
    // non-generic invoke path via get_<X>/set_<X> accessors).
    ("__property_get_value", Native::Val(reflection::builtin_property_get_value)),
    ("__property_set_value", Native::Void(reflection::builtin_property_set_value)),
    // plan-generic-reflection (serde-driven): reflective field read/write (direct
    // slot access, powers reflective deserialization onto plain public fields).
    ("__field_get_value",    Native::Val(reflection::builtin_field_get_value)),
    ("__field_set_value",    Native::Void(reflection::builtin_field_set_value)),
    // retire-test-runner: no-arg reflective construction (test-class instantiation).
    ("__activator_create",   Native::Val(reflection::builtin_activator_create)),
    // add-reflective-invoke: ConstructorInfo.Invoke — parameterised construction.
    ("__ctor_invoke",        Native::Val(reflection::builtin_ctor_invoke)),
    // retire-test-runner: load a compiled test module + return its TIDX entries.
    ("__load_module",        Native::Val(reflection::builtin_load_module)),
    // retire-test-runner: invoke a free/static [Test]/[Benchmark] function by FQN
    // (zero-arg) — stdlib tests are free functions, not class instance methods.
    ("__invoke_static",      Native::Val(reflection::builtin_invoke_static)),
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

    // ── add-std-io-polish (2026-05-12) — appended to preserve existing BuiltinIds ──
    ("__file_copy",  Native::Void(fs::builtin_file_copy)),
    ("__file_move",  Native::Void(fs::builtin_file_move)),
    ("__env_set",    Native::Void(fs::builtin_env_set)),

    // ── add-std-process (2026-05-13) — appended to preserve existing BuiltinIds ──
    ("__process_run",                 Native::Val(process::builtin_process_run)),
    ("__process_spawn",               Native::Val(process::builtin_process_spawn)),
    ("__process_handle_wait",         Native::Val(process::builtin_process_handle_wait)),
    ("__process_handle_try_wait",     Native::Val(process::builtin_process_handle_try_wait)),
    ("__process_handle_kill",         Native::Val(process::builtin_process_handle_kill)),
    ("__process_handle_write_stdin",  Native::Val(process::builtin_process_handle_write_stdin)),
    ("__process_handle_close_stdin",  Native::Val(process::builtin_process_handle_close_stdin)),
    ("__process_handle_pid",          Native::Val(process::builtin_process_handle_pid)),
    ("__process_handle_drop",         Native::Val(process::builtin_process_handle_drop)),
];

/// 完整表 = PART1 ++ PART2，**编译期**拼接：BuiltinId 就是最终下标，运行期再拼会
/// 让热路径 `BUILTINS[id].1(..)` 多一次间接。分两个文件纯粹是行数硬限所迫
/// （单文件 515 行 > 500）——切点选在历史首个「appended to preserve BuiltinIds」
/// 边界，语义上就是「原始表 ++ 追加日志」，不是按行数腰斩。
const TOTAL: usize = PART1.len() + PART2.len();

const fn joined() -> [(&'static str, Native); TOTAL] {
    // 填充值只为让数组可 const 初始化；下面的两个循环会覆盖每一格（长度即 TOTAL）。
    let mut out = [("", Native::Val(nop as NativeFn)); TOTAL];
    let mut i = 0;
    while i < PART1.len() { out[i] = PART1[i]; i += 1; }
    let mut j = 0;
    while j < PART2.len() { out[PART1.len() + j] = PART2[j]; j += 1; }
    out
}

fn nop(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    unreachable!("builtin table padding slot is always overwritten by joined()")
}

static JOINED: [(&str, Native); TOTAL] = joined();

pub(crate) const BUILTINS: &[(&str, Native)] = &JOINED;
