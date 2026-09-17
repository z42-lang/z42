# 嵌入宿主 API（VM 侧实现）

> 对齐：2026-09-17（change `restructure-docs-three-books`）。
> 代码：`src/runtime/src/host/`（`mod.rs` extern "C" 分发 · `config.rs` 配置校验 · `error.rs` 状态码与 TLS
> last_error · `state.rs` 单例状态 · `ops.rs` 加载/解析/调用 · `marshal.rs` 值编组 · `resolver.rs` zpkg 钩子）、
> `src/runtime/include/z42_host.h`（Tier 1 头文件）、`src/runtime/crates/z42-host/`（Tier 2 crate）、
> `src/runtime/src/corelib/io.rs`（sink 路由）、`src/toolchain/workload/{desktop,ios,android,wasm}/`（Tier 3 facade）。
>
> **宿主怎么调**（函数签名、返回码、生命周期与线程约束）见
> [嵌入 C ABI 契约](../../../reference/src/embedding/c-abi.md)，本页不复述。

本页讲 VM **内部怎么实现**那套 ABI：三层怎么分、单例状态放在哪、句柄怎么编码、
输出怎么从解释器路由到宿主回调、错误怎么从 `anyhow::Error` 归类成状态码、zpkg 依赖按什么顺序解析。
改 `z42_host.h` 或往 `src/runtime/src/host/` 加东西之前读这一页。

反方向（native 代码把类型**注册进** z42）是另一套：见 [native-abi.md](native-abi.md)。
两者复用同一份 `Z42Value` / `Z42Args` / `Z42Error`，在同一棵头文件树下并行，不重叠。

---

## 编译边界：host 编，mobile 跑

**z42c 是 host-only 工具**。iOS / Android / wasm 等嵌入式平台**只装 VM**，不带编译器；
mobile 端拿到的是 host 端 `z42c` 编出来的 `.zbc` / `.zpkg`，由平台 facade 在运行时 load。

后果有三条，都会在改测试基建时撞上：

- 各平台的测试资产步骤（`z42 xtask.zpkg test platform <plat> assets`，实现在
  `scripts/test/xtask_test_platform.z42`）把 `src/toolchain/workload/fixtures/*.z42` 在 **host 端**编成 `.zbc`，
  再拷进 `Z42VM.xcframework/Resources/` / `z42vm/src/main/assets/` / `pkg-{web,nodejs}/`。
- 平台 facade 的 test harness（XCTest / JUnit / Playwright）**只 load 预编 `.zbc`**，测试代码里不出现 `z42c`。
- 这条约束在编译器不再需要 host 工具链之前不会变。

---

## 三层分工

```
┌─────────────────────────────────────────────────────────────┐
│  Tier 3: 平台 facade                                          │
│    Swift Package (Z42VM)       → iOS app                    │
│    Kotlin AAR (io.z42.vm)      → Android app                │
│    npm package (@z42/wasm)     → 浏览器 / Node.js            │
│    C shell (testhost/apphost)  → 桌面自包含 app              │
├─────────────────────────────────────────────────────────────┤
│  Tier 2: Rust 嵌入 API（crate z42-host）                     │
│    Host::new() / load_zbc / resolve_entry / invoke          │
├─────────────────────────────────────────────────────────────┤
│  Tier 1: C ABI（z42_host.h）                                 │
│    z42_host_* + z42_zpkg_read_namespaces + z42_host_run_app │
└─────────────────────────────────────────────────────────────┘
                              ↓
                          z42 VM
```

| 路径 | 内容 |
|------|------|
| `src/runtime/include/z42_host.h` | Tier 1 C 头文件（与 `z42_abi.h` 平行）。**唯一一份**——iOS / Android facade 目录下那两个同名文件是只有一行 `#include` 的转发头 |
| `src/runtime/src/host/` | C ABI 在 VM 内的实现（Rust `extern "C"`） |
| `src/runtime/crates/z42-host/` | Tier 2 Rust crate（`z42-host`，lib 名 `z42_host`） |
| `src/toolchain/workload/fixtures/` | 各平台契约测试共用的 z42 夹具（`hello.z42` / `multi_line.z42`） |
| `src/toolchain/workload/{desktop,ios,android,wasm}/platform/` | Tier 3 facade |
| `src/toolchain/workload/desktop/tests/r1_r7.c` | 真·外部 C 消费者：链 `libz42`，跑 R1–R7 七个契约场景。改 ABI 时这是第一道拦截 |

### 为什么是这个形状

参照 CoreCLR `coreclrhost.h`、JNI `JavaVM`、Lua `lua_State` 三家的经验，定了五条：

1. **单实例** —— 每进程一份 VM 状态，`Z42HostRef` 是占位 sentinel。多实例要求把 VM 全局状态
   per-handle 化，工作量与收益不成比例，先不做。
2. **三层 ABI** —— 与 [native-abi.md](native-abi.md) 同构：Tier 1 稳定 C ABI；Tier 2 Rust 人因工程；Tier 3 平台 facade。
3. **AOT 友好** —— 入口按 FQN 字符串查找，运行时不依赖反射元数据生成器。iOS 禁 JIT 的场景下走 interp。
4. **零拷贝优先** —— 标量值通过 `Z42Value` 直接传递，不做自动编组。
5. **panic 隔离** —— 任何 z42 异常 / Rust panic 都不跨 FFI 线，统一翻译成 `Z42HostStatus` + `Z42Error`。

---

## 单例状态与句柄编码

`state.rs` 持一个 `static HOST: RwLock<Option<HostState>>`。
`initialize` 拿写锁、发现 `Some` 就返回 `AlreadyInit`；`shutdown` 拿写锁换成 `None`。
状态转移只在这两处，`ops` 层一律经 `with_state_read` / `with_state_write` 访问，
拿不到状态（`None`）时统一映射成 `ERR_NOT_INIT`。

```rust
pub(crate) struct HostState {
    pub config:  ResolvedConfig,
    pub modules: Vec<HostModule>,   // 每个已 load 的产物 + 它自己的 VmContext
    pub entries: Vec<HostEntry>,    // { module_idx, fn_idx }
    pub corelib: Option<HostCorelib>,
}
```

三种句柄的编码：

- `Z42HostRef` = **常量 sentinel** `HOST_SENTINEL = 0x1`，永不解引用。`is_valid_handle` 只验
  「非 NULL ∧ 等于 sentinel ∧ 当前已初始化」。
- `Z42ModuleRef` / `Z42EntryRef` = 对应 `Vec` 的**下标 + 1**（加一是为了让 NULL 与第 0 个句柄区分开）。

**没有代龄（generation）**：shutdown 抹掉整个单例，之后任何 host API 调用都返回 `ERR_NOT_INIT`，
所以陈旧句柄不会被误当成有效句柄用——但也仅此而已，同一进程内 shutdown→initialize 之后
拿老句柄会命中新 VM 的同下标条目。多实例落地时这里要换成带代龄的句柄。

`HostState` 手工 `unsafe impl Send + Sync`：`ResolvedConfig` 里带函数指针和宿主的 `user_data`；
函数指针本身是 `Send`/`Sync`，`user_data` 被存成 `usize` 而非裸指针，运行时从不解引用它。
线程安全归宿主，这是契约的一部分。

### 每个 module 一个 VmContext

`HostModule` 持 `Pin<Box<VmContext>>`，模块本身由 ctx 拥有。
这样并发 load 两个产物不会互相污染静态状态。

`ops::build_host_module` 的流程：

1. `load_artifact_from_bytes` 解析用户 `.zbc` / `.zpkg`；
2. namespace 列表 = `["z42.core"]` + 用户产物的 `import_namespaces`（去重、保持声明顺序）；
3. 逐个 namespace **先问 resolver、miss 再扫 `search_paths`**（见下一节）；
4. **eager 合并**所有依赖模块 → `merge_modules` → 重建类型注册表 / 约束校验 / block 索引 / func 索引；
5. `boot::boot_context` 建 ctx（与 `app::run` 走同一套 boot 步骤：cctor 注册、lazy loader 播种、
   availability 折叠），再 `boot::prepare_execution`。

第 4 步选 eager 而不是靠 `declared_candidates` 懒解析，是刻意的：宿主是单实例、每次 `load_zbc`
只走一遍依赖，用一点加载期开销换「invoke 期间不会有意外的懒查找」。

> 第 5 步必须走 `boot::boot_context`，不能手抄。这条路径曾经手工复制过一份更老的子集
> （没有 cctor 注册、没有 lazy loader 播种、没有 availability 折叠、module 放在 ctx 外面），
> 结果嵌入路径的静态初始化行为和 `z42vm` 不一致。

静态初始化本身挂在 `HostModule::static_init: OnceLock<Result<(), String>>` 上，
第一次 invoke 时跑、且**只跑一次**（`init_static_fields` 会先清空所有静态字段，跑第二遍会把已初始化的状态抹掉）。
失败是粘性的：之后每次 invoke 都返回同一条错误，而不是拿半初始化的静态字段继续执行。

---

## zpkg 依赖解析顺序

`load_zbc` 里对每个 namespace 走同一棵决策树：

```
for ns in ["z42.core"] + user_artifact.import_namespaces:
    1. resolver.resolve(ns)     hit → 用 resolver 给的字节
    2. corelib / search_paths   hit → 扫文件系统
    3. silent miss              → load_zbc 仍返回 OK
```

`z42.core` 是**隐式 prelude**：用户 `.zbc` 一句 `using` 都没有，运行时也会请求一次 corelib。

第 3 步「静默 miss」是有意的：自包含产物可能真的不需要 corelib。
代价是错误延后到 invoke——解释器派发时报 `undefined function`，被归类成 `ERR_VM_EXCEPTION`。

`search_paths` 分支的两个细节：

- `probe_corelib` 在 `initialize` 时就扫一遍 `search_paths` 找 `z42.core.zpkg`，并**试解析一次**
  （结果丢弃），这样坏 corelib 在 `initialize` 就报 `ERR_BAD_CONFIG`，而不是在第一次 `load_zbc` 时
  变成一条令人困惑的错误。找到它的那个目录就成了 `libs_dir`，其余 namespace 从那里找。
- 文件系统分支用 canonical 路径去重：同一个 zpkg 常常同时提供多个 namespace，
  不去重就会把 `z42.core.zpkg` 合并进去好几遍。

### 两种 resolver 形态收敛到一个 trait

```rust
pub trait ZpkgResolver: Send + Sync {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>>;
}
```

- **C 函数指针 + user_data**（`Z42ZpkgResolverFn`，来自 `Z42HostConfig`）被 `resolver::CHookResolver`
  包一层。`user_data` 存成 `usize` 以干净地继承 `Send + Sync`。回调返回后**立刻 `to_vec()` 复制**，
  所以宿主那边「字节只需活到回调返回」的契约得以成立。
- **Rust `Arc<dyn ZpkgResolver>`** 由 Tier 2 经 `host::install_zpkg_resolver()` 直接塞进
  `HostState::config`，不绕 C 回调。这个函数**不是** `extern "C"`，是 Tier 2 专用的逃生口。

### Tier 3 各平台的默认 resolver

平台 facade 自己的「namespace → 字节」表**由读 zpkg 的 `NSPC` section 派生**，没有索引文件。
一份 zpkg 通常提供多个 namespace（`z42.core.zpkg` 同时 ship `z42.core` / `Std` / `Std.Exceptions` …），
所以不能假设 `namespace == 文件名`；早先那张手维护的 `index.json` 是 `NSPC` 之外的**第二真相源**、
必然漂移，已经删掉。

| 平台 | 默认 resolver | 位置 |
|---|---|---|
| iOS | `BundleZpkgResolver(bundle: .main, subdirectory: "stdlib")` —— 枚举 `Bundle.urls(forResourcesWithExtension:"zpkg",subdirectory:)`，逐个读 NSPC 建表。`Z42VM.init` 的**默认参数**，不传就自动装 | `ios/platform/Sources/Z42VM/ZpkgResolver.swift` |
| Android | `AssetZpkgResolver(assets, subdir = "stdlib")` —— `AssetManager.list` 枚举，逐个经 `Z42VM.readNamespaces`（JNI 桥到同一个 C ABI）读 NSPC。另有 `MapZpkgResolver`。`Z42VM` 构造器里 resolver 是**必填参数** | `android/.../io/z42/vm/ZpkgResolver.kt` |
| wasm | `bundleStdlibNode(readNamespaces)` / `bundleStdlibBrowser(baseUrl, readNamespaces)` / `mapResolver(map)` —— Node 侧 `readdir`，浏览器侧 fetch 构建期生成的 `files.json`（HTTP 枚举不了目录，这是文件名清单的派生替身，**不是** namespace 映射） | `wasm/platform/js/stdlib-resolver.js` |

读 NSPC 的 helper 在三个层面各有一份入口，都落到同一段 Rust（`metadata::zbc_reader::read_zpkg_meta`）：
C ABI `z42_zpkg_read_namespaces`、Rust `z42_host::read_zpkg_namespaces`、
wasm 导出 `readNamespaces` / Android JNI `Z42VM.readNamespaces`。
Swift / Kotlin / JS 都不需要自己重写 zpkg 解析。

`./xtask build stdlib` 产 flat view 时不写任何索引文件；各平台 `build.sh` 只拷 `*.zpkg`。

---

## 输出路由：从解释器到宿主 sink

sink 的落点在 `src/runtime/src/corelib/io.rs`，由两级开关控制：

```rust
static HOST_STDOUT_SINK: RwLock<Option<HostSink>> = RwLock::new(None);   // 进程全局
static HOST_STDERR_SINK: RwLock<Option<HostSink>> = RwLock::new(None);
thread_local! { static HOST_SINK_ACTIVE: Cell<bool> = const { Cell::new(false) }; }
```

- **进程全局的 sink 槽**由 `install_host_stdout_sink` / `install_host_stderr_sink` 装卸：
  `initialize` 从 config 装、`set_*_sink` 换、`shutdown` 卸（装 `None`）。
- **线程局部的 active 标志**由 `ops::HostSinkGuard::enter()` 在 `invoke_impl` 开头置位、
  `Drop` 时复位为**进入前的值**（不是无条件 false），所以 panic 或提前 return 都不会留下悬空标志。

`route_stdout` 的优先级：active ∧ 全局槽非空 → 宿主 sink；否则 → test-IO 捕获栈；再否则 → 进程 stdout。

**为什么要这个线程局部标志**：sink 槽是进程全局的，但只有正在执行 `z42_host_invoke` 的那条线程
应该把输出交给宿主。没有这个标志，另一条线程上并发跑的 `TestIO.captureStdout` 会被劫持到宿主 sink 去。
代价是 z42 程序自己 spawn 出来的线程的输出不进宿主 sink——这条限制写进了契约。

`WriteLine` 的换行拼在 `dispatch_host_sink` 内部同一个 buffer 里一次交付，所以「一次写出 = 一次回调」，
顺序天然等于 z42 程序的写出顺序。

> `z42_host_set_stdout_sink(host, NULL, ud)` 是**卸载**（装 `None`），不是「恢复 config 里那个」。
> 头文件注释目前写的是 "NULL restores the configured default"，与实现不符。

---

## 错误归类

`error.rs` 的 `LAST_ERROR` 是 `thread_local! { RefCell<LastError> }`，
`message` 由一个 `CString` 背书，指针有效期到同线程下一次 `set_error` / `clear_error`。
没有待决错误时返回的是一个静态空串指针（不是 NULL），所以宿主不必判空。
每个 extern "C" 入口在成功路径 `clear_error()`、失败路径 `set_error(...)`，两者都返回状态码，
于是实现里可以一行写成 `return set_error(...)`。

panic 兜底靠 `guard()`：整个函数体包在 `catch_unwind(AssertUnwindSafe(..))` 里，
Err 分支翻译成 `ERR_INTERNAL` 加一条稳定消息。`z42_host_run_app` 因为返回 `i32` 而不是状态码，
自己单独 `catch_unwind`，panic 返回 70。

### 为什么用字符串前缀分类

解释器路径通篇是 `anyhow::Error`，没有结构化错误类型。`invoke` 的错误分流靠两个稳定 marker：

```rust
fn classify_invoke_error(msg: &str) -> Z42HostStatus {
    if msg.contains("arg-count-mismatch:")        { ArgMismatch }
    else if msg.contains("uncaught exception")
         || msg.contains("undefined function")    { VmException }
    else                                          { Internal }
}
```

- `arg-count-mismatch:` 由 `ops::invoke_impl` 在**任何其他检查之前**抛出，所以它优先级最高。
- `uncaught exception:` 由 `exception::format_uncaught` 钉住（`src/runtime/src/exception/mod.rs`），
  是 z42 异常输出的稳定契约。
- `undefined function` 是解释器派发期找不到符号的错误（典型场景：corelib 没解析到，
  用户代码碰 `Console.WriteLine`）。它是用户的 z42 程序可见的运行期失败，归 `VmException` 而不是 `Internal`。

为单个分类引入领域错误类型的成本远大于一个稳定 marker——这是明知丑但划算的取舍。
**代价**：改这几条消息文本等于改 ABI 行为，改之前先看 `host_tests.rs`。

### `ERR_VERIFICATION` 是个空枚举值

`Z42HostStatus::Verification`(11) 在运行时**没有任何发射点**。
`verify_constraints` 失败在 `ops::build_host_module` 里被 `.context()` 包成普通 `Err`，
到 `z42_host_load_zbc` 统一映射成 `BadZbc`(10)。Tier 2 的 `translate_status` 里那个分支是死代码。
要么给它接上发射点，要么从 ABI 里删——现状是两头不靠。

### 被接受但被忽略的配置字段

- `heap_initial_bytes` / `heap_max_bytes` 只进 `ResolvedConfig` 和 Debug 输出，**没有消费点**。
- `exec_mode` 只做 `from_raw` 合法性校验 + `check_feature_available`（feature 没开就报 `ERR_FEATURE_OFF`）。
  校验之后没人再看它：句柄式 invoke 永远走 `interp::run_returning`。
  （`z42_host_run_app` 是另一条路，用 `app::default_mode()`，与 `Z42HostConfig` 无关。）

这两处的现状写进了契约页的「当前不支持」表。要真正接上，落点分别是 GC 配置与 `vm::run` 的后端选择。

---

## 值编组

`marshal.rs` 目前只处理四个 tag：`NULL` / `I64` / `F64` / `BOOL`，两个方向对称。
其他 tag 一律 `bail!`，由 `mod.rs` 映射成 `ERR_ARG_MISMATCH`。
`None`（void 返回）编组成 NULL tag，所以宿主的 `out_result` 永远拿到一个有定义的值。

字符串 / 数组 / 对象要跨边界，需要的不只是 marshal 的分支，而是一套跨调用的生命周期方案
（GC 句柄要不要暴露到嵌入 API surface），这是没做的主要原因。

---

## Tier 2：`z42-host` crate

Tier 2 是 Tier 1 的 Rust 安全封装：所有 `unsafe` 关在 crate 内部，对外是 `Result` + RAII。

```rust
pub struct HostConfig {
    pub exec_mode:     ExecMode,
    pub heap_initial:  Option<usize>,
    pub heap_max:      Option<usize>,
    pub stdout:        Option<Box<dyn Fn(&[u8]) + Send + Sync + 'static>>,
    pub stderr:        Option<Box<dyn Fn(&[u8]) + Send + Sync + 'static>>,
    pub search_paths:  Vec<PathBuf>,
    pub zpkg_resolver: Option<Arc<dyn ZpkgResolver>>,
}

impl Host {
    pub fn new(cfg: HostConfig) -> Result<Self, HostError>;
    pub fn load_zbc(&self, bytes: &[u8]) -> Result<Module, HostError>;
    pub fn load_zbc_path<P: AsRef<Path>>(&self, path: P) -> Result<Module, HostError>;
    pub fn resolve_entry(&self, m: &Module, fqn: &str) -> Result<Entry, HostError>;
    pub fn invoke(&self, e: &Entry, args: &[Value]) -> Result<Value, HostError>;
}   // Drop 自动 shutdown
```

另有三组自由函数与类型：`read_zpkg_namespaces()`（读 NSPC，无需 VM）、
`run_app()`（一次性跑 app，对应 C 的 `z42_host_run_app`）、
内置 resolver `MapResolver`（`HashMap` eager，移动 / wasm 用）与 `SearchPathsResolver`（包文件系统扫描）。

三处**不是**照抄 Tier 1 的地方：

1. **闭包 sink 的跳板**。`HostConfig.stdout` 是 `Box<dyn Fn>`，C 那边只认函数指针 + `user_data`。
   crate 把闭包装进 `Box<SinkBox>`、由 `Host` 持有保活，`user_data` 传 `&*SinkBox`，
   `sink_trampoline` 反解引用后调闭包。
2. **两个 sink 各自的 user_data**。Tier 1 的 `Z42HostConfig` 只有一个 `sink_user_data`，
   于是 `Host::new` 先用 stdout 的那个完成 `initialize`，随后立刻调 `z42_host_set_stderr_sink`
   把 stderr 的 `user_data` 换成它自己的 `SinkBox`。
3. **Rust resolver 不走 C 往返**。`cfg.zpkg_resolver` 在 `initialize` 成功后经
   `install_zpkg_resolver(Arc<dyn ZpkgResolver>)` 直接装进运行时状态，绕开 `CHookResolver` 适配层。
   这三步里任何一步失败，`Host::new` 都会先 `z42_host_shutdown` 再返回 Err，不留半初始化的 VM。

`Host` / `Module` / `Entry` 都手工 `unsafe impl Send`（内部真正的同步在运行时那把 `RwLock` 里，
句柄本身只是 sentinel），但都**不是 `Sync`**——与「调用串行化由宿主负责」的契约一致。

---

## Tier 3：平台 facade

facade 的 API 细节由各平台自己定，这里只规定最小语义契约：

- 暴露一个 `Host` 型的类（Swift `class` / Kotlin `class` / TS `class`）；
- 至少支持：从 `Data` / `ByteArray` / `Uint8Array` 加载产物、按 FQN 调用、读取 stdout 字符串；
- stdout 默认 sink = 在内存累积成字符串（移动平台没有真 stdout）；
- 异常翻译成平台原生异常（`NSError` / `Throwable` / `Error`）；
- 资源释放绑到平台惯用的机制上（Swift `deinit` / Kotlin `AutoCloseable.close()` / Rust `Drop`）。

桌面侧还有两个直接吃 `z42_host_run_app` 的 C shell，它们是**最小可读的嵌入示例**：

- `workload/desktop/shell/testhost.c`（32 行）—— 跑 app.zpkg，`Z42_LIBS` 指 stdlib；
- `workload/desktop/shell/apphost_embed.c`（81 行）—— 自己解析可执行文件所在目录，
  跑同目录下的 `app.zpkg` + `./libs`，`z42 publish --self-contained` 直接拷它，发布时不编译。

`z42_host_run_app` 在 native 平台上**不在调用线程跑 VM**，而是 spawn 一条 16 MB 栈的
`z42-embedded-run` 线程再 join。原因：解释器在 native 调用栈上递归（一次 z42 调用一个 native 帧），
而 Android 的 `AndroidJUnitRunner` 和 iOS XCTest 的线程栈只有 ~512 KB–1 MB，
桌面 8 MB 下跑得好好的深递归程序在嵌入环境里会直接 SIGSEGV 整个进程。
16 MB = 桌面默认的 2×：既然崩的用例在桌面 8 MB 下能过，≥8 MB 就够，2× 是余量；
再大没意义（桌面过不了的程序本来就得改）。64 位上这只是虚拟保留，碰到的页才提交。
wasm 是单线程、且经 `z42_wasm` 而不是这个 C 符号进来，所以那条 spawn 路径被 `cfg` 掉。

---

## 改 ABI 时的检查清单

1. `src/runtime/include/z42_host.h` 与 `src/runtime/src/host/`（`config.rs` 的 `#[repr(C)]` 镜像、
   `error.rs` 的 `#[repr(i32)]` 枚举）必须同步，**字段只追加不重排**。
2. `src/runtime/include/README.md` 的类型 / 函数清单。
3. Tier 2：`z42-host` 的 `HostConfig` / `HostError` / `translate_status`。
4. Tier 3 三家 facade 的桥接层（Swift `Z42VM.swift` 的 `cfg` 填充、
   Android `cpp/z42vm_jni.c`、wasm `wasm/platform/src/lib.rs`）。
5. `src/runtime/src/host/host_tests.rs`（状态码路径）+ `workload/desktop/tests/r1_r7.c`（外部 C 消费者）。
6. [嵌入 C ABI 契约](../../../reference/src/embedding/c-abi.md)——对外可见的任何变化都要落到那一页。
