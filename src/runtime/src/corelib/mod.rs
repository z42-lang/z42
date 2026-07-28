/// Core library — native function implementations backing the z42 standard library.
///
/// All functions are reachable via a single stable entry point `exec_builtin(name, args)`
/// which is called by:
///   • the interpreter  (`Instruction::Builtin` in interp/mod.rs)
///   • the JIT backend  (`jit_builtin` extern "C" helper in jit/helpers.rs)
///
/// Submodules are organised by functional category (≈ CoreCLR `classlibnative/`):
///   `convert`     — value_to_str, require_str/usize, parse/to_str
///   `io`          — println, print, readline, concat, len, contains
///   `string`      — str_substring/contains/split/join/format …
///   `math`        — abs/max/min/pow/sqrt/trig …
///   `fs`          — file_* / path_* / env_* / process_exit / time_now_ms
///   `object`      — obj_get_type / obj_ref_eq / obj_hash_code / assert_*
///
/// 2026-04-26 script-first-stringbuilder: removed `string_builder` module —
/// `Std.Text.StringBuilder` is now a pure z42 script in `z42.text`,
/// backed by `List<string>` + `String.FromChars` (no VM intrinsic needed).
///
/// 2026-04-26 extern-audit-wave0: removed `collections` module (13 builtins)
/// — `Std.Collections.List<T>` / `Dictionary<K,V>` are pure z42 scripts atop
/// `T[]`; compiler stopped emitting `__list_*` / `__dict_*` after L3-G4h step3.
///
/// 2026-04-27 wave1-assert-script: removed 6 `__assert_*` builtins —
/// `Std.Assert` methods are now pure z42 scripts (`if (!cond) throw new
/// Exception(...)`), matching BCL `Debug.Assert` / Rust `assert!`.

pub mod convert;
pub mod io;
pub mod repl;
pub mod string;
pub mod str_meta;
pub mod math;
pub mod fs;
pub mod object;
pub mod reflection;
pub mod array;
pub mod char;
pub mod gc;
pub mod bench;
pub mod process;
pub mod platform;
pub mod system;
pub mod threading;
pub mod sync;
pub mod network;
pub mod tls;
pub mod crypto;

use crate::metadata::tokens::BuiltinId;
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Function pointer type for all native builtins.
///
/// Carries `&VmContext` so builtins can access the GC heap (e.g. allocate
/// `Std.Type` objects via `ctx.heap().alloc_object(...)`) and other runtime
/// state. **2026-04-29 extend-native-fn-signature** added `&VmContext` —
/// previously `fn(&[Value]) -> Result<Value>`, which forced corelib allocation
/// callsites to bypass the heap interface.
pub type NativeFn = fn(&VmContext, &[Value]) -> Result<Value>;

/// Single source of truth for all corelib builtins. Each entry's index
/// (position in this slice) is its **stable `BuiltinId`** for the lifetime
/// of the process — the resolver assigns IDs by walking this slice.
///
/// `introduce-method-token` (2026-05-08): replaces ad-hoc `HashMap` with an
/// indexed array so dispatch hot path can `BUILTINS[id.0 as usize].1(ctx, args)`
/// without hashing. The HashMap-based `exec_builtin(name, args)` entry point
/// remains as fallback for paths that haven't threaded `BuiltinId` yet (e.g.
/// JIT helpers Phase 1).
const BUILTINS: &[(&str, NativeFn)] = &[
    // ── I/O ────────────────────────────────────────────────────────────────────
    ("__println",  io::builtin_println),
    ("__print",    io::builtin_print),
    ("__eprintln", io::builtin_eprintln),
    ("__eprint",   io::builtin_eprint),
    ("__readline", io::builtin_readline),
    ("__concat",   io::builtin_concat),
    ("__len",      io::builtin_len),
    ("__contains", io::builtin_contains),

    // ── TestIO sinks (R2 完整版) ──────────────────────────────────────────────
    ("__test_io_install_stdout_sink", io::builtin_test_io_install_stdout_sink),
    ("__test_io_take_stdout_buffer",  io::builtin_test_io_take_stdout_buffer),
    ("__test_io_install_stderr_sink", io::builtin_test_io_install_stderr_sink),
    ("__test_io_take_stderr_buffer",  io::builtin_test_io_take_stderr_buffer),

    // ── Time + bencher helpers ──────────────────────────────────────────────
    ("__time_now_mono_ns", bench::builtin_time_now_mono_ns),
    ("__bench_black_box",  bench::builtin_bench_black_box),

    // ── String (minimal intrinsic core; most methods are script-side now) ────
    ("__str_length",      string::builtin_str_length),
    ("__str_byte_length", string::builtin_str_byte_length),
    ("__str_char_at",     string::builtin_str_char_at),
    ("__str_from_chars", string::builtin_str_from_chars),
    ("__str_to_string",  string::builtin_str_to_string),
    ("__str_equals",     string::builtin_str_equals),
    ("__str_hash_code",  string::builtin_str_hash_code),

    // ── Char ──────────────────────────────────────────────────────────────────
    ("__char_is_whitespace", char::builtin_char_is_whitespace),
    ("__char_to_lower",      char::builtin_char_to_lower),
    ("__char_to_upper",      char::builtin_char_to_upper),

    // ── Parse / convert ───────────────────────────────────────────────────────
    //
    // rename-primitives-to-pascal-case (2026-05-24): builtin names now follow
    // BCL convention (Int32 / Int64 / SByte / Byte / Single / Boolean / ...).
    // BUILTINS array position is the stable BuiltinId — entry order preserved.
    ("__int64_parse",   convert::builtin_int64_parse),
    ("__int32_parse",   convert::builtin_int32_parse),
    // add-narrow-int-primitives (2026-05-15): per-type Parse with range
    // validation. Underlying Value is still I64; these only differ from
    // __int32_parse in the [min, max] check.
    ("__sbyte_parse",   convert::builtin_sbyte_parse),
    ("__int16_parse",   convert::builtin_int16_parse),
    ("__byte_parse",    convert::builtin_byte_parse),
    ("__uint16_parse",  convert::builtin_uint16_parse),
    ("__uint32_parse",  convert::builtin_uint32_parse),
    ("__uint64_parse",  convert::builtin_uint64_parse),
    ("__double_parse",  convert::builtin_double_parse),
    ("__to_str",        convert::builtin_to_str),
    ("__box_prim",      convert::builtin_box_prim),

    // ── Primitive IComparable / IEquatable (L3-G4b) ───────────────────────────
    // `__int32_*` underlying routines are shared by all narrow integer wrapper
    // types (Int16 / SByte / Byte / UInt16 / UInt32 / UInt64 / Int64) since
    // VM stores them all as Value::I64.
    ("__int32_equals",      convert::builtin_int32_equals),
    ("__int32_hash_code",   convert::builtin_int32_hash_code),
    ("__int32_to_string",   convert::builtin_int32_to_string),
    ("__double_equals",     convert::builtin_double_equals),
    ("__double_hash_code",  convert::builtin_double_hash_code),
    ("__double_to_string",  convert::builtin_double_to_string),
    // add-binary-float (2026-06-09): IEEE-754 bit reinterpret for BinaryReader/Writer
    ("__single_to_bits",    convert::builtin_single_to_bits),
    ("__single_from_bits",  convert::builtin_single_from_bits),
    ("__double_to_bits",    convert::builtin_double_to_bits),
    ("__double_from_bits",  convert::builtin_double_from_bits),
    ("__char_equals",       convert::builtin_char_equals),
    ("__char_hash_code",    convert::builtin_char_hash_code),
    ("__char_to_string",    convert::builtin_char_to_string),
    ("__str_compare_to",    convert::builtin_str_compare_to),

    // ── Math ──────────────────────────────────────────────────────────────────
    ("__math_pow",     math::builtin_math_pow),
    ("__math_sqrt",    math::builtin_math_sqrt),
    ("__math_floor",   math::builtin_math_floor),
    ("__math_ceiling", math::builtin_math_ceiling),
    ("__math_round",   math::builtin_math_round),
    ("__math_log",     math::builtin_math_log),
    ("__math_log10",   math::builtin_math_log10),
    ("__math_sin",     math::builtin_math_sin),
    ("__math_cos",     math::builtin_math_cos),
    ("__math_tan",     math::builtin_math_tan),
    ("__math_atan2",   math::builtin_math_atan2),
    ("__math_exp",     math::builtin_math_exp),

    // ── File I/O ──────────────────────────────────────────────────────────────
    ("__file_read_text",   fs::builtin_file_read_text),
    ("__file_write_text",  fs::builtin_file_write_text),
    ("__file_append_text", fs::builtin_file_append_text),
    ("__file_exists",      fs::builtin_file_exists),
    ("__file_delete",      fs::builtin_file_delete),
    ("__file_last_write_time_ms", fs::builtin_file_last_write_time_ms),

    // ── Directory（add-std-io-directory，2026-05-13）──────────────────────────
    ("__dir_exists",              fs::builtin_dir_exists),
    ("__dir_create",              fs::builtin_dir_create),
    ("__dir_delete",              fs::builtin_dir_delete),
    ("__dir_enumerate",           fs::builtin_dir_enumerate),
    ("__dir_enumerate_recursive", fs::builtin_dir_enumerate_recursive),

    // ── Glob + Temp（extend-z42-io-glob-temp，2026-05-16）─────────────────────
    ("__path_glob",             fs::builtin_path_glob),
    ("__file_create_temp_dir",  fs::builtin_file_create_temp_dir),
    ("__file_create_temp_file", fs::builtin_file_create_temp_file),

    // ── Script helpers（extend-z42-io-script-helpers, 2026-05-16）────────────
    ("__file_make_executable",      fs::builtin_file_make_executable),
    ("__file_link",                 fs::builtin_file_link),
    ("__file_symlink",              fs::builtin_file_symlink),
    ("__file_get_size",             fs::builtin_file_get_size),
    ("__console_is_terminal",       fs::builtin_console_is_terminal),
    ("__console_error_is_terminal", fs::builtin_console_error_is_terminal),
    ("__env_get_cwd",               fs::builtin_env_get_cwd),
    ("__env_set_cwd",               fs::builtin_env_set_cwd),

    // ── Environment / Process ─────────────────────────────────────────────────
    ("__env_get",      fs::builtin_env_get),
    ("__env_args",     fs::builtin_env_args),
    ("__process_exit", fs::builtin_process_exit),
    ("__time_now_ms",  fs::builtin_time_now_ms),

    // ── Object protocol ───────────────────────────────────────────────────────
    ("__obj_get_type",  object::builtin_obj_get_type),
    ("__obj_ref_eq",    object::builtin_obj_ref_eq),
    ("__obj_hash_code", object::builtin_obj_hash_code),
    ("__obj_equals",    object::builtin_obj_equals),
    ("__obj_to_str",    object::builtin_obj_to_str),
    ("__delegate_eq",   object::builtin_delegate_eq),
    ("__delegate_target", object::builtin_delegate_target),
    ("__delegate_fn_name", object::builtin_delegate_fn_name),
    ("__make_closure", object::builtin_make_closure),
    ("__obj_make_weak", object::builtin_obj_make_weak),
    ("__obj_upgrade_weak", object::builtin_obj_upgrade_weak),
    // ── Reflection (add-reflection-mvp, 2026-06-08) ─────────────────────────────
    // align-type-memberinfo-hierarchy: `__type_name` removed — Type.Name inherits
    // from MemberInfo (build_type populates the field), no native getter.
    ("__type_full_name",     reflection::builtin_type_full_name),
    ("__type_element",       reflection::builtin_type_element),
    ("__type_fields",        reflection::builtin_type_fields),
    ("__type_methods",       reflection::builtin_type_methods),
    ("__type_base",          reflection::builtin_type_base),
    ("__type_generic_args",  reflection::builtin_type_generic_args),
    ("__type_interfaces",    reflection::builtin_type_interfaces),
    ("__type_members",       reflection::builtin_type_members),
    ("__type_properties",    reflection::builtin_type_properties),
    ("__type_is_abstract",   reflection::builtin_type_is_abstract),
    ("__type_is_sealed",     reflection::builtin_type_is_sealed),
    ("__type_is_value_type", reflection::builtin_type_is_value_type),
    ("__type_is_record",     reflection::builtin_type_is_record),
    ("__type_is_generic",    reflection::builtin_type_is_generic),
    ("__type_is_primitive",  reflection::builtin_type_is_primitive),
    ("__type_is_generic_definition", reflection::builtin_type_is_generic_definition),
    ("__type_generic_definition",    reflection::builtin_type_generic_definition),
    // plan-generic-reflection G1: runtime generic instantiation (MakeGenericType,
    // constraint-validated). Constructed CreateInstance is handled in the existing
    // __activator_create builtin (reifies __typeArgs onto the new instance).
    ("__type_make_generic",          reflection::builtin_type_make_generic),
    ("__type_is_interface",  reflection::builtin_type_is_interface),
    // add-enum-type-metadata (unify-type-metadata P1-a): enum reflection.
    ("__type_is_enum",       reflection::builtin_type_is_enum),
    ("__type_is_delegate",   reflection::builtin_type_is_delegate),
    ("__enum_names",         reflection::builtin_enum_names),
    ("__enum_values",        reflection::builtin_enum_values),
    ("__enum_name",          reflection::builtin_enum_name),
    ("__type_is_class",      reflection::builtin_type_is_class),
    ("__type_is_assignable_from", reflection::builtin_type_is_assignable_from),
    ("__type_custom_attributes", reflection::builtin_type_custom_attributes),
    ("__method_custom_attributes", reflection::builtin_method_custom_attributes),
    ("__field_custom_attributes", reflection::builtin_field_custom_attributes),
    ("__param_custom_attributes", reflection::builtin_param_custom_attributes),
    // add-method-invoke-non-generic (0.3.12): reflective invocation primitives.
    ("__type_get_type",      reflection::builtin_type_get_type),
    ("__method_invoke",      reflection::builtin_method_invoke),
    // add-property-getvalue-setvalue: reflective property read/write (reuses the
    // non-generic invoke path via get_<X>/set_<X> accessors).
    ("__property_get_value", reflection::builtin_property_get_value),
    ("__property_set_value", reflection::builtin_property_set_value),
    // plan-generic-reflection (serde-driven): reflective field read/write (direct
    // slot access, powers reflective deserialization onto plain public fields).
    ("__field_get_value",    reflection::builtin_field_get_value),
    ("__field_set_value",    reflection::builtin_field_set_value),
    // retire-test-runner: no-arg reflective construction (test-class instantiation).
    ("__activator_create",   reflection::builtin_activator_create),
    // retire-test-runner: load a compiled test module + return its TIDX entries.
    ("__load_module",        reflection::builtin_load_module),
    // retire-test-runner: invoke a free/static [Test]/[Benchmark] function by FQN
    // (zero-arg) — stdlib tests are free functions, not class instance methods.
    ("__invoke_static",      reflection::builtin_invoke_static),
    // add-reflection-generic-type-definition: `typeof` now lowers to the Typeof
    // opcode (interp/jit), not a builtin — the former `__typeof` is removed.

    // ── Array protocol（add-array-base-class，2026-05-07）─────────────────────
    ("__array_clone", array::builtin_array_clone),

    // ── GC control（Phase 3d.2 expose-gc-to-scripts） ────────────────────────
    ("__gc_collect",       gc::builtin_gc_collect),
    ("__gc_used_bytes",    gc::builtin_gc_used_bytes),
    ("__gc_force_collect", gc::builtin_gc_force_collect),
    // ── add-custom-allocator P2 (2026-05-22) ─────────────────────────────
    ("__gc_finalize",      gc::builtin_gc_finalize),

    // ── GCHandle struct + HeapStats（reorganize-gc-stdlib，2026-05-07）───────
    ("__gc_handle_alloc",    gc::builtin_gc_handle_alloc),
    ("__gc_handle_target",   gc::builtin_gc_handle_target),
    ("__gc_handle_is_alloc", gc::builtin_gc_handle_is_alloc),
    ("__gc_handle_kind",     gc::builtin_gc_handle_kind),
    ("__gc_handle_free",     gc::builtin_gc_handle_free),
    ("__gc_stats",           gc::builtin_gc_stats),

    // ── add-std-io-polish (2026-05-12) — appended to preserve existing BuiltinIds ──
    ("__file_copy",  fs::builtin_file_copy),
    ("__file_move",  fs::builtin_file_move),
    ("__env_set",    fs::builtin_env_set),

    // ── add-std-process (2026-05-13) — appended to preserve existing BuiltinIds ──
    ("__process_run",                 process::builtin_process_run),
    ("__process_spawn",               process::builtin_process_spawn),
    ("__process_handle_wait",         process::builtin_process_handle_wait),
    ("__process_handle_try_wait",     process::builtin_process_handle_try_wait),
    ("__process_handle_kill",         process::builtin_process_handle_kill),
    ("__process_handle_write_stdin",  process::builtin_process_handle_write_stdin),
    ("__process_handle_close_stdin",  process::builtin_process_handle_close_stdin),
    ("__process_handle_pid",          process::builtin_process_handle_pid),
    ("__process_handle_drop",         process::builtin_process_handle_drop),

    // ── add-platform-os-stdlib (2026-05-14) — appended to preserve existing BuiltinIds ──
    ("__platform_os",         platform::builtin_platform_os),
    ("__platform_arch",       platform::builtin_platform_arch),
    ("__platform_family",     platform::builtin_platform_family),
    ("__platform_os_kind",    platform::builtin_platform_os_kind),
    ("__platform_arch_kind",  platform::builtin_platform_arch_kind),
    ("__system_pid",          system::builtin_system_pid),
    ("__system_exe_path",     system::builtin_system_exe_path),
    ("__system_cwd",          system::builtin_system_cwd),
    ("__system_set_cwd",      system::builtin_system_set_cwd),
    ("__system_hostname",     system::builtin_system_hostname),
    ("__system_cpu_count",    system::builtin_system_cpu_count),
    ("__system_os_version",   system::builtin_system_os_version),
    ("__env_unset",           fs::builtin_env_unset),
    ("__env_vars",            fs::builtin_env_vars),

    // ── add-threading-stdlib (2026-05-20) — appended to preserve existing BuiltinIds ──
    ("__thread_spawn",        threading::builtin_thread_spawn),
    ("__thread_join",         threading::builtin_thread_join),

    // ── add-sync-primitives (2026-05-20) — appended to preserve existing BuiltinIds ──
    ("__mutex_new",           sync::builtin_mutex_new),
    ("__mutex_lock_acquire",  sync::builtin_mutex_lock_acquire),
    ("__mutex_store",         sync::builtin_mutex_store),
    ("__mutex_unlock",        sync::builtin_mutex_unlock),
    ("__channel_new",         sync::builtin_channel_new),
    ("__channel_send",        sync::builtin_channel_send),
    ("__channel_recv",        sync::builtin_channel_recv),
    ("__channel_try_recv",    sync::builtin_channel_try_recv),
    ("__channel_close",       sync::builtin_channel_close),

    // ── add-sync-primitives-bounded-channel (2026-05-20) — appended to preserve existing BuiltinIds ──
    ("__channel_new_bounded", sync::builtin_channel_new_bounded),

    // ── add-sync-primitives-rwlock (2026-05-20) — appended to preserve existing BuiltinIds ──
    ("__rwlock_new",           sync::builtin_rwlock_new),
    ("__rwlock_read_acquire",  sync::builtin_rwlock_read_acquire),
    ("__rwlock_read_release",  sync::builtin_rwlock_read_release),
    ("__rwlock_write_acquire", sync::builtin_rwlock_write_acquire),
    ("__rwlock_write_store",   sync::builtin_rwlock_write_store),
    ("__rwlock_write_release", sync::builtin_rwlock_write_release),

    // ── add-sync-primitives-try-variants (2026-05-20) — appended to preserve existing BuiltinIds ──
    ("__channel_try_send",     sync::builtin_channel_try_send),
    ("__rwlock_try_read",      sync::builtin_rwlock_try_read),
    ("__rwlock_try_write",     sync::builtin_rwlock_try_write),

    // ── add-gc-pause-histogram (2026-05-22) — appended to preserve existing BuiltinIds ──
    ("__gc_pause_histogram", gc::builtin_gc_pause_histogram),
    ("__gc_pause_stats_raw", gc::builtin_gc_pause_stats_raw),

    // ── add-z42-compression (2026-05-22): __deflate_* / __zstd_* / __compressor_*
    //    builtins are NOT statically registered here — they're provided by the
    //    z42-compression cdylib, dlopen'd at VM startup (or statically linked
    //    on wasm via the `bundled-compression` feature). Resolved through
    //    `VmCore.ext_builtins` (see corelib::ext_builtin_id_of below).

    // ── add-z42-io-filestream (2026-05-24) — appended to preserve existing BuiltinIds ──
    ("__file_open",      fs::builtin_file_open),
    ("__file_read",      fs::builtin_file_read),
    ("__file_write",     fs::builtin_file_write),
    ("__file_seek",      fs::builtin_file_seek),
    ("__file_length",    fs::builtin_file_length),
    ("__file_position",  fs::builtin_file_position),
    ("__file_flush",     fs::builtin_file_flush),
    ("__file_close",     fs::builtin_file_close),

    // ── add-process-stream-stdio (2026-05-24) — appended to preserve existing BuiltinIds ──
    ("__process_handle_read_stdout", process::builtin_process_handle_read_stdout),
    ("__process_handle_read_stderr", process::builtin_process_handle_read_stderr),

    // ── add-z42-net K1 (2026-05-24) — appended to preserve existing BuiltinIds ──
    ("__net_tcp_connect",       network::builtin_net_tcp_connect),
    ("__net_tcp_listen",        network::builtin_net_tcp_listen),
    ("__net_tcp_accept",        network::builtin_net_tcp_accept),
    ("__net_tcp_socket_read",   network::builtin_net_tcp_socket_read),
    ("__net_tcp_socket_write",  network::builtin_net_tcp_socket_write),
    ("__net_tcp_socket_drop",   network::builtin_net_tcp_socket_drop),
    ("__net_tcp_listener_drop", network::builtin_net_tcp_listener_drop),

    // ── add-gc-heap-snapshot-export B3 (2026-05-24) — appended to preserve existing BuiltinIds ──
    ("__gc_write_heap_snapshot", gc::builtin_gc_write_heap_snapshot),

    // ── add-gc-pause-window (2026-05-24) — appended to preserve existing BuiltinIds ──
    ("__gc_recent_pauses",         gc::builtin_gc_recent_pauses),
    ("__gc_pause_window_capacity", gc::builtin_gc_pause_window_capacity),

    // ── add-gc-oom-exception (2026-05-25) — appended to preserve existing BuiltinIds ──
    ("__gc_set_max_heap_bytes", gc::builtin_gc_set_max_heap_bytes),
    ("__gc_set_strict_oom",     gc::builtin_gc_set_strict_oom),

    // ── add-z42-net-udp K2 (2026-05-25) — appended to preserve existing BuiltinIds ──
    ("__net_udp_bind", network::builtin_net_udp_bind),
    ("__net_udp_send", network::builtin_net_udp_send),
    ("__net_udp_recv", network::builtin_net_udp_recv),
    ("__net_udp_drop", network::builtin_net_udp_drop),

    // ── add-gc-softref (2026-05-26) ──────────────────────────────────────────
    ("__soft_handle_create", gc::builtin_soft_handle_create),
    ("__soft_handle_get",    gc::builtin_soft_handle_get),

    // ── add-process-which (2026-05-26) — appended to preserve existing BuiltinIds ──
    ("__process_which", process::builtin_process_which),

    // ── add-csprng-to-crypto (2026-05-27) — OS-CSPRNG backing Std.Crypto.SecureRandom ──
    ("__crypto_random_bytes", crypto::builtin_crypto_random_bytes),

    // ── add-z42-io-ergonomics-bytes-glob (2026-05-27) — one-shot binary IO ──
    ("__file_read_bytes",  fs::builtin_file_read_bytes),
    ("__file_write_bytes", fs::builtin_file_write_bytes),

    // ── add-file-atomic-write (2026-05-27) — write-fsync-rename for durable config ──
    ("__file_write_text_atomic",  fs::builtin_file_write_text_atomic),
    ("__file_write_bytes_atomic", fs::builtin_file_write_bytes_atomic),

    // ── add-httpclient-timeout (2026-05-27) — TCP socket read/write deadlines ──
    ("__net_tcp_socket_set_read_timeout",  network::builtin_net_tcp_socket_set_read_timeout),
    ("__net_tcp_socket_set_write_timeout", network::builtin_net_tcp_socket_set_write_timeout),

    // ── add-thread-sleep (2026-05-27) — blocking sleep ──
    ("__thread_sleep", threading::builtin_thread_sleep),

    // ── add-z42-net-udp-recv-into (2026-05-27) — buffer-fill Receive variant ──
    ("__net_udp_recv_into", network::builtin_net_udp_recv_into),

    // ── add-z42-net-udp-multicast (2026-05-27) — IPv4 multicast group ops ──
    ("__net_udp_join_multicast",      network::builtin_net_udp_join_multicast),
    ("__net_udp_leave_multicast",     network::builtin_net_udp_leave_multicast),
    ("__net_udp_set_multicast_loop",  network::builtin_net_udp_set_multicast_loop),

    // ── add-z42-net-dns (2026-05-27) — synchronous DNS resolution ──
    ("__net_dns_lookup",              network::builtin_net_dns_lookup),

    // ── add-z42-net-socket-options (2026-05-27) — TCP_NODELAY / IP_TTL ──
    ("__net_tcp_socket_set_nodelay",  network::builtin_net_tcp_socket_set_nodelay),
    ("__net_tcp_socket_set_ttl",      network::builtin_net_tcp_socket_set_ttl),
    ("__net_tcp_listener_set_ttl",    network::builtin_net_tcp_listener_set_ttl),
    ("__net_udp_set_ttl",             network::builtin_net_udp_set_ttl),

    // ── add-net-socket-options-extended (2026-05-30) — connect/UDP timeout, SO_REUSEADDR, SO_KEEPALIVE ──
    ("__net_tcp_connect_with_timeout", network::builtin_net_tcp_connect_with_timeout),
    ("__net_tcp_socket_set_keepalive", network::builtin_net_tcp_socket_set_keepalive),
    ("__net_tcp_socket_set_keepalive_tuned", network::builtin_net_tcp_socket_set_keepalive_tuned),
    ("__net_tcp_listen_with_options",  network::builtin_net_tcp_listen_with_options),
    ("__net_udp_set_read_timeout",     network::builtin_net_udp_set_read_timeout),
    ("__net_udp_set_write_timeout",    network::builtin_net_udp_set_write_timeout),

    // ── add-z42-net-tls (2026-06-03) — rustls client TLS streams (HTTPS) ──
    ("__net_tls_connect",                  tls::builtin_net_tls_connect),
    ("__net_tls_socket_read",              tls::builtin_net_tls_socket_read),
    ("__net_tls_socket_write",             tls::builtin_net_tls_socket_write),
    ("__net_tls_socket_drop",              tls::builtin_net_tls_socket_drop),
    ("__net_tls_socket_set_read_timeout",  tls::builtin_net_tls_socket_set_read_timeout),
    ("__net_tls_socket_set_write_timeout", tls::builtin_net_tls_socket_set_write_timeout),

    // ── runtime-dynamic-load-call (DEFERRED) — stubs so zpkg loads cleanly ──
    ("__load_zpkg",  builtin_load_zpkg_stub),
    ("__call_static", builtin_call_static_stub),

    // ── add-z42-repl (2026-07-23) — appended to preserve existing BuiltinIds ──
    // REPL line editor (rustyline) + in-memory bytecode load. Back
    // `Std.Repl.ReadLine/ReadBlock` and z42.scripting's per-eval module load.
    ("__repl_readline",           repl::builtin_repl_readline),
    ("__repl_readblock",          repl::builtin_repl_readblock),
    ("__repl_complete_probe",     repl::builtin_repl_complete_probe),
    ("__repl_set_completer",      repl::builtin_repl_set_completer),
    ("__repl_member_names",       repl::builtin_repl_member_names),
    ("__load_bytecode_in_memory", reflection::builtin_load_bytecode_in_memory),

    // ── add-enum-parse-isdefined (2026-07-24) — appended to preserve BuiltinIds ──
    ("__enum_parse",              reflection::builtin_enum_parse),
    ("__enum_is_defined",         reflection::builtin_enum_is_defined),

    // ── add-enum-underlying-type (2026-07-25) — appended to preserve BuiltinIds ──
    ("__type_enum_underlying",    reflection::builtin_type_enum_underlying),

    // ── add-nested-types (2026-07-25) — appended to preserve BuiltinIds ──
    ("__type_is_nested",          reflection::builtin_type_is_nested),
    ("__type_declaring_type",     reflection::builtin_type_declaring_type),
    ("__type_nested_types",       reflection::builtin_type_nested_types),
];

// runtime-dynamic-load-call stubs (DEFERRED): registered so zpkgs that declare
// [Native("__load_zpkg")] / [Native("__call_static")] load cleanly; calls fail
// at runtime until the reflection MVP is complete.
fn builtin_load_zpkg_stub(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    anyhow::bail!("__load_zpkg: not yet implemented (runtime-dynamic-load-call DEFERRED)")
}
fn builtin_call_static_stub(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    anyhow::bail!("__call_static: not yet implemented (runtime-dynamic-load-call DEFERRED)")
}

/// Lazy-built `name → BuiltinId` index for `exec_builtin(name, args)` and the
/// resolver's `builtin_id_of` lookup. Built once on first access from
/// `BUILTINS` (single source of truth).
static BUILTIN_INDEX: OnceLock<HashMap<&'static str, u32>> = OnceLock::new();

fn builtin_index() -> &'static HashMap<&'static str, u32> {
    BUILTIN_INDEX.get_or_init(|| {
        BUILTINS.iter().enumerate()
            .map(|(i, (name, _))| (*name, i as u32))
            .collect()
    })
}

/// High bit of `BuiltinId.0` marks an ext (dlopen / bundled) builtin
/// resolved through `VmCore.ext_builtins` rather than the static
/// `BUILTINS` slice. Low 31 bits are the index into the ext table.
/// add-z42-compression (2026-05-22).
pub const BUILTIN_ID_EXT_BIT: u32 = 0x8000_0000;

/// Resolve a builtin name to its `BuiltinId`. Static `BUILTINS[]` first;
/// callers should fall back to [`ext_builtin_id_of`] if this returns
/// `None` (the resolver needs a `VmContext` for the ext table).
pub fn builtin_id_of(name: &str) -> Option<BuiltinId> {
    builtin_index().get(name).copied().map(BuiltinId)
}

/// Resolve a builtin name via the per-VM extension table populated at
/// VM startup by `native::ext::load_all`. Returns a `BuiltinId` whose
/// high bit is set; dispatch routes it through `ext_builtins.dispatch`.
/// Only available when the `native-interop` feature is enabled.
#[cfg(feature = "native-interop")]
pub fn ext_builtin_id_of(ctx: &VmContext, name: &str) -> Option<BuiltinId> {
    ctx.core.ext_builtins.lock().lookup_id(name)
        .map(|idx| BuiltinId(idx | BUILTIN_ID_EXT_BIT))
}

/// Fast-path dispatch by id. Static ids index into `BUILTINS`; ids with
/// the ext bit set index into `VmCore.ext_builtins.by_idx`.
#[inline]
pub fn exec_builtin_by_id(ctx: &VmContext, id: BuiltinId, args: &[Value]) -> Result<Value> {
    // add-runtime-counters (2026-05-26): observation-only fetch_add on
    // the hot path — single atomic Relaxed op, no control-flow impact.
    ctx.core.counters.builtin_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    #[cfg(feature = "native-interop")]
    if id.0 & BUILTIN_ID_EXT_BIT != 0 {
        let idx = id.0 & !BUILTIN_ID_EXT_BIT;
        let fn_ptr = {
            let ext = ctx.core.ext_builtins.lock();
            ext.dispatch(idx)
                .ok_or_else(|| anyhow::anyhow!("ext builtin id {} out of range", idx))?
        };
        return fn_ptr(ctx, args);
    }
    let idx = id.0 as usize;
    debug_assert!(idx < BUILTINS.len(), "BuiltinId {} out of range", id.0);
    BUILTINS[idx].1(ctx, args)
}

/// Stable public entry point — called by the interpreter and JIT `jit_builtin`.
/// Static `BUILTINS[]` first; ext (dlopened) second. A miss in both is a
/// hard error.
pub fn exec_builtin(ctx: &VmContext, name: &str, args: &[Value]) -> Result<Value> {
    // add-runtime-counters (2026-05-26): name-keyed slow path also increments
    // for consistency with exec_builtin_by_id (callers may hit either).
    ctx.core.counters.builtin_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    if let Some(&id) = builtin_index().get(name) {
        return BUILTINS[id as usize].1(ctx, args);
    }
    #[cfg(feature = "native-interop")]
    {
        let ext = ctx.core.ext_builtins.lock();
        if let Some(idx) = ext.lookup_id(name) {
            if let Some(fn_ptr) = ext.dispatch(idx) {
                drop(ext);  // release before invoking — wrappers may re-enter
                return fn_ptr(ctx, args);
            }
        }
    }
    Err(anyhow::anyhow!("unknown builtin `{name}`"))
}

#[cfg(test)]
mod tests;
