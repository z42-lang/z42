//! `BUILTINS` 旧表第 2 段（`PART2`）—— 2026-05-14 起的历次追加。整张表的拼接与 `BuiltinId` 的含义见
//! `builtin_table.rs` 抬头。
//!
//! **不再往这里加新 builtin**：新 builtin 写成 builtin ABI v2 的类型化函数（`#[builtins]` 模块，
//! 见 `corelib/README.md`「加一个 builtin」）；本段的表项随各领域迁移删除。删表项的判据是「已发布种子
//! 里还有没有 z42 源声明这个名字」（`strings -n 3 | grep <name>`，必须先 strings；直接 grep 二进制会假缺席）。

use super::*;

pub(crate) const PART2: &[(&str, Native)] = &[
    // ── add-platform-os-stdlib (2026-05-14) — appended to preserve existing BuiltinIds ──
    ("__platform_os",         Native::Val(platform::builtin_platform_os)),
    ("__platform_arch",       Native::Val(platform::builtin_platform_arch)),
    ("__platform_family",     Native::Val(platform::builtin_platform_family)),
    ("__platform_os_kind",    Native::Val(platform::builtin_platform_os_kind)),
    ("__platform_arch_kind",  Native::Val(platform::builtin_platform_arch_kind)),
    ("__system_pid",          Native::Val(system::builtin_system_pid)),
    ("__system_exe_path",     Native::Val(system::builtin_system_exe_path)),
    ("__system_cwd",          Native::Val(system::builtin_system_cwd)),
    ("__system_set_cwd",      Native::Void(system::builtin_system_set_cwd)),
    ("__system_hostname",     Native::Val(system::builtin_system_hostname)),
    ("__system_cpu_count",    Native::Val(system::builtin_system_cpu_count)),
    ("__system_os_version",   Native::Val(system::builtin_system_os_version)),

    // ── add-threading-stdlib (2026-05-20) — appended to preserve existing BuiltinIds ──
    ("__thread_spawn",        Native::Val(threading::builtin_thread_spawn)),

    // ── add-sync-primitives (2026-05-20) ── 🪦 **已删除**（store-sync-values-in-heap 阶段 2，2026-09-26）
    //
    // 原先这里有 19 个槽（`__mutex_*` 4 / `__channel_*` 7 / `__rwlock_*` 8）。stdlib 自 2026-09-14
    // 改走 `__monitor_*`，此后没有任何 z42 源声明它们；按归档
    // `2026-09-14-store-sync-values-in-heap` 规定的触发条件核过种子（`strings -n 3 | grep` 旧名，
    // programs/z42c 与 libs 皆 0 引用）后整批删除，连同 `corelib/sync.rs`（596 行）与
    // `VmCore.{mutexes,rwlocks,channels}` 三个 registry。
    //
    // ── add-gc-pause-histogram (2026-05-22) — appended to preserve existing BuiltinIds ──
    ("__gc_pause_histogram", Native::Val(gc::builtin_gc_pause_histogram)),
    ("__gc_pause_stats_raw", Native::Val(gc::builtin_gc_pause_stats_raw)),

    // ── add-z42-compression (2026-05-22): __deflate_* / __zstd_* / __compressor_*
    //    builtins are NOT statically registered here — they're provided by the
    //    z42-compression cdylib, dlopen'd at VM startup (or statically linked
    //    on wasm via the `bundled-compression` feature). Resolved through
    //    `VmCore.ext_builtins` (see corelib::ext_builtin_id_of below).

    // ── add-process-stream-stdio (2026-05-24) — appended to preserve existing BuiltinIds ──
    ("__process_handle_read_stdout", Native::Val(process_legacy::builtin_process_handle_read_stdout)),
    ("__process_handle_read_stderr", Native::Val(process_legacy::builtin_process_handle_read_stderr)),

    // ── add-z42-net K1 (2026-05-24) — appended to preserve existing BuiltinIds ──
    // STAGE2-DEBT(legacy-net-builtins): 种子的 z42.core 不再声明 `__net_*` 后删掉下面这些表项、corelib/network/legacy/ 与 tls_legacy.rs。
    // 它们返回 `[KIND, …]` 判别数组，只为已发布种子的 z42.net（经 z42.core 的 NetTcpNative / UdpNative /
    // NetTlsNative / DnsNative.Lookup）保留；新的是 ABI v2 的 `__tcp_*` / `__udp_*` / `__dns_lookup` / `__tls_*`。
    ("__net_tcp_connect",       Native::Val(network::builtin_net_tcp_connect)),
    ("__net_tcp_listen",        Native::Val(network::builtin_net_tcp_listen)),
    ("__net_tcp_accept",        Native::Val(network::builtin_net_tcp_accept)),
    ("__net_tcp_socket_read",   Native::Val(network::builtin_net_tcp_socket_read)),
    ("__net_tcp_socket_write",  Native::Val(network::builtin_net_tcp_socket_write)),
    ("__net_tcp_socket_drop",   Native::Void(network::builtin_net_tcp_socket_drop)),
    ("__net_tcp_listener_drop", Native::Void(network::builtin_net_tcp_listener_drop)),

    // ── add-gc-heap-snapshot-export B3 (2026-05-24) — appended to preserve existing BuiltinIds ──
    ("__gc_write_heap_snapshot", Native::Val(gc::builtin_gc_write_heap_snapshot)),

    // ── add-gc-pause-window (2026-05-24) — appended to preserve existing BuiltinIds ──
    ("__gc_recent_pauses",         Native::Val(gc::builtin_gc_recent_pauses)),
    ("__gc_pause_window_capacity", Native::Val(gc::builtin_gc_pause_window_capacity)),

    // ── add-gc-oom-exception (2026-05-25) — appended to preserve existing BuiltinIds ──
    ("__gc_set_max_heap_bytes", Native::Void(gc::builtin_gc_set_max_heap_bytes)),
    ("__gc_set_strict_oom",     Native::Void(gc::builtin_gc_set_strict_oom)),

    // ── add-z42-net-udp K2 (2026-05-25) — appended to preserve existing BuiltinIds ──
    ("__net_udp_bind", Native::Val(network::builtin_net_udp_bind)),
    ("__net_udp_send", Native::Val(network::builtin_net_udp_send)),
    ("__net_udp_recv", Native::Val(network::builtin_net_udp_recv)),
    ("__net_udp_drop", Native::Void(network::builtin_net_udp_drop)),

    // ── add-gc-softref (2026-05-26) ──────────────────────────────────────────
    ("__soft_handle_create", Native::Val(gc::builtin_soft_handle_create)),
    ("__soft_handle_get",    Native::Val(gc::builtin_soft_handle_get)),

    // ── add-csprng-to-crypto (2026-05-27) — OS-CSPRNG backing Std.Crypto.SecureRandom ──
    ("__crypto_random_bytes", Native::Val(crypto::builtin_crypto_random_bytes)),

    // ── add-httpclient-timeout (2026-05-27) — TCP socket read/write deadlines ──
    ("__net_tcp_socket_set_read_timeout",  Native::Val(network::builtin_net_tcp_socket_set_read_timeout)),
    ("__net_tcp_socket_set_write_timeout", Native::Val(network::builtin_net_tcp_socket_set_write_timeout)),

    // ── add-z42-net-udp-recv-into (2026-05-27) — buffer-fill Receive variant ──
    ("__net_udp_recv_into", Native::Val(network::builtin_net_udp_recv_into)),

    // ── add-z42-net-udp-multicast (2026-05-27) — IPv4 multicast group ops ──
    ("__net_udp_join_multicast",      Native::Val(network::builtin_net_udp_join_multicast)),
    ("__net_udp_leave_multicast",     Native::Val(network::builtin_net_udp_leave_multicast)),
    ("__net_udp_set_multicast_loop",  Native::Val(network::builtin_net_udp_set_multicast_loop)),

    // ── add-z42-net-dns (2026-05-27) — synchronous DNS resolution ──
    ("__net_dns_lookup",              Native::Val(network::builtin_net_dns_lookup)),

    // ── add-z42-net-socket-options (2026-05-27) — TCP_NODELAY / IP_TTL ──
    ("__net_tcp_socket_set_nodelay",  Native::Val(network::builtin_net_tcp_socket_set_nodelay)),
    ("__net_tcp_socket_set_ttl",      Native::Val(network::builtin_net_tcp_socket_set_ttl)),
    ("__net_tcp_listener_set_ttl",    Native::Val(network::builtin_net_tcp_listener_set_ttl)),
    ("__net_udp_set_ttl",             Native::Val(network::builtin_net_udp_set_ttl)),

    // ── add-net-socket-options-extended (2026-05-30) — connect/UDP timeout, SO_REUSEADDR, SO_KEEPALIVE ──
    ("__net_tcp_connect_with_timeout", Native::Val(network::builtin_net_tcp_connect_with_timeout)),
    ("__net_tcp_socket_set_keepalive", Native::Val(network::builtin_net_tcp_socket_set_keepalive)),
    ("__net_tcp_socket_set_keepalive_tuned", Native::Val(network::builtin_net_tcp_socket_set_keepalive_tuned)),
    ("__net_tcp_listen_with_options",  Native::Val(network::builtin_net_tcp_listen_with_options)),
    ("__net_udp_set_read_timeout",     Native::Val(network::builtin_net_udp_set_read_timeout)),
    ("__net_udp_set_write_timeout",    Native::Val(network::builtin_net_udp_set_write_timeout)),

    // ── add-z42-net-tls (2026-06-03) — rustls client TLS streams (HTTPS) ──
    ("__net_tls_connect",                  Native::Val(tls::builtin_net_tls_connect)),
    ("__net_tls_socket_read",              Native::Val(tls::builtin_net_tls_socket_read)),
    ("__net_tls_socket_write",             Native::Val(tls::builtin_net_tls_socket_write)),
    ("__net_tls_socket_drop",              Native::Void(tls::builtin_net_tls_socket_drop)),
    ("__net_tls_socket_set_read_timeout",  Native::Val(tls::builtin_net_tls_socket_set_read_timeout)),
    ("__net_tls_socket_set_write_timeout", Native::Val(tls::builtin_net_tls_socket_set_write_timeout)),

    // ── runtime-dynamic-load-call (DEFERRED) — stubs so zpkg loads cleanly ──
    ("__load_zpkg",  Native::Void(builtin_load_zpkg_stub)),
    ("__call_static", Native::Val(builtin_call_static_stub)),

    // ── add-z42-repl (2026-07-23) — appended to preserve existing BuiltinIds ──
    // REPL line editor (rustyline) + in-memory bytecode load. Back
    // `Std.Repl.ReadLine` and z42.scripting's per-eval module load. (BuiltinId is
    // resolved by name at load, so removing the retired `__repl_readline_indented`
    // slot — indent now computed script-side, sink-repl-indent-to-script — shifts no
    // persisted id.)
    ("__repl_complete_probe",     Native::Val(repl::builtin_repl_complete_probe)),
    ("__repl_set_completer",      Native::Val(repl::builtin_repl_set_completer)),
    ("__repl_set_key_editor",     Native::Val(repl_editing::builtin_repl_set_key_editor)),
    ("__repl_set_keywords",       Native::Val(repl_editing::builtin_repl_set_keywords)),
    ("__repl_member_names",       Native::Val(repl::builtin_repl_member_names)),
    ("__load_bytecode_in_memory", Native::Val(reflection::builtin_load_bytecode_in_memory)),

    // ── add-enum-parse-isdefined (2026-07-24) — appended to preserve BuiltinIds ──

    // ── add-enum-underlying-type (2026-07-25) — appended to preserve BuiltinIds ──

    // ── add-nested-types (2026-07-25) — appended to preserve BuiltinIds ──

    // ── complete-class-access-control — class visibility reflection (appended) ──
    // Single visibility-byte accessor; z42 `Type.Visibility` wraps it in the
    // `TypeVisibility` enum and pairs it with `Type.IsNested` for the C# top-level
    // vs nested distinction (no per-predicate builtin surface). BuiltinIds resolve
    // by name at load (resolver.rs), so collapsing the earlier 6 predicate builtins
    // to this one is safe — nothing bakes a positional BuiltinId across a build.

    // ── add-load-context-model (2026-07-30) — appended to preserve BuiltinIds ──
    ("__lctx_default",            Native::Val(assemblyloadcontext::builtin_lctx_default)),
    ("__lctx_create_collectible", Native::Val(assemblyloadcontext::builtin_lctx_create_collectible)),
    ("__lctx_load",               Native::Val(assemblyloadcontext::builtin_lctx_load)),
    ("__lctx_name",               Native::Val(assemblyloadcontext::builtin_lctx_name)),
    ("__lctx_is_collectible",     Native::Val(assemblyloadcontext::builtin_lctx_is_collectible)),
    ("__lctx_assemblies",         Native::Val(assemblyloadcontext::builtin_lctx_assemblies)),
    ("__asm_name",                Native::Val(assemblyloadcontext::builtin_asm_name)),
    ("__asm_is_collectible",      Native::Val(assemblyloadcontext::builtin_asm_is_collectible)),
    ("__asm_loadcontext",         Native::Val(assemblyloadcontext::builtin_asm_loadcontext)),
    ("__asm_get_types",           Native::Val(assemblyloadcontext::builtin_asm_get_types)),
    ("__type_is_collectible",     Native::Val(assemblyloadcontext::builtin_type_is_collectible)),
    ("__type_assembly",           Native::Val(assemblyloadcontext::builtin_type_assembly)),

    // ── add-exec-profile-matrix (2026-07-31) — appended to preserve BuiltinIds ──
    ("__platform_caps",           Native::Val(platform::builtin_platform_caps)),
    ("__platform_exec_modes",     Native::Val(platform::builtin_platform_exec_modes)),

    // ── add-lazy-context-unload (2026-08-05) — appended to preserve BuiltinIds ──
    ("__lctx_unload",             Native::Void(assemblyloadcontext::builtin_lctx_unload)),

    // ── add-heap-retention-diagnostics (2026-08-06) — appended to preserve BuiltinIds ──
    ("__heap_direct_referrers",   Native::Val(diagnostics::builtin_heap_direct_referrers)),
    ("__heap_retaining_roots",    Native::Val(diagnostics::builtin_heap_retaining_roots)),

    // ── mature-embed-testhost P1 (2026-08-09) — appended to preserve BuiltinIds ──
    ("__run_goldens_isolated",    Native::Val(reflection::builtin_run_goldens_isolated)),

    // ── expose-diagnostics-counters (2026-08-23) — appended to preserve BuiltinIds ──
    ("__diag_counters",           Native::Val(diagnostics::builtin_diag_counters)),
    // ── perf-stdlib-hot-paths (2026-09-03): bulk string primitives ────────────────
    ("__str_substring",     Native::Val(string::builtin_str_substring)),
    ("__str_concat_parts",  Native::Val(string::builtin_str_concat_parts)),

    // ── complete-runtime-settings P4 (2026-09-05) — appended to preserve BuiltinIds ──
    // Read-only view of the resolved runtime configuration (Std.Runtime.RuntimeConfig).
    // No setter: the config is frozen in a OnceLock after boot — see corelib/config.rs.
    ("__cfg_get",       Native::Val(config::builtin_cfg_get)),
    ("__cfg_source",    Native::Val(config::builtin_cfg_source)),
    ("__cfg_names",     Native::Val(config::builtin_cfg_names)),
    ("__search_dirs",   Native::Val(config::builtin_search_dirs)),
    ("__cfg_dump",      Native::Val(config::builtin_cfg_dump)),
    ("__cfg_describe",  Native::Val(config::builtin_cfg_describe)),
    ("__cfg_available", Native::Val(config::builtin_cfg_available)),

    // ── perf-bulk-array-copy (2026-09-05) — appended to preserve existing BuiltinIds ──
    // `Array.Copy` 的底座：一次区间搬运，取代脚本里的逐元素 for 循环。
    ("__array_copy",    Native::Void(array::builtin_array_copy)),

    // ── add-app-properties (2026-09-05) — appended to preserve existing BuiltinIds ──
    // 应用自定义配置属性（Std.Runtime.AppProperties）。不是旋钮：VM 不校验、
    // 未知键不是错误；结构化值走 __app_props_toml + Std.Toml。
    ("__app_prop",        Native::Val(appprops::builtin_app_prop)),
    ("__app_prop_has",    Native::Val(appprops::builtin_app_prop_has)),
    ("__app_prop_names",  Native::Val(appprops::builtin_app_prop_names)),
    ("__app_props_toml",  Native::Val(appprops::builtin_app_props_toml)),

    // ── add-symbol-availability-macro (2026-09-08) — appended to preserve existing BuiltinIds ──
    // `available!(X)` 的运行期回落。正常路径**永不执行**——加载期 fold_availability
    // 已把它折成 ConstBool 并剪掉死分支。走到这里 = pass 没跑，debug 下 panic 点名。
    ("__sym_available",   Native::Val(symavail::builtin_sym_available)),

    // ── add-method-reference (2026-09-11) — appended to preserve existing BuiltinIds ──
    // `methodof(Type.Member(sig))` → Std.Reflection.MethodInfo. The overload is
    // already resolved at compile time; the argument is the single qualified name.
    ("__methodof",        Native::Val(reflection::builtin_methodof)),
    // `__monitor_*`（Mutex / RwLock / Channel 的底座）已迁 ABI v2：`monitor::typed`。

    // ── fix-class-level-typeof (2026-09-25) — appended to preserve existing BuiltinIds ──
    // 类级 `typeof(T)`：读 receiver 的 per-instance type_args[idx] 产 `Std.Type`。
    // 与类级 `default(T)`（`DefaultOf` 指令）同一个载体，只是产类型而非零值；走 builtin
    // 而非新 opcode ⇒ 零 zbc 格式 bump、JIT 白送（Builtin 按名/id 通用派发）。
    ("__class_type_arg",    Native::Val(reflection::builtin_class_type_arg)),
    ("__class_default",     Native::Val(reflection::builtin_class_default)),

    // ── perf-collections-sort — appended to preserve existing BuiltinIds ──
    // `List<T>.Sort()` / `Array.Sort<T>(T[])` 的基元快路径：元素全是同一种基元时原生稳定排序。
    ("__array_sort_prims",  Native::Val(array::builtin_array_sort_prims)),
    // ── perf-strings-json — appended to preserve existing BuiltinIds ──
    // `String.Split(string)` 的扫描 + 切段：一次原生扫描，每段一次分配。
    ("__str_split",         Native::Val(string::builtin_str_split)),
    // `String.Join(sep, values)`：一次分配拼出交错结果。
    ("__str_join",          Native::Val(string::builtin_str_join)),
    // ── assembly-get-functions — appended to preserve existing BuiltinIds ──
    // `Assembly.GetFunctions()`：一个 zpkg 的自由（顶层）函数 → MethodInfo[]。
    ("__asm_get_functions", Native::Val(assemblyloadcontext::builtin_asm_get_functions)),
];
