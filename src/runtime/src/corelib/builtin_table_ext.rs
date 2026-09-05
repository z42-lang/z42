//! `BUILTINS` 第 2 段 —— 2026-05-14 起的历次**追加**（每个 change 一节）。
//!
//! 新 builtin 一律加在**本文件末尾**：最终表 = PART1 ++ PART2，BuiltinId 就是拼接
//! 后的下标，插在中间会让既有 zbc 里的调用全部错位。
//!
//! 拆成两个文件是行数硬限所迫（合并后 515 行 > 500）；切点选在历史首个
//! 「appended to preserve BuiltinIds」边界，语义上即「原始表 ++ 追加日志」。

use super::*;

pub(crate) const PART2: &[(&str, NativeFn)] = &[
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
    ("__vfs_mount",         fs_backend::memory::builtin_vfs_mount),
    ("__vfs_enable",        fs_backend::memory::builtin_vfs_enable),
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
    // `Std.Repl.ReadLine` and z42.scripting's per-eval module load. (BuiltinId is
    // resolved by name at load, so removing the retired `__repl_readline_indented`
    // slot — indent now computed script-side, sink-repl-indent-to-script — shifts no
    // persisted id.)
    ("__repl_readline",           repl::builtin_repl_readline),
    ("__repl_complete_probe",     repl::builtin_repl_complete_probe),
    ("__repl_set_completer",      repl::builtin_repl_set_completer),
    ("__repl_set_key_editor",     repl_editing::builtin_repl_set_key_editor),
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

    // ── complete-class-access-control — class visibility reflection (appended) ──
    // Single visibility-byte accessor; z42 `Type.Visibility` wraps it in the
    // `TypeVisibility` enum and pairs it with `Type.IsNested` for the C# top-level
    // vs nested distinction (no per-predicate builtin surface). BuiltinIds resolve
    // by name at load (resolver.rs), so collapsing the earlier 6 predicate builtins
    // to this one is safe — nothing bakes a positional BuiltinId across a build.
    ("__type_visibility",          reflection::builtin_type_visibility),

    // ── add-load-context-model (2026-07-30) — appended to preserve BuiltinIds ──
    ("__lctx_default",            assemblyloadcontext::builtin_lctx_default),
    ("__lctx_create_collectible", assemblyloadcontext::builtin_lctx_create_collectible),
    ("__lctx_load",               assemblyloadcontext::builtin_lctx_load),
    ("__lctx_name",               assemblyloadcontext::builtin_lctx_name),
    ("__lctx_is_collectible",     assemblyloadcontext::builtin_lctx_is_collectible),
    ("__lctx_assemblies",         assemblyloadcontext::builtin_lctx_assemblies),
    ("__asm_name",                assemblyloadcontext::builtin_asm_name),
    ("__asm_is_collectible",      assemblyloadcontext::builtin_asm_is_collectible),
    ("__asm_loadcontext",         assemblyloadcontext::builtin_asm_loadcontext),
    ("__asm_get_types",           assemblyloadcontext::builtin_asm_get_types),
    ("__type_is_collectible",     assemblyloadcontext::builtin_type_is_collectible),
    ("__type_assembly",           assemblyloadcontext::builtin_type_assembly),

    // ── add-exec-profile-matrix (2026-07-31) — appended to preserve BuiltinIds ──
    ("__platform_caps",           platform::builtin_platform_caps),
    ("__platform_exec_modes",     platform::builtin_platform_exec_modes),

    // ── add-lazy-context-unload (2026-08-05) — appended to preserve BuiltinIds ──
    ("__lctx_unload",             assemblyloadcontext::builtin_lctx_unload),

    // ── add-heap-retention-diagnostics (2026-08-06) — appended to preserve BuiltinIds ──
    ("__heap_direct_referrers",   diagnostics::builtin_heap_direct_referrers),
    ("__heap_retaining_roots",    diagnostics::builtin_heap_retaining_roots),

    // ── mature-embed-testhost P1 (2026-08-09) — appended to preserve BuiltinIds ──
    ("__run_goldens_isolated",    reflection::builtin_run_goldens_isolated),

    // ── expose-diagnostics-counters (2026-08-23) — appended to preserve BuiltinIds ──
    ("__diag_counters",           diagnostics::builtin_diag_counters),
    // ── perf-stdlib-hot-paths (2026-09-03): bulk string primitives ────────────────
    ("__str_substring",     string::builtin_str_substring),
    ("__str_concat_parts",  string::builtin_str_concat_parts),

    // ── complete-runtime-settings P4 (2026-09-05) — appended to preserve BuiltinIds ──
    // Read-only view of the resolved runtime configuration (Std.Runtime.RuntimeConfig).
    // No setter: the config is frozen in a OnceLock after boot — see corelib/config.rs.
    ("__cfg_get",       config::builtin_cfg_get),
    ("__cfg_source",    config::builtin_cfg_source),
    ("__cfg_names",     config::builtin_cfg_names),
    ("__cfg_dump",      config::builtin_cfg_dump),
    ("__cfg_describe",  config::builtin_cfg_describe),
    ("__cfg_available", config::builtin_cfg_available),

    // ── perf-bulk-array-copy (2026-09-05) — appended to preserve existing BuiltinIds ──
    // `Array.Copy` 的底座：一次区间搬运，取代脚本里的逐元素 for 循环。
    ("__array_copy",    array::builtin_array_copy),
];
