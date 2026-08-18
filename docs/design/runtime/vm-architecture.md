# z42 VM 内部实现原理

> **目的**：记录 Rust VM 的内部数据结构、算法、加载策略与关键设计决策。
> 让新接手者不必阅读大量源码即可理解"为什么这样设计"。
> 面向 VM 开发者，不面向 z42 语言使用者。
>
> 使用者视角请看 `docs/design/runtime/ir.md`、`docs/design/runtime/execution-model.md`。

---

## VmContext / VmCore —— 运行时状态归口（2026-04-28 + add-multithreading-foundation Phase 1-3 + add-threading-stdlib, 2026-05-20）

宿主代码运行 VM 的标准流程：

```rust
let ctx = VmContext::with_module(final_module);    // production: 把 Module 装入 VmCore
ctx.install_lazy_loader_with_deps(libs_dir, main_pool_len, declared, loaded);
let vm = Vm::new(ExecMode::Interp);                 // Vm 不再持 Module
vm.run(&ctx, hint)?;
```

### VmContext 构造 entry —— 两个 + 一个内部

| 方法 | 用途 | 备注 |
|------|------|------|
| `VmContext::with_module(module: Module) -> Pin<Box<Self>>` | 生产入口；构造新 VmCore + 把 `Arc<Module>` 装入 `VmCore.module` | `__thread_spawn` worker 需要 `VmCore.module = Some` 来 dispatch；test 路径若不需要 module 走 `new()` |
| `VmContext::new() -> Pin<Box<Self>>` | 测试入口；构造新 VmCore + `module = None` | 单测大量用（heap / static_fields / corelib 单测均不需要真 Module） |
| `VmContext::new_with_core(core: Arc<VmCore>) -> Pin<Box<Self>>` | spawn 入口；**复用现有 VmCore**，仅构造 per-thread 字段 + register self 到 `vm_contexts` | `__thread_spawn` worker 通过此构造，让 worker 看见父 VmCore 的 static_fields / heap / lazy_loader / native_libs |

`__thread_spawn`（[corelib/threading.rs](../../../src/runtime/src/corelib/threading.rs)）流程：拿到调用者 `ctx.core` 的 `Arc::clone` → `std::thread::spawn` → worker 内 `VmContext::new_with_core(core)` 构造 worker ctx → `interp::exec_function` dispatch。worker 的 per-thread 字段（pending_exception / call_stack / func_ref_slots）私有；通过 `vm_contexts` 注册让 GC scanner 看见 worker 自己的 roots。

### VmCore：跨线程共享状态

`VmCore` 持有 **process-globally singular** 的状态，通过 `Arc<VmCore>` 让多个 `VmContext`（每个 OS 线程一个）共享：

- `static_fields: Mutex<Vec<Value>>` — 用户类 static 字段槽位，按 `StaticFieldId.0` 索引
- `static_field_index: Mutex<HashMap<String, u32>>` — FQN → 槽位 id 映射
- `lazy_loader: Mutex<Option<LazyLoader>>` — 按需 zpkg 加载器
- `native_types: RwLock<HashMap<(String,String), Arc<RegisteredType>>>` — Tier 1 native interop 注册表（**RwLock**，读多写少：dispatch 是纯读，写仅在 module 加载期）
- `native_libs: Mutex<Vec<libloading::Library>>` — 已加载的 native 库句柄
- `pinned_owned_buffers: Mutex<HashMap<u64, Box<[u8]>>>` — `Value::PinnedView` 的 owned 缓冲（spec C10）
- `processes: Mutex<HashMap<u64, ProcessSlot>>` — `Std.IO.Process` 子进程注册表
- `heap: Box<dyn MagrGC>` — GC 子系统接口（默认 ArcMagrGC 后端）
- `module: Option<Arc<Module>>` — 用户编译后的 Module，跨线程共享（add-threading-stdlib 2026-05-20）；测试路径 `None`，生产路径 `Some(Arc::new(module))`
- `threads: Mutex<HashMap<u64, JoinHandle<Result<()>>>>` — `Std.Threading.Thread` 的 JoinHandle slot table（add-threading-stdlib 2026-05-20）；`__thread_spawn` 插入，`__thread_join` take-out 后 join
- `next_thread_id: AtomicU64` — Thread slot id 计数器（同 processes 模式，单调递增）
- `mutexes: Mutex<HashMap<u64, Arc<parking_lot::Mutex<Value>>>>` — `Std.Threading.Mutex<T>` slot table（add-sync-primitives 2026-05-20）。`__mutex_lock_acquire` 把 Arc clone 到调用方 thread-local guard map；`__mutex_store` / `__mutex_unlock` 通过 thread-local 查回 + `force_unlock` 释放
- `next_mutex_id: AtomicU64` — Mutex slot id 计数器
- `channels: Mutex<HashMap<u64, ChannelSlot>>` — `Std.Threading.Channel<T>` slot table（add-sync-primitives 2026-05-20）。`ChannelSlot` 持 `Option<ChannelSender>` + `Arc<std::sync::Mutex<mpsc::Receiver>>`：多 producer 通过 `Sender::clone()` 分发，多 consumer 通过内部 Mutex 串行。`ChannelSender` enum 区分 `Unbounded(mpsc::Sender)` / `Bounded(mpsc::SyncSender)`（add-sync-primitives-bounded-channel 2026-05-20）
- `next_channel_id: AtomicU64` — Channel slot id 计数器
- `rwlocks: Mutex<HashMap<u64, Arc<parking_lot::RwLock<Value>>>>` — `Std.Threading.RwLock<T>` slot table（add-sync-primitives-rwlock 2026-05-20）。多读单写 lock 与 Mutex 同款 Arc + thread-local guard parking 模式；thread-local 用 enum `RwLockHeld { Read(Arc), Write(Arc) }` 区分释放路径
- `next_rwlock_id: AtomicU64` — RwLock slot id 计数器

`VmCore` 满足 `Send + Sync`（编译期 assertion 在 `src/runtime/src/gc/arc_heap_tests/send_sync.rs`）。

### VmContext：per-thread 视图

每个 OS 线程拿一个 `VmContext`，它持有：

- `core: Arc<VmCore>` — 指向 VmCore 共享状态
- `pending_exception: Arc<Mutex<Option<Value>>>` — JIT extern "C" 边界异常槽位
- `call_stack: Arc<Mutex<Vec<VmFrame>>>` — 当前线程帧栈（unify-frame-chain，2026-05-10）
- `func_ref_slots: Arc<Mutex<Vec<Value>>>` — method-group-conversion FuncRef cache 槽位（D1b）
- `process_next_id: AtomicU64` — 进程 slot id 计数器

每个 `VmFrame` 同时承载 `(regs ptr, env_arena ptr, func_name, file, line, column)`：GC root scanner 扫 regs+env_arena，stack-trace 读 name/file/line/col，interp `RefKind::Stack` 跨帧 deref 通过 `frame.regs`。

### Send-safety 与 GC scanner 设计

`MagrGC` trait 要求 `Send + Sync`（add-multithreading-foundation Phase 3.4）。GC scanner closure（mark 阶段被调用）也要求 `Send + Sync`，进而所有 closure 捕获都必须 Send + Sync —— 这是 per-thread 字段从 `Rc<RefCell<>>` 升级到 `Arc<Mutex<>>` 的根因（design Decision 2 Phase 3 amendment）。

Scanner closure 通过 `Weak<VmCore>` 捕获 VmCore（避免 `VmCore → heap → scanner → Arc<VmCore>` 循环引用），upgrade 失败时 silent skip。

**VmContext 注册表（add-vmcontext-registry 2026-05-20）**：VmCore 持 `vm_contexts: Mutex<Vec<VmContextPtr>>` 注册表。`VmContext::new()` 返回 `Pin<Box<VmContext>>` 以保证地址稳定（`PhantomPinned` 标 !Unpin 防 move-out），构造时 push 自身到注册表，Drop 时 retain 移除。GC scanner 改为：**1**) 上锁 vm_contexts → **2**) 遍历每个 VmContext ptr → **3**) `unsafe { &*ptr }` 扫其 `pending_exception` / `call_stack` 帧 / `func_ref_slots`。所有 VmContext 的 per-thread roots 在 mark 阶段都被看见 —— multi-thread 安全。Lock 持有期间 Drop 阻塞，无 use-after-free。

API 方法都用 `&self`（内部 Mutex/RwLock），调用方代码风格基本不变。详见 [`object-protocol.md`](../language/object-protocol.md)、[`interop.md`](../language/interop.md)、[`concurrency.md`](concurrency.md) 与 review2 §3 / §5.5 / §5.2。

**Native interop 入口**：`VmContext::register_native_type(Rc<RegisteredType>)` /
`resolve_native_type(module, name)` / `load_native_library(path)`；后者打开 `.dylib`
/`.so`/`.dll` 并调用其 `<basename>_register` 入口（约定）让 native 库通过
`z42_register_type` 把类型推入 `native_types`。Interp 入口设置 thread-local
`CURRENT_VM` 让 native callback 能找回 VM；详见 `src/runtime/src/native/exports.rs`
的 `VmGuard` RAII。

---

## VM 启动流程

```
z42vm <file>
  │
  ├── resolve_libs_dir()                       [main.rs]
  │      → $Z42_LIBS | <binary>/../libs | <cwd>/artifacts/build/libraries/dist/release
  │      → 解析结果回写 $Z42_LIBS（仅当未设/为空）：进程内运行的程序
  │        （尤其 z42c，直接读 $Z42_LIBS 做跨包 dep 解析）与 VM 看到同一 libs 目录，
  │        SDK 布局无需手动 `Z42_LIBS=`；显式设置不覆盖。[libs_env_to_publish]
  │
  ├── 5.1b 加载 z42.core.zpkg (eager，隐式 prelude)
  │      → modules[0]
  │      → initially_loaded_zpkgs = ["z42.core.zpkg"]
  │
  ├── 5.1c 加载 user artifact (.zbc / .zpkg)
  │      → user_artifact.module + dependencies + import_namespaces
  │      → probe `<basename>.zsym` 同目录；存在且 build_id 匹配 →
  │        合并 sidecar DBUG 到 per-module funcs（详见下方 sidecar 章节）
  │
  ├── 5.1d 依赖加载策略（search_dirs = [入口zpkg目录, stdlib libs]，见下「同址搜索」）
  │      interp: 纯懒加载（build_declared_candidates 填充 LazyLoader）
  │      jit/aot: eager 预加载所有声明依赖（**transitive BFS**，见下）
  │              （同时也填 LazyLoader 以防 type/func 零碎 miss）
  │
  ├── 5.1e merge_modules → final_module
  │      + build_type_registry / verify_constraints / build_*_index
  │
  ├── install_with_deps(search_dirs, pool_len, declared, initially_loaded)
  │      [lazy_loader.rs]
  │
  └── Vm::new(final_module, default_mode).run(entry)
```

### 依赖同址搜索（support-colocated-zpkg-deps，2026-06-20）

依赖 zpkg 按文件名在 `search_dirs` 列表里**按序**解析，而非单一 `libs_dir`。z42vm CLI
组装 `search_dirs = [入口 zpkg 所在目录, stdlib libs 目录]`（去重、顺序固定 →
解析确定性；入口目录优先）。**动机**：apphost 把 payload 与它的包依赖放在一起发布——
`bin/z42c`(apphost) → `programs/z42c/z42c.driver.zpkg`，其兄弟 `z42c.core.zpkg` 等也在
`programs/z42c/`，**不在 stdlib `libs/`**。同址搜索让 driver 既能找到同址的 `z42c.*`，
又能从 `libs/` 找 `z42.*`，无需把两者拍平到一个目录。

落点：`LazyLoader.search_dirs: Vec<PathBuf>`（原 `libs_dir: Option`）；transitive unfold
用 `ZpkgCandidate::build_in_dirs(dirs, file)`（首个含该文件的目录胜出）。eager BFS 与
`build_declared_candidates` 同样遍历 search_dirs。FFI/embedded（test-runner）从 bytes 加载、
无入口目录，search_dirs 退化为 `[libs_dir]`，行为不变。

---

## Sidecar 调试符号加载（2026-05-10 split-debug-symbols）

### 加载策略：eager + 同步

`loader::load_zbc` / `load_zpkg` 加载主 artifact 完成后**立即**探测同目录 `<basename>.zsym` 并按 BLID 校验合并。设计选择：

| 选项 | 选择 | 理由 |
|------|------|------|
| eager vs lazy | **eager** | 异常路径（trace 构造）必须零 IO；startup 开销可控（典型 sidecar < 100KB） |
| sidecar 路径 | `<basename>.zsym` 同目录 | 不需要 search path 概念；CI/CD 部署单一目录即可 |
| BLID 算法 | BLAKE3-128 截 16B + payload 零填 hash | Rust/C# 生态成熟；payload 零填使 sidecar 与 main 字节同步 |
| BLID 不匹配 | warn + ignore，加载继续 | 调试符号缺失不应让程序无法启动 |

### 合并机制：直接在已加载的 `Module` 上 mutate

主 zpkg/zbc 解析返回 `Vec<(Module, namespace)>` 后立刻 mutate per-module Function：

```rust
for ((module, ns), (sym_ns, fns)) in module_pairs.iter_mut().zip(sidecar.modules) {
    for (i, fb) in fns.into_iter().enumerate() {
        if !fb.line_table.is_empty() { module.functions[i].line_table = fb.line_table; }
        if !fb.local_vars.is_empty() { module.functions[i].local_vars = fb.local_vars; }
    }
}
```

不用 `RefCell` / `OnceCell` 包装 `line_table` — sidecar merge 发生在 `merge_modules` 之前的可变所有权窗口内。这避免引入运行期 borrow check 开销。

### 与 `merge_modules` 的顺序

`load_zpkg_bytes_with_sidecar` 调用顺序：

1. `read_zpkg_modules(raw)` → `Vec<(Module, ns)>` 拥有可变 Module
2. `apply_zpkg_sidecar(&mut module_pairs, raw, sym, sym_path)` — 把 sidecar DBUG 合入 per-module funcs
3. `merge_modules(modules)` — 命名空间扁平化 + 函数表合并
4. `build_type_registry` / `verify_constraints` / `build_func_index`

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

> **实现位置**：[src/runtime/src/metadata/loader.rs](../../../src/runtime/src/metadata/loader.rs) `apply_zbc_sidecar` / `apply_zpkg_sidecar`；解析在 [zbc_reader.rs](../../../src/runtime/src/metadata/zbc_reader.rs) `parse_zbc_sidecar` / `parse_zpkg_sidecar`。

## Embedding Entry（2026-05-10 add-embedding-api H1）

> **本节边界**：从 VM 内部视角描述 `crate::host` 模块如何融入 VM 架构（数据归属、状态管理、与 `crate::native` 的边界）。**API 形态、Host C ABI 函数签名、宿主使用模式**归 [`embedding.md`](embedding.md)，不在本文重复。

`src/runtime/src/host/` 是 z42 VM 的**宿主嵌入入口**：与上方 `z42vm` CLI 启动流程并列，存在于另一条进入 VM 的路径。

```
host application (iOS / Android / IDE 插件 / 其他 native)
   │
   │ z42_host_initialize(&cfg, &handle)
   ▼
host::state::HOST  (RwLock<Option<HostState>>，进程单实例)
   │
   │ z42_host_load_zbc(handle, bytes, len, &mod)        [H2 待实施]
   │ z42_host_resolve_entry(handle, mod, fqn, &entry)   [H2 待实施]
   │ z42_host_invoke(entry, args, n, &result)           [H2 待实施]
   ▼
（H2）interp::run_method / jit::run（与 CLI 启动流程汇合到同一执行路径）
```

### 模块边界与 VM 全局状态

| 关系 | H1 状态 | H2 计划 |
|------|---------|---------|
| host 模块 ↔ `VmContext` | host 模块**未触及**任何 VM 全局状态；只在 `host::state::HOST` 持有 `ResolvedConfig` | `load_zbc` 接 `metadata::ZbcReader` + `merge_modules`；`invoke` 走 `interp::run_method` |
| stdout / stderr | `Z42WriteSink` 函数指针存在 `HostState`，未注入 VM stdout writer | sink 包装为 `impl Write`，在 `invoke` 启动前 swap VM stdout writer，结束 / shutdown 时复原 |
| panic 隔离 | 每个 `extern "C"` 入口经 `host::guard()` `catch_unwind` 兜底，panic → `Z42_HOST_ERR_INTERNAL` | 不变 |
| 错误诊断 | TLS `LAST_ERROR` 由 `host::error::set_error / clear_error` 管理；与 `native::error::LAST_ERROR` 独立（两条 ABI 各自维护） | 不变 |

### 与 `crate::native` 的边界

| 方向 | 模块 | 解决问题 |
|------|------|---------|
| native 代码 → 注册类型/方法 | `crate::native` (`z42_register_type` 等) | 扩展语言（CPython C 扩展类比） |
| 宿主 app → 启动 VM | `crate::host` (`z42_host_initialize` 等) | 嵌入运行时（CoreCLR `coreclrhost.h` 类比） |

两者复用 `z42_abi::Z42Value` / `Z42Args` / `Z42Error` 类型，互不调用对方。

### 单实例 vs 多实例

v0.1 单实例：`HOST: RwLock<Option<HostState>>`。`Z42HostRef` 是一个 sentinel pointer（`0x1`），所有 host API 调用读 `HOST` 验证活跃。

多实例 / ALC-like 上下文进 [embedding.md §12 Deferred](embedding.md)。届时 `RwLock<Option<...>>` 升级为 `Slab<HostState>`，`Z42HostRef` 编码 `(idx, gen)`，VM 全局状态（GC heap、JIT cache、type registry）必须 per-handle 化。zpkg 重载/卸载/回收的完整设计（含保留根诊断、内部缓存回收）见 [load-context.md](load-context.md)。

详见 [docs/design/runtime/embedding.md](embedding.md) 与 [docs/spec/archive/2026-05-10-add-embedding-api/design.md](../../spec/archive/2026-05-10-add-embedding-api/design.md) D1/D5。

---

## LazyLoader：zpkg-based 依赖懒加载（2026-04-25 重构）

### 为什么改成 zpkg-based

**旧模型（namespace-based）**：Call miss 时从 FQ func_name 提取 namespace
前缀，调 `resolve_namespace` 在 libs 目录找**唯一**一个声明该 namespace 的
zpkg。若 ≥2 个 zpkg 共享同 namespace → `bail!("AmbiguousNamespaceError")`。

**为什么不行**：C# BCL 对齐后（stdlib 重组 W1），`Std.Collections` namespace 同
时出现在 `z42.core.zpkg`（List / Dictionary）和 `z42.collections.zpkg`
（Queue / Stack）。旧模型拒绝这种合法情况。

**新模型（zpkg-based）**：对齐 C# CLR 的 AssemblyRef 模型 —— **zpkg 是加载
单位，namespace 是逻辑分组**，多 zpkg 可共享同 namespace。

### 核心数据结构

```rust
struct LazyLoader {
    libs_dir: Option<PathBuf>,
    main_pool_len: usize,          // 主模块 string pool 长度（索引偏移基准）
    string_pool: Vec<String>,       // 聚合懒加载 string pool

    loaded_zpkgs: HashSet<String>,                  // 已加载 zpkg 文件名
    declared_zpkgs: HashMap<String, ZpkgCandidate>, // 声明但未加载

    function_table: HashMap<String, Arc<Function>>, // FQ name → Function
    type_registry: HashMap<String, Arc<TypeDesc>>,  // FQ name → TypeDesc
}

struct ZpkgCandidate {
    file_path: PathBuf,
    namespaces: Vec<String>,  // 该 zpkg 导出的 namespace 列表（从 NSPC section 读）
}
```

### Call miss 触发策略（Decision 1：策略 C + 回退 B）

```
try_lookup_function(func_name):
  if function_table has func_name → return hit

  # 策略 C：精确路由 —— 按 namespace 前缀筛选候选 zpkg
  ns = namespace_prefix(func_name)   // e.g. "Std.Collections.Stack.Push" → "Std.Collections"
  for zpkg_file in declared_zpkgs:
    if zpkg_file not in loaded_zpkgs
       and zpkg.namespaces 含 ns 或以 ns. 开头:
      load_zpkg_file(zpkg_file)
      if function_table has func_name → return hit

  # 策略 B 回退：若策略 C 无匹配，遍历所有剩余 declared-but-not-loaded
  for zpkg_file in declared_zpkgs - loaded_zpkgs:
    load_zpkg_file(zpkg_file)
    if function_table has func_name → return hit

  return None  # 真正 undefined
```

策略 C 的精确路由等价于 C# CLR 的 TypeRef → TypeDef 查找：按 namespace
（assembly 的 public type 的父 namespace）作为高效过滤器。

策略 B 是安全网，处理 zpkg 元数据不完整 / 用户 zbc 的 import_namespaces
不全等边界情况。

### Per-site cross-zpkg Call 目标缓存（review.md C7，2026-06-11 cache-cross-zpkg-call-target）

`try_lookup_function` 即使命中（function_table 已有），也是一次
`HashMap<String, Arc<Function>>` 探测（String hash + compare）。本模块 Call 有
`ResolvedTokens.method_tokens[site]`（`AtomicU32`）缓存 `module.functions` 下标，
第二次起纯整数索引；但 **cross-zpkg 目标不在 `module.functions`**（在 lazy loader
的 `function_table`），u32 下标够不着 → 这类 site 的 token slot 永远 `UNRESOLVED`，
**每次调用都重跑** `try_lookup_function`（上面的策略 C/B，至少一次 String hash）。

修法：`ResolvedTokens` 加一条与 `method_tokens` 平行、同 site 索引的
`cross_module_targets: Vec<OnceLock<Arc<Function>>>`。dispatch（[exec_call.rs](../../../src/runtime/src/interp/exec_call.rs)）在
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
- **仅 interp**：JIT 的 cross-zpkg 走 `jit_call` helper，本机制不动它（见下）。

### 惰性加载函数的 token 首执解析（perf-lazy-resolve-tokens，2026-08-18）

上面所有 per-site 缓存（`method_tokens` / `cross_module_targets` / `vcall_ic` /
`field_ic` / `builtin_tokens` / `static_field_tokens` / `type_tokens` / `site_index`）
都挂在 `Function.resolved: OnceLock<ResolvedTokens>`，由
[`resolver::resolve_module`](../../../src/runtime/src/metadata/resolver.rs) 一次性填充。
但 `resolve_module` **只在 `Vm::run` 里对 entry module 跑一次**——而
interp/JIT 模式下依赖是**纯惰性加载**（[app.rs](../../../src/runtime/src/app.rs)
`is_eager = matches!(mode, Aot)`，非 AOT 全 false）：除用户 artifact 外，
**所有依赖 zpkg**（自编译时即 z42c.core / z42c.syntax / z42c.semantics /
z42c.pipeline 全部）经 `LazyLoader::load_zpkg_file` 进 `function_table`，其
`Function.resolved` **永不被 set**。

后果：**整个自编译工作负载（跑在惰性加载的 z42c.* 里）dispatch 时所有 per-site
缓存全失效**——`resolved == None` → `site_idx` 恒 `UNRESOLVED` → VCall 无 PIC、
Field 无 IC、Builtin/Static 走名字查、每个 Call 都对 entry module 的
`func_index` 做一次 String hash+`memcmp` 且 miss、再 `try_lookup_function`。
profile 里 `get_inner`+`memcmp`+`try_lookup_*` 的大头即源于此。

修法：把 `resolve_module` 的**单函数体**抽成
[`resolve_function_tokens(func, module, ctx)`](../../../src/runtime/src/metadata/resolver.rs)，
并在 [`exec_function_body`](../../../src/runtime/src/interp/mod.rs) 顶部**首次执行时**
按需填充（`if func.resolved.get().is_none()`，OnceLock 门禁 → 每函数只解析一次；
热路径仅一次 relaxed atomic load，指令循环本就要再读它）。

**模块身份不变式（关键正确性约束）**：填充用的 `module` 必须是该函数**运行期实际
dispatch 所对的 Module**——始终是 entry module（惰性 callee 由调用方的 `module`
一路透传，根在 entry）。`method_tokens` / `type_tokens` 是 `module.functions` /
`module.type_registry` 的下标；对**别的** module 解析会铸出错下标。跨模块目标
（不在 entry module）在此正确解析为 `UNRESOLVED`，交由 `cross_module_targets`
per-site 首执缓存兜住（见上一节）。**`vcall_ic` / `field_ic`（运行期首派填充）、
`builtin_tokens`（全局闭集）、`static_field_tokens`（全局 `ctx.resolve_static_field_id`，
锁保护幂等）** 均与 module 下标无关 → 这几条是本优化的主要收益来源。

- **只填被执行的函数**：比"加载时对整个惰性 module 跑 `resolve_module`"更省
  （加载但从不调用的函数零解析开销），且天然拿到正确的 entry-module 身份。
- **并发安全**：worker 线程可并发首执同一函数；两者各自构建等价 `ResolvedTokens`
  （静态字段 id 按名幂等、同值），`OnceLock::set` 取胜者，落败者丢弃无副作用。
- **实测**：自编译前端 typecheck **1.26× faster**，`--dump-bound` 输出逐字节一致，
  无格式/wire/语义变更。

### JIT cross-zpkg 调用解析（fix-jit-cross-zpkg-call，2026-06-20）

JIT 把一个模块的全部函数编译成原生码，`jit_call` 跳转到 `fn_entries`（本 JIT 模块
编入的函数）。**跨 zpkg 的 callee 不在 `fn_entries`**，需两条配合机制：

1. **eager 加载必须 transitive**（[main.rs](../../../src/runtime/src/main.rs) `is_eager` 分支）。
   旧实现只加载 entry 的**直接** `dependencies` / `import_namespaces`，**传递依赖漏掉** ——
   例如 `Std.Toml.TomlValue.Parse` 经 `z42c.project → z42.toml` 间接到达，既没 merge
   进 `final_module` 也没进 `declared_zpkgs`（interp 靠运行期懒加载的传递 unfold 兜住，
   JIT 没有这一步）→ `--mode jit` 下 `undefined function`。修法：BFS 排空一个
   `{dep 文件, namespace}` 工作队列，每加载一个 artifact 再把它自身的 deps 入队，直到
   依赖图穷尽（`loaded_paths` 按 canonical path 去重）。于是整个传递闭包都 merge 进
   `final_module`、全部 JIT 编译、调用原生解析。

2. **`jit_call` 的 interp 兜底**（[jit/helpers/call.rs](../../../src/runtime/src/jit/helpers/call.rs)
   `cross_zpkg_via_interp`）。`fn_entries` miss 后镜像 interp `exec_call::call` 的解析
   顺序：① `module.func_index` → `module.functions`（已 merge 但未 JIT 编入的函数）；
   ② `try_lookup_function`（仅懒加载可达的 zpkg）——两者都在**解释器**上执行 callee，
   结果回填 JIT caller 帧。与 `jit_vcall` 既有懒加载兜底对称；在机制 1 完整 merge 后
   它是防御性安全网（把"漏 merge → 硬崩"降级为"正确但较慢的 interp 执行"）。

> 取舍：机制 1 让 JIT 每次运行都编译整个传递闭包（含全部触达的 stdlib），小输入下
> upfront JIT-compile 开销显著（z42.core 编译 interp 16.9s → jit 7.9s，约 2.1×，
> 仍 ~8× C#）。换来跨包调用全原生、零 interp 回退。未来 lazy-JIT / 原生码缓存可摊薄。

### 单函数 interp 降级：不可 JIT 翻译的 opcode（fix-jit-cross-zpkg-transitive-eager，2026-06-20）

机制 1（transitive eager merge）+ `--mode jit` 默认化的**联合副作用**：`compile_module`
现在要把整个传递闭包的**每一个**函数都喂给 cranelift 翻译，而闭包里必然含有 JIT 尚未
支持的 opcode —— 最常见的是 `out`/`ref`/`in` 参数（`LoadLocalAddr` / `LoadElemAddr` /
`LoadFieldAddr`，spec impl-ref-out-in-runtime）和 native interop（`CallNative` /
`CallNativeVtable` / `PinPtr` / `UnpinPtr`）。旧 `compile_module` 在 `translate_function`
里碰到这些 opcode 直接 `bail!`，`?` 把错误冒泡到顶层 → **整个程序 abort**（CI Bootstrap 的
`xtask build stdlib` 因此报 `JIT cannot translate LoadLocalAddr yet`）。

修法：把"整模块要么全编译要么全失败"改成**逐函数降级**。

1. **预扫描跳过**（[jit/mod.rs](../../../src/runtime/src/jit/mod.rs) `compile_module` +
   [jit/translate.rs](../../../src/runtime/src/jit/translate.rs) `jit_unsupported_reason`）：
   declare / translate 前先扫描每个函数，含不可翻译 opcode 的**既不 declare 也不 translate**
   （留在 `func_ids` / `fn_entries` 之外）。`fn_entries_by_id` 仍按 `functions` 顺序逐槽
   push `None`，MethodId↔槽位对齐不变。`jit_unsupported_reason` 的 opcode 名单必须与
   `translate_function` 里所有 `bail!` 分支严格同步。

2. **调用点自动走 interp 兜底**：`Call` 永远经 `hr_call`（`jit_call`）helper 跳转，**从不**
   emit cranelift 直接调用，所以被跳过的 callee 在 `fn_entries` miss → `cross_zpkg_via_interp`
   Case 1（`module.func_index` 命中已 merge 但未编入的函数）→ 解释器执行。`VCall` 三条派发路径
   （IC 快路径 / 原始类型接收者 / 对象 vtable）都补齐了同样的 merged-module interp 兜底
   （[jit/helpers/vcall.rs](../../../src/runtime/src/jit/helpers/vcall.rs)）。

3. **entry / static-init 也兜底**：`JitModule::run_fn` 在 `fn_entries` miss 时（被跳过的
   entry 或 `__static_init__`）改用 `interp::exec_function` 执行，不再硬报 `entry not found`；
   `JitModule::run` 的 `__static_init__` 名单改从 `module.functions` 枚举（对齐 interp 的
   `init_static_fields`），确保被跳过的静态初始化器**仍会运行**（否则静态字段静默不初始化）。

> 净效果：JIT 模块尽量编原生码，含 interp-only opcode 的函数（及其整棵调用子树）在解释器上
> 跑，全程对外行为与 `--mode interp` 一致。这才兑现 translate.rs 里"Function falls back to
> interp"那句注释（此前是谎言：实际是整模块崩）。回归测试：`src/tests/refs/{out_var,
> in_param,ref_local,ref_nested}` 摘掉 `interp_only` 标记，现在 interp + jit 双模式皆过。

### 依赖传递（Decision 4：着色算法）

```
load_zpkg_file(file_name):
  if file_name in loaded_zpkgs → return    # 已加载/正在加载（防 re-entry）
  loaded_zpkgs.insert(file_name)           # ★ 着色：先标记后加载

  artifact = load_artifact(file_path)
  remap ConstStr indices + 合并 function_table / type_registry（first-wins）

  # 递归展开该 zpkg 自己的 ZpkgDep 进 declared 集合
  for dep in artifact.dependencies:
    if dep.file not in loaded_zpkgs and not in declared_zpkgs:
      declared_zpkgs[dep.file] = ZpkgCandidate::build(libs_dir, dep.file)
```

**为什么先着色后加载**：若 A 依赖 B、B 又依赖 A（或自依赖），加载 B 时
`load_zpkg_file("a.zpkg")` 立刻返回（因 A 已在 loaded 集合），避免无限递归。

### 函数 / 类型冲突（Decision 6：first-wins）

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

`z42.core.zpkg` 在 main.rs 5.1b 阶段就被 eager 加载并 merge 进 main
module。`initially_loaded_zpkgs` 含 `"z42.core.zpkg"`，所以 LazyLoader
的 `declared_zpkgs` 根本不含它。这保证 prelude 语义（所有 `Std.*` 符号
启动即可用）。

### 两阶段类型加载：cross-zpkg subclass 字段继承（2026-05-14 fix-cross-pkg-subclass-fields）

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
- 在 [`LazyLoader::load_zpkg_file`](../../src/runtime/src/metadata/lazy_loader.rs)
  把新 zpkg 的 TypeDesc 插入全局 `type_registry` 之后调用。
- 扫描整个 `type_registry`，对每个 base 链 *新近可解析* 的 TypeDesc
  （`needs_fixup` 检测），用 `merge_with_base` 用全局 registry 重算
  layout，然后 `Arc::get_mut` mutate 该 TypeDesc。
- 固定点循环（`while try_fixup_inheritance() > 0`）—— 一次 fix-up 可能
  让另一个 TypeDesc 变可解析（三级链 `A → B → C`：B 先 fix → C 后 fix）。

**`Arc::get_mut` 可行性**：lazy_loader 是当前 TypeDesc 的**唯一**强引
用持有者（type 还没被任何对象实例化）。`build_type_registry` 之后立刻
做 `module.type_registry_vec.clear()` 释放 by-id Vec 的 Arc 副本，
确保 fixup 时 strong_count = 1。

**Eager-loaded 类型的可见性**：merge 后的 main module（含 z42.core）
的 TypeDesc 通过 `VmContext::seed_lazy_loader_types(&final_module.type_registry)`
克隆到 LazyLoader 的 `type_registry`。这些 Arc strong_count = 2（main
+ lazy_loader）—— 它们不需要 mutate，因为 `build_type_registry` 已正
确解析了它们的 base（base 在同一 merged module 内）。

**`needs_fixup` 计算**：
```text
expected_field_count  = base.fields.len() + len(unique own_fields not in base)
expected_vtable_count = base.vtable.len() + len(distinct simple_names of
                        own_methods not already in base.vtable_index)
needs_fixup = td.fields.len() != expected || td.vtable.len() != expected_v
```

**幂等性**：fixup 运行两次产生相同结果。已正确的 TypeDesc（`needs_fixup`
返回 false）跳过；BLOCKED（strong_count > 1）也不算 newly_fixed → 收
敛到 fixed point。

**延后未解析**：base 一直不可解析时 → 不报错，TypeDesc 保持 own-only
状态。下次新 zpkg 加载触发的 fixup 会重试。VM 退出时若仍有 own-only
的子类，相关字段访问会写入越界 slot（潜在 UB）—— 当前依赖 build 系统
保证依赖完整性；未来可在 try_lookup_type 路径加 "all-deps-loaded" 断言。

**前置 spec**：[docs/spec/archive/2026-05-14-fix-cross-pkg-subclass-fields/](../../spec/archive/2026-05-14-fix-cross-pkg-subclass-fields/)
（首次触发：add-std-process 的 4 个 z42.io exception 类继承 z42.core
的 Std.Exception）。

---

## resolve_namespace / resolve_dependency 的分工

位置：`src/runtime/src/metadata/loader.rs`

| 函数 | 签名 | 语义 | 调用方 |
|------|------|------|--------|
| `resolve_namespace(ns, module_paths, libs_paths)` | `-> Result<Vec<PathBuf>>` | 返回**所有**声明该 namespace 的 zbc/zpkg 文件（不再 bail on ambiguous）| 编译期诊断 / main.rs 启动反查 (.zbc → zpkg) |
| `resolve_dependency(zpkg_file, libs_paths)` | `-> Result<Option<PathBuf>>` | 按 zpkg 文件名直接查找 | LazyLoader 内部（按文件名精确定位）|

**设计权衡（Decision 2）**：`resolve_namespace` 保留但改语义，而非彻底删除。
理由：编译器 / 诊断工具仍可能问"哪些 zpkg 提供某 namespace"，保留此 API
更通用。

---

## VCall 分发与 TypeDesc

### ObjNew dispatch（与 VCall 对称）

`Instruction::ObjNew { dst, class_name, ctor_name, args }`：

```
1. type_desc = module.type_registry[class_name] | lazy_loader.try_lookup_type(class_name)
            | make_fallback_type_desc(...)
2. allocate ScriptObject(type_desc, slots=fields.iter().map(|f| default_value_for(&f.type_tag)).collect())
3. ctor_fn = module.func_index[ctor_name] | lazy_loader.try_lookup_function(ctor_name)
4. if ctor_fn: exec_function(ctor_fn, [obj, ...args])
   else:       skip ctor call（默认无参 ctor 语义；TypeChecker 已确保
               有显式 ctor 时 ctor_name 命中）
5. frame[dst] = obj
```

**与 VCall 对齐的核心**：编译期完整 overload resolution，VM 直查 `ctor_name`，
不再做 `${class}.${simple}` 名字推断（2026-04-26 add-objnew-ctor-name 重构）。
`ctor_name` 含 `$N` arity suffix（重载场景）；单 ctor 时无 suffix。

**字段默认值（2026-05-02 fix-class-field-default-init）**：步骤 2 把
slot 初始化为对应类型的默认值（`int*`/`f64*` → 0、`bool` → false、`char` →
`'\0'`、`str` / 引用 → null），不再一律 `Null`。具体映射由
`metadata::default_value_for(type_tag) -> Value` 单一函数提供，interp 与
JIT (`jit_obj_new`) 共享实现。需要这一步的前提是 `FieldSlot` 携带
`type_tag: String`（从 zbc `FieldDesc.type_tag` 透传），见下文 TypeDesc 结构。

ctor 入口由编译器侧 IrGen 注入字段 init（base ctor call 之后、用户 body
之前）；无显式 ctor 但本类或本地祖先链有字段 init 的类，编译器合成无参
隐式 ctor 内联整条链的 init 表达式。详见
`docs/design/language/language-overview.md` §6.3 + `docs/spec/archive/2026-05-02-fix-class-field-default-init/`。

### TypeDesc 结构

```rust
struct TypeDesc {
    name: String,                          // FQ 类名，e.g. "Std.Collections.Stack"
    base_name: Option<String>,             // 父类 FQ 名
    fields: Vec<FieldSlot>,                // 布局顺序，基类字段在前
    //   FieldSlot { name: String, type_tag: String }
    //   type_tag 用于 ObjNew 选默认值（fix-class-field-default-init）
    field_index: NameIndex,                // 字段名 → slot（linear scan，见下）
    vtable: Vec<(method_name, func_name)>, // 方法名 → FQ func name
    vtable_index: NameIndex,               // method → vtable slot（同上）
    type_params: Vec<String>,              // 泛型类型参数（L3-G 起）
    type_args: Vec<String>,                // 实例化时具体类型（运行时填）
    type_param_constraints: ...,            // L3-G3a 约束元数据
}
```

TypeDesc 由 `build_type_registry`（loader.rs）在模块加载完成后按 topo
sort 预计算一次，避免运行时重复构建。

#### NameIndex：linear-scan 替代 HashMap

`field_index` / `vtable_index` 用 `NameIndex`（`Vec<(Box<str>, usize)>`）
存储，hot path 是 linear scan。**不是 HashMap**。

**为什么**：
- review.md C4 / C5 P1（2026-06-01）：z42 stdlib + 用户代码典型 class 字段
  / 方法数 ≤ 16。linear scan ≤16 项的 `Box<str>` ≡ `&str` 比
  `HashMap<String, usize>` 探测 + 字符串 compare **快**：cache locality 友好，
  无 hash 函数计算开销，分支预测对小循环友好。
- IC + PIC（add-jit-polymorphic-ic, 2026-05-28）已拦截大部分 hot path 命中。
  NameIndex 替换的是 **IC miss 时的 fallback 路径**，把"miss = hash + compare"
  改为"miss = linear scan + str compare"。命中 IC 时完全不走这里。
- 内存：`Box<str>` 比 `String` 省 8 B / entry（无 capacity 字段）。stdlib ~80
  classes × ~8 fields/methods = **几 KB heap 省**。

**何时该退化为 HashMap**：N ≥ 64 entries 时 linear scan 开始亏。当前 z42 没
有这种 class；若未来出现，把 `NameIndex` 内部实现改为 hybrid（N ≤ K linear，
N > K HashMap），调用方零改动 —— `NameIndex` 的 public API 故意按
`HashMap<String, usize>` 子集设计。

**API**：`get(&str) -> Option<&usize>` / `insert(String, usize) -> Option<usize>` /
`iter()` / `FromIterator<(String, usize)>` / `Clone`。位于
[`src/runtime/src/metadata/name_index.rs`](../../../src/runtime/src/metadata/name_index.rs)。

### VCall 指令执行

位置：`src/runtime/src/interp/exec_instr.rs:416`

```rust
let type_desc = obj.type_desc.clone();

// 1. 优先走预计算 vtable
let func_name = if let Some(&slot) = type_desc.vtable_index.get(method) {
    type_desc.vtable[slot].1.clone()
}
// 2. fallback：沿继承链查（基类可能没在 vtable 里）
else if let Ok(f) = resolve_virtual(module, &type_desc.name, method) {
    f.name.clone()
}
// 3. 最后兜底：直接 format!("{}.{}", type_desc.name, method)
else {
    format!("{}.{}", type_desc.name, method)
};

// 查 main module func_index → fallback lazy_loader
```

**关键不变量**：`type_desc.name` 必须是 FQ 名（否则 fallback 3 生成的
func_name 是 bare 名 → miss）。`build_type_registry` 从 `Module.classes[].name`
构建 TypeDesc.name，而 ClassDesc.name 由编译器写入 —— 编译期 `QualifyClassName`
是最终决定者。

---

## 闭包 dispatch（Closure / StackClosure / FuncRef）

`Value` 三种 callable variant 的 CallIndirect 路径：

```
match callee_value {
  FuncRef(name)               → 直接 call name；无 env
  Closure { env, fn_name }    → 把 env (GcRef) 作 implicit first arg → call fn_name
  StackClosure { env_idx, fn_name } → 从 caller frame.env_arena[env_idx] clone Vec<Value>
                                       → 升格为临时 GcRef → 作 implicit first arg → call fn_name
}
```

**StackClosure**（2026-05-02 impl-closure-l3-escape-stack）：env 在 caller frame 的 `env_arena: Vec<Vec<Value>>` 中，零堆分配。CallIndirect 时复制内容到临时 GcRef，callee 不区分 stack/heap 来源。

**GC root**：每个 `VmFrame` 内嵌 `regs` + `env_arena` 指针（unify-frame-chain），GC scanner 单循环遍历 `call_stack` 即可同时 mark frame regs 和 stack closure env 中的 Object/Array refs，确保不被回收。

**lifetime 安全**：StackClosure value 的有效性由分析器（`ClosureEscapeAnalyzer`）在编译期保证 —— closure value 永不离开创建它的 frame；CallIndirect 复制 env 内容也意味着 callee 不会持有指向 caller arena 的悬空指针。

## interp vs JIT 分发

位置：`src/runtime/src/vm.rs` + `interp/` + `jit/`

- **interp**：`exec_function` → `exec_instr` match 分发到 ~60 个 IR 指令
  实现。`Value` 是 `Rc<RefCell<ValueKind>>` 或 enum 变体
- **JIT**：Cranelift 后端编译每个 `Function` 成 native code。`ExecMode`
  注解决定 function-level 的分发路径；模块默认模式由 `Vm::default_mode`。
  **z42vm CLI 默认 = jit**（make-jit-default，2026-06-20；built `--features jit`
  时；jit-less 构建如 wasm 回落 interp）。`--mode interp|jit` 显式覆盖。
  注意：嵌入式 FFI 路径（test-runner 等，`host/ops.rs`）的默认模式由 embedder
  config（`Z42ExecMode`）决定，**不受 CLI 默认影响**——测试仍走 interp。
- **AOT**：留白（interp 全绿前不填实现）

两种后端共享：
- `Module` / `Function` / `Instruction` 数据结构
- `TypeDesc` / `FieldSlot` / vtable
- corelib builtin 调度表（`corelib/dispatch.rs`）
- LazyLoader（但 JIT 走 eager 预加载路径，基本不触发 lazy load）

---

## JIT↔VM 元数据契约（review.md Part 1 P0 / E1.P2 Phase 1, 2026-06-02）

位置：[`src/runtime/src/jit/vm_interface.rs`](../../../src/runtime/src/jit/vm_interface.rs)

`pub trait JitVm` 是 JIT 后端对 metadata 模块的**只读契约** ——
codifies "JIT 编译期需要从 module 拿到什么"，与 helper 运行期通过
`*const Module` raw pointer 拿到的具体字段访问分离。Phase 1 surface 是 4
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

### 为什么 Phase 1 只是契约 codification（review.md 原描述 vs 现实）

review.md Part 1 P0 原稿说"translate.rs 只 take `&dyn VmInterface`，不
import metadata 内部类型；helpers/* 同样改成走 trait"。两点结构性约束让
**完整愿景**不可一步到位：

1. **translate.rs 必须看见 `Instruction` enum**：100+ arm 模式匹配是 JIT 的
   输入语言形态。把 IR 藏在 trait 后等于丢弃 visitor pattern 的编译期穷举性
2. **helpers 通过 `*const Module` raw pointer 访问 Module**（extern "C" ABI
   约束）：raw pointer 必须指向具体 sized 类型。`*const dyn JitVm` 是 fat
   pointer，会破坏 Cranelift 生成代码对 helpers 的 ABI 调用

Phase 1 现实路径：
- **define + impl on Module**：codify "JIT compile-time 需要哪些 module 读"
- **`compile_module` 改走 trait method**：`module.functions()` /
  `module.string_pool()` 等替代直接字段访问
- **一个 helper exemplar**（`jit_obj_new`）：示范 helpers 内部如何调 trait
  method（`module.type_lookup(name)` 替代 `module.type_registry.get(name)`），
  raw pointer 不变
- **signature 不变**：`compile_module(&Module)` 保持原签名；`JitModuleCtx.module:
  *const Module` 仍是具体类型

### Phase 2+ 路径（独立 spec）

- 把余下 9 个 helper 全迁到 trait method
- 探索 `compile_module<M: JitVm + ?Sized>(module: &M)` generic 化（前提：解决
  helper 端的 raw pointer ABI 问题，可能需要 type-erased dispatch table）
- 远期 AOT 后端通过另一个 `JitVm` impl 接入相同 contract

### Mockability

Phase 1 的具体 ROI：单元测试可以构造一个 minimal `MockMetadata` struct
（只实现 trait 关心的 4 个方法）而不必拼装整个 `Module`。
`vm_interface_tests.rs` 包含一个 `MockMetadata` 示例，覆盖
`&dyn JitVm` dyn-dispatch 调用。

---

## JIT/EE helper 边界（2026-05-07 formalize-jit-vm-interface）

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
├── object.rs     — ObjNew / Field* / IsInstance / AsCast / Static* / default_of
├── vcall.rs      — 虚调用（独立文件，含 primitive-as-struct + 懒加载 fallback）
└── closure.rs    — load_fn / load_fn_cached / mk_clos / call_indirect
```

**与 `interp/exec_*.rs` 命名对称**: 加新 IR 指令时，interp 与 JIT 改的文件名一一对应（`exec_value.rs` ↔ `helpers/value.rs`），认知负担最小。

**消费者**: 
- `jit/mod.rs::compile_module` 调 `helpers::register_symbols(&mut builder)` 一次
- `jit/translate.rs` 通过 `pub use super::helpers::HelperIds;` 重导，并消费 `helpers::declare_imports(&mut jit)` 拿 `HelperIds`

**新增 helper 改 2 处**（不再是 3 处）:
1. `helpers/<category>.rs` 添加 `pub unsafe extern "C" fn jit_xxx(...)` 函数体
2. `helpers/registry.rs` 添加:
   - `HelperIds.xxx: FuncId` 字段
   - `register_symbols` 中 `reg!("jit_xxx", category::jit_xxx);` 行
   - `declare_imports` 返回的 `HelperIds {...}` 中 `xxx: decl!("jit_xxx", [params...], [returns...]);` 行

`mod.rs` 与 `translate.rs` 都不再需要知道 helper 列表。

**`VM_JIT_INTERFACE_VERSION: u32 = 1` 作为 hook**: 当前没有运行时校验消费者——单一 JITModule 实现，bump 这个常量当 helper 集合或签名变化时即可。未来若引入第二个 JIT 后端（LLVM / wasm）或多版本 tier-up，启动时校验该值与编译期版本是否兼容，避免 cross-version helper 错配。属于 review.md Part 4 §4.2 提出的"边界形式化"目标，**不引入校验代码避免过度设计**，留 hook 即可。

**为什么不引入 trait**: trait object dispatch 会带来间接调用开销，违背 zero-cost helper 设计；且 helper 已经走 cranelift symbol 解析机制（按名解析），用 trait 反而绕路。保留 `extern "C"` 直调零开销，仅在边界**形态**上做形式化（HelperIds + registry），不动**调用机制**。

**与 CoreCLR `ICorJitInfo` 对照**: CoreCLR 的 JIT/EE 边界是 callback-based vtable（`ICorJitInfo` 含 ~100 callback），versioned via GUID。z42 当前 ~50 helper、单实现、symbol-by-name 解析——比 ICorJitInfo 简洁但同方向（"单一边界 + 版本号"）。等需要支持多 JIT 后端时再考虑升级到 vtable 形态。

---

## JIT type specialization（2026-05-28 jit-type-specialization C2 P1）

位置：`src/runtime/src/jit/translate.rs` 的 `emit_i64_binop` / `emit_i64_cmp` /
`emit_bool_binop` / `emit_bool_not` / `emit_const_*` + `is_*_typed` 谓词。
依赖：`Function.reg_types: Box<[IrType]>`（来自 zbc REGT section）+
`Value` enum 锁布局（`#[repr(C, u8)]`，24 B，pinned by
`metadata::types_tests::value_*_payload_at_offset_8`）。

**问题**: JIT-compiled 代码每条算术 / 比较 / 逻辑 op 都 call 一个
`extern "C"` helper（`jit_add` / `jit_eq` / ...）。helper 内部走
`match (Value::I64(x), Value::I64(y)) => ...` 模式匹配 + 回退 clone
+ 类型错误异常。在已知静态类型的热路径上这层 dispatch 是纯开销。

**策略**: zbc 携带每寄存器的静态 `IrType`（C2 P0：新增 REGT section，
zbc 1.7→1.8 / zpkg 0.8→0.9）。translate.rs 在 emit 每条 op 前查
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
        │   idx * 24 + 8   │  │  path; helper does │
        │ native op        │  │  match + clone +   │
        │ raw store TAG +  │  │  exception)        │
        │   payload        │  │                    │
        └──────────────────┘  └────────────────────┘
```

**Raw memory access invariants**:

- `frame.regs` 是 `Vec<Value>`；data pointer 在 JitFrame 构造时分配
  并稳定到函数结束（`take_pooled_regs(max_reg + 1)` 不会再 grow）。
  `jit_regs_ptr(frame)` helper 在 translate 入口调一次，缓存 SSA
  `regs_base`。
- Slot 地址 `= regs_base + idx * 24`（VALUE_STRIDE = `size_of::<Value>()`，
  pinned by `value_size_observed` test）。
- 写时只写 1 B discriminant + 8 B payload；不调 `drop` 因为前任 slot
  值在 `reg_types[dst]` 静态类型下必是 Null（首次写）或同类型 primitive
  （I64 / Bool / F64 都无 Drop）。
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

**性能验证**: `bench/scenarios/04_c2_p1_arith_loop.z42` 跑 10M-iter
SumSquares loop（每 iter 一次 mul + 两次 add + 一次 lt + 一次 brif），
M-series macOS 5-run 平均：

| 阶段 | 时间 | 相对 baseline |
|------|------|--------------|
| pre-P1 (all helpers) | ~456 ms | 1.00× |
| arith + cmp 内联 (`98426e40` / `fc3936f0`) | ~392 ms | 1.16× |
| + BrCond i8 load (`3727e469`) | ~302 ms | **1.51×** |

低于 spec 2× 上限的主要原因：`jit_check_safepoint` 仍是 helper call
（在每个 backward branch 上 GC cooperation）。Inline 该 atomic fast
path 是后续 spec 的事，~1-2 day 工作量推到 ~1.8×。

**与 CoreCLR / Java JIT 对照**: 这是 monomorphic specialization 的
基础形态——所有跨类型 op 都退到 helper，所有静态已知类型 op 都 inline。
CoreCLR 在此之上还有：tier-1 type-feedback 收集 + tier-2 多型/单型
IC + 内联 String.Concat + escape analysis 删 box——都是 z42 当前规模
不需要的。我们的 reg_types 已经是"全局 monomorphic"，覆盖率约 80-
90%（per spot-check on stdlib zbc），剩余在 generic / `default(T)` /
变量重命名等场景。

**Out of band**: `Value::I64` 内联 fast path 仍保留在 `jit_add` 等
helper 内（C2 P2 task 2.1 评估后决定不删）——因为 helper 仍被 ~10%
mixed-type sites 调用，删除会让那些 sites 慢一档。

---

## Method token system（2026-05-08 introduce-method-token Phase 1）

位置：`src/runtime/src/metadata/tokens.rs` + `metadata/resolver.rs` + `vm_context.rs::resolve_static_field_id` + 各 `interp/exec_*.rs` 热路径

**问题**: review.md Part 4 §4.1 痛点 — z42 IR 所有跨引用 dispatch 用 `String` + `HashMap.get()` 做身份。每次虚调用一次 hash + 字符串等价比较；IR 内存表示膨胀；反射 R-series 设计无 token 锚点。

**设计**: 加载期解析所有可解析的 string 引用为 newtype token (`MethodId(u32)` / `TypeId(u32)` / `BuiltinId(u32)` / `FieldId(u32)` / `StaticFieldId(u32)` / `VTableSlot(u32)`)，存到外置 `Function.resolved: OnceLock<ResolvedTokens>`，热路径直接 Vec/Array 索引。无需改 IR `Instruction` struct 字段类型（zbc 格式不动；compiler 端不变）。

### 数据结构

```rust
// metadata/tokens.rs — 6 个 newtype + UNRESOLVED sentinel
pub const UNRESOLVED: u32 = u32::MAX;
pub struct MethodId(pub u32);     // → Module.functions[id]
pub struct TypeId(pub u32);       // → Module.classes order
pub struct BuiltinId(pub u32);    // → BUILTINS[id] 全局静态表
pub struct FieldId(pub u32);      // → TypeDesc.fields[id]
pub struct StaticFieldId(pub u32);// → VmContext.static_fields[id]
pub struct VTableSlot(pub u32);   // → TypeDesc.vtable[id]

// metadata/resolver.rs — Function.resolved 内容
pub struct ResolvedTokens {
    pub method_tokens:        Vec<AtomicU32>,   // Call 站点（cross-zpkg 留 UNRESOLVED）
    pub builtin_tokens:       Vec<u32>,         // Builtin 站点（100% 命中）
    pub type_tokens:          Vec<AtomicU32>,   // ObjNew 站点
    pub vcall_ic:             Vec<VCallIC>,     // VCall 单态 IC
    pub field_ic:             Vec<FieldIC>,     // FieldGet/Set 单态 IC
    pub static_field_tokens:  Vec<AtomicU32>,   // StaticGet/Set 站点
    pub site_index:           Vec<Vec<u32>>,    // (block, instr) → per-kind site_idx
}
```

### 解析时序

1. **`metadata::loader::build_type_registry`**: 在 topo order 中给每个 `TypeDesc.id` 分配（0..N）
2. **`vm.rs::Vm::run`**: 调 `resolver::resolve_module(&module, ctx)`：
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

- **Call**: 命中 → `module.functions[cached]`；UNRESOLVED → `func_index` 查找 + 写回 cache（cross-zpkg 单点回填）
- **Builtin**: 直接 `BUILTINS[id]`（无 fallback；100% 命中）
- **ObjNew**: 仍走 `type_registry`（HashMap by name）；TypeId cache 用作 cross-zpkg observability
- **StaticGet/Set**: 命中 → `static_fields[id]`；UNRESOLVED → name lookup + 回填
- **VCall**: PIC 命中（4-slot 线性扫描；`recv.type_desc.id` 匹配任一槽位的 `type_id`）→ 直调 `module.functions[entry.fn_idx]`；miss → 走原 4 段 dispatch + 在 vtable_index hit 时通过 `vcall_ic_install` 填入第一个空槽（或 round-robin 牺牲一个槽）
- **FieldGet/Set**: PIC 命中（4-slot 线性扫描）→ 直读/写 `obj.slots[entry.slot]`；miss → `field_index` 查 + 通过 `field_ic_install` 填槽

> **C4 P2 + C5 P2 (jit-polymorphic-ic, 2026-05-28)**：原单态 IC（一对 `(type_id, slot, fn_idx)`）升级为 4-slot polymorphic IC。线性扫描使用 `UNRESOLVED` sentinel 提前退出（mono 站点首槽命中即返回，0 额外开销）。超过 4 个 receiver type 的站点用 round-robin counter 牺牲槽位（`ic.round_robin.fetch_add(1, Relaxed) % 4`）。所有 atomic 操作均为 `Relaxed` —— `type_id` 守门 payload，torn-read 等价于"刚好遇到迁移中的同型 dispatch"，下一次会收敛到稳定态。Helpers `field_ic_lookup` / `field_ic_install` / `vcall_ic_lookup` / `vcall_ic_install` 在 `metadata::resolver` 公开，供 interp + JIT helpers 共用。
>
> **extract-typedesc-from-mutex (2026-05-31)**：PIC scan 读 receiver `type_id` 不再走 Mutex lock。`GcRef<ScriptObject>::type_desc()` 通过 `parking_lot::Mutex::data_ptr()` 直接读 type_desc（write-once-at-alloc invariant 锁定 safety），跳过 ~5–10 ns 的 atomic CAS。详见 `docs/design/runtime/gc.md` "extract-typedesc-from-mutex" 节。这一步是 PIC inline 入 Cranelift IR（Phase 4 future）的前置条件 —— 之前 PIC 因为要 lock 不能 inline。

### 跨 zpkg 时序

- **Intra-module**：`build_type_registry` + `resolve_module` 都在同一 module 加载完成后跑，所有 intra-module ID 立即可用。
- **Cross-zpkg lazy load**：lazy_loader 触发的 zpkg 加载后，对方模块的 `func_index` / `type_registry` 才填充。caller 模块的 `Function.resolved` 中的 cross-zpkg 引用初始为 `UNRESOLVED`；首次 dispatch 通过 string lookup 命中后**写回 cache**，单点回填。
- **StaticFieldId 全局 lazy 增量**：`VmContext::resolve_static_field_id(name)` idempotent — 任何模块在加载期或 dispatch 期遇到新的 static field name，立即分配新 id 并 resize Vec。已分配的 id 不变（resolver-populated cache 跨 module reload 仍有效）。

### Phase 化路线

| Phase | 内容 | 状态 |
|---|---|---|
| 1 | Token newtypes + 加载期 resolver + interp 8 热路径 + StaticField 全局编号 + Field/VCall 单态 IC | 🟢 2026-05-08 |
| 2 (sibling) | JIT helpers 接 MethodId/BuiltinId/StaticFieldId + VCall/Field IC（Option A — helper carries IC） | 🟢 2026-05-08 |
| 3 | zbc 1.0 wire format（IR 字段 token 化：本地 = `module.Functions/Classes` 索引；cross-zpkg = `IMPORT_BASE + STRS idx`）+ type_registry Vec-by-TypeId（吸收 Phase 2.D） | 🟢 2026-05-09 |
| 4 (future) | Compiler 端 token-aware emit perf；Phase 2.E Option B（JIT 机器码 inline IC check）作 perf 工作 | 📋 待启动 |

> **Phase 2 边界**: Phase 2 已把 JIT helper 切到 token / IC 形态，但 dispatch
> 仍走 helper-call 一次（Option A）。机器码 inline IC check（Option B，跳过
> helper 调用本身）需要 cranelift 端的复杂 control-flow，留给 Phase 4 perf
> 阶段。Option A 已让 JIT 的 hot path 与 interp 行为完全对齐：IC hit 时直接
> 通过 `JitModuleCtx.fn_entries_by_id[cached_fn_idx]` 跳到目标 native 代码，
> 无 HashMap 哈希、无 vtable 解析。
>
> **Phase 2.D 推迟**: ObjNew TypeId 在 Phase 2 阶段是纯 observability（type_registry
> 仍是 HashMap-by-name，dispatch 路径没有热路径键可换）；与其单独写一遍 helper
> 签名变更，不如等 Phase 3 把 type_registry 改 Vec-by-TypeId 时一起切。
>
> **Phase 3 实施期决策（2026-05-09 redesign）**：原 Phase 3 设计要求 IMPT 区段
> 扩展 + sort coordination + Rust Instruction enum 字段改 newtype。实施期发现
> 三件复杂度叠加导致无法 GREEN（commit `833193a` 在 `wip/phase3-s3-broken` 分支
> 保留作为反例）。重设计取消上述三项，改用：(1) STRS 池复用 + IMPORT_BASE bit
> 编码 cross-zpkg 引用（不需新 IMPT 格式）；(2) 源序分配 token id（不需 sort
> coordination）；(3) IR enum 字段保持 `String`，token 化只在 wire 边界发生。
> 三步骤 (S3a/b/c) 每步独立 GREEN：S3a 让 Rust reader 双版本支持，S3b 切换
> 编译器 + stdlib regen，S3c 清理 v0.x fallback。

### 与 D-1b `func_ref_cache_slots` 的关系

`func_ref_cache_slots`（D-1b add-method-group-conversion）是 method group 转换的 module-level 缓存，与本 spec 的 `method_tokens` 是两套独立的运行时 cache。Phase 2（JIT 端）若做 method-token 整合时，可考虑统一到一套 token 系统。

---

## corelib builtin dispatch

位置：`src/runtime/src/corelib/`

`Builtin` IR 指令在 interp / JIT 都走 `corelib::exec_builtin(name, args)` 统一入口。
builtin 按功能分 submodule：`string.rs` / `io.rs` / `math.rs` / `collections.rs` 等。

**新增 builtin 三处同改（强制规则）**：
1. `corelib/<module>.rs` — 实现
2. `corelib/dispatch.rs` — match 分发
3. `src/compiler/z42.Compiler/TypeCheck/BuiltinTable.cs` — 类型签名

---

## 关键设计权衡

### 为什么 Value 用 Rc<RefCell>

- **GC 模型**：z42 是 GC 语言；`Rc<RefCell>` 给每个对象引用计数 + 运行时
  borrow check
- **已知限制**：`Rc` 是 `!Send`，阻塞跨线程传值。L3 async / 线程模型时会
  重新设计（L2 backlog A6）

### 为什么懒加载现在归 VmContext 持有（2026-04-28 consolidate-vm-state）

**之前**：`lazy_loader::STATE` thread_local + `install` / `uninstall` /
`try_lookup_*` 自由函数集合。简单但带来的问题：

- 同一进程跑多个 VM 实例时 STATE 互串（多 VM-per-process 卖点失效）
- 测试间状态污染靠手动 `uninstall`，遗漏即漂移
- 与 README 第 7 行 "Multi-threaded: GC-safe concurrency" 设计目标矛盾

**现在**：`VmContext.lazy_loader: RefCell<Option<LazyLoader>>` 持有，
`ctx.install_lazy_loader_with_deps(...)` / `ctx.try_lookup_function(name)` /
`ctx.declared_namespaces()` 等方法委托到 `LazyLoader` struct（保留逻辑不变）。
两个 ctx 实例的 lazy_loader 完全隔离，跨 ctx 切换不污染。

同时迁移到 VmContext 的还有：

| 状态 | 之前 | 现在 |
|------|------|------|
| 用户类静态字段 (interp) | `interp/dispatch.rs::STATIC_FIELDS` thread_local | `ctx.static_fields` |
| Pending exception (interp) | `interp/mod.rs::PENDING_EXCEPTION` thread_local + `UserException` sentinel + `user_throw`/`user_exception_take` | 删除 — interp 全程走 `ExecOutcome::Thrown(Value)` |
| Pending exception (JIT) | `jit/helpers.rs::PENDING_EXCEPTION` thread_local | thread_local 保留（extern "C" ABI）；`JitModule::run` 在边界与 `ctx.pending_exception` sync |
| 静态字段 (JIT) | `jit/helpers.rs::STATIC_FIELDS` thread_local | thread_local 保留；同样 sync 机制 |

> 2026-04-28 update（extend-jit-helper-abi）：JIT 端的 2 个 thread_local
> 已删除。所有 ~37 个 extern "C" helper 签名都加了 `ctx: *const JitModuleCtx`
> 第 2 参（含 macro 实例化的批量 helper），Cranelift translate.rs 同步在
> 调用点插入 `ctx_val`。helper 内部通过 `vm_ctx_ref(ctx)` → `(*ctx).vm_ctx`
> 两层间接拿到 `VmContext`，不再有任何同步桥接代码。
>
> Runtime 内仅余 `jit/frame.rs::FRAME_POOL`（pure allocator cache，每线程
> 独立池子合理）。`VmContext` 是所有 runtime-mutable 状态的唯一规范来源。

### 为什么不预加载所有 stdlib

User 裁决（2026-04-25）：保留懒加载，按需加载。原因：
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

- 📄 [`docs/design/runtime/gc.md`](gc.md)

简要状态（更新自 2026-05-22 add-generational-gc P4 归档）：

- **核心 trait**：[`crate::gc::MagrGC`](../../../src/runtime/src/gc/heap.rs) ——
  对齐 [MMTk](https://www.mmtk.io/) `VMBinding` porting contract
- **Backing**：[`Region<T>`](../../../src/runtime/src/gc/region.rs) chunked
  allocator + `NonNull<RegionEntry>` 12B handle（add-custom-allocator）
- **三种 mode 可选**（`GcMode` enum + `Z42_GC_MODE` env var）:
  - `StwMarkSweep` (default) — stop-the-world mark + sweep
  - `ConcurrentMarkSweep` (opt-in) — STW root snapshot → 后台并发 mark →
    短 STW handshake → STW sweep (add-concurrent-gc)
  - `GenerationalMarkSweep` (opt-in) — minor GC 扫 young + dirty cards
    (~4× faster minor pause vs full STW)；major GC 全堆扫描 (add-generational-gc)
- **Write barriers**：interp + JIT 5 个 FieldSet / ArraySet 写入点全 wired；
  call-site 通过 `Value::is_heap_ref()` 过滤 primitive；trait override 由
  各 mode 实现（concurrent: tricolor shading；generational: cross-gen card marking）
- **Finalizer**：sweep 时触发 + `Std.GC.Finalize(x)` 显式 API
  (add-custom-allocator P2)
- **Safepoint**：counter-throttled fast path + multi-collector arbitration
  + interp + JIT 全 instrumented

后续可选迭代（A1 / A2 / A3 / A4 已落地；剩余 backlog 见
[gc.md "GC 后续迭代规划"](gc.md#gc-后续迭代规划)）。


## 延伸阅读

- `docs/design/runtime/gc.md` — GC 子系统完整设计（接口、phases、modes、benchmarks、迭代规划）
- `docs/design/runtime/ir.md` — IR 指令集、zbc 二进制格式
- `docs/design/runtime/jit.md` — Cranelift JIT 后端设计
- `docs/design/runtime/execution-model.md` — ExecMode 注解、interp/JIT/AOT 切换语义
- `docs/design/stdlib/overview.md` — stdlib 三层架构（intrinsics / HAL / script BCL）
- `.claude/rules/runtime-rust.md` — Rust VM 开发规范（错误处理、测试组织、Value 类型约定）
