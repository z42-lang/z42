# z42 VM 内部实现原理

> **目的**：记录 Rust VM 的内部数据结构、算法、加载策略与关键设计决策。
> 让新接手者不必阅读大量源码即可理解"为什么这样设计"。
> 面向 VM 开发者，不面向 z42 语言使用者。
>
> 使用者视角请看 `docs/internals/src/formats/ir.md`、`docs/internals/src/runtime/execution-model.md`。

---

## VmContext / VmCore —— 运行时状态归口

宿主代码运行 VM 的标准流程：

```rust
let ctx = VmContext::with_module(final_module);    // production: 把 Module 装入 VmCore
ctx.install_lazy_loader_with_deps(search_dirs, main_pool_len, declared, loaded);
let vm = Vm::new(ExecMode::Interp);                 // Vm 不持 Module
vm.run(&ctx, hint)?;
```

上述装配（`available!` 折叠 → `with_module` → cctor 登记 → `install_lazy_loader_with_deps` → 种入 lazy loader 的类型/impl）在 `src/runtime/src/boot.rs::boot_context` 一处完成，`app::run` 与嵌入 host（`host/ops.rs`）共用。

### VmContext 构造 entry —— 三个

| 方法 | 用途 | 备注 |
|------|------|------|
| `VmContext::with_module(module: Module) -> Pin<Box<Self>>` | 生产入口；构造新 VmCore + 把 `Arc<Module>` 装入 `VmCore.module` | `__thread_spawn` worker 需要 `VmCore.module = Some` 来 dispatch；test 路径若不需要 module 走 `new()` |
| `VmContext::new() -> Pin<Box<Self>>` | 测试入口；构造新 VmCore + `module = None` | 单测大量用（heap / static_fields / corelib 单测均不需要真 Module） |
| `VmContext::new_with_core(core: Arc<VmCore>) -> Pin<Box<Self>>` | spawn 入口；**复用现有 VmCore**，仅构造 per-thread 字段 + register self 到 `vm_contexts` | `__thread_spawn` worker 通过此构造，让 worker 看见父 VmCore 的 static_fields / heap / lazy_loader / native_libs |

`__thread_spawn`（[corelib/threading.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/corelib/threading.rs)）流程：拿到调用者 `ctx.core` 的 `Arc::clone` → `std::thread::spawn` → worker 内 `VmContext::new_with_core(core)` 构造 worker ctx → `interp::exec_function` dispatch。worker 的 per-thread 字段（pending_exception / call_stack / func_ref_slots）私有；通过 `vm_contexts` 注册让 GC scanner 看见 worker 自己的 roots。

### VmCore：跨线程共享状态

`VmCore` 持有 **process-globally singular** 的状态，通过 `Arc<VmCore>` 让多个 `VmContext`（每个 OS 线程一个）共享：

- `static_fields: Mutex<Vec<Value>>` — 用户类 static 字段槽位，按 `StaticFieldId.0` 索引
- `static_field_index: Mutex<FxHashMap<String, u32>>` — FQN → 槽位 id 映射
- `lazy_loader: RwLock<Option<LazyLoader>>` — 按需 zpkg 加载器（**RwLock**：符号查找稳态下是纯读，只有真正加载 zpkg 才取写锁）
- `native_types: RwLock<HashMap<(String,String), Arc<RegisteredType>>>` — Tier 1 native interop 注册表（**RwLock**，读多写少：dispatch 是纯读，写仅在 module 加载期；`#[cfg(feature="native-interop")]`）
- `native_libs: Mutex<Vec<libloading::Library>>` — 已加载的 native 库句柄（同上 cfg）
- `ext_builtins: Mutex<ExtBuiltinTable>` — stdlib 原生扩展库注册的 builtin（`native::ext::load_all` 填充；同上 cfg）
- `pinned_owned_buffers: Mutex<HashMap<u64, Box<[u8]>>>` — `Value::PinnedView` 的 owned 缓冲
- `fatal: AtomicBool` — 致命 VM 错误（栈溢出）正在展开，见下文「原生栈预算」
- `processes: ResourceRegistry<ProcessSlot>` — `Std.IO.Process` 子进程注册表
- `heap: Box<dyn MagrGC>` — GC 子系统接口（后端 `ArcMagrGC`）
- `module: Option<Arc<Module>>` — 用户编译后的 Module，跨线程共享；测试路径 `None`，生产路径 `Some(Arc::new(module))`
- `threads: ResourceRegistry<JoinHandle<Result<()>>>` — `Std.Threading.Thread` 的 JoinHandle slot table；`__thread_spawn` 插入，`__thread_join` take-out 后 join
- `file_handles: ResourceRegistry<FileHandleSlot>` + `tcp_sockets` / `tcp_listeners` / `tls_sockets` / `udp_sockets`（后四者 `#[cfg(not(target_arch="wasm32"))]`）— `Std.IO.FileStream` 句柄 + `Std.Net.Sockets` 各类 socket slot table
- `vm_contexts: Mutex<Vec<VmContextPtr>>` — 本 core 上所有存活 `VmContext` 的注册表（见下「Send-safety 与 GC scanner 设计」）
- 静态初始化：`cctors: Arc<CctorRegistry>`（有静态构造器的类型的初始化状态，见 [static-ctor-init.md](static-ctor-init.md)）/ `pending_type_inits` / `pending_type_init_count` / `init_batch_inflight` / `static_init_error`
- `context_registry: Mutex<ContextRegistry>` — load context / assembly 注册表（见 [load-context.md](load-context.md)）
- GC safepoint 协议：`gc_phase` / `gc_phase_cv` / `parked_count` / `collector_active` / `needs_auto_collect`（见 [safepoint-design.md](safepoint-design.md)）
- `program_args: Mutex<Vec<String>>` — `--` 之后的程序参数（`Std.IO.Environment.GetCommandLineArgs()`）
- 观测：`counters`（`RuntimeCounters`，只累计已销毁 `VmContext` 的计数；各线程在自己的 `VmContext.counters` 上计，`counters_snapshot()` 汇总）/ `runtime_observers` / `park_histogram`（safepoint park 时长分布）/ `lock_contentions` + `lock_wait_us`（仅 `profile-contention` feature 写入）/ `sampler`（`Z42_SAMPLE_HZ` 开启的采样 profiler）

> **`ResourceRegistry<T>`**：slot table 统一为 `ResourceRegistry<T>`（内嵌锁 + 表 + 单调 id 计数器），
> 避免每类资源各写「表 + `next_*_id`」两个字段，「新增一类资源」只需加一个字段。API：`insert_new(v)->id`（分配+插入）/
> `alloc_id()`（只 bump 计数器，供 `corelib::network` 先算 id 再自行上锁插入的路径）/ `take(id)` /
> `with_mut(id, f)` / `count()` / `get_cloned(id)`（Arc-wrapped 的 Mutex/RwLock 用）/ `lock()`（逃生舱：
> 返回裸 `MutexGuard<HashMap<u64,T>>`，供 `corelib::network`/`fs` 那种「取出→阻塞 I/O→放回」必须在单次
> 上锁内跨多步的场景）。计数器随表进 registry，ids（含 `processes`）与其它
> socket/handle 表一样 per-core 唯一。
> **模块布局**：`vm_context/`：`mod.rs`（hub：VM_CORES / snapshot /
> `VmContextPtr` / `CoreContextReclaimer`）+ `resource_registry.rs` + `types.rs`（VmCore/VmContext struct）+
> `construct.rs`（构造 + Drop）+ `resources.rs` / `native.rs` / `frames.rs` / `statics.rs` / `lookup.rs`
> （各 concern 的 `impl VmContext` 块，inherent impl 可跨文件）+ `cctor.rs`（静态构造器状态机）/ `isa_cache.rs`（`is`/`as`/`catch` 前置缓存）/ `symres.rs`（「确定不存在」判定）。

`VmCore` 满足 `Send + Sync`（编译期 assertion 在 `src/runtime/src/gc/arc_heap_tests/send_sync.rs`）。

### VmContext：per-thread 视图

每个 OS 线程拿一个 `VmContext`，它持有：

- `core: Arc<VmCore>` — 指向 VmCore 共享状态
- `pending_exception: Arc<Mutex<Option<Value>>>` — JIT extern "C" 边界异常槽位
- `pending_thrown: Mutex<Option<Value>>` — callback 型 builtin（反射 `MethodInfo.Invoke`）把原异常值带出 builtin 边界的槽位；每线程一份，GC 根
- `call_stack: Arc<Mutex<Vec<VmFrame>>>` — 当前线程帧栈
- `func_ref_slots: Arc<Mutex<Vec<Value>>>` — method-group-conversion FuncRef cache 槽位
- `stack_arena` / `struct_arena` / `transient_arena`（及其发布长度原子，见下一节）、`next_frame_id`、`safepoint_skip`（safepoint 节流计数，JIT 内联读写）、`jit_ctx`（混合模式下指向当前 `JitModuleCtx`）
- `interned_cache`（`ConstStr` 字面量的 per-context 驻留缓存，GC root）、`subclass_memo` + `isa_cache`（`is`/`as`/`catch` 子类判定缓存）、`type_lookup_cache` + `fn_lookup_cache`（`try_lookup_type/function` 命中的前置缓存，免去共享 `lazy_loader` 锁）

每个 `VmFrame` 同时承载 `(regs ptr, env_arena ptr, func_name, file, line, column, offset)` 及三个 arena 的截断 base：GC root scanner 扫 regs+env_arena，stack-trace 读 name/file/line/col，interp `RefKind::Stack` 跨帧 deref 通过 `frame.regs`。

### Send-safety 与 GC scanner 设计

`MagrGC` trait 要求 `Send + Sync`。GC scanner closure（mark 阶段被调用）也要求 `Send + Sync`，进而所有 closure 捕获都必须 Send + Sync —— 这是 per-thread 字段用 `Arc<Mutex<>>` 而非 `Rc<RefCell<>>` 的根因。

Scanner closure 通过 `Weak<VmCore>` 捕获 VmCore（避免 `VmCore → heap → scanner → Arc<VmCore>` 循环引用），upgrade 失败时 silent skip。

**VmContext 注册表**：VmCore 持 `vm_contexts: Mutex<Vec<VmContextPtr>>` 注册表。`VmContext::new()` 返回 `Pin<Box<VmContext>>` 以保证地址稳定（`PhantomPinned` 标 !Unpin 防 move-out），构造时 push 自身到注册表，Drop 时 retain 移除。GC scanner 改为：**1**) 上锁 vm_contexts → **2**) 遍历每个 VmContext ptr → **3**) `unsafe { &*ptr }` 扫其 `pending_exception` / `pending_thrown` / `call_stack` 帧 / `func_ref_slots`。所有 VmContext 的 per-thread roots 在 mark 阶段都被看见 —— multi-thread 安全。Lock 持有期间 Drop 阻塞，无 use-after-free。

API 方法都用 `&self`（内部 Mutex/RwLock）。详见 [`object-protocol-dispatch.md`](object-protocol-dispatch.md)、[`native-abi.md`](native-abi.md)、[`concurrency.md`](concurrency.md)。

### 帧 arena 的锁瘦身：发布长度原子

`VmContext` 持三个 per-thread arena —— `stack_arena`（逃逸对象/数组）、`struct_arena`（值 struct
blob）、`transient_arena`（`Ref`/`PinnedView`/`StackClosure`/`StructRefHeap` 的 payload）——均 `Arc<Mutex<>>`，
因为 **GC scanner 跨线程读它们**（见上「Send-safety」：不能退成 `Rc<RefCell<>>`）。每个函数调用的
`push_frame` 要戳记三个 arena 的当前长度作 truncation base，`pop_frame` 要 LIFO-truncate 回去 —— 若朴素实现则是**每调用 8 把锁**（push 4 + pop 4，含 `call_stack`）。call-heavy workload（z42c 自编译）下这是
仅次于 dispatch 的第二大桶，纯锁开销（实测跳过全部 6 个 arena 锁 = 3.9% faster）。

**关键观察：这三个 arena 只有 mutator 线程写，GC 线程只读。** 于是给每个 arena 加一个**发布长度**
`AtomicUsize`（挂 `VmContext`、在 `Mutex` 之外；`stack_arena` 两个 Vec → 两个原子）：

- **单写者**：mutator 在 arena 锁内、于每个 alloc（经 `stack_alloc_obj`/`stack_alloc_arr`/`struct_alloc`/
  `transient_alloc` 四个包装）与 `pop_frame` 的 truncate 后 `store(len, Relaxed)`。GC 从不碰这些原子
  （它在 `Mutex` 下读 arena **数据**）。⇒ `Relaxed` 足够（线程观察自己的写按程序序）。
- **push_frame** 从原子 `Relaxed` load 取 base（**无锁**），不再锁三个 arena → 4 把锁降到 1 把（`call_stack`）。
- **pop_frame** 对每个 arena **仅当本帧确实增长过**（发布长度 ≠ 戳记 base）才加锁 truncate + 重发布；
  常态帧在这三个 arena 上分配为零 → 三个比较全短路 → 只剩 `call_stack` 一把锁。

**alloc 漏斗铁律**：所有 arena 分配必须经四个包装之一。绕过的裸 `arena.lock().alloc(..)` 会让原子失准 →
`pop_frame` 误跳 truncate → arena **泄漏**（非崩溃——`frame_id` staleness 守卫仍保护读；失败模式是内存
泄漏 + 性能退化，不是读到错值）。长度只在 alloc（增）/ truncate（减）改变，truncate 生产代码里只在
`pop_frame`——两者全仓核对无遗漏。

**GC race**：pop 的 skip 分支对 arena 零操作 → 与 GC 扫描一致；truncate 分支在 arena `Mutex` 内 → 与
`scan_roots` 互斥，GC 要么扫到待释放槽（仍合法存活 `Value`）要么扫到已释放，皆安全。

实测：前端 typecheck 4.757→4.617s = **2.9%**，`--dump-bound` 逐字节一致，`push_frame` 197→125 /
`pop_frame` 151→110 samples，无格式/wire/语义变更。

**Native interop 入口**：`VmContext::register_native_type(Arc<RegisteredType>)` /
`resolve_native_type(module, name)` / `load_native_library(path)`；后者打开 `.dylib`
/`.so`/`.dll` 并调用其 `<basename>_register` 入口（约定）让 native 库通过
`z42_register_type` 把类型推入 `native_types`。Interp 入口设置 thread-local
`CURRENT_VM` 让 native callback 能找回 VM；详见 `src/runtime/src/native/exports.rs`
的 `VmGuard` RAII。

---

## 原生栈预算：栈溢出是致命错误

两个引擎都在原生栈上递归（每个 z42 调用至少一个 Rust / 机器码帧），所以递归深度受线程栈限制。
`stack_guard`（`src/runtime/src/stack_guard.rs`）在**每个 z42 帧入口**比较栈指针与本线程的下限：

- 下限 = 栈底 + 余量，余量为栈大小的 1/8、夹在 256 KiB–1 MiB（给两次检查之间的原生工作：builtin、
  一次惰性 JIT 编译、生成报告）。栈边界来自 `pal::stack`，线程首次进入时算一次，存 TLS。
- interp：`exec_function_body` 在 `push_frame` 之后检查（报告里含这一帧），越界返回内部错误。
- JIT：函数 prologue 内联 `get_stack_pointer` 与 `JitModuleCtx::stack_limit` 比较（`run_fn` 按当前
  线程设置；JIT 代码只在该线程上跑），越界调 `jit_stack_overflow` 后返回「已抛出」。

越界时记下 z42 调用栈、置 `VmCore.fatal`（每个 VM 一份，同一 VM 的所有线程共享）。之后：

- interp 的 `find_handler` 与 JIT 的 catch 分发（`jit_fatal_pending`）一律不进 handler，`finally` 也不跑；
- builtin 错误不再转成 `Std.Exception`（`exec_call::builtin`）；
- 于是错误一路退到最外层入口：`z42vm` 打印报告、以退出码 3 结束；`z42_host_run_app` 返回 3；
  `z42_host_invoke` 返回 `Z42_HOST_ERR_FATAL`。

不支持 catch 的理由与决策见 [reference：栈溢出是致命错误](https://z42-lang.github.io/z42/reference/language/exceptions.html)。
VM 创建的线程（`Std.Threading.Thread`、`z42_host_run_app` 的运行线程）栈大小来自 `thread-stack-bytes`
（默认 16 MiB）。

## VM 启动流程

```
z42vm <file>                                   [main.rs：CLI + 运行时配置 → app::run]
  │
  ├── resolve_libs_dir()                       [startup.rs]
  │      → libs 旋钮（$Z42_LIBS / --set libs= / [runtime].libs）| <binary>/../libs
  │        | <cwd>/artifacts/intermediate/libraries/flat/{release,debug}
  │      → 解析结果回写 $Z42_LIBS（仅当未设/为空）：进程内运行的程序
  │        （尤其 z42c，直接读 $Z42_LIBS 做跨包 dep 解析）与 VM 看到同一 libs 目录，
  │        SDK 布局无需手动 `Z42_LIBS=`；显式设置不覆盖。[libs_env_to_publish]
  │
  │   以下 5.1b–5.1e 在 `app::run`（app.rs），与嵌入入口 `z42_run_app` 共用
  │
  ├── 5.1b 加载 z42.core.zpkg (eager，隐式 prelude；按 search_dirs 顺序取首个可读的)
  │      → modules[0]
  │      → initially_loaded_zpkgs = ["z42.core.zpkg"]
  │
  ├── 5.1c 加载 user artifact (.zbc / .zpkg)
  │      → user_artifact.module + dependencies + import_namespaces
  │      → probe `<basename>.zsym` 同目录；存在且 build_id 匹配 →
  │        合并 sidecar DBUG 到 per-module funcs（详见下方 sidecar 章节）
  │
  ├── 5.1d 依赖加载策略（search_dirs = [入口zpkg目录, probing-paths 展开结果, stdlib libs]，见下「同址搜索」）
  │      interp / jit: 纯懒加载（build_declared_candidates 填充 LazyLoader；jit 另按函数惰性编译，见 [jit.md](jit.md)）
  │      aot（`is_eager`）: eager 预加载所有声明依赖（transitive BFS，整个闭包 merge 进 final_module）
  │
  ├── 5.1e merge_modules → final_module
  │      + build_type_registry / verify_constraints / build_*_index
  │
  ├── boot::boot_context(final_module, BootPlan)        [boot.rs]
  │      fold_availability → VmContext::with_module → register_cctor_of
  │      → install_lazy_loader_with_deps(search_dirs, pool_len, declared, initially_loaded)
  │      → seed_lazy_loader_types / seed_lazy_loader_impls
  │
  └── Vm::new(mode).run(&ctx, entry)
         → boot::prepare_execution（FuncRef 槽 + resolve_module）
         → interp::run_with_static_init | jit::run
```

### `$Z42_LIBS` 的交接规则：两个组件、同一条规则

`$Z42_LIBS` 有**两个**写入方，两边都只填「未设/为空」：

| 写入方 | 函数 | 写什么 |
|---|---|---|
| z42vm 自己 | `libs_env_to_publish`（`startup.rs`）| `resolve_libs_dir()` 的结果，给**进程内**运行的程序看 |
| apphost | `libs_env_for_child`（`hostrun.rs`）| 它定位到的 `<runtime>/libs`，给它 exec 的**子** z42vm 看 |

⚠️ **apphost 那格不能简化成「不设」**：安装布局下 z42vm 在 `<dir>/z42vm`，VM 自己的第 ②
档探的是 `<binary-dir>/../libs` = `<dir>/../libs`，**不是** apphost 找到的 `<dir>/libs`。
删掉这次 set，安装布局就定位不到 stdlib 了。

🔴 **apphost 不能无条件覆写 `Z42_LIBS`**，否则显式值被静默丢弃。影响面远不止
已发布的 app：**SDK 自己的 `bin/z42c` 就是一个 apphost**（见
[packaging.md](../devinfra/packaging.md) 的 `kind = apphost`），所以拿装好的工具链跑
`Z42_LIBS=… z42c build …` 在无条件覆写时是**完全无效**的——不报警、不报错，就是没生效。`Z42_LIBS` 在
`knob_table.rs` 里标着 `PUBLIC`、在 [runtime-settings.md](https://z42-lang.github.io/z42/reference/toolchain/runtime-settings.html)
是有名有姓的一行，实测口径（一个把收到的
`Z42_LIBS` 回显出来的 `z42vm` 桩）：

| `Z42_LIBS` | 无条件覆写时子进程看到 | 现行为 |
|---|---|---|
| 未设 | `<runtime>/libs` | `<runtime>/libs` |
| `/my/explicit/libs` | `<runtime>/libs` ← **被吞** | `/my/explicit/libs` |
| 空串 | `<runtime>/libs` | `<runtime>/libs`（空串等同未设）|

### 依赖同址搜索

依赖 zpkg 按文件名在 `search_dirs` 列表里**按序**解析，而非单一 `libs_dir`。z42vm CLI
组装 `search_dirs = [入口 zpkg 所在目录, probing-paths 展开结果, stdlib libs 目录]`
（去重、顺序固定 → 解析确定性；入口目录优先）。**动机**：apphost 把 payload 与它的包依赖放在一起发布——
`bin/z42c`(apphost) → `programs/z42c/z42c.driver.zpkg`，其兄弟 `z42c.core.zpkg` 等也在
`programs/z42c/`，**不在 stdlib `libs/`**。同址搜索让 driver 既能找到同址的 `z42c.*`，
又能从 `libs/` 找 `z42.*`，无需把两者拍平到一个目录。

中间那一档由 `probing::expand_probing_paths` 展开：
相对项按 entry 目录解析、`*`/`**` 展开成目录、不存在的静默跳过，另外认一个 `${Z42_HOME}` 占位符
（侧车随产物分发，不能烤具体路径；候选根 = `$Z42_HOME` →
`$Z42_PORTABLE_VM` 反推 → 正在跑的 z42vm 自己的位置）。规则全表见
[runtime-settings.md](https://z42-lang.github.io/z42/reference/toolchain/runtime-settings.html)。
🔴 待办：占位符目前**只有 support 侧**，z42c 尚未发射它（受 bootstrap-seed 分阶段纪律约束）。

落点：`LazyLoader.search_dirs: Vec<PathBuf>`；transitive unfold
用 `ZpkgCandidate::build_in_dirs(dirs, file)`（首个含该文件的目录胜出）。eager BFS 与
`build_declared_candidates` 同样遍历 search_dirs。FFI/embedded（test-runner）从 bytes 加载、
无入口目录，search_dirs 退化为 `[libs_dir]`。

---

## Sidecar 调试符号加载

### 加载策略：eager + 同步

`loader::load_artifact`（zbc / zpkg 两条路径）加载主 artifact 完成后**立即**探测同目录 `<basename>.zsym` 并按 BLID 校验合并。设计选择：

| 选项 | 选择 | 理由 |
|------|------|------|
| eager vs lazy | **eager** | 异常路径（trace 构造）必须零 IO；startup 开销可控（典型 sidecar < 100KB） |
| sidecar 路径 | `<basename>.zsym` 同目录 | 不需要 search path 概念；CI/CD 部署单一目录即可 |
| BLID 算法 | MurmurHash3 x86_128 + payload 零填 hash | 只做配对识别、runtime 从不重算 ⇒ 不需密码学强度；解释执行下比 BLAKE3 快 15×。payload 零填使 sidecar 与 main 字节同步 |
| BLID 不匹配 | warn + ignore，加载继续 | 调试符号缺失不应让程序无法启动 |

### 合并机制：直接在已加载的 `Module` 上 mutate

主 zpkg/zbc 解析返回 `Vec<(Module, namespace, TestEntry[])>` 后立刻 mutate per-module Function：

```rust
for ((module, ns, _tidx), (sym_ns, fns)) in module_pairs.iter_mut().zip(sidecar.modules) {
    for (i, fb) in fns.into_iter().enumerate() {
        if !fb.line_table.is_empty() { module.functions[i].cold_mut().line_table = fb.line_table.into_boxed_slice(); }
        if !fb.local_vars.is_empty() { module.functions[i].cold_mut().local_vars = fb.local_vars.into_boxed_slice(); }
    }
}
```

不用 `RefCell` / `OnceCell` 包装 `line_table` / `local_vars` — sidecar merge 发生在 `merge_modules` 之前的可变所有权窗口内。这避免引入运行期 borrow check 开销。

### 与 `merge_modules` 的顺序

`load_zpkg_bytes_with_sidecar`（`metadata/loader/artifact.rs`）调用顺序：

1. `read_zpkg_modules(raw)` → `Vec<(Module, ns, TestEntry[])>` 拥有可变 Module
2. `apply_zpkg_sidecar(&mut module_pairs, raw, sym, sym_path)` — 把 sidecar DBUG 合入 per-module funcs
3. `assemble_zpkg_artifact`：TIDX 聚合 → `merge_modules(modules)`（命名空间扁平化 + 函数表合并）
4. `build_type_registry` / `verify_constraints` / `build_block_indices` / `build_func_index`

为什么 sidecar 在 merge 之前：sidecar 的 MDBG section 按 module namespace 索引，merge 后 namespace 信息丢失到扁平化 IR；提前 merge 会让 sidecar 的"按 ns 配对"丢失对应关系。

### 容错路径

| 情况 | 行为 |
|------|------|
| sidecar 文件不存在 | 静默继续；trace 退化为 `at <FQN>(<sig>)` |
| sidecar 文件 magic 错 / 缺 BLID | `tracing::warn` + 忽略，加载继续 |
| BLID 不匹配 | `tracing::warn` 显示 `main=<8hex>` / `sidecar=<8hex>` + 忽略 |
| sidecar module 数 ≠ main module 数 | warn + 忽略 |
| 单个 module 内 function 数不一致 | warn + 跳过该 module，其他 module 仍合并 |
| Module 加载主 zbc / zpkg 被发现 `SymOnly` flag | `bail!` —— sidecar 不可作为主模块加载 |

> **实现位置**：[src/runtime/src/metadata/loader/](https://github.com/z42-lang/z42/tree/main/src/runtime/src/metadata/loader) `apply_zbc_sidecar` / `apply_zpkg_sidecar`；解析在 [zbc_reader/](https://github.com/z42-lang/z42/tree/main/src/runtime/src/metadata/zbc_reader)（`sidecar.rs`）`parse_zbc_sidecar` / `parse_zpkg_sidecar`。

## Embedding Entry

> **本节边界**：从 VM 内部视角描述 `crate::host` 模块如何融入 VM 架构（数据归属、状态管理、与 `crate::native` 的边界）。**API 形态、Host C ABI 函数签名、宿主使用模式**归 [`embedding.md`](embedding.md)，不在本文重复。

`src/runtime/src/host/` 是 z42 VM 的**宿主嵌入入口**：与上方 `z42vm` CLI 启动流程并列，存在于另一条进入 VM 的路径。

```
host application (iOS / Android / IDE 插件 / 其他 native)
   │
   │ z42_host_initialize(&cfg, &handle)
   ▼
host::state::HOST  (RwLock<Option<HostState>>，进程单实例)
   │
   │ z42_host_load_zbc(handle, bytes, len, &mod)
   │ z42_host_resolve_entry(handle, mod, fqn, &entry)
   │ z42_host_invoke(entry, args, n, &result)
   ▼
interp::run_returning（解释执行；装配与 CLI 启动流程共用 boot::boot_context）
```

### 模块边界与 VM 全局状态

| 关系 | 说明 |
|------|------|
| host 模块 ↔ `VmContext` | `host::state::HOST` 持有 `HostState { config: ResolvedConfig, modules, entries, corelib }`；每个已加载 module 独占一个 `VmContext`（`HostModule.ctx`），`load_zbc` 经 `load_artifact_from_bytes` + `merge_modules`（急切并入 `z42.core` 与 `import_namespaces` 对应的 zpkg）后走 `boot::boot_context` + `prepare_execution`（与 `app::run` 同一套装配）；`invoke` 先（每 module 一次）`interp::init_static_fields`，再 `interp::run_returning`。`Z42ExecMode` 在 `initialize` 时只校验所选后端是否编入，不改变 `invoke` 的执行后端 |
| stdout / stderr | `Z42WriteSink` 函数指针经 `corelib::io::install_host_stdout_sink`（stderr 同理）登记；`invoke` 期间 `HostSinkGuard` 置位 per-thread「host sink active」标志，`corelib/io` 据此把输出路由到宿主回调，结束时复原 |
| panic 隔离 | 每个 `extern "C"` 入口经 `host::guard()` `catch_unwind` 兜底，panic → `Z42_HOST_ERR_INTERNAL` |
| 错误诊断 | TLS `LAST_ERROR` 由 `host::error::set_error / clear_error` 管理；与 `native::error::LAST_ERROR` 独立（两条 ABI 各自维护） |

### 与 `crate::native` 的边界

| 方向 | 模块 | 解决问题 |
|------|------|---------|
| native 代码 → 注册类型/方法 | `crate::native` (`z42_register_type` 等) | 扩展语言（CPython C 扩展类比） |
| 宿主 app → 启动 VM | `crate::host` (`z42_host_initialize` 等) | 嵌入运行时（CoreCLR `coreclrhost.h` 类比） |

两者复用 `z42_abi::Z42Value` / `Z42Args` / `Z42Error` 类型，互不调用对方。

### 单实例 vs 多实例

当前单实例：`HOST: RwLock<Option<HostState>>`。`Z42HostRef` 是一个 sentinel pointer（`0x1`），所有 host API 调用读 `HOST` 验证活跃。

多实例 / ALC-like 上下文进 [embedding.md §12 Deferred](embedding.md)。届时 `RwLock<Option<...>>` 升级为 `Slab<HostState>`，`Z42HostRef` 编码 `(idx, gen)`，VM 全局状态（GC heap、JIT cache、type registry）必须 per-handle 化。zpkg 重载/卸载/回收的完整设计（含保留根诊断、内部缓存回收）见 [load-context.md](load-context.md)。

详见 [docs/internals/src/runtime/embedding.md](embedding.md)。

---

## LazyLoader：zpkg-based 依赖懒加载

### 为什么是 zpkg-based

**为什么不是 namespace-based**：若 Call miss 时从 FQ func_name 提取 namespace
前缀，调 `resolve_namespace` 在 libs 目录找**唯一**一个声明该 namespace 的
zpkg。若 ≥2 个 zpkg 共享同 namespace → `bail!("AmbiguousNamespaceError")`。

**为什么不行**：与 C# BCL 对齐后，`Std.Collections` namespace 同
时出现在 `z42.core.zpkg`（List / Dictionary）和 `z42.collections.zpkg`
（Queue / Stack）。namespace-based 模型会拒绝这种合法情况。

**zpkg-based 模型**：对齐 C# CLR 的 AssemblyRef 模型 —— **zpkg 是加载
单位，namespace 是逻辑分组**，多 zpkg 可共享同 namespace。

### 核心数据结构

```rust
struct LazyLoader {
    search_dirs: Vec<PathBuf>,     // 按序解析依赖 zpkg 文件名的目录列表
    main_pool_len: usize,          // 主模块 string pool 长度（索引偏移基准）
    string_pool: Vec<String>,       // 聚合懒加载 string pool

    loaded_zpkgs: FxHashSet<String>,                  // 已加载 zpkg 文件名
    declared_zpkgs: FxHashMap<String, ZpkgCandidate>, // 声明但未加载

    function_table: FxHashMap<String, Arc<Function>>, // FQ name → Function
    type_registry: FxHashMap<String, Arc<TypeDesc>>,  // FQ name → TypeDesc
    impls: FxHashMap<String, Vec<String>>,            // target FQ → [trait FQ]（各包 IMPL 段汇总）
    // …另有 newly_loaded 暂存区、「确定解析不出」的负缓存、歧义名登记
}

struct ZpkgCandidate {
    file_path: PathBuf,
    namespaces: Vec<String>,  // 该 zpkg 导出的 namespace 列表（从 NSPC section 读）
}
```

> **解析 vs 生命周期分层**：
> `ZpkgCandidate` 与「扫目录 → 逐 zpkg 读 NSPC → 建 namespace↔path 候选」这段
> **无状态解析**逻辑住在 `metadata/namespace_index.rs`（`scan_zpkg_candidates` /
> `scan_zbc_candidates`，返回 owned 候选）。两个消费者按「弃/留」分工同一原语：
> **loader**（`resolve_namespace`，编译期工具/诊断）扫完精确匹配即 **drop** 候选（transient）；
> **lazy_loader** 把候选**留存**进 `declared_zpkgs`、配 `loaded_zpkgs` 管加载/释放
> 生命周期（retaining）。生命周期不进原语——「何时加载/加载什么/何时释放」只在 lazy_loader。

### Call miss 触发策略（策略 C + 回退 B）

```
try_lookup_function(func_name):        # VmContext 先查 per-context fn_lookup_cache，再取 lazy_loader 读锁探测
  if function_table has func_name → return hit
  if 负缓存命中 → return None

  # 策略 C：精确路由 —— 按 namespace 前缀筛选候选 zpkg
  ns = namespace_prefix(func_name)   // e.g. "Std.Collections.Stack.Push" → "Std.Collections"
  for zpkg_file in declared_zpkgs:
    if zpkg_file not in loaded_zpkgs
       and zpkg.namespaces 含 ns 或以 ns. 开头:
      load_zpkg_file(zpkg_file)
      if function_table has func_name → return hit

  # 策略 B 回退：若策略 C 无匹配，遍历所有剩余 declared-but-not-loaded，
  # 并迭代到不动点（加载一个包会把它自己的依赖登记成新候选）
  loop over declared_zpkgs - loaded_zpkgs:
    load_zpkg_file(zpkg_file)
    if function_table has func_name → return hit

  记入「确定解析不出」负缓存; return None  # 真正 undefined
```

策略 C 的精确路由等价于 C# CLR 的 TypeRef → TypeDef 查找：按 namespace
（assembly 的 public type 的父 namespace）作为高效过滤器。

策略 B 是安全网，处理 zpkg 元数据不完整 / 用户 zbc 的 import_namespaces
不全等边界情况。

### Per-site cross-zpkg Call 目标缓存

`try_lookup_function` 即使命中（function_table 已有），也是一次
`HashMap<String, Arc<Function>>` 探测（String hash + compare）。本模块 Call 有
`ResolvedTokens.method_tokens[site]`（`AtomicU32`）缓存 `module.functions` 下标，
第二次起纯整数索引；但 **cross-zpkg 目标不在 `module.functions`**（在 lazy loader
的 `function_table`），u32 下标够不着 → 这类 site 的 token slot 永远 `UNRESOLVED`，
**每次调用都重跑** `try_lookup_function`（上面的策略 C/B，至少一次 String hash）。

做法：`ResolvedTokens` 有一条与 `method_tokens` 平行、同 site 索引的
`cross_module_targets: Vec<OnceLock<Arc<Function>>>`。dispatch（[exec_call.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/interp/exec_call.rs)）在
**本模块两级 miss 之后**：

```
if let Some(cell) = cross_cell:
  match cell.get():
    Some(arc) => 借用 arc            # 命中：零 hash、零 atomic-RMW（仅一次 acquire load）
    None      => arc = try_lookup_function(fname)?   # 首次：解析一次
                 cell.set(arc)        # 写一次（OnceLock 幂等：并发双填取胜者，同函数无害）
                 borrow
```

设计要点（对照 CoreCLR：cross-assembly call 首次 prestub 解析后 patch call site，
第二次直接跳）：

- **per-site cell，不建全局整数寻址表**：避免全局可变注册表 + 并发 append 协议
  （runtime 热子系统返工高发区）；每个 `OnceLock` 独立、天然 `Sync`。
- **`OnceLock`（write-once）而非可失效缓存**：一次运行内 FQ-name → 目标函数稳定
  （function_table 装入后 `Arc<Function>` 不变；hot-reload 是独立失效路径，不经此 site 缓存）。
- **只在本模块 miss 后介入**：本模块命中路径一条多余指令都不加（`cross_cell` 仅在
  既有 cross-zpkg 慢路径处取用）。
- **仅 interp**：JIT 的 cross-zpkg 走 `jit_call` helper（站点缓存是 `ResolvedTokens.call_jit_ic`，见下），本机制不动它。

### 惰性加载函数的 token 首执解析

上面所有 per-site 缓存（`method_tokens` / `cross_module_targets` / `vcall_ic` /
`field_ic` / `builtin_tokens` / `static_field_tokens` / `type_tokens` / `site_index`）
都挂在 `Function.resolved: OnceLock<ResolvedTokens>`，由
[`resolver::resolve_module`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/resolver.rs) 一次性填充。
但 `resolve_module` **只经 `Vm::run` → `boot::prepare_execution` 对 entry module 跑一次**——而
interp/JIT 模式下依赖是**纯惰性加载**（[app.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/app.rs)
`is_eager = matches!(mode, Aot)`，非 AOT 全 false）：除用户 artifact 外，
**所有依赖 zpkg**（自编译时即 z42c.core / z42c.syntax / z42c.semantics /
z42c.pipeline 全部）经 `LazyLoader::load_zpkg_file` 进 `function_table`，其
`Function.resolved` **永不被 set**。

若不做首执解析，后果是：**整个自编译工作负载（跑在惰性加载的 z42c.* 里）dispatch 时所有 per-site
缓存全失效**——`resolved == None` → `site_idx` 恒 `UNRESOLVED` → VCall 无 PIC、
Field 无 IC、Builtin/Static 走名字查、每个 Call 都对 entry module 的
`func_index` 做一次 String hash+`memcmp` 且 miss、再 `try_lookup_function`。
profile 里 `get_inner`+`memcmp`+`try_lookup_*` 的大头即源于此。

做法：`resolve_module` 的**单函数体**抽成
[`resolve_function_tokens(func, module, ctx)`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/resolver.rs)，
并在 [`exec_function_body`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/interp/mod.rs) 顶部**首次执行时**
按需填充（`if func.resolved.get().is_none()`，OnceLock 门禁 → 每函数只解析一次；
热路径仅一次 relaxed atomic load，指令循环本就要再读它）。

**模块身份不变式（关键正确性约束）**：填充用的 `module` 必须是该函数**运行期实际
dispatch 所对的 Module**——始终是 entry module（惰性 callee 由调用方的 `module`
一路透传，根在 entry）。`method_tokens` / `type_tokens` 是 `module.functions` /
`module.type_registry` 的下标；对**别的** module 解析会铸出错下标。跨模块目标
（不在 entry module）在此正确解析为 `UNRESOLVED`，交由 `cross_module_targets`
per-site 首执缓存兜住（见上一节）。**`vcall_ic` / `field_ic`（运行期首派填充）、
`builtin_tokens`（全局闭集）、`static_field_tokens`（全局 `ctx.resolve_static_field_id`，
锁保护幂等）** 均与 module 下标无关 → 这几条是首执解析的主要收益来源。

- **只填被执行的函数**：比"加载时对整个惰性 module 跑 `resolve_module`"更省
  （加载但从不调用的函数零解析开销），且天然拿到正确的 entry-module 身份。
- **并发安全**：worker 线程可并发首执同一函数；两者各自构建等价 `ResolvedTokens`
  （静态字段 id 按名幂等、同值），`OnceLock::set` 取胜者，落败者丢弃无副作用。
- **实测**：自编译前端 typecheck **1.26× faster**，`--dump-bound` 输出逐字节一致，
  无格式/wire/语义变更。

### JIT cross-zpkg 调用解析

JIT 按函数**惰性**编译（首次或达调用阈值才编，机制见 [jit.md](jit.md)），`jit_call` 经
`JitModuleCtx::resolve_fn_by_id_tiered` 取已编译入口。依赖 zpkg 在 interp / JIT 下都是惰性加载
（AOT 才在 `app::run` 里 eager 做 transitive BFS 并整体 merge），跨 zpkg 的 callee 因此不在 entry module
的 `functions` 里，由两条机制配合：

1. **惰性槽**（[jit/frame.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/jit/frame.rs)
   `resolve_id_by_name`）：站点 `method_id` 为 `UNRESOLVED` 时，`jit_call` 按名解析，经 lazy loader
   取到函数后在 `JitModuleCtx.lazy_table` 登记一个合成 id（≥ `merged_len`），之后与 merged 函数同样按需编译；
   解析出的 id 缓存在该站点的 `ResolvedTokens.call_jit_ic`，下次免去按名哈希。

2. **`jit_call` 的 interp 兜底**（[jit/helpers/call.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/jit/helpers/call.rs)
   `cross_zpkg_via_interp`）。入口取不到（不可翻译 / 未达阈值 / 编译失败）时镜像 interp `exec_call::call` 的解析
   顺序：① `module.func_index` → `module.functions`（已 merge 但未编译的函数）；
   ② `try_lookup_function`（仅懒加载可达的 zpkg）——两者都在**解释器**上执行 callee，
   结果回填 JIT caller 帧；随后依次过签名 arity 校验与 cctor 屏障，与 interp 同序。

### 单函数 interp 降级：不可 JIT 翻译的 opcode

stdlib 里必然含有 JIT 尚未支持的 opcode —— `out`/`ref`/`in` 参数（`LoadLocalAddr` /
`LoadElemAddr` / `LoadFieldAddr`）、native interop（`CallNative` / `CallNativeVtable` /
`PinPtr` / `UnpinPtr`）、以及泛型方法体 / 泛型调用（`MethodTypeArg` / `MethodDefault` / 带 `method_type_args` 的
`Call`/`VCall`）。碰到这些 opcode 若直接 `bail!`，错误冒泡到顶层会让**整个程序 abort**，所以采用
**逐函数降级**而非「整模块要么全编译要么全失败」。

1. **首编前判定**（`jit_unsupported_reason`，[jit/translate/unsupported.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/jit/translate/unsupported.rs)；
   调用点 `JitModuleCtx::resolve_merged_slot` / `resolve_lazy_slot`）：函数达到编译条件时先扫描一次，含不可翻译 opcode 的
   **不编译**：merged 函数的槽位记为 Rejected（`FnEntry.ptr == null` 的负缓存，此后不再重扫），lazy 函数在 `resolve_id_by_name` 登记槽位前判定、不登记。名单由 `unsupported_reason(instr)`
   单一来源给出，`translate_function` 里的 `bail!` 分支也取自它，两处不会漂移。

2. **调用点自动走 interp 兜底**：`Call` 永远经 `jit_call` helper 跳转，**从不**
   emit cranelift 直接调用，所以被拒绝的 callee 取不到入口 → `cross_zpkg_via_interp`
   Case 1（`module.func_index` 命中已 merge 但未编译的函数）→ 解释器执行。`VCall` 的目标解析由与 interp 共用的
   `interp::vcall_resolve::resolve_vcall` 完成，[jit/helpers/vcall.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/jit/helpers/vcall.rs)
   只决定怎么调：有编译入口走原生，否则在解释器上以填好接收者的帧执行。

3. **entry 也兜底**：`JitModule::run_fn` 在入口取不到时（被拒绝的 entry）改用 `interp::exec_function`
   执行，而不是硬报 `entry not found`。静态字段初始化器随类型初始化器惰性触发（`run_pending_static_inits`），
   不再有 `__static_init__` 名单需要在 JIT 侧枚举。

> 净效果：JIT 尽量编原生码，含 interp-only opcode 的函数（及其整棵调用子树）在解释器上
> 跑，全程对外行为与 `--mode interp` 一致。回归测试：`src/tests/refs/{out_var,
> in_param,ref_local,ref_nested}` 在 interp + jit 双模式皆过。

### 依赖传递（着色算法）

```
load_zpkg_file(file_name):
  if file_name in loaded_zpkgs → return    # 已加载/正在加载（防 re-entry）
  loaded_zpkgs.insert(file_name)           # ★ 着色：先标记后加载

  artifact = load_artifact(file_path)
  remap ConstStr indices + 合并 function_table / type_registry（first-wins）

  # 递归展开该 zpkg 自己的 ZpkgDep 进 declared 集合
  for dep in artifact.dependencies:
    if dep.file not in loaded_zpkgs and not in declared_zpkgs:
      declared_zpkgs[dep.file] = ZpkgCandidate::build_in_dirs(search_dirs, dep.file)
```

**为什么先着色后加载**：若 A 依赖 B、B 又依赖 A（或自依赖），加载 B 时
`load_zpkg_file("a.zpkg")` 立刻返回（因 A 已在 loaded 集合），避免无限递归。

### 函数 / 类型冲突（first-wins）

合并 function_table / type_registry 时若遇同名 entry：保留先加载者 +
`tracing::warn`。预期同名冲突不应发生（编译期类型检查捕获）；若发生，稳定
的行为（不随加载顺序变化）比"最后加载者覆盖"更安全，和 C# CLR 一致。

### ConstStr 索引重映射

主模块 string pool 的索引域是 `[0, main_pool_len)`；懒加载 zpkg 的
ConstStr 原始索引是相对自己 pool 的。为统一，合并时：

```rust
offset = main_pool_len + self.string_pool.len()
// 新加载 zpkg 的每个 Function 的 ConstStr.idx += offset
self.string_pool.extend(artifact.module.string_pool)
```

运行时 `try_lookup_string(absolute_idx)` 返回：
- `idx < main_pool_len` → 主模块 pool
- `idx ≥ main_pool_len` → 懒加载 pool[idx - main_pool_len]

### `z42.core` 永不经过懒加载

`z42.core.zpkg` 在 `app::run` 的 5.1b 阶段就被 eager 加载并 merge 进 main
module。`initially_loaded_zpkgs` 含 `"z42.core.zpkg"`，所以 LazyLoader
的 `declared_zpkgs` 根本不含它。这保证 prelude 语义（所有 `Std.*` 符号
启动即可用）。

### 两阶段类型加载：cross-zpkg subclass 字段继承

**问题**：`build_type_registry` 在每个 module 加载时**单独运行**，只能
基于本 module 的 `type_registry` 解析 base 类。当 z42.io 中的 `class
Sub : Std.Exception { ... }` 加载时，`Std.Exception`（在 z42.core）尚
不在 z42.io 的本地 registry —— `registry.get("Std.Exception")` 返回
`None`，继承的字段 / vtable 槽位丢失。`Sub.fields` 只含自己声明的字段，
`field.set @Message` 触不到正确的 slot，`Message` 永远是 null。

**解决方案**：两阶段（skeleton + fixup）类型加载。

**阶段 1 — skeleton**（`build_type_registry`）：
- 计算 **own_fields / own_methods**（本类自己声明的字段 / 方法），保存
  到 TypeDesc。这部分不依赖外部信息，是稳定的。
- 用 `merge_with_base` 计算**初始** `fields / field_index / vtable /
  vtable_index`。本 module 内 base 可解析 → 完整继承；跨 zpkg base 不
  可解析 → 仅含 own 部分（后续 fixup 补齐）。

**阶段 2 — fixup**（`try_fixup_inheritance`）：
- 在 [`LazyLoader::load_zpkg_file`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/lazy_loader/registry.rs)
  把新 zpkg 的 TypeDesc 插入全局 `type_registry` 之后调用（函数本体在 `metadata/loader/type_registry.rs`）。
- 扫描整个 `type_registry`，对每个 base 链 *新近可解析* 的 TypeDesc
  （`needs_fixup` 检测），用 `merge_with_base` 用全局 registry 重算
  layout，然后 `Arc::make_mut` mutate 该 TypeDesc。
- 固定点循环（`while try_fixup_inheritance() > 0`）—— 一次 fix-up 可能
  让另一个 TypeDesc 变可解析（三级链 `A → B → C`：B 先 fix → C 后 fix）。

**`Arc::make_mut`（clone-on-write）**：常态下 lazy_loader 是当前 TypeDesc 的
**唯一**强引用持有者（type 还没被实例化；`build_type_registry` 之后立刻
`module.type_registry_vec.clear()` 释放 by-id Vec 的 Arc 副本），此时
`make_mut` = in-place mutate。**例外**：seeded 的 eager 类型若本身 own-only（见下），
strong_count ≥ 2 且 `needs_fixup` 为真 —— 若用 `get_mut` 会失败而**跳过 +
每轮 WARN**（对一个从不派发、base 无字段的死类刷屏假警报）。故 `make_mut` 用
clone-on-write：给 lazy registry 一份私有、已 merge 的副本（lazy-lookup 路径
拿到完整 vtable/fields、收敛不再 warn），eager 源 module 保留其 own-only 副本
（不受影响）。

**Eager-loaded 类型的可见性**：merge 后的 main module（含 z42.core）
的 TypeDesc 通过 `VmContext::seed_lazy_loader_types(&final_module.type_registry, &func_names)`
克隆到 LazyLoader 的 `type_registry`。这些 Arc strong_count = 2（main
+ lazy_loader）—— **通常**不需要 mutate，因为 `build_type_registry` 已正
确解析了它们的 base（base 在同一 merged module 内）。**唯一例外**：base 位于
*仅惰性加载* 的 zpkg（如 `class ProjectHooks : BuildHooks`，app 未静态链
`z42.build`），则该 eager 类型 own-only 且 `needs_fixup` 为真 —— 由上面的
`make_mut` CoW 兜底。

**`needs_fixup` 计算**：
```text
expected_field_count  = base.fields.len() + len(unique own_fields not in base)
expected_vtable_count = base.vtable.len() + len(distinct simple_names of
                        own_methods not already in base.vtable_index)
needs_fixup = td.fields.len() != expected || td.vtable.len() != expected_v
```

**幂等性**：fixup 运行两次产生相同结果。已正确的 TypeDesc（`needs_fixup`
返回 false）跳过；`make_mut` CoW 后该副本 `needs_fixup` 即为 false → 下一轮
不再重算 → 收敛到 fixed point。

**延后未解析**：base 一直不可解析时 → 不报错，TypeDesc 保持 own-only
状态。下次新 zpkg 加载触发的 fixup 会重试。VM 退出时若仍有 own-only
的子类，相关字段访问会写入越界 slot（潜在 UB）—— 当前依赖 build 系统
保证依赖完整性；未来可在 try_lookup_type 路径加 "all-deps-loaded" 断言。

---

## resolve_namespace / resolve_dependency 的分工

位置：`src/runtime/src/metadata/loader/`

| 函数 | 签名 | 语义 | 调用方 |
|------|------|------|--------|
| `resolve_namespace(ns, libs_paths)` | `-> Result<Vec<PathBuf>>` | 返回**所有**声明该 namespace 的 zbc/zpkg 文件（不 bail on ambiguous）| 编译期诊断 / `app.rs` 启动反查 (.zbc → zpkg) |
| `resolve_dependency(zpkg_file, libs_paths)` | `-> Result<Option<PathBuf>>` | 按 zpkg 文件名直接查找 | LazyLoader 内部（按文件名精确定位）|

**设计权衡**：`resolve_namespace` 保留（返回所有声明者而非 bail），而非彻底删除。
理由：编译器 / 诊断工具仍可能问"哪些 zpkg 提供某 namespace"，保留此 API
更通用。

---

## VCall 分发与 TypeDesc

### ObjNew dispatch（与 VCall 对称）

`Instruction::ObjNew { dst, class_name, ctor_name, args }`：

```
1. type_desc = module.type_registry[class_name] | lazy_loader.try_lookup_type(class_name)
            | make_fallback_type_desc(...)
2. allocate ScriptObject(type_desc)，字段槽按 fields[i].type_tag 经 default_value_for 初始化
3. ctor_fn = module.func_index[ctor_name] | lazy_loader.try_lookup_function(ctor_name)
4. if ctor_fn: exec_function(ctor_fn, [obj, ...args])
   else:       skip ctor call（默认无参 ctor 语义；TypeChecker 已确保
               有显式 ctor 时 ctor_name 命中）
5. frame[dst] = obj
```

**与 VCall 对齐的核心**：编译期完整 overload resolution，VM 直查 `ctor_name`，
不做 `${class}.${simple}` 名字推断。
`ctor_name` 含 `$N` arity suffix（重载场景）；单 ctor 时无 suffix。

**字段默认值**：步骤 2 把
slot 初始化为对应类型的默认值（`int*`/`f64*` → 0、`bool` → false、`char` →
`'\0'`、`str` / 引用 → null），而非一律 `Null`。具体映射由
`metadata::default_value_for(type_tag) -> Value`（`metadata/types/field.rs`）单一函数提供，interp 与
JIT (`jit_obj_new`) 共享实现。需要这一步的前提是 `FieldSlot` 携带
`type_tag: Box<str>`（从 zbc `FieldDesc.type_tag` 透传），见下文 TypeDesc 结构。

ctor 入口由编译器侧 IrGen 注入字段 init（base ctor call 之后、用户 body
之前）；无显式 ctor 但本类或本地祖先链有字段 init 的类，编译器合成无参
隐式 ctor 内联整条链的 init 表达式。详见
`docs/reference/src/language/README.md` §6.3。

### TypeDesc 结构

```rust
struct TypeDesc {
    name: String,                          // FQ 类名，e.g. "Std.Collections.Stack"
    id: TypeId,                            // 进程内全局唯一（见下「TypeId 的作用域」）
    base_name: Option<String>,             // 父类 FQ 名
    fields: Vec<FieldSlot>,                // 布局顺序，基类字段在前
    //   FieldSlot { name: Box<str>, type_tag: Box<str>, … }
    //   type_tag 用于 ObjNew 选默认值
    field_index: NameIndex,                // 字段名 → slot（linear scan，见下）
    vtable: Vec<(String, String)>,         // 方法名 → FQ func name
    vtable_index: NameIndex,               // method → vtable slot（同上）
    class_flags: u8,
    visibility: u8,
    cold: Option<Box<TypeDescCold>>,       // own_fields / own_methods / type_params / type_args /
                                           // type_param_constraints / custom_attributes …（非热路径）
}
```

TypeDesc 由 `build_type_registry`（`metadata/loader/type_registry.rs`）在模块加载完成后按 topo
sort 预计算一次，避免运行时重复构建。

#### NameIndex：linear-scan 替代 HashMap

`field_index` / `vtable_index` 用 `NameIndex`（`Vec<(Box<str>, usize)>`）
存储，hot path 是 linear scan。**不是 HashMap**。

**为什么**：
- z42 stdlib + 用户代码典型 class 字段
  / 方法数 ≤ 16。linear scan ≤16 项的 `Box<str>` ≡ `&str` 比
  `HashMap<String, usize>` 探测 + 字符串 compare **快**：cache locality 友好，
  无 hash 函数计算开销，分支预测对小循环友好。
- IC + PIC 已拦截大部分 hot path 命中。
  NameIndex 替换的是 **IC miss 时的 fallback 路径**，把"miss = hash + compare"
  改为"miss = linear scan + str compare"。命中 IC 时完全不走这里。
- 内存：`Box<str>` 比 `String` 省 8 B / entry（无 capacity 字段）。

**何时该退化为 HashMap**：N ≥ 64 entries 时 linear scan 开始亏。当前 z42 没
有这种 class；若未来出现，把 `NameIndex` 内部实现改为 hybrid（N ≤ K linear，
N > K HashMap），调用方零改动 —— `NameIndex` 的 public API 故意按
`HashMap<String, usize>` 子集设计。

**API**：`get(&str) -> Option<&usize>` / `insert(String, usize) -> Option<usize>` /
`iter()` / `FromIterator<(String, usize)>` / `Clone`。位于
[`src/runtime/src/metadata/name_index.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/name_index.rs)。

### VCall 指令执行

位置：`src/runtime/src/interp/exec_vcall.rs::vcall`（PIC 命中直调；miss 时调 `interp/vcall_resolve.rs::resolve_vcall`）

对象 receiver 的解析（`resolve_vcall` 阶梯第 4 档，伪代码）：

```rust
let type_desc = obj.type_desc_arc().clone();

// 4a. 优先走预计算 vtable（按重载名映射到编译器绑定的 override 槽）
if let Some(&slot) = type_desc.vtable_index.get(method) {
    func_name = type_desc.vtable[slot].1   // → module.func_index，未命中再 try_lookup_function
}
// 4b. module 类层级：resolve_virtual(module, &type_desc.name, method)
// 4c. 沿基类链逐层试 "{cur}.{method}"（及擦除名），
//     基类取自 module.classes，再取自全局 type_registry（cross-zpkg 基类）
// 全部 miss → bail!("VCall: function `{}.{}` not found")
```

命中 module-local 函数时把 `(type_id, slot, fn_idx)` 写回该站点的 `VCallIC`（见下「Method token system」）。

**关键不变量**：`type_desc.name` 必须是 FQ 名（否则 4b/4c 生成的候选名是 bare 名 → miss）。
`build_type_registry` 从 `Module.classes[].name`
构建 TypeDesc.name，而 ClassDesc.name 由编译器写入 —— 编译期 `QualifyClassName`
是最终决定者。

> 注：派发目标的**决议**集中在
> `src/runtime/src/interp/vcall_resolve.rs`，interp 与 JIT 共用同一条阶梯（boxed primitive →
> boxed struct → primitive/array receiver → object vtable / 继承链），各自只决定「怎么调」。

### 非 Object receiver 的派发：`primitive_class_name`

`Value` 不是 `Object` 时没有 `TypeDesc` 可查，VM 改用
`primitive_class_name`（`src/runtime/src/interp/exec_vcall.rs`）把 `Value` 变体映射成
stdlib 类的 FQ 名，再构造 `{class}.{method}` 直查 `func_index`：

```
I64 → Std.Int32   F64 → Std.Double   Bool → Std.Boolean
Char → Std.Char   Str → Std.String   Array → Std.Array
```

`Value::Array` 走的正是这一条：`T[]` 不携带 TypeDesc
引用，`arr.Clone()` / `GetType()` / `ToString()` / `Equals()` / `GetHashCode()` 先试
`Std.Array.<method>`，未命中再沿基类回落 `Std.Object.<method>`——与 `Std.Int32` / `Std.String`
完全同款。`is_instance` / `as_cast` 侧则硬编码识别 `Array` / `Object` / `Std.Array` /
`Std.Object` 的子类型关系。

### 接口 `static abstract` 成员的派发（值驱动，复用 VCall）

`interface INumber { static abstract Self op_Add(Self a, Self b); }` 这类**接口静态抽象成员**，
在泛型代码 `T Add<T>(T a, T b) where T : INumber { return a + b; }` 里编译期无法确定 `T`。

**没有引入新 IR 指令**（不需要 `StaticCallViaIface` / `InterfaceStaticCall` 之类）：binder（`ExprTyper._bindBinary`）见左操作数是型参，就去该型参的 `where` 约束接口里
找 `static abstract op_X`，发一条**接收者驱动的普通 `VCall`**（`vcall a.op_Add(b)`）。运行期由
`a` 的具体类决定跑哪个实现——`Value::Object` 走 TypeDesc / vtable，基元与数组走上一节的
`primitive_class_name`，两条路都是既有阶梯（`vcall_resolve.rs`），**零新增派发机制**。

代价就是一次普通函数调用：`Std.Int32.op_Add` 的 body 是 `return a + b` → 一条 `add` 指令，
所以泛型 `a + b` ≈ 「1 次调用 + 原生加法」。

**值驱动的固有边界**：派发靠 `args[0]` 的运行期值，因此**无参的类型级静态成员**
（`T.Zero` / `T.Parse(s)`）这条路走不通——那需要把 `T` 的 TypeDesc 传到泛型 callsite，未实现。

面向用户的规则（含实现方必须写 `static override`、结果类型恒为 `T`）见
[泛型约束 · 运算符如何在型参上派发](https://z42-lang.github.io/z42/reference/language/generic-constraints.html)。

---

## 闭包 dispatch（Closure / StackClosure / FuncRef）

`Value` 三种 callable variant（`FuncRef(Str)` / `Closure(VarGcRef)` / `StackClosure`）的 CallIndirect 路径：

```
match callee_value {
  FuncRef(name)               → 直接 call name；无 env
  Closure(VarGcRef)           → ClosureData { env, fn_name }（在 GC var region）；env 作 implicit first arg → call fn_name
  StackClosure { env_idx, fn_name } → 从 caller frame.env_arena[env_idx] clone Vec<Value>
                                       → 升格为临时 GcRef → 作 implicit first arg → call fn_name
}
```

**StackClosure**：env 在 caller frame 的 `env_arena: Vec<Vec<Value>>` 中，零堆分配。`Value` 侧是 8B 句柄
`{ idx, frame_id }`，载荷 `StackClosureData { env_idx, fn_name }` 在 `transient_arena`；CallIndirect 时从
`frame.env_arena[env_idx]` 复制内容**物化出独立 GcRef**，callee 不区分 stack/heap 来源。

**GC root**：每个 `VmFrame` 内嵌 `regs` + `env_arena` 指针，GC scanner 单循环遍历 `call_stack` 即可同时 mark frame regs 和 stack closure env 中的 Object/Array refs，确保不被回收。

> ⚠️ **这条路径当前是死的**：置位 `MkClosInstr.StackAlloc` 的编译期 pass（`ClosureEscapeAnalyzer`）
> 已从 `src/` 消失，三个发射点全部传常量 `false` ⇒ 编译产物里**永不出现** `Value::StackClosure`，
> 闭包一律走 `Value::Closure` 堆路径。运行时这一半完整保留、随时可用。
> lifetime 安全原本由那个分析器在编译期保证（closure value 永不离开创建帧），要复活得先重建编译期
> 一侧——细节与复活路线见[逃逸分析](escape-analysis.md)末节。

## interp vs JIT 分发

位置：`src/runtime/src/vm.rs` + `interp/` + `jit/`

- **interp**：`exec_function` → `exec_instr` match 分发到近 70 个 IR 指令
  实现。`Value` 是 `Copy` 的 16B tagged enum（见 [object-abi.md](object-abi.md)）
- **JIT**：Cranelift 后端把 `Function` **惰性逐函数**编译成 native code（见 [jit.md](jit.md)）；
  模块默认模式由 `Vm::default_mode`。
  **z42vm CLI 默认 = jit**（built `--features jit`
  时；jit-less 构建如 wasm 回落 interp）。`--mode interp|jit` 显式覆盖。
  注意：嵌入式 FFI 的 `z42_host_invoke`（`host/ops.rs`）固定走 `interp::run_returning`，
  **不受 CLI 默认影响**；`z42_run_app` 路径（`app::run`）的模式由 `RunOpts.mode` 决定，未指定时取 `app::default_mode()`（jit 编入则 jit，否则 interp）。
- **AOT**：未实现（`Vm::run` 对 `ExecMode::Aot` 直接报错；`app::run` 里已有 eager 预加载分支）

两种后端共享：
- `Module` / `Function` / `Instruction` 数据结构
- `TypeDesc` / `FieldSlot` / vtable
- corelib builtin 调度表（`corelib/builtin_table.rs`）
- LazyLoader（interp 与 JIT 都经它按需加载依赖 zpkg）

---

## JIT↔VM 元数据契约

位置：[`src/runtime/src/jit/vm_interface.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/jit/vm_interface.rs)

`pub trait JitVm` 是 JIT 后端对 metadata 模块的**只读契约** ——
codifies "JIT 编译期需要从 module 拿到什么"，与 helper 运行期通过
`*const Module` raw pointer 拿到的具体字段访问分离。surface 是 4
个方法：

```rust
pub trait JitVm {
    fn functions(&self) -> &[Function];
    fn string_pool(&self) -> &[String];
    fn module_name(&self) -> &str;
    fn type_lookup(&self, class_name: &str) -> Option<&Arc<TypeDesc>>;
}
impl JitVm for Module { ... }   // 默认实现
```

### 为什么只是契约 codification

"translate 只 take `&dyn JitVm`，不
import metadata 内部类型；helpers/* 同样走 trait"这一**完整愿景**受两点结构性约束：

1. **translate 必须看见 `Instruction` enum**：近 70 个 arm 的模式匹配是 JIT 的
   输入语言形态。把 IR 藏在 trait 后等于丢弃 visitor pattern 的编译期穷举性
2. **helpers 通过 `*const Module` raw pointer 访问 Module**（extern "C" ABI
   约束）：raw pointer 必须指向具体 sized 类型。`*const dyn JitVm` 是 fat
   pointer，会破坏 Cranelift 生成代码对 helpers 的 ABI 调用

现状：
- **define + impl on Module**：codify "JIT compile-time 需要哪些 module 读"
- **`jit::run` 走 trait method**：`module.functions()` /
  `module.module_name()` 等替代直接字段访问
- **一个 helper exemplar**（`jit_obj_new`）：示范 helpers 内部如何调 trait
  method（`module.type_lookup(name)` 替代 `module.type_registry.get(name)`），
  raw pointer 不变
- **签名保持具体类型**：`JitModule::setup(&Module)`；`JitModuleCtx.module:
  *const Module` 是具体类型

### 待办

- 把余下 9 个 helper 全迁到 trait method
- 探索 `JitModule::setup<M: JitVm + ?Sized>(module: &M)` generic 化（前提：解决
  helper 端的 raw pointer ABI 问题，可能需要 type-erased dispatch table）
- 远期 AOT 后端通过另一个 `JitVm` impl 接入相同 contract

### Mockability

具体 ROI：单元测试可以构造一个 minimal `MockMetadata` struct
（只实现 trait 关心的 4 个方法）而不必拼装整个 `Module`。
`vm_interface_tests.rs` 包含一个 `MockMetadata` 示例，覆盖
`&dyn JitVm` dyn-dispatch 调用。

---

## JIT/EE helper 边界

位置：`src/runtime/src/jit/helpers/`

**问题**: JIT-compiled native code 需要回调 VM 完成所有非纯算术操作（构造对象、字符串、调度虚方法、读静态字段、抛/捕异常等）。这些回调被称为 "helper"，每个是一个 `#[no_mangle] pub unsafe extern "C" fn jit_xxx(...)`。Cranelift 通过 symbol-by-name 解析（`JITBuilder::symbol(name, ptr)`）让生成的 native code 调到 helper。

**要求**: 单一改动点。加新 helper 不能让人在 3 个文件里同步签名（容易漂移）。

**结构**: 三个职责文件 + 按 `Instruction` 类别拆的 helper 子文件:

```
jit/helpers/
├── mod.rs        — 共享工具（vm_ctx_ref / set_exception / 数值 helper）+
│                   VM_JIT_INTERFACE_VERSION
├── registry.rs   — 中央注册表（单一真相源）
│                   ├── HelperIds {fields...}     ← 一字段一 FuncId
│                   ├── register_symbols(builder) ← 给 JITBuilder 绑名→指针
│                   └── declare_imports(jit) -> HelperIds
│                                                 ← 给 JITModule 声明签名
├── value.rs      — Const* / Copy / 字符串 / get_bool / set_ret
├── arith.rs      — 算术 / 比较 / 逻辑 / 一元 / 位运算
├── control.rs    — throw / install_catch / match_catch_type
├── call.rs       — jit_call / jit_builtin
├── array.rs      — Array*
├── object.rs     — ObjNew / IsInstance / AsCast / Static* / default_of
├── object_field.rs — FieldGet / FieldSet
├── struct_ops.rs — struct 值类型指令（桥接共享 struct_arena）
├── vcall.rs      — 虚调用调用侧（PIC 命中 → 编译入口；miss → 共享 resolve_vcall → 编译入口或 interp 回退）
└── closure.rs    — load_fn / load_fn_cached / mk_clos / call_indirect
```

**与 `interp/exec_*.rs` 命名对称**: 加新 IR 指令时，interp 与 JIT 改的文件名一一对应（`exec_value.rs` ↔ `helpers/value.rs`），认知负担最小。

**消费者**:
- `jit/lazy.rs::LazyCompiler::setup` 调 `helpers::register_symbols(&mut jit_builder)` 与 `helpers::declare_imports(&mut jit)` 各一次
- `jit/translate/mod.rs` 通过 `pub use super::helpers::HelperIds;` 重导，逐函数翻译时消费 `HelperIds`

**新增 helper 改 2 处**:
1. `helpers/<category>.rs` 添加 `pub unsafe extern "C" fn jit_xxx(...)` 函数体
2. `helpers/registry.rs` 添加:
   - `HelperIds.xxx: FuncId` 字段
   - `register_symbols` 中 `reg!("jit_xxx", category::jit_xxx);` 行
   - `declare_imports` 返回的 `HelperIds {...}` 中 `xxx: decl!("jit_xxx", [params...], [returns...]);` 行

`mod.rs` 与 `translate/` 都不需要知道 helper 列表。

**`VM_JIT_INTERFACE_VERSION: u32 = 1` 作为 hook**: 当前没有运行时校验消费者——单一 JITModule 实现，bump 这个常量当 helper 集合或签名变化时即可。未来若引入第二个 JIT 后端（LLVM / wasm）或多版本 tier-up，启动时校验该值与编译期版本是否兼容，避免 cross-version helper 错配。属于"边界形式化"目标，**不引入校验代码避免过度设计**，留 hook 即可。

**为什么不引入 trait**: trait object dispatch 会带来间接调用开销，违背 zero-cost helper 设计；且 helper 已经走 cranelift symbol 解析机制（按名解析），用 trait 反而绕路。保留 `extern "C"` 直调零开销，仅在边界**形态**上做形式化（HelperIds + registry），不动**调用机制**。

**与 CoreCLR `ICorJitInfo` 对照**: CoreCLR 的 JIT/EE 边界是 callback-based vtable（`ICorJitInfo` 含 ~100 callback），versioned via GUID。z42 当前 ~70 helper、单实现、symbol-by-name 解析——比 ICorJitInfo 简洁但同方向（"单一边界 + 版本号"）。等需要支持多 JIT 后端时再考虑升级到 vtable 形态。

---

## JIT type specialization

位置：`src/runtime/src/jit/translate/`（`emit_int.rs` / `emit_fc.rs` / `predicates.rs`）的 `emit_i64_binop` / `emit_i64_cmp` /
`emit_bool_binop` / `emit_bool_not` / `emit_const_*` + `is_*_typed` 谓词。
依赖：`Function.reg_types: Box<[IrType]>`（来自 zbc REGT section）+
`Value` enum 锁布局（`#[repr(C, u8)]`，16 B，tag@0、payload@8，pinned by
`metadata::types_tests::value_*_payload_at_offset_8` 与 `size_of::<Value>() == 16` 编译期断言）。

**问题**: JIT-compiled 代码每条算术 / 比较 / 逻辑 op 都 call 一个
`extern "C"` helper（`jit_add` / `jit_eq` / ...）。helper 内部走
`match (Value::I64(x), Value::I64(y)) => ...` 模式匹配 + 回退 clone
+ 类型错误异常。在已知静态类型的热路径上这层 dispatch 是纯开销。

**策略**: zbc 携带每寄存器的静态 `IrType`（REGT section）。translate.rs 在 emit 每条 op 前查
`reg_types[dst]`、`reg_types[a]`、`reg_types[b]`；当三者都是 I64
（算术）/ I64（比较输入）/ Bool（逻辑）时，直接 emit 原生 Cranelift
指令 + 对 `frame.regs[idx]` 的原始 load/store，跳过 helper call ABI
+ variant match + clone。其它情况落回原有 helper。

**Decision tree**（每条 op 重复一次该判断）：

```
                 ┌──────────────────────────────────────────────┐
                 │ Instruction::<arith/cmp/logical>              │
                 └──────────────┬───────────────────────────────┘
                                ▼
                  ┌─────────────────────────────┐
                  │ is_X_typed(z42_func, regs)? │
                  └────────┬────────────────────┘
                  ┌────────┴────────┐
              YES │                 │ NO（含 Unknown / Str / mixed）
                  ▼                 ▼
        ┌──────────────────┐  ┌────────────────────┐
        │ raw load i64/i8  │  │ call hr_<op>       │
        │ from regs_base + │  │ (existing slow     │
        │   idx * 16 + 8   │  │  path; helper does │
        │ native op        │  │  match + clone +   │
        │ raw store TAG +  │  │  exception)        │
        │   payload        │  │                    │
        └──────────────────┘  └────────────────────┘
```

**Raw memory access invariants**:

- `frame.regs` 是 `Vec<Value>`；data pointer 在 JitFrame 构造时分配
  并稳定到函数结束（`take_pooled_regs(max_reg + 1)` 不会再 grow）。
  `jit_regs_ptr(frame)` helper（`jit/helpers/value.rs`）在 translate 入口调一次，缓存 SSA
  `regs_base`。
- Slot 地址 `= regs_base + idx * 16`（`VALUE_STRIDE = size_of::<Value>()`，`jit/reg_access.rs`；
  pinned by `value_size_observed` test）。
- 写时只写 1 B discriminant + 8 B payload；不调 `drop` 因为前任 slot
  值在 `reg_types[dst]` 静态类型下必是 Null（首次写）或同类型 primitive
  （`Value` 为 `Copy`，无 Drop）。
- 读时取 payload @ offset 8（VPALUE/I64/F64/Bool 用同一 offset；u8
  discriminant 不参与计算）。

**为什么不全程 inline**: helper 仍负责（且会持续负责）：
1. Div / Rem — i64 /0 必须抛 catchable z42 exception，原生 sdiv 触
   SIGFPE 不行；
2. Str concat — 需要 `Arc::clone` + alloc，非 inline-able；
3. Mixed / Unknown 类型 — `Value::Str + i64`、`Value::Object.ToString` 等
   user-visible coercion 路径；
4. Object / Array / Closure 路径 — heap allocation / vcall dispatch /
   IC slot 管理，complexity 远超 inline 阈值。

**性能验证**: `src/bench/scenarios/04_c2_p1_arith_loop.z42` 跑 10M-iter
SumSquares loop（每 iter 一次 mul + 两次 add + 一次 lt + 一次 brif），
M-series macOS 5-run 平均：

| 阶段 | 时间 | 相对 baseline |
|------|------|--------------|
| 全 helper | ~456 ms | 1.00× |
| arith + cmp 内联 | ~392 ms | 1.16× |
| + BrCond i8 load | ~302 ms | **1.51×** |

**与 CoreCLR / Java JIT 对照**: 这是 monomorphic specialization 的
基础形态——所有跨类型 op 都退到 helper，所有静态已知类型 op 都 inline。
CoreCLR 在此之上还有：tier-1 type-feedback 收集 + tier-2 多型/单型
IC + 内联 String.Concat + escape analysis 删 box——都是 z42 当前规模
不需要的。我们的 reg_types 已经是"全局 monomorphic"，覆盖率约 80-
90%（per spot-check on stdlib zbc），剩余在 generic / `default(T)` /
变量重命名等场景。

**Out of band**: `Value::I64` 内联 fast path 仍保留在 `jit_add` 等
helper 内——因为 helper 仍被 ~10%
mixed-type sites 调用，删除会让那些 sites 慢一档。

---

## Method token system

位置：`src/runtime/src/metadata/tokens.rs` + `metadata/resolver.rs`（+ `resolver/ic.rs`）+ `vm_context/statics.rs::resolve_static_field_id` + 各 `interp/exec_*.rs` 热路径

**问题**: z42 IR 所有跨引用 dispatch 用 `String` + `HashMap.get()` 做身份。每次虚调用一次 hash + 字符串等价比较；IR 内存表示膨胀；反射 R-series 设计无 token 锚点。

**设计**: 加载期解析所有可解析的 string 引用为 newtype token (`MethodId(u32)` / `TypeId(u32)` / `BuiltinId(u32)` / `FieldId(u32)` / `StaticFieldId(u32)` / `VTableSlot(u32)`)，存到外置 `Function.resolved: OnceLock<ResolvedTokens>`，热路径直接 Vec/Array 索引。无需改 IR `Instruction` struct 字段类型（zbc 格式不动；compiler 端不变）。

### 数据结构

```rust
// metadata/tokens.rs — 6 个 newtype + UNRESOLVED sentinel
pub const UNRESOLVED: u32 = u32::MAX;
pub struct MethodId(pub u32);     // → Module.functions[id]
pub struct TypeId(pub u32);       // → 进程内全局唯一（见下「TypeId 的作用域」）
pub struct BuiltinId(pub u32);    // → BUILTINS[id] 全局静态表
pub struct FieldId(pub u32);      // → TypeDesc.fields[id]
pub struct StaticFieldId(pub u32);// → VmCore.static_fields[id]
pub struct VTableSlot(pub u32);   // → TypeDesc.vtable[id]

// metadata/resolver.rs — Function.resolved 内容
pub struct ResolvedTokens {
    pub method_tokens:        Vec<AtomicU32>,   // Call 站点（cross-zpkg 留 UNRESOLVED）
    pub cross_module_targets: Vec<OnceLock<Arc<Function>>>, // Call 站点的 cross-zpkg 目标缓存（interp）
    pub call_jit_ic:          Vec<AtomicU32>,   // Call 站点的 lazy 目标函数 id 缓存（JIT）
    pub builtin_tokens:       Vec<u32>,         // Builtin 站点（100% 命中）
    pub type_tokens:          Vec<AtomicU32>,   // ObjNew 站点
    pub ctorless_marks:       Vec<AtomicUsize>, // ObjNew 站点「该类无 ctor」的已证明标记
    pub vcall_ic:             Vec<VCallIC>,     // VCall 多态 IC（4 槽）
    pub field_ic:             Vec<FieldIC>,     // FieldGet/Set 多态 IC（4 槽）
    pub static_field_tokens:  Vec<AtomicU32>,   // StaticGet/Set 站点
    pub site_index:           Vec<Vec<u32>>,    // (block, instr) → per-kind site_idx
}
```

### 解析时序

1. **`metadata::loader::build_type_registry`**: 在 topo order 中给每个 `TypeDesc.id` 分配 —— 号从 **进程级全局发号器** `tokens::alloc_type_id_block(n)` 批量取（每模块一次 `fetch_add`，模块内连续），**不是每模块从 0 重开**（见下）
2. **`Vm::run`** → `boot::prepare_execution` 调 `resolver::resolve_module(&module, ctx)`：
   - 走每个 Function 的每个 (block, instr) 元组
   - 对每个 token-bearing instruction 分配 per-kind site_idx
   - 解析能解析的 token：
     - `Call.func` → `module.func_index` 命中 → `MethodId`，否则 `UNRESOLVED`
     - `Builtin.name` → `corelib::builtin_id_of` 命中 → `BuiltinId`（必须，否则 panic）
     - `ObjNew.class_name` → `module.type_registry` → `TypeId`
     - `StaticGet/Set.field` → `ctx.resolve_static_field_id(name)` 懒分配
     - `VCall` / `FieldGet/Set` → 留 IC UNRESOLVED（receiver-type-dependent）
   - `function.resolved.set(...)` (OnceLock idempotent)

### 热路径行为

每条 token-bearing 指令在 `interp::exec_instr` 入口查 `resolved.site_index[block_idx][instr_idx] → site_idx`，传给对应 helper：

- **Call**: 命中 → `module.functions[cached]`；UNRESOLVED → `func_index` 查找 + 写回 cache；cross-zpkg 目标走 `cross_module_targets`（见上）
- **Builtin**: 直接 `BUILTINS[id]`（无 fallback；100% 命中）
- **ObjNew**: 仍走 `type_registry`（HashMap by name）；TypeId cache 用作 cross-zpkg observability
- **StaticGet/Set**: 命中 → `static_fields[id]`；UNRESOLVED → name lookup + 回填
- **VCall**: PIC 命中（4-slot 线性扫描；`recv.type_desc.id` 匹配任一槽位的 `type_id`）→ 直调 `module.functions[entry.fn_idx]`；miss → 走原 4 段 dispatch + 在 vtable_index hit 时通过 `vcall_ic_install` 填入第一个空槽（或 round-robin 牺牲一个槽）
- **FieldGet/Set**: PIC 命中（4-slot 线性扫描）→ 直读/写 `obj.slots[entry.slot]`；miss → `field_index` 查 + 通过 `field_ic_install` 填槽

> **Polymorphic IC**：IC 是 4-slot polymorphic IC（每槽 `(type_id, 载荷)`）。线性扫描使用 `UNRESOLVED` sentinel 提前退出（mono 站点首槽命中即返回，0 额外开销）。超过 4 个 receiver type 的站点用 round-robin counter 牺牲槽位（`ic.round_robin.fetch_add(1, Relaxed) % 4`）。所有 atomic 操作均为 `Relaxed` —— `type_id` 守门 payload，torn-read 等价于"刚好遇到迁移中的同型 dispatch"，下一次会收敛到稳定态。Helpers `field_ic_lookup` / `field_ic_install` / `vcall_ic_lookup` / `vcall_ic_install` 在 `metadata::resolver` 公开，供 interp + JIT helpers 共用。
>
#### TypeId 的作用域：为什么必须进程内全局唯一

六个 token 里，`MethodId` / `FieldId` / `VTableSlot` / `StaticFieldId` 都是**某个模块或某个
类内部的下标**，出了那个范围没有意义；`BuiltinId` 索引全局静态表。**`TypeId` 是唯一一个
「在一个作用域里发号、却要在跨作用域的比较中当身份用」的 token** —— 上面两条 PIC 都靠
`recv.type_desc.id == entry.type_id` 这一个 u32 相等来判定「是不是同一个 receiver 类型」。

这就要求发号范围 ⊇ 比较范围。若 `TypeId` 每个 `Module` 从 0 重开（per module），而跨 zpkg 的 `TypeDesc` 由
`VmContext::try_lookup_type` **原样返回**、保留外来模块的号，则不同 zpkg 的两个类会共号，任何**跨 zpkg 多态**的站点都会把
后到的 receiver 误命中先到者的缓存条目：

- `VCallIC` 撞键 → **调用另一个类的方法**，`this` 却是本类对象 ⇒ 按错误的字段布局解释内存
- `FieldIC` 撞键 → **读写错误的字段槽**，不崩不报错，**静默数据损坏**

典型现场：`Z42.Semantics.ParallelFor.Run` 的 `body.Run(i)`（`IParallelBody` 接口调用）同时
接 `CompileCuTask`（z42c.semantics）与 `SrcReadHashTask`（z42c.driver）。两者共号 139 时，
`CompileCuTask` 的 receiver 跑进 `SrcReadHashTask.Run`，其首行 `File.ReadAllText(this._srcs[i])`
读到槽 0 上的 `CompilationUnit[] _cus` ⇒ 自举链崩在
`__file_read_text: arg 0 expected string, got CompilationUnit`。触发条件只是两边的号碰巧对齐。

**做法**：`alloc_type_id_block` 从进程级 `AtomicU32` 批量发号，号段限定在
`[0, IMPORT_BASE)`，越界即 panic（回绕会引入撞号）。debug 构建下两条 PIC
的命中点各有一道常驻断言（`vcall_resolve::assert_pic_target` /
`resolver::assert_field_ic_slot`），任何再次违反该不变量的改动会**在误派发当场 panic**，
而不是变成一个远在天边的崩溃或错数据；release 构建下这两道断言被编译掉，热路径不变。

> **无锁读 type_id**：PIC scan 读 receiver `type_id` 不走 Mutex lock。`GcRef<ScriptObject>::type_desc()`（`metadata/types/object.rs`）通过 `data_ptr_unlocked()` 直接读 type_desc（write-once-at-alloc invariant 锁定 safety），跳过 per-entry 锁的 atomic CAS。详见 `docs/internals/src/runtime/gc.md`。这是 PIC inline 入 Cranelift IR（待办）的前置条件 —— 需 lock 的话 PIC 不能 inline。

### 跨 zpkg 时序

- **Intra-module**：`build_type_registry` + `resolve_module` 都在同一 module 加载完成后跑，所有 intra-module ID 立即可用。
- **Cross-zpkg lazy load**：lazy_loader 触发的 zpkg 加载后，对方模块的 `func_index` / `type_registry` 才填充。caller 模块的 `Function.resolved` 中的 cross-zpkg 引用初始为 `UNRESOLVED`；首次 dispatch 通过 string lookup 命中后**写回 cache**，单点回填。
- **StaticFieldId 全局 lazy 增量**：`VmContext::resolve_static_field_id(name)` idempotent — 任何模块在加载期或 dispatch 期遇到新的 static field name，立即分配新 id 并 resize Vec。已分配的 id 不变（resolver-populated cache 跨 module reload 仍有效）。

### JIT 与 wire 形态

- **JIT**：JIT helper 走 token / IC 形态，但 dispatch 仍走 helper-call 一次（helper carries IC）。IC hit 时直接通过
  `JitModuleCtx.fn_entries_by_id[cached_fn_idx]` 跳到目标 native 代码，无 HashMap 哈希、无 vtable 解析，
  hot path 与 interp 行为对齐。
- **wire**：zbc 中 IR 字段 token 化——本地 = `module.Functions/Classes` 索引；cross-zpkg = `IMPORT_BASE + STRS idx`
  （STRS 池复用 + IMPORT_BASE bit 编码，无需单独 IMPT 格式）；token id 按源序分配；IR enum 字段在内存里保持
  `String`，token 化只在 wire 边界发生。
- **待办**：JIT 机器码 inline IC check（跳过 helper 调用本身，需要 cranelift 端的复杂 control-flow）；
  compiler 端 token-aware emit perf。

### 与 `func_ref_cache_slots` 的关系

`func_ref_cache_slots` 是 method group 转换的 module-level 缓存，与 `method_tokens` 是两套独立的运行时 cache；JIT 端若做 method-token 整合，可考虑统一到一套 token 系统。

---

## corelib builtin dispatch

位置：`src/runtime/src/corelib/`

`Builtin` IR 指令在 interp / JIT 都经 `BuiltinId`（加载期由 `corelib::builtin_id_of` 解析）索引 `corelib::BUILTINS` 表分发（`exec_builtin_by_id`；按名入口 `exec_builtin(name, args)`）。
builtin 按功能分 submodule：`string.rs` / `io.rs` / `math.rs` / `fs.rs` 等，详见 `src/runtime/src/corelib/README.md`。

**新增 builtin 三处同改（强制规则）**：
1. `corelib/<module>.rs` — 实现
2. `corelib/builtin_table.rs` — `BUILTINS` 表**表尾追加**一行（下标即 `BuiltinId`，不可插入中间）
3. stdlib 中对应的 `[Native("__name")]` 声明 — 类型签名（`native_decl_tests.rs` 对账）

---

## 关键设计权衡

### Value 的表示：16B `Copy` + GC 句柄

- **GC 模型**：z42 是 GC 语言，对象/数组/字符串/闭包都在自有 GC 堆（`gc/`）里；`Value` 里只放 8B 句柄
  （`GcRef<T>` 标记指针、`Str` 细指针、`VarGcRef`），无引用计数、无运行时 borrow check。
- **`Value` 是 `Copy` 的 16B POD**（`#[repr(C, u8)]`，tag + 8B payload，`size_of::<Value>() == 16` 编译期断言）：
  clone = memcpy，`Vec<Value>` 无 Drop glue。瞬态变体（`Ref` / `PinnedView` / `StackClosure` / `StructRefHeap`）
  只带 `{idx, frame_id}` 句柄，载荷在 per-thread `transient_arena`。
- **跨线程**：`Value` 与 `VmCore` 满足 `Send + Sync`（`gc/arc_heap_tests/send_sync.rs` 编译期断言）。

表示细节、JIT 共享布局与 ABI 约束见 [object-abi.md](object-abi.md)。

### 为什么懒加载归 VmContext 持有

若用 thread_local `STATE` + `install` / `uninstall` /
`try_lookup_*` 自由函数集合：

- 同一进程跑多个 VM 实例时 STATE 互串（多 VM-per-process 卖点失效）
- 测试间状态污染靠手动 `uninstall`，遗漏即漂移
- 与「多线程共享一个 VM、GC 安全」的设计目标矛盾

故：`VmCore.lazy_loader: RwLock<Option<LazyLoader>>` 持有（同一 core 上的各线程 `VmContext` 共享），
`ctx.install_lazy_loader_with_deps(...)` / `ctx.try_lookup_function(name)` /
`ctx.declared_namespaces()` 等方法委托到 `LazyLoader` struct。
两个独立 `VmContext::new()`（各自一份 `VmCore`）的 lazy_loader 完全隔离，跨 ctx 切换不污染。

同样归 VmContext 的还有：

| 状态 | 归属 |
|------|------|
| 用户类静态字段 | `VmCore.static_fields`（经 `ctx.core`） |
| Pending exception (interp) | 无 thread_local —— interp 全程走 `ExecOutcome::Thrown(Value)` |
| Pending exception / 静态字段 (JIT) | `ctx.pending_exception` / `VmCore.static_fields`，无同步桥接 |

> JIT 端所有 extern "C" helper 签名都带 `ctx: *const JitModuleCtx`
> 第 2 参，Cranelift translate.rs 在调用点插入 `ctx_val`。helper 内部通过 `vm_ctx_ref(ctx)` → `(*ctx).vm_ctx`
> 两层间接拿到 `VmContext`。
>
> Runtime 内仅余 `jit/frame.rs::FRAME_POOL`（pure allocator cache，每线程
> 独立池子合理）。`VmContext` 是所有 runtime-mutable 状态的唯一规范来源。

### 为什么不预加载所有 stdlib

保留懒加载，按需加载。原因：
- 启动更快（stdlib 可能扩到几十个 zpkg，一次全加载浪费）
- lazy_loader 状态机虽然复杂，但代码已存在，维护成本可接受
- 失败模式一致：miss 时才真正报错，符合语言用户直觉

### 为什么 ConstStr 要重映射索引

主模块的 IR 在编译期生成时已经基于其 string pool；懒加载的 Function 的
ConstStr 索引相对自己 pool。合并时若不重映射，懒加载函数里的 `ConstStr(3)`
会引用主模块 pool[3] 而不是它自己的 pool[3]。`remap_const_str` 加一个
`offset` 把懒加载索引推到 `main_pool_len + 相对偏移`，`try_lookup_string`
在运行时分段查找。

---

## GC 子系统 —— MagrGC

详细 GC 设计（接口形状、phase 路线、`GcMode` opt-in 模式、并发标记、自定义
allocator、分代 GC、card marking、finalizer 契约、迭代规划等）已抽取到独立文档：

- 📄 [`docs/internals/src/runtime/gc.md`](gc.md)

简要状态：

- **核心 trait**：[`crate::gc::MagrGC`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/gc/heap.rs) ——
  对齐 [MMTk](https://www.mmtk.io/) `VMBinding` porting contract
- **Backing**：[`Region<T>`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/gc/region.rs) chunked
  allocator + 8B `GcRef` 句柄（标记指针指向 `RegionEntry`）
- **三种 mode 可选**（`GcMode` enum + `Z42_GC_MODE` / `--set gc-mode=`）:
  - `GenerationalMarkSweep` (default) — minor GC 扫 young + dirty cards
    (~4× faster minor pause vs full STW)；major GC 全堆扫描
  - `StwMarkSweep` (opt-in，`stw`) — stop-the-world mark + sweep
  - `ConcurrentMarkSweep` (opt-in) — STW root snapshot → 后台并发 mark →
    短 STW handshake → STW sweep
- **Write barriers**：interp + JIT 5 个 FieldSet / ArraySet 写入点全 wired；
  call-site 通过 `Value::is_heap_ref()` 过滤 primitive；trait override 由
  各 mode 实现（concurrent: tricolor shading；generational: cross-gen card marking）
- **Finalizer**：sweep 时触发 + `Std.GC.Finalize(x)` 显式 API
  
- **Safepoint**：counter-throttled fast path + multi-collector arbitration
  + interp + JIT 全 instrumented

剩余 backlog 见
[gc.md "GC 后续迭代规划"](gc.md#gc-后续迭代规划)。


## 延伸阅读

- `docs/internals/src/runtime/gc.md` — GC 子系统完整设计（接口、phases、modes、benchmarks、迭代规划）
- `docs/internals/src/formats/ir.md` — IR 指令集、zbc 二进制格式
- `docs/internals/src/runtime/jit-design.md` — Cranelift JIT 后端设计
- `docs/internals/src/runtime/execution-model.md` — ExecMode 注解、interp/JIT/AOT 切换语义
- `docs/internals/src/stdlib/architecture.md` — stdlib 三层架构（intrinsics / HAL / script BCL）
- `docs/agent/rules/runtime-rust.md` — Rust VM 开发规范（错误处理、测试组织、Value 类型约定）
