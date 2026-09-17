# Embedding API（宿主嵌入 API）

> **Status**: Design Draft（2026-05-10）。MVP 目标 = Hello World：宿主 app 启动 VM、加载一个 `.zbc`、调用入口、捕获 stdout、关闭。
>
> 本文与下列规范的关系：
>
> - [native-abi.md](native-abi.md) 解决 **native 代码 → 注册类型/方法进 z42**（"扩展语言"）。本文解决 **宿主 app → 启动并驱动 VM**（"嵌入运行时"）。两者复用同一份 `Z42Value` / `Z42Args` 类型，互不重叠。
> - [cross-platform.md](cross-platform.md) 决定 VM **如何编译**到 ios/android/wasm。本文决定编译产物**如何被宿主调用**。
> - [cross-platform-testing.md](../../../design/testing/cross-platform-testing.md) 的 test-runner 是本 API 的**首要消费者**之一；MVP 之后 runner 将基于本 API 重构（见 §12 Deferred）。

---

## §0 编译边界（host 编 / mobile 跑）

**z42c 是 host-only 工具**。iOS / Android / wasm 等嵌入式平台**只装 VM**，不带编译器；mobile 端拿到的是 host 端 `z42c` 编出来的 `.zbc` / `.zpkg`，由平台 facade 在运行时 load。原则在自举完成（compiler 用 z42 写）之前不会变。

后果：

- 各 platform 的测试资产步骤（`xtask test platform <plat> assets`）把 `src/toolchain/workload/fixtures/*.z42` 在 **host 端**用 `z42c` 编出 `.zbc`，复制进 `Z42VM.xcframework/Resources/` / `z42vm/src/main/assets/` / `pkg-{web,nodejs}/`。
- 平台 facade 的 test harness（XCTest / JUnit / playwright）**只 load 预编 `.zbc`**，测试代码**不调用 `z42c` / `dotnet`**。
- 该约束是 v0.1 facade test 契约的硬性前提，详见 `docs/spec/archive/<date>-define-platform-test-contract/specs/platform-test-contract/spec.md`。

---

## §1 设计目标

宿主嵌入场景：

1. **iOS / Android app** 内嵌 z42 运行业务脚本或测试用例
2. **桌面 IDE 插件**（VSCode 扩展）调用 z42 评估表达式
3. **CI / 测试 runner** 在不 fork 子进程的前提下批量跑 `.zbc`
4. **C/C++/Rust/Go 原生应用**通过稳定 ABI 接入 z42

## §2 设计原则

参照 CoreCLR `coreclrhost.h`、JNI `JavaVM`、Lua `lua_State` 三家的经验，确立五条：

1. **单实例（v0.1）** — 每进程一份 VM 状态。`Z42HostRef` 是占位 handle，所有调用复用同一全局 context。多实例 / ALC / Isolated context 进 Deferred。
2. **三层 ABI**（与 [native-abi.md §1](native-abi.md) 同构） — Tier 1 稳定 C ABI；Tier 2 Rust 人因工程；Tier 3 平台 facade（Swift / Kotlin / JS）。
3. **AOT 友好** — 入口解析按 FQN（fully qualified name）字符串查找，运行时不依赖反射元数据生成器。iOS 禁 JIT 场景下走 interp 或 AOT。
4. **零拷贝优先** — 标量值通过 `Z42Value` 直接传递；`String` / `Array<T>` 通过 `pinned` 块跨边界（沿用 [native-abi.md §5](native-abi.md) 的 `PinnedView`）。
5. **panic 隔离** — 任何 z42 异常 / Rust panic 不跨 FFI 线；统一翻译为 `Z42HostStatus` 错误码 + `Z42Error` 详情。

---

## §3 架构

```
┌─────────────────────────────────────────────────────────────┐
│  Tier 3: 平台 facade                                          │
│    Swift Package (z42)        → iOS app                     │
│    Kotlin AAR (z42-android)   → Android app                 │
│    npm package (@z42/wasm)    → 浏览器 / Node.js             │
├─────────────────────────────────────────────────────────────┤
│  Tier 2: Rust 嵌入 API                                       │
│    z42_host::Host::new()      → 应用 / 内部测试 runner       │
├─────────────────────────────────────────────────────────────┤
│  Tier 1: C ABI（z42_host.h）                                 │
│    z42_host_initialize / load_zbc / invoke / shutdown       │
└─────────────────────────────────────────────────────────────┘
                              ↓
                    z42 VM（Interp / JIT / AOT）
```

代码归属：

| 路径 | 内容 |
|------|------|
| `src/runtime/include/z42_host.h` | Tier 1 C 头文件（与 `z42_abi.h` 平行） |
| `src/runtime/src/host/` | C ABI 在 VM 内的实现（Rust `extern "C"`） |
| `src/toolchain/workload/host-api/` | Tier 2 Rust crate（`z42-host`）—— consolidate-platform-into-workload S1 迁此 |
| `src/toolchain/workload/fixtures/` | 各平台 R1–R7 契约测试共用的 z42 夹具（`hello.z42` / `multi_line.z42`）；面向用户的 C / Rust 嵌入示例由学习手册嵌入章节提供 |
| `src/toolchain/workload/{ios,android,wasm}/platform/` | Tier 3 facade（与 P4.x spec 协同） |

---

## §4 Tier 1 C ABI（`z42_host.h`）

### 4.1 句柄与值类型

```c
/* 不透明句柄。v0.1 全部是进程单例的占位指针；多实例时升级为真句柄。 */
typedef struct Z42Host*   Z42HostRef;     /* VM 实例 */
typedef struct Z42Module* Z42ModuleRef;   /* 已加载的 .zbc */
typedef struct Z42Entry*  Z42EntryRef;    /* 解析后的入口（方法/函数） */

/* Z42Value / Z42Args 复用 z42_abi.h，不重新定义 */
```

### 4.2 初始化配置

```c
typedef enum Z42ExecMode {
    Z42_EXEC_MODE_DEFAULT = 0,   /* 由 .zbc 元数据 + 编译时 feature 决定 */
    Z42_EXEC_MODE_INTERP  = 1,
    Z42_EXEC_MODE_JIT     = 2,   /* feature=jit 关闭时初始化失败 */
    Z42_EXEC_MODE_AOT     = 3,   /* feature=aot 关闭时初始化失败 */
} Z42ExecMode;

/* stdout/stderr sink 回调。length 不含 NUL，sink 不应假设 NUL 结尾。 */
typedef void (*Z42WriteSink)(const char* bytes, size_t length, void* user_data);

/* zpkg resolver hook — 详见 §11. */
typedef int (*Z42ZpkgResolverFn)(
    const char* namespace_name,
    const uint8_t** out_bytes, size_t* out_length,
    void* user_data);

typedef struct Z42HostConfig {
    uint32_t      abi_version;        /* = Z42_HOST_ABI_VERSION */
    uint32_t      reserved;

    Z42ExecMode   exec_mode;
    size_t        heap_initial_bytes; /* 0 = 默认（VM 决定） */
    size_t        heap_max_bytes;     /* 0 = 不限 */

    Z42WriteSink  stdout_sink;        /* NULL = 真 stdout */
    Z42WriteSink  stderr_sink;        /* NULL = 真 stderr */
    void*         sink_user_data;

    /* 模块搜索路径（NULL 结尾的 C 字符串数组）。NULL = 仅 in-memory load。 */
    const char* const* search_paths;

    /* 2026-05-11 append-only：zpkg resolver hook + 用户数据。NULL = 无 hook。 */
    Z42ZpkgResolverFn  zpkg_resolver;
    void*              zpkg_resolver_user_data;
} Z42HostConfig;

#define Z42_HOST_ABI_VERSION 1
```

### 4.3 状态码

```c
typedef enum Z42HostStatus {
    Z42_HOST_OK                  = 0,
    Z42_HOST_ERR_ALREADY_INIT    = 1,   /* 单实例：重复 initialize */
    Z42_HOST_ERR_NOT_INIT        = 2,
    Z42_HOST_ERR_BAD_CONFIG      = 3,   /* abi_version 不匹配 / 配置非法 */
    Z42_HOST_ERR_FEATURE_OFF     = 4,   /* JIT/AOT mode 但 feature 关闭 */
    Z42_HOST_ERR_BAD_ZBC         = 10,  /* magic / 校验失败 */
    Z42_HOST_ERR_VERIFICATION    = 11,  /* IR 校验失败 */
    Z42_HOST_ERR_ENTRY_NOT_FOUND = 20,
    Z42_HOST_ERR_ARG_MISMATCH    = 21,  /* 参数数量/类型不匹配 */
    Z42_HOST_ERR_VM_EXCEPTION    = 30,  /* z42 throw 跨出顶层 */
    Z42_HOST_ERR_INTERNAL        = 99,  /* Rust panic 等 */
} Z42HostStatus;
```

### 4.4 生命周期 API

```c
/* 进程生命周期内仅可成功一次（v0.1）。线程安全。 */
Z42HostStatus z42_host_initialize(const Z42HostConfig* cfg, Z42HostRef* out_host);

/* 加载 .zbc 字节流。bytes 在调用期间必须存活；VM 按需 copy。 */
Z42HostStatus z42_host_load_zbc(
    Z42HostRef host,
    const uint8_t* bytes, size_t length,
    Z42ModuleRef* out_module);

/* 按 FQN 解析入口。例 "examples.hello::Main" 或 "examples.hello.Greeter::greet"。 */
Z42HostStatus z42_host_resolve_entry(
    Z42HostRef host, Z42ModuleRef module,
    const char* fqn,
    Z42EntryRef* out_entry);

/* 同步调用入口。args/n 与签名匹配；result 可为 NULL 表示不取返回值。 */
Z42HostStatus z42_host_invoke(
    Z42EntryRef entry,
    const Z42Value* args, size_t n,
    Z42Value* out_result);

/* 详细错误信息（线程局部）。每次成功调用清空。 */
Z42Error z42_host_last_error(Z42HostRef host);

/* 释放整个 VM。Module/Entry 句柄随之失效；shutdown 后可重新 initialize。 */
Z42HostStatus z42_host_shutdown(Z42HostRef host);
```

### 4.5 ABI 演化规则（沿用 Tier 1 的演进规则）

- `abi_version` 字段保持 offset 0；新版本只 append 字段
- VM 按 `abi_version`-aware 大小读取 `Z42HostConfig`，不假设布局
- 主版本号变更 = 显式 break，semver-major

---

## §5 Tier 2 Rust API（`src/toolchain/workload/host-api/`）

最小 surface（v0.1）：

```rust
// crate: z42-host

pub struct Host { /* opaque */ }

pub struct HostConfig {
    pub exec_mode:        ExecMode,
    pub heap_initial:     Option<usize>,
    pub heap_max:         Option<usize>,
    pub stdout_sink:      Option<Box<dyn Fn(&[u8]) + Send + Sync>>,
    pub stderr_sink:      Option<Box<dyn Fn(&[u8]) + Send + Sync>>,
    pub search_paths:     Vec<PathBuf>,
}

impl Host {
    pub fn new(cfg: HostConfig) -> Result<Self, HostError>;

    pub fn load_zbc(&self, bytes: &[u8]) -> Result<Module, HostError>;
    pub fn load_zbc_path(&self, path: &Path) -> Result<Module, HostError>;

    pub fn resolve_entry(&self, m: &Module, fqn: &str) -> Result<Entry, HostError>;
    pub fn invoke(&self, e: &Entry, args: &[Value]) -> Result<Value, HostError>;
}

// Drop 自动 shutdown
```

Tier 2 是 Tier 1 的 Rust 安全封装：所有 unsafe 隔离在 `z42-host` 内部，对外是 `Result` + RAII。

---

## §6 Tier 3 平台 facade

各平台 facade 落在对应 `add-platform-{ios,android,wasm}` spec 的 host 子段；本文只规定 facade 的最小语义契约，不规定 API 细节。

### 6.1 共同语义

- 暴露 `Host` 类（Swift `class`、Kotlin `class`、TS `class`）
- 至少支持：从 `Data` / `ByteArray` / `Uint8Array` 加载 `.zbc`、按 FQN 调用、读取 stdout 字符串
- stdout 默认 sink = 在内存累积成字符串（移动平台无真 stdout）
- 异常翻译为平台原生异常（`NSError` / `Throwable` / `Error`）

### 6.2 具体 API 形态

留待各 P4.x spec 拍板。本文只保证 Tier 1 ABI 足以让 facade 实现上述语义。

---

## §7 生命周期与线程模型

- **初始化**：`z42_host_initialize` 进程内单次。重复调用返回 `ERR_ALREADY_INIT`。
- **线程**：v0.1 假定**调用串行化**。多线程并发调用同一 `Z42EntryRef` 的行为未定义，由宿主负责加锁。后续是否做内置 mutex 进 Deferred。
- **回调线程**：`Z42WriteSink` 在调用 `z42_host_invoke` 的同一线程被同步触发。
- **关闭**：`shutdown` 释放 VM 全部状态（heap、模块表、JIT cache）。任何在途调用必须先返回。

---

## §8 stdout / stderr 重定向

iOS / Android 没有真 stdout，必须重定向。

- VM 内 `Console.WriteLine` 等输出统一走 `Z42WriteSink`
- `sink == NULL` 时退化为平台 stdout（桌面）
- 移动 facade 默认绑一个**累积型 sink**，调用结束后 `host.lastStdout()` 取字符串
- 二进制安全：sink 接 `(bytes, length)`，不假设 UTF-8

实现侧：`src/runtime/src/native/io.rs` 现已有 stdout writer 抽象，扩展支持 sink 注入。

---

## §9 Hello World 示例

### 9.1 z42 源（`hello.z42`）

```z42
namespace examples.hello;

public static class Greeter {
    public static int Greet(string name) {
        Console.WriteLine($"Hello, {name}!");
        return 0;
    }
}
```

编译：`z42c --emit-zbc hello.z42 hello.zbc`

### 9.2 C 宿主

```c
#include "z42_host.h"
#include "z42_abi.h"
#include <stdio.h>

static void on_stdout(const char* b, size_t n, void* _) {
    fwrite(b, 1, n, stdout);
}

int main(void) {
    Z42HostConfig cfg = {
        .abi_version = Z42_HOST_ABI_VERSION,
        .exec_mode   = Z42_EXEC_MODE_DEFAULT,
        .stdout_sink = on_stdout,
    };
    Z42HostRef host;
    if (z42_host_initialize(&cfg, &host) != Z42_HOST_OK) return 1;

    /* ... read hello.zbc into bytes/len ... */
    Z42ModuleRef mod;
    z42_host_load_zbc(host, bytes, len, &mod);

    Z42EntryRef entry;
    z42_host_resolve_entry(host, mod, "examples.hello.Greeter::Greet", &entry);

    Z42Value name = z42_value_string("World");   /* helper from z42_abi.h */
    Z42Value result;
    z42_host_invoke(entry, &name, 1, &result);

    z42_host_shutdown(host);
    return 0;
}
```

### 9.3 Rust 宿主

```rust
use z42_host::{Host, HostConfig, ExecMode, Value};

fn main() -> anyhow::Result<()> {
    let cfg = HostConfig {
        exec_mode:   ExecMode::Default,
        stdout_sink: Some(Box::new(|b| std::io::stdout().write_all(b).unwrap())),
        ..Default::default()
    };
    let host  = Host::new(cfg)?;
    let m     = host.load_zbc_path("hello.zbc".as_ref())?;
    let entry = host.resolve_entry(&m, "examples.hello.Greeter::Greet")?;
    host.invoke(&entry, &[Value::string("World")])?;
    Ok(())
}
```

### 9.4 Swift / Kotlin

详细 API 形态进 `add-platform-ios` / `add-platform-android` spec。本文只承诺：MVP 完成后，两平台都能跑通"加载 hello.zbc → invoke → 取 stdout 字符串 → 断言 == 'Hello, World!\n'"。

---

## §10 错误处理

### 设计原则

- 所有 API 返回 `Z42HostStatus`；详细信息走 `z42_host_last_error`（线程局部）
- 成功路径必须 clear last_error；失败路径必须 set
- z42 端 `throw` 跨出顶层 → `ERR_VM_EXCEPTION`，错误信息含异常类型 + message
- Rust panic（不应该发生，但兜底）→ `ERR_INTERNAL`
- ABI 不暴露异常对象本身（v0.1）；后续 catch-from-host 进 Deferred

### 状态码 → 触发条件（H1–H3 实测）

| 状态码 | 触发条件 | 测试 |
|--------|---------|------|
| `OK` (0) | 任何 API 成功；副作用：`last_error.code = 0` | 所有 happy-path 测试 |
| `ERR_ALREADY_INIT` (1) | `initialize` 在已初始化状态再次调用 | `initialize_twice_returns_already_init` |
| `ERR_NOT_INIT` (2) | 任何 API 在未初始化状态调用（含 stale handle） | `shutdown_when_not_initialized_returns_not_init` / `load_zbc_before_init_returns_not_init` |
| `ERR_BAD_CONFIG` (3) | `cfg == NULL` / `abi_version` 不匹配 / 未知 `exec_mode` / `search_path` 含 NUL | `null_config_returns_bad_config` / `bad_abi_version_returns_bad_config` / `unknown_exec_mode_returns_bad_config` |
| `ERR_FEATURE_OFF` (4) | 请求 JIT/AOT 但对应 feature 编译时关闭（[cross-platform.md](cross-platform.md)） | `jit_mode_when_feature_off_returns_feature_off` |
| `ERR_BAD_ZBC` (10) | bytes 长度 < 4 / magic 不匹配 / `read_zbc` 解析失败 | `load_zbc_with_garbage_bytes_returns_bad_zbc` |
| `ERR_VERIFICATION` (11) | IR 校验失败（暂未独立测试覆盖；通过 `verify_constraints` 抛出） | — |
| `ERR_ENTRY_NOT_FOUND` (20) | FQN 不在 `module.func_index` / module handle 是 NULL | `resolve_entry_unknown_fqn_returns_entry_not_found` |
| `ERR_ARG_MISMATCH` (21) | `args.len() != func.param_count`（v0.1 仅检查数量，类型在 H4+ 引入完整 marshal 后扩展） | `invoke_arg_count_mismatch_returns_arg_mismatch` |
| `ERR_VM_EXCEPTION` (30) | z42 `throw` 跨出 invoke 顶层（错误消息以 `"uncaught exception:"` 开头，由 `exception::format_uncaught` 生成） | `z42_throw_escapes_as_vm_exception_with_message` |
| `ERR_INTERNAL` (99) | Rust panic 经 `catch_unwind` / 锁中毒 / 其他未分类错误 | （兜底，无单独测试用例） |

### 错误消息分类机制（实施细节）

`host::ops::invoke_impl` 把"参数数量不符"先于其他错误检查，并以前缀 `arg-count-mismatch:` 抛出 `anyhow::Error`。`host::mod::classify_invoke_error` 按字符串前缀分流：

```rust
fn classify_invoke_error(msg: &str) -> Z42HostStatus {
    if msg.contains("arg-count-mismatch:")     { Z42HostStatus::ArgMismatch }
    else if msg.contains("uncaught exception") { Z42HostStatus::VmException }
    else                                       { Z42HostStatus::Internal }
}
```

为何用字符串前缀而不是结构化 enum：interp 路径已是 `anyhow::Error`-only；为单个分类增加领域错误类型成本远大于一个稳定 marker。`uncaught exception:` marker 由 `exception::format_uncaught` 钉住（参见 `src/runtime/src/exception/mod.rs`），是 z42 异常输出的稳定契约。

### Sink 顺序保证

`route_stdout` / `route_stderr` 在 `HOST_SINK_ACTIVE` 设为 `true` 的线程上**同步**派发；
多次 `Console.WriteLine` 严格按调用顺序触发 sink，sink 收到的字节顺序与 z42 程序的写出顺序一致。
`sink_called_in_correct_order_for_multiple_lines` 用 3 行输出验证该保证。

---

## §11 实施里程碑

| Milestone | 内容 | 依赖 |
|-----------|------|------|
| **H0 spec** | 本文档 + docs/spec/archive/2026-05-10-add-embedding-api/ DRAFT | — |
| **H1 C ABI scaffold** | `z42_host.h` + Rust `extern "C"` 空实现 + 链接通 | H0 |
| **H2 hello-world (interp)** | initialize / load_zbc / resolve / invoke / shutdown 跑通 hello-world；sink 工作；C + Rust 两个 example | H1，interp 已就绪（M4 已完成）|
| **H3 错误路径** | 所有 `Z42HostStatus` 路径有测试覆盖；VM 异常 → `ERR_VM_EXCEPTION` | H2 |
| **H4 平台接入** | 与 P4.3 / P4.4 协同：Android JNI bridge / iOS Swift facade 调本 ABI 跑 hello-world | H3 |
| **H5 runner 重构** | test-runner library 内部改用 z42-host crate | H4 |

H1–H3 为本 spec 的实施范围；H4 / H5 由各 P4.x / runner spec 主导。

---

## §12 Deferred（明确不做的）

> 本节遵循 [feedback_deferral_location.md] 约定：所有延后写在本节，roadmap.md "Deferred Backlog Index" 横向索引。

| 项 | 推迟原因 | 触发条件 |
|----|---------|---------|
| **多 VM 实例 / ALC-like context** | 单实例足够覆盖 hello-world 与移动测试场景；多实例需要 VM 全局状态 per-handle 化，工作量大 | IDE 多 workspace 隔离需求 / hot-reload 实现 |
| **Hot reload** | 依赖多实例 + 模块卸载语义；先把 [hot-reload.md] 的命名空间级方案跑通 | 多实例落地后 |
| **GC handle 跨调用持有** | hello-world 不需要宿主缓存 z42 对象；直接复用 [gc-handle.md] 即可，但要提升到嵌入 API surface 涉及生命周期与 ABI 设计，先观望真实需求 | 出现宿主缓存 z42 service 对象的实际场景 |
| **Async / 协程式 invoke** | v0.1 同步即可；移动 UI 线程切换由宿主负责 | z42 引入 async 之后（L3） |
| **VM 内部 mutex 自动加锁** | 让宿主显式串行化，更简单 | 出现宿主自加锁开销大的实际场景 |
| **从宿主 catch z42 异常对象** | v0.1 只暴露 status code + message string；不暴露异常对象本身 | 用户反馈需要细粒度异常分支 |
| **test-runner 重构到本 API 之上** | 先完成 hello-world，再回头收敛 | H4 完成后启动 H5 |
| **Tier 3 facade API 细节** | 每平台 spec 各自落地，避免本文超载 | 进入 P4.3 / P4.4 实施 |
| **Facade threading 测试**（R8）| v0.1 runtime / facade 是单实例 + 同步 invoke，threading 还没有正式语义；现在测出来的"后台 invoke + 主线程 sink"契约会随后续 threading 设计推翻 | runtime threading 模型落地（multi-VM / async invoke / per-thread context 任一）后回到 `platform-test-contract` 补 R8 scenario |

---

## §13 与现有规范的关系

- **[native-abi.md](native-abi.md)**：本文 §4.1 复用 `Z42Value` / `Z42Args`；不重复定义。两份 ABI 在同一 `z42_abi.h` / `z42_host.h` 头文件树下并行。
- **cross-platform.md**：本文 §4.2 的 `Z42_EXEC_MODE_JIT` / `_AOT` 在对应 feature 关闭时返回 `ERR_FEATURE_OFF`，与 cross-platform.md "feature off → CLI 直接报错" 同精神。
- **cross-platform-testing.md**：runner library 在 H5 重构为本 API 的消费者；现有 platform-binding 形态不变。
- **hot-reload.md**：本文不动 hot-reload 语义；后续多实例落地时再桥接。
- **gc-handle.md**：内部 `Std.GCHandle` 不变；宿主侧 GC handle 进 Deferred。
- **vm-architecture.md**：H1 实施时同步追加"嵌入入口"小节，描述 host context 如何挂接到 VM 全局状态。

---

## §11 ZpkgResolver Hook（2026-05-11 add-zpkg-resolver-hook）

桌面端宿主用 `search_paths` 扫文件系统找 `.zpkg`；移动 / wasm 没有文件系统（或不便扫），需要回调机制让宿主告诉运行时"namespace X 的 zpkg 字节在这里"。本节定义这个 hook。

### 11.1 Tier 1 C ABI

```c
/* 返回非 0 = hit，*out_bytes / *out_length 写入字节范围；
 * 返回 0  = miss，运行时继续 fallback 到 search_paths。
 * 字节生命周期 = 仅 callback 调用期间，运行时立即复制。 */
typedef int (*Z42ZpkgResolverFn)(
    const char*       namespace_name,    /* "Std.IO" / "z42.core" / ... */
    const uint8_t**   out_bytes,
    size_t*           out_length,
    void*             user_data);
```

`Z42HostConfig` 末尾两个新字段：

| 字段 | 用途 |
|------|------|
| `zpkg_resolver` | 函数指针；NULL = 不用 resolver，只走 `search_paths` |
| `zpkg_resolver_user_data` | 透传给 callback 的不透明指针 |

### 11.2 Tier 2 Rust trait

```rust
pub trait ZpkgResolver: Send + Sync {
    /// Return zpkg bytes for the given namespace, or None to miss.
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>>;
}

// HostConfig 字段
pub zpkg_resolver: Option<Arc<dyn ZpkgResolver>>;
```

Tier 2 提供两个内置实现：

- **`MapResolver`** — `HashMap<String, Vec<u8>>` eager 模式。移动 / wasm 端预先把 stdlib bundle 字节装进 map
- **`SearchPathsResolver`** — 包装现有 `search_paths` 扫文件系统行为。桌面端可选

### 11.3 解析顺序

`load_zbc` 内部对每个 namespace 走如下决策树：

```
for ns in ["z42.core"] + user_artifact.import_namespaces:
    1. resolver.resolve(ns)         hit → 用 resolver 字节
    2. corelib / search_paths       hit → 扫文件系统
    3. silent miss                  → load_zbc 仍返回 OK
                                      （invoke 用到时报 VmException "undefined function"）
```

`z42.core` 是**隐式 prelude**：即使 user `.zbc` 没有任何 `using` 语句，runtime 也会请求一次 corelib。

### 11.4 字节生命周期

callback 的 `*out_bytes` 缓冲**仅在 callback 调用期间有效**。运行时在 callback 返回前完成 `read_zbc` + 解析，之后 host 可立即释放或复用缓冲。这条契约让 host 实现可以用栈缓冲（Android JNI `GetByteArrayElements` + `Release` 等）而无需手动管理 heap。

### 11.5 错误归类

| 情况 | `Z42HostStatus` |
|------|-----------------|
| resolver hit + bytes 解析失败 | `ERR_BAD_ZBC`（错误信息含 namespace 名） |
| resolver miss + search_paths miss + user 代码引用该 namespace | invoke 时报 `ERR_VM_EXCEPTION`（消息含 `"undefined function ..."`） |
| resolver hit + 用户代码不实际引用任何符号 | load_zbc 返回 `OK`；invoke 也 OK |

### 11.6 与现有 `search_paths` 共存

`search_paths` **未废弃**。桌面端可继续用；新 resolver 优先，miss 后才扫 `search_paths`。两者可同时设置。

### 11.7 平台默认 resolver（H4 实施）

平台默认 resolver 实现 §11.2 的 hook，但**自身的 namespace → bytes 映射由读 zpkg 的 `NSPC` section 派生**，不再读 `index.json`：

| 平台 | 默认 resolver | 备注 |
|------|--------------|------|
| iOS | `BundleZpkgResolver(bundle: .main, subdirectory: "stdlib")` —— 枚举 `Bundle.urls(forResourcesWithExtension:"zpkg", subdirectory:)`，对每个读 NSPC（`z42_zpkg_read_namespaces`）建 namespace → bytes 表 | facade `Z42VM` 构造时自动装 |
| Android | `AssetZpkgResolver(context.assets, "stdlib")` —— `AssetManager.list("stdlib")` 枚举，对每个读 NSPC（`Z42VM.readNamespaces`，JNI 桥到同一 C ABI）建表 | facade `Z42VM` 构造时需传 `Context` |
| WASM | `bundleStdlibNode(readNamespaces)` / `bundleStdlibBrowser(url, readNamespaces)` —— Node `readdir` / 浏览器 fetch 生成的 `files.json`，对每个 zpkg 调 wasm `readNamespaces` 导出建 `mapResolver(Map<string, Uint8Array>)`。自定义 host 仍可直接传 `(name) => Uint8Array` 函数 / `{ resolve(name) }` 对象（主动注入）。| wasm-bindgen 包装层 |

### 11.7.1 namespace 归属来自 NSPC（无 index 文件）

一份 zpkg 通常提供**多个** namespace（如 `z42.core.zpkg` 同时 ship `z42.core` / `Std` / `Std.Exceptions`），不能假设 `namespace == 文件名`。早期版本用一张手维护的 `index.json`（namespace → 文件名）表达这层映射，但它是 zpkg `NSPC` section 之外的**第二真相源**、易漂移（[common-pitfalls §1](../../../agent/rules/common-pitfalls.md)）。`drop-index-json-self-describing` 删掉 `index.json`：归属一律由各 zpkg 的 `NSPC` section 权威表达，resolver 枚举可见 zpkg、读 NSPC 自建 namespace → bytes 表。

- **读取 helper**：`z42_zpkg_read_namespaces(bytes, len, visit, user_data)`（C ABI，visitor 回调每个 namespace）/ wasm `readNamespaces(bytes)` 导出 / `Z42VM.readNamespaces`（Android JNI）—— 让 Swift / Kotlin / JS 不必重写 zpkg 解析（Rust 内部 `read_zpkg_namespaces` 已存在）
- **生成**：`./xtask build stdlib` 产 flat view 时**不再写** index 文件
- **分发**：iOS / Android / wasm `build.sh` 只拷 `*.zpkg`（浏览器额外生成 `files.json` 纯文件名清单——HTTP 无法枚举目录的派生替身，非 namespace 映射）
- **主动注入**：web playground / REPL 等宿主持有 zpkg 字节时，自建 `MapResolver`（读 NSPC 填表）经同一 hook 提供

Spec：[`docs/spec/archive/2026-06-06-drop-index-json-self-describing/`](../../../spec/archive/2026-06-06-drop-index-json-self-describing)（取代已归档的 `2026-05-12-fix-bundle-resolver-namespace-index/`）

### 11.8 Spec 与归档

设计：[`docs/spec/archive/2026-05-12-add-zpkg-resolver-hook/`](../../../spec/archive/2026-05-12-add-zpkg-resolver-hook) — proposal + design + 8 个 Requirement scenario + tasks.

实施：5 个 host:: 测试覆盖 trait / C hook / fallback / 自包含 / VmException 全部路径；22/22 host:: tests pass。

### 11.9 分发 package 形态（per-arch flat，2026-05-13 define-package-layout）

每个 z42 release 产 **9 个 per-arch SDK package** 到 `artifacts/packages/`，按 `z42-<version>-<rid>-<config>` 命名（不带 `<target>` 前缀，RID 完全标识平台 + 架构）。RID 白名单 = memory `project_supported_platforms`：

| 类别 | RID 枚举 | package 数 |
|------|----------|----------|
| Desktop SDK (host = C 嵌入同一份) | `macos-arm64` / `linux-arm64` / `linux-x64` / `windows-x64` | 4 |
| iOS (per slice) | `ios-arm64` / `iossim-arm64` | 2 |
| Android (per ABI) | `android-arm64` / `android-x64` | 2 |
| wasm | `browser-wasm` | 1 |

不在白名单：`macos-x64`（Apple Intel 退场）/ `ios-x64-sim`（依赖 Intel Mac host）/ `android-armv7` + `android-x86`（Google Play 自 2019 要求 64-bit 原生库）。

每个 package 统一目录：

```
z42-<v>-<rid>-<config>/
├── bin/                   desktop: z42c+z42vm；mobile/wasm: README 占位
├── libs/                  stdlib zpkg + zsym（跨包 byte-identical；无 namespace 索引——读 NSPC）
├── native/                平台静态/动态库 + 单 slice container（如 iOS xcframework）+ C ABI 头
├── (root) <平台原生入口>  iOS: Sources/+Package.swift；Android: kotlin/+cpp/；wasm: pkg-*/+package.json
└── manifest.toml          统一 schema（abi-version / rid / contents.platform / compat）
```

> **examples 不再随包分发（remove-examples-from-packaging, 2026-06-20）**：早期包形态曾含 `examples/`（hello_c/hello_rust），现已移除——`examples/` 是仓内测试夹具，示例分发后移到 workload。

**核心 invariant**：`libs/` 与 `native/include/` 跨 9 包 byte-identical（C ABI 头 + zpkg 字节码都是平台无关）。

**multi-arch 合并 container**（multi-slice xcframework / multi-ABI AAR）进 Deferred；Phase 2 用户呼声出来再加 `z42-<v>-ios-xcframework-<config>` / `z42-<v>-android-aar-<config>` 两个 convenience 包。

Spec：[`docs/spec/archive/<date>-define-package-layout/`](../../../spec/archive) — 契约 + 9 个 decision + Phase 1 spec 簇说明。

Phase 1 下游 spec：
- 1.1 `add-host-package-conform` — 5 个 desktop RID 包
- 1.2 `add-ios-package` — 3 个 iOS slice 包
- 1.3 `add-android-package` — 4 个 Android ABI 包
- 1.4 `add-wasm-package` — wasm32 包（含 staticlib）

#### Release 分发（GitHub Releases）

每个语义版本 tag（`v[0-9]+.[0-9]+.[0-9]+*`，例 `v0.2.5` / `v0.2.5-rc1`）触发 [`.github/workflows/release.yml`](../../../../.github/workflows/release.yml)，把 9 个 SDK package 压缩并上传到 GitHub Releases：

| RID | 压缩格式 | Release filename |
|-----|---------|------------------|
| linux-x64 / linux-arm64 / macos-arm64 | `.tar.gz` | `z42-<v>-<rid>.tar.gz` |
| windows-x64 | `.zip` | `z42-<v>-windows-x64.zip`（Windows 原生格式） |
| ios-arm64 / iossim-arm64 | `.tar.gz` | `z42-<v>-<rid>.tar.gz` |
| android-arm64 / android-x64 | `.tar.gz` | `z42-<v>-<rid>.tar.gz` |
| browser-wasm | `.tar.gz` | `z42-<v>-browser-wasm.tar.gz` |

外加一个 `SHA256SUMS` 文件（coreutils 格式：每行 `<hex>  <filename>`），下游用 `sha256sum -c SHA256SUMS` 校验。

**Pre-release 规则**（自动设置 `--prerelease`）：
- 版本号 < `1.0.0`（pre-1.0 阶段全部）
- Tag 含 `-` 后缀（`v0.2.5-rc1` / `v1.0.0-rc.1` 等）

**版本号 SoT**：`versions.toml [project].version` 单一来源；`src/runtime/Cargo.toml [workspace.package].version` 必须镜像（漂移由 `./xtask deps check` 强制检出）。Release pipeline 在 `verify` job 验证 `tag.strip_prefix('v') == versions.toml [project].version`，漂移即 fail-fast。

**Bump 流程**：① 改 `versions.toml [project].version` ② drift-check 通过 ③ 同步更新 `src/runtime/Cargo.toml [workspace.package].version` ④ commit ⑤ `git tag v<version> && git push --tags`。

Spec：[`docs/spec/archive/<date>-add-release-automation/`](../../../spec/archive) — 8 个 decision + 11 个 scenario + 4 个 Deferred（reusable workflow / cargo-release / signing / 多渠道分发）。
