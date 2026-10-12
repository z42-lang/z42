use super::*;

/// `__test_module_load(path: str) -> Std.Harness.TestEntry[]` — load a compiled
/// test / bench module and return its TIDX entries. Kept only for a seed SDK's
/// pre-reflection runner; the in-tree runner discovers tests by reflection
/// (`Std.Test.TestDiscovery`).
// STAGE2-DEBT(load-module-entries): 种子 SDK 的 runner 换成反射版后，随 TIDX 链（编译器写出 / VM 解码）删除本 builtin
pub fn builtin_test_module_load(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let entries = load_path(ctx, args)?;
    test_entries_to_values(ctx, &entries)
}

/// `__load_module(path: str)` — the generic load behind `Std.Runtime.ModuleLoader.Load`
/// (component injection, project hooks, `[analyzers]` zpkgs, and the test runner, which then
/// discovers tests by reflection). Pure load: returns Null.
pub fn builtin_load_module(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    load_path(ctx, args)?;
    Ok(Value::Null)
}

/// Shared body: merge the artifact (+ dependency closure) into the live registries.
fn load_path(
    ctx: &VmContext, args: &[Value],
) -> Result<Vec<crate::metadata::test_index::LoadedTestEntry>> {
    let path = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("ModuleLoader.Load: expected a path string"),
    };
    // Static fields of the loaded closure need nothing here: their initializers are
    // per-type and run lazily at first use, and `load_module_into_vm` already drains
    // the pending type-load queue. Do NOT call `init_static_fields` — it clears every
    // static field in the VM, so the caller's own state (and any already-initialized
    // type) would silently read back as zero after each load.
    let entries = ctx.load_module_into_vm(&path)?;
    Ok(entries)
}

/// Materialize TIDX entries as `Std.Harness.TestEntry[]`. A module with no TIDX
/// allocates nothing, so plain component loads never need the type.
fn test_entries_to_values(
    ctx: &VmContext, entries: &[crate::metadata::test_index::LoadedTestEntry],
) -> Result<Value> {
    let mut objs: Vec<Value> = Vec::with_capacity(entries.len());
    for e in entries {
        let obj = alloc_named(
            ctx,
            TEST_ENTRY_TYPE,
            &[
                ("Qualified", Value::Str(e.qualified.clone().into())),
                ("Kind", Value::I64(e.kind as i64)),
                ("Flags", Value::I64(e.flags as i64)),
                ("SkipReason", load_module_opt(&e.skip_reason)),
                ("SkipPlatform", load_module_opt(&e.skip_platform)),
                ("SkipFeature", load_module_opt(&e.skip_feature)),
                ("ShouldThrow", load_module_opt(&e.expected_throw)),
            ],
        )?;
        objs.push(obj);
    }
    Ok(ctx.heap().alloc_array(objs))
}

/// z42 type the TIDX entries are materialized as (the seed SDK runner's `Std.Harness.TestEntry`).
const TEST_ENTRY_TYPE: &str = "Std.Harness.TestEntry";

pub(super) fn load_module_opt(o: &Option<String>) -> Value {
    match o {
        Some(s) => Value::Str(s.clone().into()),
        None => Value::Null,
    }
}

/// `__load_bytecode_in_memory(bytes: byte[]) -> bool` — load a compiled
/// artifact (packed zpkg / bare zbc) from an in-memory byte array into the live
/// VM registries, so its functions become reflectively invocable with zero disk
/// I/O. Backs `z42.scripting`'s per-eval load (REPL): `PackageCompile` emits the
/// session package's bytes in memory, this registers them, then `$Eval_N()` is
/// called via `MethodInfo.Invoke`. Idempotent per module name (first-wins merge,
/// like `__load_module`). Returns `true` on success; a malformed/empty buffer or
/// missing lazy loader throws. (add-z42-repl)
pub fn builtin_load_bytecode_in_memory(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let bytes = match args.first() {
        Some(Value::Array(rc)) => {
            let borrowed = rc.borrow();
            let mut out = Vec::with_capacity(borrowed.len());
            for (i, v) in borrowed.iter_boxed().enumerate() {
                match v {
                    Value::I64(n) if (0..=255).contains(&n) => out.push(n as u8),
                    other => bail!(
                        "__load_bytecode_in_memory: byte {} not u8 in 0..=255: {:?}", i, other
                    ),
                }
            }
            out
        }
        Some(other) => bail!(
            "__load_bytecode_in_memory: expected a byte[] argument, got {:?}", other
        ),
        None => bail!("__load_bytecode_in_memory: missing byte[] argument"),
    };
    ctx.load_module_bytes_into_vm(&bytes)?;
    // unify-static-init-into-cctor（7.4）：此前在此**只跑本轮**的 `__static_init__`
    // —— 不能跑全量 `init_static_fields`，那会清空所有静态字段、毁掉 REPL 跨轮
    // carry-forward（用户 `.Add` 进 List 的东西会没）。
    //
    // 静态初始化器现已是**每类型的类型初始化器**、按首次使用惰性触发：本轮的
    // `Vars{N}` 在被读/写时自然初始化（读写静态字段都是触发点），且天然**只**初始化
    // 本轮触及的类型 —— 既不需要这条通道，也不再有「跑全量」的诱惑。
    Ok(Value::Bool(true))
}

/// `__run_goldens_isolated(paths: string[], entries: string[], libsDir: string)
/// -> string[]` — run each golden program (a self-contained app.zpkg/.zbc) in a
/// FRESH isolated VM and return its captured stdout, in input order
/// (mature-embed-testhost P1). Each case gets its own `VmContext` via
/// `z42::app::run` (fresh heap + static-fields + function table), so goldens
/// that share `Main` / namespaces don't collide and static state doesn't leak
/// across cases — the isolation the host golden runner gets from spawning a
/// fresh z42vm per case, but in-process (works on mobile too). stdout is
/// captured via the thread-local sink stack.
///
/// Runs interp (parity with the host golden gate). `jobs` fans out over OS
/// threads with DISJOINT VmContexts (P2): each golden's heap/static-fields are
/// its own and stdout capture is thread-local (`STDOUT_SINKS`), so parallel runs
/// don't interfere and there is no global GC lock to serialize them. The only
/// invariant — never share a `Value`/`GcRef` across contexts — holds because
/// workers exchange only Rust `String`s; `Value`s are built in the caller's heap
/// AFTER join. `jobs <= 0` → auto (available cores); `1` → sequential. Order is
/// preserved. On a run error the captured stdout gets a `<z42-run-error: …>`
/// marker so the caller's comparison fails visibly.
pub fn builtin_run_goldens_isolated(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let paths = read_str_vec(args.first(), "paths")?;
    let entries = read_str_vec(args.get(1), "entries")?;
    let libs_dir = match args.get(2) {
        Some(Value::Str(s)) => s.to_string(),
        _ => String::new(),
    };
    let jobs = match args.get(3) {
        Some(Value::I64(n)) => *n,
        _ => 1,
    };
    let libs = if libs_dir.is_empty() { None } else { Some(std::path::PathBuf::from(libs_dir)) };
    let n = paths.len();

    // Effective worker count: jobs<=0 → cores, capped at n.
    let workers = if jobs <= 0 {
        std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1)
    } else {
        jobs as usize
    }.max(1).min(n.max(1));

    let captured: Vec<String> = if workers <= 1 || n <= 1 {
        // Sequential.
        (0..n)
            .map(|i| {
                let entry = entries.get(i).map(|s| s.as_str()).filter(|s| !s.is_empty());
                run_one_golden_isolated(&paths[i], entry, libs.clone())
            })
            .collect()
    } else {
        // Parallel: `workers` threads pull indices off a shared counter; each runs
        // its golden in a fresh isolated VmContext. Results slotted by index.
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        let next = Arc::new(AtomicUsize::new(0));
        let slots: Arc<Vec<Mutex<String>>> =
            Arc::new((0..n).map(|_| Mutex::new(String::new())).collect());
        let paths = Arc::new(paths);
        let entries = Arc::new(entries);
        let libs = Arc::new(libs);
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let next = next.clone();
            let slots = slots.clone();
            let paths = paths.clone();
            let entries = entries.clone();
            let libs = libs.clone();
            handles.push(std::thread::spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= paths.len() {
                    break;
                }
                let entry = entries.get(i).map(|s| s.as_str()).filter(|s| !s.is_empty());
                let cap = run_one_golden_isolated(&paths[i], entry, (*libs).clone());
                *slots[i].lock().unwrap() = cap;
            }));
        }
        // fix-park-blocking-natives (2026-09-14): the join blocks for the whole batch ⇒ parked.
        // The workers run their own isolated VmContexts, so nothing here touches this ctx's
        // heap. The sequential branch above can't park: its nested VM allocates on this very
        // thread, and the park tripwire (`debug_assert_not_native_parked`) is per-thread.
        {
            let _park = crate::gc::NativeParkGuard::enter(ctx);
            for h in handles {
                let _ = h.join();
            }
        }
        Arc::try_unwrap(slots)
            .unwrap_or_else(|a| (*a).iter().map(|m| Mutex::new(m.lock().unwrap().clone())).collect())
            .into_iter()
            .map(|m| m.into_inner().unwrap())
            .collect()
    };

    let out: Vec<Value> = captured.into_iter().map(|s| Value::Str(s.into())).collect();
    Ok(ctx.heap().alloc_array(out))
}

/// Extract a `Vec<String>` from a z42 `string[]` argument.
pub(super) fn read_str_vec(v: Option<&Value>, what: &str) -> Result<Vec<String>> {
    match v {
        Some(Value::Array(rc)) => {
            let borrowed = rc.borrow();
            let mut out = Vec::with_capacity(borrowed.len());
            for e in borrowed.iter_boxed() {
                match e {
                    Value::Str(s) => out.push(s.to_string()),
                    Value::Null => out.push(String::new()),
                    _ => bail!("__run_goldens_isolated: {what}[] must be strings"),
                }
            }
            Ok(out)
        }
        _ => bail!("__run_goldens_isolated: expected {what} as string[]"),
    }
}

/// Run one golden in a fresh isolated VM (interp), returning its captured stdout.
/// Never propagates — a run error is folded into the returned string as a marker.
pub(super) fn run_one_golden_isolated(path: &str, entry: Option<&str>, libs: Option<std::path::PathBuf>) -> String {
    crate::corelib::io::push_stdout_sink();
    let opts = crate::app::RunOpts {
        mode: crate::metadata::ExecMode::Interp,
        libs_dir: libs,
        program_args: Vec::new(),
        print_stats: false,
        stats_json: false,
    };
    let res = crate::app::run(path, entry, opts);
    let captured = crate::corelib::io::take_stdout_sink();
    let mut s = String::from_utf8_lossy(&captured).into_owned();
    if let Err(e) = res {
        s.push_str(&format!("\n<z42-run-error: {e:#}>"));
    }
    s
}
