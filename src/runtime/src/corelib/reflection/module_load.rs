use super::*;

/// `__load_module(path: str)` — the generic load behind `Std.Runtime.ModuleLoader.Load`
/// (component injection, project hooks, `[analyzers]` zpkgs, and the test runner, which then
/// discovers tests by reflection): merge the artifact (+ dependency closure) into the live
/// registries. Returns Null.
pub fn load_module(ctx: &VmContext, path: &crate::metadata::vstr::Str) -> anyhow::Result<Value> {
    let path = path.to_string();
    // Static fields of the loaded closure need nothing here: their initializers are
    // per-type and run lazily at first use, and `load_module_into_vm` already drains
    // the pending type-load queue. Do NOT call `init_static_fields` — it clears every
    // static field in the VM, so the caller's own state (and any already-initialized
    // type) would silently read back as zero after each load.
    ctx.load_module_into_vm(&path)?;
    Ok(Value::Null)
}

/// `__load_bytecode_in_memory(bytes: byte[]) -> bool` — load a compiled
/// artifact (packed zpkg / bare zbc) from an in-memory byte array into the live
/// VM registries, so its functions become reflectively invocable with zero disk
/// I/O. Backs `z42.scripting`'s per-eval load (REPL): `PackageCompile` emits the
/// session package's bytes in memory, this registers them, then `$Eval_N()` is
/// called via `MethodInfo.Invoke`. Idempotent per module name (first-wins merge,
/// like `__load_module`). Returns `true` on success; a malformed/empty buffer or
/// missing lazy loader throws. (add-z42-repl)
pub fn load_bytecode_in_memory(ctx: &VmContext, bytes_arr: &Value) -> anyhow::Result<Value> {
    let args = [*bytes_arr];
    let args: &[Value] = &args;
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
/// invariant — never share a `Value`/`ArrRef` across contexts — holds because
/// workers exchange only Rust `String`s; `Value`s are built in the caller's heap
/// AFTER join. `jobs <= 0` → auto (available cores); `1` → sequential. Order is
/// preserved. On a run error the captured stdout gets a `<z42-run-error: …>`
/// marker so the caller's comparison fails visibly.
pub fn run_goldens_isolated(
    ctx: &VmContext, paths_arr: &Value, entries_arr: &Value,
    libs_dir: &crate::metadata::vstr::Str, jobs: i64,
) -> anyhow::Result<Value> {
    let paths = read_str_vec(Some(paths_arr), "paths")?;
    let entries = read_str_vec(Some(entries_arr), "entries")?;
    let libs_dir = libs_dir.to_string();
    let jobs = jobs;
    let libs = if libs_dir.is_empty() { None } else { Some(std::path::PathBuf::from(libs_dir)) };
    let n = paths.len();
    // The child VMs run on this VM's file system: on wasm the goldens and their libs are
    // mounted in it, not on any disk.
    let vfs = std::sync::Arc::clone(ctx.vfs());

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
                run_one_golden_isolated(&paths[i], entry, libs.clone(), &vfs)
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
            let vfs = vfs.clone();
            handles.push(std::thread::spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= paths.len() {
                    break;
                }
                let entry = entries.get(i).map(|s| s.as_str()).filter(|s| !s.is_empty());
                let cap = run_one_golden_isolated(&paths[i], entry, (*libs).clone(), &vfs);
                *slots[i].lock().unwrap() = cap;
            }));
        }
        // fix-park-blocking-natives : the join blocks for the whole batch ⇒ parked.
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
pub(super) fn run_one_golden_isolated(
    path: &str,
    entry: Option<&str>,
    libs: Option<std::path::PathBuf>,
    vfs: &std::sync::Arc<dyn crate::pal::fs::Vfs>,
) -> String {
    crate::corelib::io::push_stdout_sink();
    let opts = crate::app::RunOpts {
        mode: crate::metadata::ExecMode::Interp,
        libs_dir: libs,
        program_args: Vec::new(),
        print_stats: false,
        stats_json: false,
        exit_after_run: false,
        vfs: Some(std::sync::Arc::clone(vfs)),
    };
    let res = crate::app::run(path, entry, opts);
    let captured = crate::corelib::io::take_stdout_sink();
    let mut s = String::from_utf8_lossy(&captured).into_owned();
    if let Err(e) = res {
        s.push_str(&format!("\n<z42-run-error: {e:#}>"));
    }
    s
}

/// 模块装载迁 builtin ABI v2(P1-8 批次 6):`load_module` 的包初始化器与
/// `load_bytecode_in_memory` 装入的初始化器都会执行 z42 代码,`run_goldens_isolated`
/// 在隔离子 VM 里跑整套 golden——全部标 `reenters`。批次 12b 顺带把两个 DEFERRED 桩
/// (`Std.Runtime.Runtime` 的 `__load_zpkg` / `__call_static`,runtime-dynamic-load-call)
/// 收进来:仍是桩,但失败是类型化的 `NotImplementedException`(实参收 `&Value`,
/// 保证无论传什么先报「未实现」而不是先卡在参数校验上)。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__load_module", reenters)]
    pub fn load(ctx: &VmContext, path: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
        reentry(ctx, load_module(ctx, path))
    }

    #[builtin(name = "__load_bytecode_in_memory", reenters)]
    pub fn load_bytes(ctx: &VmContext, bytes: &Value) -> Result<Value, BuiltinError> {
        reentry(ctx, load_bytecode_in_memory(ctx, bytes))
    }

    #[builtin(name = "__run_goldens_isolated", reenters)]
    pub fn run_goldens(
        ctx: &VmContext, paths: &Value, entries: &Value,
        libs_dir: &crate::metadata::vstr::Str, jobs: i64,
    ) -> Result<Value, BuiltinError> {
        reentry(ctx, run_goldens_isolated(ctx, paths, entries, libs_dir, jobs))
    }

    fn not_implemented(who: &str) -> BuiltinError {
        BuiltinError::Exception {
            class: "Std.NotImplementedException",
            message: format!("{who}: not yet implemented (runtime-dynamic-load-call DEFERRED)"),
        }
    }

    /// `Std.Runtime.Runtime.LoadZpkg(string)` — DEFERRED 桩:按逻辑名动态加载 zpkg
    /// 的 VM 入口尚未实现(workload 派发由宿主侧 ZpkgResolver 走,见 load-context.md)。
    #[builtin(name = "__load_zpkg")]
    pub fn load_zpkg(_name: &Value) -> Result<(), BuiltinError> {
        Err(not_implemented("__load_zpkg"))
    }

    /// `Std.Runtime.Runtime.CallStatic(string, string[])` — DEFERRED 桩:按全限定名
    /// 驱动 `(string[]) -> int` 入口点的 VM 侧派发尚未实现。
    #[builtin(name = "__call_static")]
    pub fn call_static(_fqn: &Value, _args: &Value) -> Result<i64, BuiltinError> {
        Err(not_implemented("__call_static"))
    }
}
