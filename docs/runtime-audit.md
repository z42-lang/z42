# z42 运行时改进计划（临时迭代底稿）

> **临时底稿**：运行时改进迭代的工作文档，不属于三本书，**只记还要做的事**。已完成的阶段 0 / 阶段 1（身份与调用、对象模型 R0–R9、GC 与内存 M1–M14、对标差距 T1–T5、执行层修补 X-1–X-5 等）的记录已删除，机制知识在 internals / reference；删减前的全文见 git `b84441618:docs/runtime-audit.md`。迭代全部完成后删除本文件（同时移除 `docs/README.md` 里的入口链接），届时仍未完成的项转入 `docs/internals/` 对应机制页的「待办 / Deferred」。
> 范围：`src/runtime`。证据级别：**【实测】** profile / trace / 计时（方法见附录）；**【确认】** 读代码确认；**【推断】** 有代码依据但需测量。文中行号与数字对应测量时的版本，随修改漂移。

---

## 0. 现状

- **启动**：hello 9.6 ms / 13 MB，快于 CPython（20 ms）与 .NET JIT（24 ms）；与 NativeAOT（3.3 ms / 3.8 MB）还差约 6 ms / 9 MB，主要在 z42.core 加载与库目录扫描（§1、§6.4）。
- **内存**：对象 / 数组 / 字符串 / `List<int>` / `Dictionary` 的密度都已低于 CPython，小对象与 .NET 持平（§1）。
- **吞吐**：比 CPython 快（除 json、dict_ops、字符串拼接）；比 .NET 慢 5–16 倍，差距集中在调用协议、不内联、分配路径（§6.2）。
- **结构**：目标目录与模块分层已定（§4），按 §6.1 迁移；L-1 分层门的基线即当前欠账。

---

## 1. 基线数据（2026-10-28）

10-25 nightly z42vm（release，jit + mimalloc），M2 Ultra，load 3–6。对照 Python 3.9.6、.NET 10.0.102（Release apphost，workstation GC）、Node 26。24 条对标负载（`bench/cross-language-compare` 分支）在当前编译器下原样编译通过、checksum 全对；每格 3 次中位，hello 15 次。

**吞吐**（墙钟 ms，含启动；比值 > 1 表示 z42 更慢）：

| 负载 | z42 jit | z42 interp | Python | .NET | z42 / Python | z42 / .NET |
|---|---|---|---|---|---|---|
| fib 32 | 202 | 384 | 432 | 40 | 0.47 | 5.1 |
| nbody 500k | 309 | 1735 | 3847 | 59 | 0.08 | 5.3 |
| vcall_mono 20M | 434 | 988 | 1537 | 39 | 0.28 | **11.2** |
| vcall_poly 20M | 597 | 1592 | 2392 | 81 | 0.25 | 7.4 |
| closures 2M | 241 | 350 | 384 | 52 | 0.63 | 4.7 |
| list_ops 2M | 921 | — | 1897 | 56 | 0.49 | **16.3** |
| dict_ops 1M | 530 | 1262 | 304 | 63 | **1.74** | 8.5 |
| str_builder 1M | 371 | 1173 | 264 | 58 | **1.41** | 6.4 |
| str_split_join 20k | 203 | — | 140 | 120 | **1.45** | 1.7 |
| json 10k | 251 | 513 | 61 | 95 | **4.15** | 2.6 |
| sort 1M | 130 | 281 | 478 | 94 | 0.27 | 1.4 |
| binary_trees 16 | 1550（STW 1567） | 2630 | 4671 | 221 | 0.33 | 7.0 |
| large_heap 50k | 836（STW 668） | 1275 | 2312 | 836 | 0.36 | 1.0 |

比 CPython 还慢的只剩 json、dict_ops、str_builder、str_split_join；对 .NET 的差距集中在调用与内联（list_ops 的 `List<int>` Add / 索引约 40 ns 一次、单态虚调用约 22 ns 一次）。

**密度**（B/个，(RSS(N)−RSS(0))/N）：小对象 32.3（.NET 30.6、Python 81.4）；`long[8]` 分代 117.3 / STW 80.7（.NET 91.1）；短字符串 58.4（.NET 74.0、Python 73.1）；`List<int>` 每元素 13.3（.NET 10.6、Python 41.4）；`Dictionary<string,int>` 每条 124.5（.NET 135.7、Python 181.3）。2026-10-07 时全部高于 Python，现在全部低于 Python、小对象与 .NET 持平。待查：`long[8]` 分代比 STW 每个多约 37 B。

**GC**（`Z42_GC_TRACE=1`）：binary_trees 分代停顿合计 226 ms（GC 约占墙钟 15%，大头在分配与调用）；large_heap 的 STW 比分代墙钟快 20%、RSS 低 20%，但最大停顿是分代的 3 倍（26.5 vs 8.6 ms）。

**启动**（hello，hyperfine 30 次均值）：z42 jit 9.6 ms / 13.3 MB；CPython 3.9 20.5 ms / 7.4 MB；.NET 10 JIT 23.9 ms / 35.7 MB、NativeAOT 3.3 ms / 3.8 MB；Node 31.3 ms / 42 MB。z42 不加载 z42.core 的空程序 3.0 ms / 5.3 MB——hello 的约 6 ms、7 MB 花在 z42.core 加载与库目录扫描上：

| 相位 | 耗时 | 说明 |
|---|---|---|
| exec + dyld + Rust 运行时 + clap + 配置 / tracing | 2.3 ms | 配置与 tracing 约 0.1 ms，dyld 可忽略 |
| VmContext + GC 堆初始化 | ≈0.7 ms | 启动期不起线程 |
| `load_artifact(z42.core)` | 3.6–4.2 ms | 10.1 万次分配；261 KB 文件解码后驻留 4 MB。全部 1033 个函数的指令解码约 39%、`assemble_zpkg_artifact`（合并内层模块、type registry、block indices）约 40%、.zsym 约 7%、STRS 重复解码约 12% |
| 库目录 NSPC 扫描（19 个 zpkg） | 1.0–1.5 ms | hello 用到的命名空间 z42.core 已提供，扫描白做 |
| 合并入口模块后全量重建 | ≈1.1 ms | `build_type_registry` 第二遍 0.7 ms |
| `resolve_module` 全量预填 token | 0.27 ms | |
| JIT 安装 + 执行前编译 Main | ≈0.5 ms | `z42c --help` 编 Main 2.1 ms |
| 输出 + 退出析构 | 0.3–0.4 ms | |

`z42c --help` 25.5 ms / 26.7 MB：为打印帮助惰性加载了 5 个包，GC 堆只用 1 KB，RSS 几乎全是元数据。改进项见 §6.2–6.4。

---

## 2. z42vm 二进制构成

z42vm `__text` 构成（macOS arm64，共 4.97 MB），是模块分层与可选依赖的依据：

| 组成 | 体积 | 占比 |
|---|---|---|
| z42 VM 本体 | 1.46 MB | 29% |
| Cranelift + regalloc2 | 1.35 MB | 27%（Linux x64 上 3.82 MB / 47%，其中 `cranelift_assembler_x64` 1.88 MB） |
| rustls + ring + webpki | 0.57 MB + 约 0.2 MB 表 | 无条件依赖，移动端 staticlib 也带着 |
| regex-syntax / automata | 0.29 MB | 只因 tracing-subscriber 开了 `env-filter` |
| clap | 0.20 MB | |
| toml | 0.11 MB | |
| std | 0.53 MB | |

---

## 4. 目标目录与模块分层（User 2026-10-29 定案）

> 10-06 的草案（format / ir / object / link / dispatch / objops / heap / engines / stdlib-* / z42vm-cli）已被否决，本节是经逐项讨论后的定案。迁移步骤见 §6.1。对照的实现：CoreCLR（`vm/` `gc/` `jit/` `md/` `pal/`，GC 经 `IGCHeap` / `IGCToCLR`、JIT 经 `ICorJitInfo` 与 VM 互通）、HotSpot（`classfile/` `oops/` `gc/shared`、平台代码集中在 `os/`）、CPython（`Objects/` `Python/` `Modules/`，PEP 669 `sys.monitoring`，PEP 684 每解释器状态）、wasmtime（`environ` 纯数据层、`Compiler` trait 接入 Cranelift、WASI 独立 crate）、Rust std（`std::sys` 按平台实现 + 上层公开 API）。

### 4.1 模块总览

VM 是一个 crate（`src/runtime/src`，包名 `z42`），**暂不拆成多个 crate**（User 2026-10-29）：下表每一层都是它的顶层模块，目录就是 `src/runtime/src/<模块>/`（`pal/`、`metadata/`、`object/`、`gc/`、`vm/` ……），名字不带 `z42-` 前缀；层间依赖靠 xtask 分层门禁守住，不靠 crate 边界。`crates/` 下已有的独立 crate（`z42-abi` / `z42-host` / `z42-rs` / `z42-macros` / `z42-compression` / `z42-repl`）不动。

| 层 | 模块 | 回答的问题 | 依赖 |
|---|---|---|---|
| 地基 | `z42-abi`（已有 crate） | no_std C ABI 镜像 | — |
| 地基 | `pal` | 怎么跟 OS 打交道（只用 std 类型，不认识 z42 类型） | std / libc |
| 地基 | `metadata` | 包文件里静态写着什么（只读、无副作用） | — |
| 地基 | `object` | 值和对象在内存里长什么样：布局与**不带检查的原语** | metadata |
| 地基 | `gc` | 内存从哪来、何时回收、怎么回收 | object、pal |
| 核心 | `vm` | 一台 VM 怎么加载、组装运行时元数据、多线程按统一协议执行 | gc、object、metadata、pal |
| 引擎 | `interp`、`jit`（将来拆出 `codegen` + `aot`） | 指令怎么执行 | vm |
| 能力 | `corelib` | 标准库里必须用 Rust 实现的 builtin（按 feature 分组） | vm、pal |
| 能力 | `interop` | 外部 C 库互操作与扩展库加载 | vm、pal、abi |
| 工具 | `debugger` | 断点、单步、查看（调试引擎 + DAP 服务） | vm |
| 工具 | `profiler` | CPU / 分配 / 锁 / 堆剖析与覆盖率 | vm |
| 组装 | 门面（crate 根：`lib.rs`、`config/`、`app.rs`） | 配置装配，按 feature 组装 Runtime | 以上全部 |
| 产品 | `z42-host`（已有 crate，吸收 `src/host` C ABI 薄壳）、`z42vm`（可执行文件，`main.rs`） | 嵌入 / 命令行 | 门面 |

硬规则（xtask 分层门禁检查）：

1. **平台 `#[cfg(target_os/unix/windows)]` 与 `libc` 只出现在 `pal/sys/`**；wasm 的功能裁剪用 cargo feature 表达。
2. **同层互不依赖**：interp ↔ jit、corelib / interop / debugger / profiler 之间及它们与引擎之间都没有依赖，一律经 vm 的注册接口接入，由门面组装。
3. **vm 不知道有哪些引擎、builtin、工具**：只提供注册槽（Engine、builtin 注册表、监控工具槽、extern 提供方）。
4. **命名**：「native」只指面向用户的原生扩展（SDK `native/`、z42-rs）；VM 内建函数叫 builtin，调外部 C 库叫 interop。VM 互操作模块使用 `interop/`；interp 的 `exec_ffi.rs::ffi_call` 执行 `CallNative` 指令，jit 的 `invoke.rs::enter_compiled` 进入已编译代码。

### 4.2 地基模块内部

- **`pal/`**：`fs.rs`（含 `Vfs` trait，每台 VM 一个句柄，文件系统状态归属 VmCore）+ `vfs/{local,memory}.rs`、`process.rs`（spawn / wait / kill / 进程树）、`net.rs`、`mem.rs`（GC 页管理）、`thread.rs`（栈边界、sigaltstack）、`signal.rs`、`dynlib.rs`（运行时唯一的动态库装载入口）、`console.rs`、`time.rs`、`system.rs`；上层 concern 保留平台无关接口，操作系统实现与后端选择集中在 `sys/`。`sys/mod.rs` 在编译期选择 `unix/`、`windows/`、`wasm/` 或 `fallback/`，`common/` 复用共享实现与降级行为。
- **`metadata/`**：`format/`（常量、版本、build_id）、`reader/`（二进制解码）、`ir/`（解码结构与寄存器类型）、`image.rs`（PackageImage：零拷贝、字符串表解析一次、函数体按需解码）、`index/`（包内按名找下标、知名类型名、类型名格式化与型参标记）、`debug/`（行号表、局部变量、帧名，读 .zsym 按需）。**纯静态的查询（名字、行号、attribute、签名）调用方直接用本模块，不经 vm。**
- **`object/`**：`value/`（Value、ref word、codec、cells）、`header/`（16 B 统一头、`var_ref` / `obj_ref` / `arr_ref` 句柄）、`types/`（TypeDesc 形状、字段、elem kind、TypeArgs 形状、布局计算纯函数、引用槽位图）、`object/`、`array/`、`string/`（Str 句柄、非 ASCII 字符索引）。不抛异常、不查缓存、不管类型从哪来。
- **`gc/`**：`heap.rs`（具体类型 `Heap`，取代 51 方法的 `dyn MagrGC`）、`config.rs`（`GcConfig` 由门面传入，取代 gc→config 27 处引用）、`to_vm.rs`（`GcToVm`：根枚举、`suspend_all` / `resume_all`）、`region/`、`alloc/`（含 TLAB）、`collect/`（标记、分代、增量、屏障、记忆集）、`policy/`、`refs/`、`walk.rs`（堆遍历原语）、`observe/`。移出：`sampler` / `snapshot` / `retention` → profiler；`safepoint` → vm/threads；`ambient` → vm。

### 4.3 vm 内部

每个子目录是一个子系统、拥有一份状态；`VmCore` 只是这些状态的组合（不再挂散字段），`VmContext` 同理组合每线程状态。

```
vm/
  mod.rs           对外 API：Vm、VmContext、invoke(FnId, args)、注册接口、ext::<T>() 每 VM 扩展状态
  vm.rs            VmCore = { diagnostics, threads, runtime, loader, exec, heap, ext_state }
  diagnostics/     发生了什么：counters（L1）、events（合并 RuntimeObserver 与 GcObserver）、monitor（监控 API）
  threads/         谁在跑、怎么让大家停下：registry、safepoint（轮询 / park / suspend_all，实现 GcToVm）、fatal
  runtime/         运行时元数据：组装、查询、缓存
    ids/           FnId / TypeId / StrId 表；location(id) → (PackageImage, 包内下标)
    types/         组装 TypeDesc（调 object 算布局）、泛型实例化、TypeArgs 驻留、constraints
    statics/       每类型静态字段存储、初始化状态、屏障纪元
    resolve/       名字 / token → 运行时身份（跨包、按加载上下文）、缺符号判定
    dispatch/      ic（PIC / FieldIC）、vcall（接收者类型 + 方法 → 目标）、ctor（类 + 构造函数 → 目标）、isa（子类型判定 + IsaCache）
    builtins.rs    builtin 注册表（静态表与扩展库动态表合一）
  loader/          东西怎么进来、怎么卸载：locate/（probing、namespace 索引、第三方依赖）、package、lazy、
                   contexts（加载上下文与卸载）、extern_bind（[Native] / extern 绑定，interop 注册提供方）、availability、init_queue
  exec/            执行：会执行代码、抛异常、触发加载、等待的都在这里
    engine.rs      Engine trait、注册槽、分层 / OSR / deopt 交接
    call/          调用协议：frames、frame_stack、reg_pool、forward、builtin_abi/（参数、返回值、错误种类、invoke）
    ops/           指令语义：scalar、field、array、statics、struct_ops、vcall、obj_new、cast
                   （检查 → 查 runtime → 调 object 原语 → 屏障 → 抛异常）
    init.rs        类型初始化的执行：认领 / 等待、执行 cctor、失败包装
    exception.rs   异常对象与传播
    stack.rs       统一栈遍历（异常栈回溯、GC 根、调试器、profiler 共用）
    stack_guard.rs 栈余量检查
```

内部依赖自下而上：`diagnostics, threads → runtime → loader → exec → vm.rs / lib.rs`；exec 之下的子系统只拿自己的状态，不拿 `VmContext`。

**runtime 的「四不」**：不执行 z42 代码、不抛 z42 异常、不触发加载、不等待其他线程——只返回结果或「未加载 / 不存在」，由 exec 的慢路径请 loader 加载后重查。这是 runtime 不依赖 exec 的保证（现 resolver「查询时加载包、排空初始化队列」的环由此消除）。按此标准：类型初始化的**状态**归 `runtime/statics`、**执行**归 `exec/init`；字段偏移、派发目标、子类型判定等**查询与缓存**归 `runtime/dispatch`，指令的**执行语义**归 `exec/ops`。

现 `VmCore` 39 个字段的去向：身份类（module、funcs、types、strings）、加载类（lazy_loader、context_registry、pending_type_inits 等）、初始化类（statics、cctors、static_init_error）分别进 runtime / loader；线程类（vm_contexts、gc_phase、parked_count、collector_active 等）进 threads；counters、observers、sampler、锁统计进 diagnostics；builtin 注册表归 runtime；jit 变成 exec 的引擎槽；**移出 vm**：processes / file_handles / tcp / tls / udp 七个 OS 资源表进 corelib 各模块的扩展状态，native_types / native_libs / pinned_owned_buffers 进 interop 的扩展状态，program_args 进门面。

### 4.4 引擎、能力、工具、门面

- **`interp/`**：`run/`（主循环）、`ops/`（指令到 vm/exec 的薄适配；`exec_ffi::ffi_call`）、`frame/`（帧与 arena；若 JIT 也用 transient arena 则下沉 vm/exec/call）、`superinstr.rs`（现 `metadata/superinstr.rs`）。
- **`jit/`**：`compile/`（Cranelift 管线、代码 arena 与后台编译队列）、`translate/`、`helpers/`（原样保留）、`runtime/`（FnEntry、`enter_compiled`、JitFrame）。AOT（M9）落地时把 `translate/` 与 helper 声明拆成 `codegen/`，由 `jit/` 与 `aot/` 共用。
- **`corelib/`**：只写 builtin 胶水（参数、异常映射、句柄表、park），碰 OS 一律调 pal。`core/`（string、convert、math、object、array、gc、threading、monitor、reflection/ 等，无 feature 开关）、`io/`（console、fs）、`os/`（feature `process`）、`net/`（feature `net`）、`tls/`（feature `tls`，启用 rustls）、`repl/`（feature `repl`）、`test_host.rs`。不再另设 stdlib-os / stdlib-net 模块：OS 实现在 pal、胶水在 corelib，rustls 可选用 feature 表达。builtin 的 ABI 与注册表在 vm（`exec/call/builtin_abi`、`runtime/builtins.rs`），实现全部在 corelib 与 interop。
- **`interop/`**：`tier1/`（Tier 1 C ABI：registry、marshal、dispatch、exports）、`ext/`（统一的扩展库加载器、REPL 编辑器懒加载、实现 loader 的 extern 提供方）。feature `ffi`（libffi，wasm 关闭）、`ext`（只需 libloading）。
- **`debugger/`**：VM 进程内的调试代理（同 Node inspector / JVM JDWP 代理）。`engine/`（断点表——源位置经 metadata 行号表映射到 (FnId, pc)、条件与异常断点；step in / over / out；**默认全体暂停**，经 vm/threads 的 `suspend_all`，即路线图 Q14 的建议答案；帧 / 局部变量 / 对象检查；表达式求值复用 REPL 编译通道）+ `adapter/`（DAP：stdio / TCP 传输、会话分派，`z42vm --debug` 时起服务线程，feature `dap`）。前端（`z42d dbg`、VS Code 插件）不在 runtime 目录。前置：编译器把局部变量名与作用域写进 .zsym。
- **`profiler/`**：`cpu/`（safepoint 采样，现 `gc/sampler.rs`）、`alloc/`（TLAB 每满 N KB 采一次栈，对标 .NET AllocationTick / JFR）、`locks/`（锁争用探针，现 `corelib/sync_contention.rs`）、`heap/`（堆快照、保留链，现 `gc/snapshot.rs` / `gc/retention.rs`，遍历用 `gc/walk.rs`）、`coverage/`、`jitmap/`（perf map / jitdump）、`export/`（folded、chrome / perfetto trace、V8 heapsnapshot、pprof）。`Std.Diagnostics` 的剖析类 builtin 由它自己登记。
- **监控 API（vm/diagnostics/monitor）**：工具槽 DEBUGGER / PROFILER / COVERAGE；事件含函数进出、行 / pc 单步、分支、异常抛出与捕获、分配采样、safepoint 采样、线程起止、模块加载与卸载、JIT 编译、GC 阶段；按函数开关，未开启的函数零成本，开启后解释器走插桩分派、JIT 函数回退解释器（deopt）。对标 CPython PEP 669、CoreCLR ICorDebug / ICorProfiler、JVMTI。
- **门面（crate 根）**：`lib.rs`（Runtime、RuntimeBuilder）、`config/`（现 `config.rs` + `config/*`，新增 `split.rs` 拆出 VmConfig / GcConfig / JitConfig）、`app.rs`（现 app / boot / startup / vm）、`crash.rs`（现 `signal_handler.rs`，可选安装）。feature：jit、interop、debugger、profiler、net、tls、process、repl、mimalloc；平台预设是 feature 组合。
- **hot-reload** 不设模块：loader 重载新版本包、runtime 做函数与类型的版本映射、exec/engine 负责 deopt 与替换，与调试器共用 deopt 和全体暂停。
- 删除：`aot.rs`、`thread/`（空桩）。

### 4.5 易混边界

| 两处 | 怎么分 |
|---|---|
| metadata ↔ vm/runtime | metadata 是包文件里的静态信息，纯静态查询直接用它；runtime 是组装后的运行时身份、实例化与缓存 |
| object ↔ vm/runtime | object 定义形状、计算布局；runtime 组装类型、分配 id。判据：GC 标记一个对象只需要 object |
| object ↔ vm/exec/ops | object 是不带检查的原语；ops 是带检查、带异常、带 IC 的指令语义 |
| vm/runtime ↔ vm/loader | loader 负责装进来与卸载；runtime 负责装进来以后是谁、怎么查 |
| vm/runtime ↔ vm/exec | runtime 守「四不」；会执行、抛异常、触发加载、等待的都在 exec |
| gc ↔ vm/threads | gc 决定何时、怎么回收；threads 负责让所有线程停下 |
| pal ↔ corelib | pal 直接调 OS、只用 std 类型；corelib 是 builtin 胶水 |
| vm（builtin 注册表 + 调用约定）↔ corelib / interop | vm 是插座，corelib 与 interop 是插头 |
| vm/diagnostics ↔ debugger / profiler | vm 提供计数、事件总线与监控 API；工具在各自模块 |
| 门面 ↔ vm | vm 是一台 VM 的运行机制；门面负责配置装配、启动编排与组件选择 |

---

## 5. 打包与嵌入的遗留项

z42vm 继续静态链接 VM，不改为动态链接 `native/libz42`（结论与重新考虑的触发条件见 [packaging.md](internals/src/devinfra/packaging.md)）。SDK 瘦身、dylib 元数据（`@rpath` / SONAME / `z42.dll.lib`）、glibc 下限已修。剩下的：

1. **嵌入路径比 z42vm 慢 1.26–1.55 倍**（分配器能解释大约 2/3）：给 staticlib/cdylib 打包构建加 lib 内 mimalloc feature；剩余差距需要 profile。影响面：self-contained、iOS、Android、wasm。
2. libz42.a 导出 69 个 `jit_*` 和 37 个 `ffi_*` 全局符号，嵌入者容易撞名。
3. clap / tracing-subscriber 只给 z42vm 可执行文件用（feature 开关，嵌入的库不带），`env-filter` 换成 `Targets`（省 0.3–0.45 MB）。
4. z42-compression 没做 LTO；macOS 部署目标不一致。
5. Android `opt-level="z"`；打包吞掉错误（nightly Android 包里 compression 确实缺失）。
6. apphost 用 spawn + wait 而不是 exec，子进程被信号杀死时退出码变成 1。
7. PGO 机会：70 KB 的 `exec_function_body`。
8. **Tier-1 native 扩展改用函数表 ABI**：扩展要调全局符号 `z42_register_type`，而 z42vm 不导出它；函数表 ABI 让这个问题消失（与 §6.5 interop 那一条同一件事）。

---

## 6. 待办

> 状态：⬜ 待做 · 🟡 进行中 · ✅ 已合并（附 PR 号）· ⏸ 需 User 决策。每项一个 PR（大项拆成多个），各自过完整 GREEN（`./xtask test` + `./xtask test runtime`；纯文档 PR 走 docs 快速通道）。fix / perf / refactor 项按 PR → GREEN → auto-merge → 下一项推进；**带设计取舍的项先在对话里出方案**，User 确认后再实施。表格在阶段收尾时用一个文档 PR 统一更新，单个 PR 不改本表（避免相邻行冲突）；完成的行在收尾时删除。

### 6.1 目录与模块化迁移（User 2026-10-29 定案，目标结构见 §4）

在单 crate 内把目录重排成 §4 的形状并由门禁守住，暂不物理拆 crate。每步一个 PR、行为不变（GREEN 双模式 + `./xtask test runtime`）。

| ID | 类型 | 内容 | 验证要点 | 状态 |
|---|---|---|---|---|
| L-4 | refactor | metadata 剩余拆分：静态包数据与每台 VM 的链接状态分离；`types/` 形状进 object，组装与查询进 vm/runtime，加载进 vm/loader；`semantics.rs` 进 exec/ops | metadata 不再引用 gc / vm_context / corelib；启动项 S1–S4 可在新结构上做 | 🟡 |
| L-5 | refactor | gc 断环：注入 `GcConfig`、加 `GcToVm`、safepoint 移到 vm/threads、`MagrGC` 改具体类型；sampler / snapshot / retention 移到 `profiler/` | gc 不再引用 config / vm_context / corelib；GC 模型测试与 loom 测试；13 / 09 微基准不退化 | ⬜ |
| L-6 | refactor | 引擎断环：vcall / obj_new / isa 拆成 runtime/dispatch（查询）与 exec/ops（执行）；加 Engine 注册槽与统一栈遍历；interp ↔ jit 不再互相引用 | 双引擎一致性测试；OSR / 分层测试；`jit_native_from_interp` 计数不变 | ⬜ |
| L-7 | refactor | VmCore 按子系统重组为状态组合；corelib 收尾（builtin ABI 移 vm/exec/call、OS 资源表移进扩展状态、corelib 不再引用 interp）；配置与启动编排移到门面层 | VmCore 无散字段；z42-host 嵌入与 C ABI 测试 | ⬜ |
| L-8 | — | 物理拆 crate：暂不做，VM 保持单 crate、各层是 `src/runtime/src/<模块>/`（User 2026-10-29）；以后是否拆另议 | — | ⏸ |
| L-9 | feature | 监控 API（vm/diagnostics/monitor）→ debugger（0.8.x，前置：.zsym 局部变量信息）→ profiler 的分配采样与覆盖率；AOT（M9）时拆出 codegen | 各自立项，动工前出方案 | ⬜ |

依赖与顺序：模块拆层当前为 L-4；L-4 先于 L-5、L-6（object 与 runtime 先成形）；L-7 在 L-5、L-6 之后。阶段 2 第 3 项（PackageImage / 每 VM 链接状态 / TierApi / GC 具体类型）分别落在 L-4、L-6、L-5。

以下 6.2–6.4 依据 §1 与 2026-10-28 一轮只读代码审读。排序按预期收益；「结构性」项属阶段 2，动工前需 User 确认方案。行号对应审读时的 main `bd14a4958`。

### 6.2 执行（X 系列）

| ID | 类型 | 内容 | 对照 | 预期 | 状态 |
|---|---|---|---|---|---|
| X-7 | perf/jit | **JIT 第二档：以 VCall PIC 为画像做守卫去虚化 + 内联**。现状：`emit_vcall_helper`（`jit/translate/object.rs`）一律调 helper，JIT 不内联任何 z42 callee；编译器 `IrInline` 只内联同包、直接调用、单块的函数体。改法：热函数按站点 PIC entry0 重编，生成「TypeId 守卫 → 内联 callee」，守卫失败回落 helper（不需要 deopt）；callee 寄存器映射到调用方寄存器文件的 `+max_reg` 窗口，栈回溯靠内联 pc 表 | CoreCLR tier1 + PGO 守卫去虚化 | 单态 getter / setter 调用 ≈22 → 1–2 ns；list_ops、vcall、z42c visitor | ⏸ 结构性 |
| X-8 | perf/vm | **JIT→JIT 直接调用 + 每线程连续 Value 栈**（即阶段 2 第 2 项、X-5 后方向 2）。现状：JIT 帧是堆上寄存器数组，一次调用经 `jit_call`（10 参）→ 槽表 → 屏障 → `new_args_from` → `enter_compiled` 约 6 层 Rust 调用、2 次 safepoint 轮询 | CoreCLR 机器栈直接 call；CPython 3.11 连续帧栈零分配 | 每次调用 ≈20 → 3–5 ns | ⏸ 结构性 |
| X-9 | perf/jit | **数组访问全内联**：`ArrayLen` 恒走 helper（`translate/array.rs`）；hoist 只覆盖整函数不写的寄存器，`this._items[i]` 每次都调 `hr_array_data`；引用 / T 元素写恒走 `jit_array_set`。改法：内联 tag → 头 → len / data 解码，引用写复用字段 IC 的屏障判定，`i < arr.Length` 形态消除边界检查 | CoreCLR 内联长度、内联写屏障、BCE | 集合密集代码 1.3–2× | ⬜ |
| X-10 | perf/jit | **is / as 内联**：`IsInstance` / `AsCast` 恒调 helper 再查 `isa_cache` 哈希。改法：TypeDesc 加 Cohen display（深度 + 祖先数组），类目标 O(1) 内联判定；接口目标用站点单态 memo | CoreCLR 精确类型比较 + cast cache | 类型判定密集代码该部分 3–5× | ⬜ |
| X-11 | perf/jit | **方法级泛型函数体可 JIT + 方法类型实参驻留**：mta 是 `Box<[String]>`，解释器每次泛型调用深拷贝（`interp/exec_support.rs`）；读 T 的函数体被整函数拒译（`translate/unsupported.rs`）。改法：mta 改为驻留 `TypeArgs` 句柄（M14 同法）放进 Frame / JitFrame，`MethodTypeArg` 等经 helper 读句柄，`$mta` 转发直接传句柄 | CoreCLR 共享规范代码 + 泛型字典参数 | 最后一类泛型解释回落消失、泛型调用零分配 | ⬜ |
| X-12 | perf/vm | **分配快路**（依赖 M15、M16）：`jit_obj_new` 是 11 参 helper，每次过 dyn `MagrGC`、2 次 TLS、全局原子记账、`Arc<TypeDesc>` clone，ctor 再走完整调用协议；解释器 `new` 还为 ctor 实参 malloc 一个 Vec。改法：TLAB cursor / limit 挪进 VmContext，站点稳定后用纪元门（同 X-4）跳过屏障，生成码直接 bump、写头、清零；空洞复用只走慢路 | CoreCLR 每线程分配上下文 + 内联 bump（≈10 条指令、0 原子） | 09_alloc_ctorless 1.5–2×；z42c `jit_obj_new` 14.5% inclusive | ⬜ |
| X-13 | perf/jit | **后台 / 批量编译 + 代码 arena**：编译在调用线程上、持全局 `Mutex`，持锁期间其他线程直接等不 park；每函数一次 `finalize_definitions`、各占一块 mmap。动工前先对齐编译耗时两个口径（计数 1.59 s vs 采样 3.9%） | CoreCLR tier1 后台编译 | z42c 墙钟 −5~15%，`--jobs` 不再串行等锁 | ⬜ |
| X-14 | perf/interp | **解释器调用路径继续瘦身**：调用仍有两道屏障，并逐次检查 `$mta:` 转发。将 X-4 的纪元快路用于解释器调用，减少重复屏障与实参扫描；须保留属主初始化、加载和歧义诊断的语义 | — | 减少屏障与扫描开销（需重测） | ⬜ |
| X-15 | perf/vm | **异常惰性栈**：throw 时当场格式化整条栈（`exception/mod.rs`），`find_handler` 按 String label 查 HashMap。改法：只存 (func, pc) 快照，读 StackTrace 时才格式化；异常表用块下标 | CoreCLR 惰性 IP 数组 | 异常密集代码 5–10× | ⬜ |
| X-16 | perf/jit | **调用 safepoint 轮询去重**：被调方入口与调用方返回后各轮询一次。先确定 GC 停顿延迟约束，再判断哪些调用可安全省去一次 | — | 降低调用轮询开销（需重测） | ⏸ 需确认延迟约束 |

### 6.3 内存与 GC（M 系列）

| ID | 类型 | 内容 | 对照 | 预期 | 状态 |
|---|---|---|---|---|---|
| M15 | perf/gc | **对象类型字不计数**：`Arc<TypeDesc>` → arena 持有的 `*const TypeDesc`。现状每次 `new` 一次 `Arc::clone`（`interp/obj_new_resolve.rs`），每个死对象经 drop glue 一次 fetch_sub + 间接调用；多线程分配同类时争同一条 refcount 缓存行。改后 Object 块是纯 POD，是 X-12、M18 的前提。风险：可回收加载上下文卸载时须保证无活实例（`context_reclaimer` 钩子） | .NET MethodTable* 不计数 | 每次分配 / 死亡各省 1 次竞争原子 RMW | ⬜ |
| M16 | perf/gc | **分配记账下沉到 TLAB + 热字段缓存行隔离**：`record_alloc_fast`（`gc/arc_heap/alloc.rs`）每次分配两次共享 fetch_add；`used_bytes` / `allocations` 与 JIT 屏障常读的 `mode` / `promotion_age` 同结构体无 padding（推断伪共享）。改法：claim 时按 cursor 差值记账，回收闸门做成 claim 的 limit 指针 | .NET alloc_limit：只在换缓冲时记账 | 单线程每次分配省 2 次 RMW；多线程扩展性（gc-tlab 实测 24 线程仅 2.76×） | ⬜ |
| M17 | perf/gc | **引用实参的泛型数组 REF 打包**（M13 延伸）：`objops/array.rs` 只为基元实参打包，`List<string>` / `List<Node>` / 字典键数组仍是 16 B 一格的 MIXED | .NET 8 B / 元素 | 每元素 −8 B | ⬜ |
| M18 | perf/gc | **免清扫 nursery**（移动式 nursery 的中间步，依赖 M15）：`sweep_young_with` 对每个死的年轻块做 tombstone / CAS / drop / 位图 / free-list push，minor 成本 ∝ 分配量。改法：标记时按 chunk 计存活，零存活 chunk 整块入池，半活 chunk 交给空洞补货时惰性清扫（Immix 式） | .NET gen0 成本 ∝ 存活量 | minor 时间 ÷2–4（推断） | ⬜ |
| M19 | perf/gc | **标记内核**：单线程 BFS，队列元素 16 B `Value`，每条边一次 `&mut dyn FnMut`，经 `Arc<TypeDesc>` 多跳取引用偏移。改法：TypeDesc 热区放引用偏移位图（GCDesc 式），标记栈 8 B 头指针；之后用已 park 线程并行标记（阶段 3） | .NET GCDesc + 并行标记 | 单核 1.3–2×（推断） | ⬜ |
| M20 | perf/gc | **大数组卡表**：记忆集按属主整个重扫，100 万元素老 `object[]` 每次 minor ≈6 ms。≥1–4 KB 数组配逐区间卡字节，屏障置 `idx >> k` | .NET / HotSpot card table | 去掉记忆集穿透的停顿地板 | ⬜ |
| M21 | perf/gc | **尺寸档细化**：`gc/var_region/chunk.rs` `SUB_LOG2 = 2`，注释实测取整浪费约占 footprint 15%；改 3 后 < 128 B 全按 8 B 对齐（stash 的 class 位图需加宽） | — | 堆 footprint −5~10%（推断） | ⬜ |
| M22 | perf/gc | **chunk 64 KB 对齐 + 池按时间 purge**：chunk 从 mimalloc 按 16 B 对齐分配（arm64 64 KB chunk 只有 3 页可 decommit），`chunk_of` 走 BTreeMap，池至少留 64 MB 或 occupied/4（`gc/arc_heap/footprint.rs`）。改法：64 KB 对齐 mmap 段，`chunk_of` 一次 mask、整页 decommit、侧表固定偏移；池改延时 purge、去掉固定地板 | .NET regions / mimalloc segment | 小堆 RSS 降几十 MB | ⬜ |
| M23 | perf/vm | **字符串**：分配先清零再拷贝（冗余）、之后再扫一遍 `is_ascii`，拼接不复用源串 ASCII 位；`""` 与单字符串无缓存；非 ASCII 串按字符索引靠 8 条目 TLS LRU，> 8 串交错退化 O(n²)。改法：去冗余清零；块尾 breadcrumb（每 64 字符）+ 前缀字符数 | CPython 单字符缓存、PEP 393 O(1) 索引 | 拼接密集负载几个百分点；非 ASCII 索引去掉退化 | ⬜ |
| M24 | perf/vm | **闭包单块**：闭包块 32 B + FROZEN env 16 + 16n B 两次分配，env 内联进闭包块 | — | 每闭包 −1 次分配、−16 B（T5 剩余大头） | ⬜ |
| M25 | perf/gc | 低频全局锁：弱引用 downgrade / upgrade 每次拿 `region_var` 全局锁查 BTreeMap（`gc/arc_heap/side_tables.rs`）；safepoint 慢路径每 1024 次 poll 取一次 `gc_phase` Mutex；锁路径 `record_alloc` 为读 sampler 每次 `inner.lock()` | — | 争用场景 | ⬜ |

不做：NaN-boxing（`long` 需 64 位全宽，大整数要装箱；16 B `Value` 只剩寄存器 / 型参单元 / MIXED 数组，走 M17 与阶段 2 类型化寄存器更划算）。

### 6.4 启动与加载（S 系列；目标 hello ≈4–5 ms / 6–7 MB，即 NativeAOT 量级）

| ID | 类型 | 内容 | 对照 | 预期 | 状态 |
|---|---|---|---|---|---|
| S1 | perf/load | **合并入口模块时增量构建类型表**：加载单包、合并入口后仍各建一遍。先确定跨包成员贡献与构造泛型继承的归属判据，再增量追加，保证 vtable 完整 | — | 省去第二次全量构建（需重测） | ⬜ |
| S2 | perf/load | 库目录 NSPC 扫描冷启动全价（`metadata/namespace_index.rs` + `metadata/reader/zpkg.rs` 整读、全解 STRS）。改法：z42.core 已提供的命名空间不扫；只读文件头、目录与 NSPC 引用的串；SDK 安装时生成 `namespace → zpkg` 索引 | .NET TPA / deps.json | −1.0~1.5 ms | ⬜ |
| S3 | perf/load | STRS 每包解码 3 次、每项一个 `String`。改为一次解析 + `Arc<[u8]>` 偏移零拷贝 | — | −0.5~0.8 ms | ⬜ |
| S4 | perf/load | `.zsym` 与 `frame_meta` 启动时 eager（`metadata/loader/artifact.rs` 解析 sidecar ≈0.3 ms，`metadata/loader/indices.rs` 给全部函数预格式化帧名）。改为首次生成栈回溯 / 调试器附加时加载 | .NET PDB 按需 | −0.3~0.4 ms、≈−0.5 MB | ⬜ |
| S6 | perf/load | 入口函数不预编译：`jit/runtime.rs` 的 `resolve_fn_by_name` 先编译 Main 再执行；只跑一次的 Main / cctor 交给解释器 + OSR | .NET tier0 / OSR | hello −0.2 ms、`z42c --help` −2 ms | ⬜ |
| S7 | perf/load | **函数体按需解码**（阶段 2 第 3 项 PackageImage）：`read_mods_section` 把每条指令还原成带 `String` 的结构，`indices.rs` 给每个函数建 block_index / fused_tails，`resolve_module` 全量预填 token。改为启动只解 SIGS / TYPE，函数体首次调用时解码 | .NET 按方法懒 JIT、CPython 按模块导入 | hello −2.2~2.5 ms / −3~4 MB；`z42c --help` 25 → 10–12 ms | ⬜ |
| S8 | perf/load | z42.core 链接后快照（阶段 3，S1/S3/S7 之后）：TypeId、函数表、驻留串做成可 mmap 只读镜像 | Java CDS、V8 snapshot、CPython deepfreeze | core 加载 ≈0.3 ms，多进程共享干净页 | ⬜ |

不做：二进制体积与 dyld（比 C `return 0` 只多 0.9 ms）、toml / tracing 初始化（≈0.1 ms）。

### 6.5 遗留待办（来自阶段 0 / 1 的分析，未进上面各表）

| 领域 | 内容 |
|---|---|
| GC | M12 空洞复用的后续：① 局部性——每 chunk 死槽位图、按连续空闲段认领（同段内顺序 bump）；② 自适应 nursery 对 minor 成本的判定，避免局部性变差时 nursery 收缩级联成更多晋升 / major |
| GC | R5：Monitor 侧表（首次加锁才膨胀），待语言暴露 `lock(obj)` / `Volatile` / `Interlocked` 时做 |
| JIT | 字段 IC：型参字段与 `F32` 字段仍走 helper；原生 IC 只比 entry 0 |
| JIT | 整函数拒译：带 `ref` / `out` 实参（`LoadLocalAddr` 等）、`CallNative` 仍让整个函数走解释器（z42c 里 `out` 25 处、库里 ref/out 195 处）；JIT 寄存器文件不移动，取地址可用槽地址 + 帧 id 守卫 |
| JIT | 类型化寄存器全有或全无：寄存器只要有一处走内存路径（如作数组下标）就整个失去常驻资格（归阶段 2 第 2 项类型化 SSA） |
| JIT | JIT 忽略逃逸分析的 `stack_alloc`，比 interp 多做 GC 分配 |
| JIT | helper 里的 panic 会 abort 进程（`extern "C"` 不展开）；新增 helper 要改 6 处、Cranelift 签名与 Rust 签名分开手写——`helpers!{}` 宏统一生成并编译期校验 |
| 加载 | 惰性加载在 lazy_loader 写锁内做文件 I/O 与全量解码，期间所有线程的 lookup miss 被阻塞——改为读锁下规划 → 锁外解码 → 写锁下发布；每个 VmCore（每个 golden、每个 host module）都从头解码 z42.core——随 PackageImage 进程级缓存解决 |
| 身份 | 「身份按名字」的残余：三套函数注册表合一的清理、PIC 未覆盖的反射侧 |
| builtin | STAGE2-DEBT：旧 builtin 名与 z42.core 旧包装在种子不再声明后删除（运行时表项、legacy 实现一并删） |
| interop | native 扩展改用函数表 ABI v2（`const Z42Api*`，与 repl 的 `ReplCallbacks` 同构），扩展不再依赖 z42vm 导出全局符号（同 §5 第 8 条，本条为准） |
| 清理 | 死代码：`Function.exec_mode`（解码了从不读）、零注册的 finalizer 机制、`collect()` 默认 no-op、死旋钮 `Z42_GC_THROTTLE_RATIO`、`versions.rs` 的 changelog 注释、z42-macros 的 `compile_error!` 桩仍从 z42-rs prelude 导出 |
| z42c | StrMap 数据结构（z42c 侧，z42 层内建采样 ≈11%）：键哈希驻留、探测局部性、rehash 次数——属 z42c 战役，开工前与该线对齐 |

### 6.6 待 User 决策

- `int` 等窄整数的算术溢出不回绕到本宽度（`int.MaxValue + 1`、`int.MinValue / -1` 两路都得 `2147483648`，值按 i64 存、运算后不截断）：要不要按声明宽度回绕、在哪一层截断（编译器还是 VM）。

### 6.7 阶段 2 / 阶段 3（动工前另行确认方案）

阶段 2：
1. 解释器预链接紧凑码（u16 寄存器 / 16 B op / 偏移跳转 / 内嵌 IC 槽 / quickening / handler 表 + 尾调用分发）——iOS / wasm（只有解释器）性能的主要杠杆。
2. 每线程连续 VM 栈，interp 与 JIT 共用；JIT→JIT 直接调用（X-8）；JIT 代码 arena 与后台编译（X-13）；类型化 SSA 寄存器。
3. `PackageImage` + 每 VM 链接状态、TierApi 注册槽、GC 编译期具体类型——随 §6.1 的 L-4…L-7。

阶段 3：精确 stack map + 移动式 nursery（M18 是中间步）；用已 park 的 mutator 线程做并行标记；AOT 复用 JIT 翻译核（前提：改用符号化常量，Cranelift 现在到处烘焙进程内绝对地址）；按 packaging.md 记录的触发条件再评估动态链接 / 可选 JIT。

---

## 附录：测量方法

- **微基准**：先 `z42c --emit-zbc src/benches/scenarios/<n>.z42 <n>.zbc --opt-all`，再 `z42vm --mode jit|interp <n>.zbc <Namespace>.Main`，取 5 次中位数。
- **真实负载剖面**：
  1. 在 `src/compiler` 下执行 `z42c build --workspace --release --output-dir <tmp> -q`，事先把 stdlib 的 zpkg 拷进 `<tmp>`；
  2. 运行期间用 macOS 的 `sample <z42vm pid> 18 -file out.sample` 采样；
  3. 按函数名统计包含时间（对每条调用链只计最外层匹配）。
  - `Z42_PORTABLE_VM=<z42vm>` 可以让同一个 z42c 跑在指定的 VM 上，便于新旧对比。
- **GC**：`Z42_GC_TRACE=1`（每次回收一行）、`Z42_GC_PHASES=1`（trip 行带退避倍数 `xN`），`--set gc-mode=stw|generational`。
- **惰性加载**：`Z42_LOG=z42::metadata::lazy_loader=debug`，统计 `lazy-loaded zpkg` 行数。
- **启动**：`/usr/bin/time -l`（RSS），用 Python `subprocess` 循环计时取中位数。
