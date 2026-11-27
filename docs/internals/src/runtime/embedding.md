# 嵌入宿主 API（VM 侧实现）

> 代码：`src/runtime/src/host/`（`ffi.rs` C 入口、`abi_config.rs` C 配置与回调适配、`error.rs` 状态码与 C 线程局部错误、
> `session.rs` 共享嵌入会话、`marshal.rs` 值编组、`rust.rs` Rust 接口）、
> `src/runtime/include/z42_host.h`（宿主声明）、`src/runtime/include/z42.h`（统一入口）、
> `src/runtime/src/corelib/io/mod.rs`（sink 路由）、`src/toolchain/workload/{desktop,ios,android,wasm}/`（Tier 3 facade）。
>
> **宿主怎么调**（函数签名、返回码、生命周期与线程约束）见
> [嵌入 C ABI 契约](https://z42-lang.github.io/z42/reference/embedding/c-abi.html)，本页不复述。

本页讲 VM **内部怎么实现**那套 ABI：三层怎么分、单例状态放在哪、句柄怎么编码、
输出怎么从解释器路由到宿主回调、执行错误类别怎样映射为状态码、zpkg 依赖按什么顺序解析。
改 `z42_host.h` 或往 `src/runtime/src/host/` 加东西之前读这一页。

反方向（native 代码把类型**注册进** z42）是另一套：见 [native-abi.md](native-abi.md)。
两者复用同一份 `Z42Value` / `Z42Args` / `Z42Error`，在同一棵头文件树下并行，不重叠。
门面在创建每个 VM 时装配核心库与 interop 扩展状态；宿主启动入口同样经过该构造路径。
owned pin buffer 在非 native 平台也保留，native 库句柄则活到其他 VM 资源销毁之后。

---

## 编译边界：host 编，mobile 跑

**z42c 是 host-only 工具**。iOS / Android / wasm 等嵌入式平台**只装 VM**，不带编译器；
mobile 端拿到的是 host 端 `z42c` 编出来的 `.zbc` / `.zpkg`，由平台 facade 在运行时 load。

后果有三条，都会在改测试基建时撞上：

- 各平台的测试资产步骤（`z42 xtask.zpkg test platform <plat> assets`，实现在
  `scripts/src/test/app/platform.z42`）把 `src/toolchain/workload/fixtures/*.z42` 在 **host 端**编成 `.zbc`，
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
│  Tier 2: Rust 嵌入 API（z42::host::rust）                     │
│    Host::new() / load_zbc / resolve_entry / invoke          │
├─────────────────────────────────────────────────────────────┤
│  Tier 1: C ABI（z42.h）                                 │
│    z42_host_* + z42_zpkg_read_namespaces + z42_host_run_app │
└─────────────────────────────────────────────────────────────┘
                              ↓
                          z42 VM
```

| 路径 | 内容 |
|------|------|
| `src/runtime/include/z42_host.h` | 宿主 C ABI。统一头 `z42.h` 汇总该接口。iOS 安装器使用 Runtime 的头文件和模块描述组装 XCFramework，Android CMake 直接使用 Runtime 的 include 目录 |
| `src/runtime/src/host/` | Rust 宿主服务及 C ABI 适配（`ffi.rs`） |
| `src/runtime/src/host/` | 统一宿主 z42 包及 Tier 2 Rust 接口 |
| `src/toolchain/workload/fixtures/` | 各平台契约测试共用的 z42 夹具（`hello.z42` / `multi_line.z42`） |
| `src/toolchain/workload/{desktop,ios,android,wasm}/platform/` | Tier 3 facade |
| `src/toolchain/workload/desktop/tests/r1_r7.c` | 真·外部 C 消费者：链 `libz42`，跑 R1–R7 七个契约场景。改 ABI 时这是第一道拦截 |

### 为什么是这个形状

参照 CoreCLR `coreclrhost.h`、JNI `JavaVM`、Lua `lua_State` 三家的经验，定了五条：

1. **单会话、每加载单元一 VM** —— `z42_host_initialize` 建立进程会话（配置、输出与共享解码缓存），
   `Z42HostRef` 是不透明的会话标识。每次 `z42_host_load_zbc` 返回一个**独立加载单元**：入口产物与其
   依赖闭包合并为一个链接模块，独占一个 VM（类型表 / 静态字段 / 派发缓存互不可见）。同一加载单元
   内的多个 zpkg 天然共享（装载时已合并）；**依赖的 zpkg 若已被会话内任一加载单元装入，则按内容
   共享解码定义**（C# ALC「已加载即共享」规则，`loader::ImageCache`）——函数 IR 等定义数据在会话
   内只保留一份，每 VM 状态不共享（对应不同 ALC 各自的 RuntimeTypes）。多 VM 句柄化仍按
   docs/design/runtime/embedding.md §12 Deferred 跟踪。
2. **三层 ABI** —— 与 [native-abi.md](native-abi.md) 同构：Tier 1 稳定 C ABI；Tier 2 Rust 人因工程；Tier 3 平台 facade。
3. **AOT 友好** —— 入口按 FQN 字符串查找，运行时不依赖反射元数据生成器。iOS 禁 JIT 的场景下走 interp。
4. **零拷贝优先** —— 标量值通过 `Z42Value` 直接传递，不做自动编组。
5. **panic 隔离** —— 任何 z42 异常 / Rust panic 都不跨 FFI 线，统一翻译成 `Z42HostStatus` + `Z42Error`。

---

## 单例状态与句柄编码

Rust 和 C 两个适配层共用 `session.rs` 的会话操作。会话持有配置、解码缓存、模块和入口注册表。
会话与输出回调在同一次加锁中发布或撤销。初始化会先在选定的 VFS 上验证 corelib，失败时不改动已发布会话。

`Z42HostRef`、`Z42ModuleRef` 和 `Z42EntryRef` 都是不透明的非零标识，运行时不会解引用它们。
进程内发出的标识不会复用。即使关闭后马上建立新会话，原句柄也返回 `ERR_NOT_INIT`。

- 加载程序、验证新 VFS 时先取得会话快照，再在锁外执行 I/O 和回调。取回写锁时只有当前会话仍匹配才发布结果。
- 入口查找与登记在同一临界区完成，仅检查已装配程序的入口索引。
- 调用前取得程序的 `Arc` 并释放锁。回调可以在调用中关闭会话，此次调用持有的程序仍然可用。
- 关闭、替换 VFS 或 resolver 时，在释放会话锁后析构被替换的资源，允许析构回调重入。

### 每个 module 一个 VmContext

`LoadedProgram` 持 `Pin<Box<VmContext>>`，模块本身由 ctx 拥有。
这样并发 load 两个产物不会互相污染静态状态。

`SessionConfig::load_options` 将会话配置转换为 `LoadOptions`。会话通过 `LoadedProgram::load_bytes_with_cache` 加载程序，与文件应用共用启动流程。C 宿主启用依赖预加载，以便在执行前解析依赖中的入口；resolver 优先于文件搜索。

初始化由 `LoadedProgram::initialized` 保证每个程序只准备一次，失败保持终态；静态字段、初始化器及 VM 资源在后续调用之间保留。调用参数检查、执行模式选择、异常和 fatal 收尾由统一调用服务负责，C 层只转换结果。

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

- **C 函数指针 + user_data** 由 `host/abi_config.rs` 的 `CHookResolver` 适配到同一个 loader 接口。
  原始指针回调在返回后复制字节，宿主持有缓冲到复制结束。写入回调在宿主提供的缓冲借用期间同步复制。
  Swift 在 `withUnsafeBytes` 作用域内提交数据，Android JNI 在释放借用的数组元素前提交。
- **Rust `Arc<dyn ZpkgResolver>`** 由 Tier 2 经 `host::install_zpkg_resolver()` 直接塞进
  `HostState::config`，不绕 C 回调。这个函数**不是** `extern "C"`，是 Tier 2 专用的逃生口。

### Tier 3 各平台的默认 resolver

平台 facade 自己的「namespace → 字节」表**由读 zpkg 的 `NSPC` section 派生**，没有索引文件。
一份 zpkg 通常提供多个 namespace（`z42.core.zpkg` 同时 ship `z42.core` / `Std` / `Std.Exceptions` …），
所以不能假设 `namespace == 文件名`；手维护的 `index.json` 会是 `NSPC` 之外的**第二真相源**、
必然漂移，所以不用。

| 平台 | 默认 resolver | 位置 |
|---|---|---|
| iOS | `BundleZpkgResolver(bundle: .main, subdirectory: "stdlib")` —— 枚举 `Bundle.urls(forResourcesWithExtension:"zpkg",subdirectory:)`，逐个读 NSPC 建表。`Z42VM.init` 的**默认参数**，不传就自动装 | `ios/platform/Sources/Z42VM/ZpkgResolver.swift` |
| Android | `AssetZpkgResolver(assets, subdir = "stdlib")` —— `AssetManager.list` 枚举，逐个经 `Z42VM.readNamespaces`（JNI 桥到同一个 C ABI）读 NSPC。另有 `MapZpkgResolver`。`Z42VM` 构造器里 resolver 是**必填参数** | `android/.../io/z42/vm/ZpkgResolver.kt` |
| wasm | `bundleStdlibNode(readNamespaces)` / `bundleStdlibBrowser(baseUrl, readNamespaces)` / `mapResolver(map)` —— Node 侧 `readdir`，浏览器侧 fetch 构建期生成的 `files.json`（HTTP 枚举不了目录，这是文件名清单的派生替身，**不是** namespace 映射） | `wasm/platform/js/stdlib-resolver.js` |

包解析键统一由 `loader::program::read_zpkg_namespaces` 读取：
C ABI `z42_zpkg_read_namespaces`、Rust `z42::host::read_zpkg_namespaces` 与 `z42::host::rust::read_zpkg_namespaces`、
wasm 导出 `readNamespaces` / Android JNI `Z42VM.readNamespaces`。
Host 的 C 入口转译状态码和回调，并更新宿主线程局部错误。Swift、Kotlin 和 JS 共用这条解析路径。

`./xtask build stdlib` 产 flat view 时不写任何索引文件；各平台 `build.sh` 只拷 `*.zpkg`。

---

## 输出路由：从解释器到宿主 sink

输出回调保存在 `corelib::io` 的进程级槽位。会话初始化发布两个槽位，`set_*_sink` 可单独替换，关闭会话时清空。
C 槽位持有函数指针与宿主管理的 `user_data`，Rust 槽位持有 `Arc` 闭包。
路由复制回调并释放锁后才调用它，Rust 闭包在调用期间保持存活。

仅当前宿主调用线程启用回调路由，`HostSinkGuard` 退出时恢复进入前的线程局部标志。
有回调时优先交付宿主，否则交给 test-IO 捕获栈或进程输出。程序创建的 worker 沿用自己的输出路由。

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

### 入口执行错误

`exec::invoke_entry` 在参数检查、引擎调用和异常返回处产生类型化的 `EntryError`。
`host::session` 将参数不符、未捕获的托管异常、fatal 和内部错误分别映射到
`ERR_ARG_MISMATCH`、`ERR_VM_EXCEPTION`、`ERR_FATAL` 和 `ERR_INTERNAL`。
缺失符号由执行层产生托管异常，因此随未捕获异常走同一条路径。
错误消息和栈迹继续用于诊断，托管异常的消息可以包含任意文本。

### `ERR_VERIFICATION` 是个空枚举值

`Z42HostStatus::Verification`(11) 在运行时**没有任何发射点**。
`verify_constraints` 在加载阶段执行。会话将该阶段的失败统一映射成 `BadZbc`(10)，并保留详细诊断。

### 被接受但被忽略的配置字段

- `heap_initial_bytes` / `heap_max_bytes` 只进 `ResolvedConfig` 和 Debug 输出，**没有消费点**。
- `exec_mode` 在宿主初始化时仅经合法性和构建/目标支持校验，来源装入与 ABI 校验在程序准备时完成；模式存入 `LoadedProgram`，调用时选择对应引擎。Default 使用构建默认后端；JIT 支持参数和返回值，AOT 尚未实现，会在执行时明确报错。一次性 `z42_host_run_app` 使用构建默认模式。

堆容量字段的限制见契约页「当前不支持」；执行模式由统一宿主服务处理。

---

## 值编组

`marshal.rs` 目前只处理四个 tag：`NULL` / `I64` / `F64` / `BOOL`，两个方向对称。
其他 tag 一律 `bail!`，由 `ffi.rs` 映射成 `ERR_ARG_MISMATCH`。
`None`（void 返回）编组成 NULL tag，所以宿主的 `out_result` 永远拿到一个有定义的值。

字符串 / 数组 / 对象要跨边界，需要的不只是 marshal 的分支，而是一套跨调用的生命周期方案
（GC 句柄要不要暴露到嵌入 API surface），这是没做的主要原因。

---

## Tier 2：`z42::host::rust` 接口

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
    pub vfs:           Option<Arc<dyn Vfs>>,   // VM 的文件系统；None = pal::fs::default_vfs()
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

Rust 接口直接调用共享会话操作。应用提供的 VFS、resolver 与两个独立的 Rust 输出闭包在会话发布前准备好。
失败以 `HostError` 返回，C API 的线程局部最近错误由 C 适配层独立维护。
`Host` 持有会话标识，Drop 只关闭该会话。`Host`、`Module`、`Entry` 可在线程之间移动，调用仍由宿主串行驱动。

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

`z42_host_run_app` 和 Rust 一次性入口共享应用侧车、VFS 默认值和搜索目录的准备。
C 入口在启动 VM 线程前解析进程配置，专用栈大小取 `thread-stack-bytes`，默认 16 MiB。
`z42_host_run_app` 在 native 平台上 spawn 一条
`z42-embedded-run` 线程再 join。原因：解释器在 native 调用栈上递归（一次 z42 调用一个 native 帧），
而 Android 的 `AndroidJUnitRunner` 和 iOS XCTest 的线程栈只有 ~512 KB–1 MB，
桌面 8 MB 下跑得好好的深递归程序在嵌入环境里会直接 SIGSEGV 整个进程。
16 MB = 桌面默认的 2×：既然崩的用例在桌面 8 MB 下能过，≥8 MB 就够，2× 是余量；
再大没意义（桌面过不了的程序本来就得改）。64 位上这只是虚拟保留，碰到的页才提交。
wasm 是单线程、且经 `z42_wasm` 而不是这个 C 符号进来，所以那条 spawn 路径被 `cfg` 掉。

---

## 改 ABI 时的检查清单

1. `src/runtime/include/z42_host.h` 与 `src/runtime/src/host/`（`abi_config.rs` 的 `#[repr(C)]` 镜像、
   `error.rs` 的 `#[repr(i32)]` 枚举）必须同步，**字段只追加不重排**。追加到 `Z42HostConfig` 的字段
   只在 `struct_size` 覆盖到时才读（`abi_config::validate`），`MIN_CONFIG_SIZE` 固定为原始 v2 前缀大小。
2. `src/runtime/include/README.md` 的类型 / 函数清单。
3. Tier 2：`z42::host::rust` 的 `HostConfig` / `HostError` / `translate_status`。
4. Tier 3 三家 facade 的桥接层（Swift `Z42VM.swift` 的 `cfg` 填充、
   Android `cpp/z42vm_jni.c`、wasm `wasm/platform/src/lib.rs`）。
5. `src/runtime/src/host/embedding_tests.rs`（状态码路径）+ `workload/desktop/tests/r1_r7.c`（外部 C 消费者）。
6. [嵌入 C ABI 契约](https://z42-lang.github.io/z42/reference/embedding/c-abi.html)——对外可见的任何变化都要落到那一页。

## CLI 与嵌入端共用的 Rust 服务

`host::LoadedProgram` 封装 VM 程序会话，持有已装载模块及其固定地址上下文。`load_file` 与 `load_bytes` 共用 `loader/program.rs` 的包准备流程，经 resolver 和搜索目录准备 prelude、依赖和惰性候选，再由 `vm/assembly.rs` 完成可用性裁剪、初始化器登记和链接。

`LoadOptions` 显式携带 VFS、搜索目录、resolver、执行模式及依赖预加载策略。字节输入没有隐式入口目录；文件应用按入口目录、配置 probing 路径、标准库目录构造搜索顺序。需要在调用前解析依赖包入口的 C 宿主启用预加载；应用默认按需加载。两种策略共用解析与合并实现。

`LoadedProgram::invoke` 统一校验参数数量、安装可嵌套恢复的活动 VM/堆守卫、初始化、按模式执行及异常收尾。初始化状态由程序持有，只执行一次，准备失败保持终态。JIT 调用保留参数和返回值，并复用该 VM 的编译器及代码生命周期；不可用模式返回错误。

`host::run_app` 在同一服务上提供一次性应用运行，CLI 只解析参数及处理进程退出。C ABI 位于 `host/ffi.rs`，将句柄映射到 `Arc<LoadedProgram>`；加载前复制配置并释放注册表锁，调用前取得程序强引用并释放锁，宿主 resolver 或输出回调可以重入而不持有宿主注册表锁。


## 可选原生组件

Rust 宿主通过 LoadOptions 表达执行模式、native 来源许可/搜索目录和 allow_profiling，不指定组件 ID 或工厂。VM 的私有 ComponentInputs 负责装配，内部测试通过专用入口注入提供方。

loader 分别校验静态链接、共享链接和运行时加载来源。配置在 VM 发布前冻结；命中进程代码缓存或共享工厂缓存也要重新检查当前 VM 的许可，拒绝不污染后续 VM 的工厂缓存。

最终链接胶水使用 components::abi 的 register_static_component 或 register_shared_component；原生模块使用 loader::linked 的对应登记入口。登记不执行工厂或符号解析器。只有实际使用的能力/模块才装入或创建实例。

组件管理查询只存在于内部测试构建。生产 VM 通过能力入口获取接口；租约保留 Core、实例与代码，实例按依赖逆序销毁，不支持热卸载。组件协议与所有权见 [组件化运行时](componentized-runtime.md)。
