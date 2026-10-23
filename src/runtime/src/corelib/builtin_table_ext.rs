//! `BUILTINS` 旧表第 2 段（`PART2`）——  起的历次追加。整张表的拼接与 `BuiltinId` 的含义见
//! `builtin_table.rs` 抬头。
//!
//! **不再往这里加新 builtin**：新 builtin 写成 builtin ABI v2 的类型化函数（`#[builtins]` 模块，
//! 见 `corelib/README.md`「加一个 builtin」）；本段的表项随各领域迁移删除。删表项的判据是「已发布种子
//! 里还有没有 z42 源声明这个名字」（`strings -n 3 | grep <name>`，必须先 strings；直接 grep 二进制会假缺席）。

use super::*;

pub(crate) const PART2: &[(&str, Native)] = &[
    // ── add-platform-os-stdlib — appended to preserve existing BuiltinIds ──
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

    // ── add-threading-stdlib — appended to preserve existing BuiltinIds ──
    ("__thread_spawn",        Native::Val(threading::builtin_thread_spawn)),

    // ── add-sync-primitives  ── 🪦 **已删除**（store-sync-values-in-heap 阶段 2）
    //
    // 原先这里有 19 个槽（`__mutex_*` 4 / `__channel_*` 7 / `__rwlock_*` 8）。stdlib 已
    // 改走 `__monitor_*`，此后没有任何 z42 源声明它们；按归档
    // `2026-09-14-store-sync-values-in-heap` 规定的触发条件核过种子（`strings -n 3 | grep` 旧名，
    // programs/z42c 与 libs 皆 0 引用）后整批删除，连同 `corelib/sync.rs`（596 行）与
    // `VmCore.{mutexes,rwlocks,channels}` 三个 registry。
    //
    // ── add-gc-pause-histogram — appended to preserve existing BuiltinIds ──

    // ── add-z42-compression : __deflate_* / __zstd_* / __compressor_*
    //    builtins are NOT statically registered here — they're provided by the
    //    z42-compression cdylib, dlopen'd at VM startup (or statically linked
    //    on wasm via the `bundled-compression` feature). Resolved through
    //    `VmCore.ext_builtins` (see corelib::ext_builtin_id_of below).

    // ── add-gc-heap-snapshot-export B3  ──

    // ── add-gc-pause-window  ──

    // ── add-gc-oom-exception  ──

    // ── add-gc-softref  ──────────────────────────────────────────
    ("__soft_handle_create", Native::Val(gc::builtin_soft_handle_create)),
    ("__soft_handle_get",    Native::Val(gc::builtin_soft_handle_get)),

    // ── add-csprng-to-crypto  — OS-CSPRNG backing Std.Crypto.SecureRandom ──
    ("__crypto_random_bytes", Native::Val(crypto::builtin_crypto_random_bytes)),


    // ── runtime-dynamic-load-call (DEFERRED) — stubs so zpkg loads cleanly ──
    ("__load_zpkg",  Native::Void(builtin_load_zpkg_stub)),
    ("__call_static", Native::Val(builtin_call_static_stub)),

    // ── add-z42-repl — appended to preserve existing BuiltinIds ──
    // REPL line editor (rustyline) + in-memory bytecode load. Back
    // `Std.Repl.ReadLine` and z42.scripting's per-eval module load. (BuiltinId is
    // resolved by name at load, so removing the retired `__repl_readline_indented`
    // slot — indent now computed script-side, sink-repl-indent-to-script — shifts no
    // persisted id.)

    // ── add-enum-parse-isdefined — appended to preserve BuiltinIds ──

    // ── add-enum-underlying-type — appended to preserve BuiltinIds ──

    // ── add-nested-types — appended to preserve BuiltinIds ──

    // ── complete-class-access-control — class visibility reflection (appended) ──
    // Single visibility-byte accessor; z42 `Type.Visibility` wraps it in the
    // `TypeVisibility` enum and pairs it with `Type.IsNested` for the C# top-level
    // vs nested distinction (no per-predicate builtin surface). BuiltinIds resolve
    // by name at load (resolver.rs), so collapsing the earlier 6 predicate builtins
    // to this one is safe — nothing bakes a positional BuiltinId across a build.

    // ── add-load-context-model — appended to preserve BuiltinIds ──

    // ── add-exec-profile-matrix — appended to preserve BuiltinIds ──
    ("__platform_caps",           Native::Val(platform::builtin_platform_caps)),
    ("__platform_exec_modes",     Native::Val(platform::builtin_platform_exec_modes)),

    // ── add-lazy-context-unload — appended to preserve BuiltinIds ──

    // ── add-heap-retention-diagnostics — appended to preserve BuiltinIds ──
    ("__heap_direct_referrers",   Native::Val(diagnostics::builtin_heap_direct_referrers)),
    ("__heap_retaining_roots",    Native::Val(diagnostics::builtin_heap_retaining_roots)),

    // ── mature-embed-testhost P1 — appended to preserve BuiltinIds ──

    // ── expose-diagnostics-counters — appended to preserve BuiltinIds ──
    ("__diag_counters",           Native::Val(diagnostics::builtin_diag_counters)),
    // ── perf-stdlib-hot-paths : bulk string primitives ────────────────

    // ── complete-runtime-settings P4 — appended to preserve BuiltinIds ──
    // Read-only view of the resolved runtime configuration (Std.Runtime.RuntimeConfig).
    // No setter: the config is frozen in a OnceLock after boot — see corelib/config.rs.
    ("__cfg_get",       Native::Val(config::builtin_cfg_get)),
    ("__cfg_source",    Native::Val(config::builtin_cfg_source)),
    ("__cfg_names",     Native::Val(config::builtin_cfg_names)),
    ("__search_dirs",   Native::Val(config::builtin_search_dirs)),
    ("__cfg_dump",      Native::Val(config::builtin_cfg_dump)),
    ("__cfg_describe",  Native::Val(config::builtin_cfg_describe)),
    ("__cfg_available", Native::Val(config::builtin_cfg_available)),

    // ── perf-bulk-array-copy — appended to preserve existing BuiltinIds ──
    // `Array.Copy` 的底座：一次区间搬运，取代脚本里的逐元素 for 循环。
    ("__array_copy",    Native::Void(array::builtin_array_copy)),

    // ── add-app-properties — appended to preserve existing BuiltinIds ──
    // 应用自定义配置属性（Std.Runtime.AppProperties）。不是旋钮：VM 不校验、
    // 未知键不是错误；结构化值走 __app_props_toml + Std.Toml。
    ("__app_prop",        Native::Val(appprops::builtin_app_prop)),
    ("__app_prop_has",    Native::Val(appprops::builtin_app_prop_has)),
    ("__app_prop_names",  Native::Val(appprops::builtin_app_prop_names)),
    ("__app_props_toml",  Native::Val(appprops::builtin_app_props_toml)),

    // ── add-symbol-availability-macro — appended to preserve existing BuiltinIds ──
    // `available!(X)` 的运行期回落。正常路径**永不执行**——加载期 fold_availability
    // 已把它折成 ConstBool 并剪掉死分支。走到这里 = pass 没跑，debug 下 panic 点名。
    ("__sym_available",   Native::Val(symavail::builtin_sym_available)),

    // ── add-method-reference — appended to preserve existing BuiltinIds ──
    // `methodof(Type.Member(sig))` → Std.Reflection.MethodInfo. The overload is
    // already resolved at compile time; the argument is the single qualified name.
    // `__monitor_*`（Mutex / RwLock / Channel 的底座）已迁 ABI v2：`monitor::typed`。

    // ── fix-class-level-typeof — appended to preserve existing BuiltinIds ──
    // 类级 `typeof(T)`：读 receiver 的 per-instance type_args[idx] 产 `Std.Type`。
    // 与类级 `default(T)`（`DefaultOf` 指令）同一个载体，只是产类型而非零值；走 builtin
    // 而非新 opcode ⇒ 零 zbc 格式 bump、JIT 白送（Builtin 按名/id 通用派发）。

    // ── perf-collections-sort — appended to preserve existing BuiltinIds ──
    // `List<T>.Sort()` / `Array.Sort<T>(T[])` 的基元快路径：元素全是同一种基元时原生稳定排序。
    ("__array_sort_prims",  Native::Val(array::builtin_array_sort_prims)),
    // ── perf-strings-json — appended to preserve existing BuiltinIds ──
    // `String.Split(string)` 的扫描 + 切段：一次原生扫描，每段一次分配。
    // `String.Join(sep, values)`：一次分配拼出交错结果。
    // ── assembly-get-functions — appended to preserve existing BuiltinIds ──
    // `Assembly.GetFunctions()`：一个 zpkg 的自由（顶层）函数 → MethodInfo[]。
];
