# Platform Abstraction Layer (PAL)

> single home for every `#[cfg(target_os)]` /
> `#[cfg(unix)]` / `#[cfg(windows)]` split in the runtime.
> 待办：动态库加载（dynlib）、每 VM 一个 Vfs（runtime-audit §6.1 L-2）；Windows 实现、wasm32-wasi 见下文。

## 设计目标

让 runtime 其余模块**零 cfg 调用** OS 服务。OS-specific 实现集中在
`src/runtime/src/pal/<concern>.rs`，公开 surface 返回 OS-neutral 类型
(`Option<String>` / `Result<T, E>`)。

## CoreCLR 对照

CoreCLR `src/coreclr/pal/` 含 ~50 个文件抽象：

| Concern | CoreCLR pal/ 文件 | z42 pal/ 计划 | Status |
|---|---|---|---|
| Thread / stack | `thread.cpp` | `pal/thread.rs` | Done |
| File I/O | `file.cpp` / `path.cpp` | `pal/fs.rs` | Done |
| Signal / exceptions | `signal.cpp` | `pal/signal.rs`（仅 OS 原语；z42 reporter 留 signal_handler.rs） | Done |
| Memory / pages | `virtual.cpp` | `pal/mem.rs` | Done |
| Process / environ | `process.cpp` / `environ.cpp` | `pal/process.rs`、`pal/system.rs` | Done |
| Sockets | —（.NET 放在 System.Native 的 pal_networking） | `pal/net.rs` | Done |
| Clock / time | `time.cpp` | `pal/time.rs` | Done |
| Entropy | `random.cpp` | `pal/random.rs` | Done |

z42 不抄 CoreCLR 的 PAL 内部分层（CoreCLR 还有 `pal/src/<arch>/` per-arch
ASM）—— Cranelift / Rust 已经兜底 arch 差异。z42 PAL 只关心 **OS 差异**。

## z42 现状

`src/runtime/src/pal/`:

```
pal/
├── mod.rs               — 模块入口 + 子模块 re-exports
├── README.md            — 快速 orientation
├── system.rs            — hostname / os_version / os_name、进程环境（pid / cwd / env / temp）、OS_CAPS
├── fs.rs                — make_executable / symlink / sync_before_rename / PATH_LIST_SEP
├── process.rs           — 退出码解码、进程树 kill（进程组 / job object）、PATH 查找
├── random.rs            — OS CSPRNG
├── mem.rs               — 页大小、GC chunk 页的 decommit / recommit
├── thread.rs            — 栈边界、线程 token、在大栈线程上运行
├── time.rs              — 单调时钟与墙钟（wasm32 无时钟时单调钟退为计数器、墙钟为 0）
├── net.rs               — socket 的 OS 部分：errno → Winsock 码、可中断 accept 等待、TCP keep-alive
├── signal.rs            — fatal 信号原语（非 POSIX 平台注册返回 false、写入为空操作）
└── *_tests.rs           — 单元测试
```

`pal::system` surface：

```rust
pub fn hostname() -> Option<String>;
pub fn os_version() -> String;
pub fn os_name() -> &'static str;                  // wasm32 统一为 "wasm"
pub fn pid() -> u32;                               // 以下在 wasm32 上返回固定值 / 空操作
pub fn current_exe() -> Option<PathBuf>;
pub fn current_dir() -> io::Result<PathBuf>;
pub fn set_current_dir(path: &str) -> io::Result<()>;
pub fn temp_dir() -> PathBuf;
pub fn set_env(name: &str, value: Option<&str>);
pub const OS_CAPS: OsCaps;                         // Std.Platform.Capabilities() 的 OS 部分
```

`#[cfg(unix)]` 走 libc::gethostname / libc::uname；`#[cfg(target_arch =
"wasm32")]` 返回 None / "wasm"；其他 (Windows-without-impl) 返回 None / 空
字符串。**所有 cfg 切分都在 `pal/system.rs` 内部** —— `corelib/system.rs`
零 cfg。

## 不变量（必须遵守）

1. **OS-neutral surface**：pub fn signatures 不带 OS 类型（不暴露
   `libc::c_char` / `winapi::HANDLE` 等）
2. **每个 concern 一个文件**：`#[cfg(...)]` 切分在文件内部，consumer 零 cfg。pal 外现存的平台 cfg 与
   `libc::` 是欠账，由 GREEN 的 `layers` stage 按组件计数、只减不增（见[测试门禁](../devinfra/test-gate.md)）
3. **graceful degrade**：未实现的平台返回 `None` / `""` / `Err` 而非 panic
4. **错误 OS-neutral**：`Option<T>` / `Result<T, E>` 抽象，不暴露 errno
5. **每个 pal 子模块必须有 unit tests**：smoke test 覆盖 unix 路径 + non-unix
   sentinel return

## 各 concern 与待办

### `pal/fs.rs` — 文件系统

承接 `corelib/fs.rs` 的 2 个 `#[cfg(unix)]` 块（`make_executable` / `symlink`）：

```rust
// pal/fs.rs
pub fn make_executable(path: &str) -> Result<()>;  // unix: mode|0o111；非 unix no-op
pub fn symlink(src: &str, dst: &str) -> Result<()>; // unix symlink；非 unix bail
```

`corelib/fs.rs::builtin_file_make_executable` / `builtin_file_symlink` call
`crate::pal::fs::*`，零 cfg。

> `read_permissions` / `set_permissions` 是 make_executable 的 building
> block——当前无独立 consumer，按 YAGNI 折进 `make_executable`（read+modify+set 一体），
> 单独 split 留待有 consumer 时（不做 speculative API）。

### `pal/signal.rs` — POSIX signal 原语

> **边界裁决**：`signal_handler.rs` **不整文件**迁入 PAL，
> 否则与「OS-neutral surface」不变量冲突——该文件混了 OS 原语**和** z42 崩溃 reporter
> （`write_call_stacks` 走 `VM_CORES`/`vm_contexts`，是 runtime 内省，非 OS 代码）。整文件
> 搬会把 VM 内部知识塞进 `pal/`。故**只抽 OS 原语**，z42 崩溃逻辑留 `signal_handler.rs`。

抽到 `pal/signal.rs`（async-signal-safe）。接口在所有目标上都存在：没有 POSIX 信号的目标（Windows、wasm）上
注册返回 `false`，写入与 reset 是空操作，所以 `signal_handler.rs` 不需要平台 cfg：

```rust
// pal/signal.rs
pub const STDERR_FD: i32;
pub fn register_fatal_handlers(handler: extern "C" fn(i32)) -> bool; // 5 fatal 信号注册
pub fn leak_fd(file: std::fs::File) -> Option<i32>;                // Z42_CRASH_DIR 文件交给 sigsafe 写
pub fn signal_name(sig: i32) -> &'static [u8];
pub fn reset_default_and_reraise(sig: i32);                         // SIG_DFL + raise
pub mod sigsafe { pub fn write_str / write_dec_u32 / write_hex_u64 }
```

`register_fatal_handlers` 注册后给每个信号补上 `SA_ONSTACK`：`signal-hook-registry` 注册时不带它，
原生代码撞上栈的 guard page 时 SIGSEGV handler 就跑在已经耗尽的栈上，进程无声死亡。带上后 handler
跑在线程的备用信号栈上（Rust 运行时为主线程和每个 `std::thread` 都建了一个）。

`pal/thread.rs` 的 `current_thread_stack() -> Option<(low, high)>`：macOS / iOS 用
`pthread_get_stackaddr_np` + `pthread_get_stacksize_np`，Linux / Android 用 `pthread_getattr_np`，
Windows 用 `GetCurrentThreadStackLimits`；其他平台返回 `None`，栈检查对该线程关闭。

`signal_handler.rs` 保留 z42 崩溃 reporter（`install` / `handler` / `write_call_stacks`
走 VM_CORES），call `crate::pal::signal::*`（async-signal-safe 约束）。Windows VEH
走 `pal::signal` 同接口不同 impl，仍延后（无 Windows CI runner）。

### `pal/net.rs` — socket 的 OS 部分

承接 `corelib/network/{error,listener,tcp}.rs` 的平台分支（runtime-audit §6.1 L-2b）。`corelib/network`
只剩 builtin 胶水（参数、.NET 异常映射、句柄表、park），零平台 cfg：

```rust
// pal/net.rs
pub fn winsock_code(raw: i32) -> Option<i32>;                  // OS 错误 → Winsock 码（= .NET SocketError）
pub fn timed_out_error() -> Option<io::Error>;                 // OS 自己的 ETIMEDOUT / WSAETIMEDOUT
pub fn wait_readable(l: &TcpListener, timeout_ms: i32) -> io::Result<bool>; // unix poll / windows WSAPoll
#[cfg(feature = "net")]
pub fn set_tcp_keepalive(s: &TcpStream, idle: Duration, interval: Duration, retries: u32) -> io::Result<()>;
```

Unix 用 .NET `SocketErrorPal` 的 errno 表；Windows 的 OS 错误本身就是 Winsock 码；其他目标（wasm32）
没有映射（`None`），`wait_readable` 报 `Unsupported`。`set_tcp_keepalive` 经 `socket2`，只在 `net`
feature 下存在；`interval` / `retries` 只在 OS 提供对应 knob 时生效。

### `pal/thread.rs` — 线程

```rust
// pal/thread.rs
pub const CAN_SPAWN: bool;                                  // wasm32 为 false
pub fn run_with_stack<R>(name, stack_bytes, f) -> Result<R, RunError>; // 不能起线程时在当前线程跑
pub fn current_token() -> usize;                            // unix 为 pthread_self，信号处理中可用
pub fn current_thread_stack() -> Option<(usize, usize)>;
```

`run_with_stack` 供嵌入入口（`host`）在大栈线程上跑 VM；`current_token` 供帧栈记录归属线程。

### `pal/mem.rs` — 页

```rust
// pal/mem.rs
pub(crate) const CAN_DECOMMIT: bool;                        // macOS / iOS / Linux / Android
pub(crate) fn page_size() -> usize;
pub(crate) fn inner_pages(start, end) -> Option<(usize, usize)>;
pub(crate) unsafe fn decommit(addr, len);                   // MADV_FREE_REUSABLE / MADV_FREE
pub(crate) unsafe fn recommit(addr, len);
```

GC 的 chunk 池用它把多余空 chunk 的物理页还给 OS（见 [GC TLAB](gc-tlab.md)）。

### `pal/process.rs` — 子进程

```rust
// pal/process.rs
pub fn exit_code(status: ExitStatus) -> i64;                 // unix 被信号杀 → 128 + 信号
pub fn own_process_group(cmd: &mut Command);                 // unix：子进程自成进程组
pub struct ProcessTree;                                      // attach(child, own_group) / kill(child)
pub fn find_executable(name: &str) -> Option<String>;        // POSIX `command -v` / Windows `where`
```

超时 kill 要连孙进程一起杀：unix 杀整个进程组，Windows 把子进程放进 job object 后 `TerminateJobObject`。

### `pal/random.rs` — 熵

`fill(buf) -> io::Result<()>`；wasm32 没有接入熵源，返回 `Unsupported`（`Std.Crypto` 映射为 NotSupportedException）。

### `pal/time.rs` — 时钟

```rust
// pal/time.rs
pub const HAS_CLOCK: bool;                                  // wasm32-unknown-unknown 为 false
pub fn monotonic_us() -> u64;                               // 无时钟时每次调用返回下一个计数
pub fn monotonic_ns() -> u64;
pub fn wall_unix_ms() -> i64;                               // 无时钟时为 0
pub fn wall_unix_ns() -> u64;
```

GC 的停顿计时用 `monotonic_us`；增量 GC 的分片预算与停顿自适应在 `HAS_CLOCK` 为 `false` 时改按工作量计。

## Platform feature gates

Cargo.toml 的 `wasm` / `ios` / `android` preset feature 与 PAL 互补：

| Feature | PAL 行为 |
|---|---|
| 默认（无 preset） | 完整 unix / windows 实现 |
| `wasm` | 所有 PAL fn 走 `cfg(target_arch = "wasm32")` 分支 |
| `ios` / `android` | 走 unix 分支（mobile 仍是 POSIX）；个别 syscall 受 sandbox 约束 |

PAL **不** 是 Cargo feature；它是 cfg-based static dispatch。Feature gate
的角色是关掉某些 module（如 `jit`），PAL 始终在。wasm 的功能裁剪用 feature 表达而不是
`cfg(target_arch = "wasm32")`：网络是 `net` / `tls`（`default` 与 ios / android preset 开，`wasm`
preset 不开），关掉时 `corelib/network/unsupported.rs` 的桩抛 `NetUnsupportedException`，任何目标上都一样。

## Deferred / Future Work

- **Windows real impl** — 各 concern 的 Windows 分支都 stub。等
  Windows CI runner 投产后实现（review.md backlog 项）。
- **wasm32-wasi** — 目前 wasm32 全 stub，wasi 有 fs / network syscall 可
  实装；视用户需求引入。
- **PAL trait 化** — 目前 PAL 是 free functions + cfg dispatch。若未来需要
  runtime-injectable PAL（测试 mock / 仿真平台），可加 `trait Pal` + Vtable，
  保持 cfg-based 实现是默认。
