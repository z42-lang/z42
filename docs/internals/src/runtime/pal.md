# Platform Abstraction Layer (PAL)

> single home for every `#[cfg(target_os)]` /
> `#[cfg(unix)]` / `#[cfg(windows)]` split in the runtime.
> 待办：Windows 实现、wasm32-wasi 见下文。

## 设计目标

让 runtime 其余模块**零 cfg 调用** OS 服务。OS-specific 实现集中在
`src/runtime/src/pal/sys/`，`pal/<concern>.rs` 保留接口和共享策略，公开 surface 返回 OS-neutral 类型
(`Option<String>` / `Result<T, E>`)。

## CoreCLR 对照

CoreCLR `src/coreclr/pal/` 含 ~50 个文件抽象：

| Concern | CoreCLR pal/ 文件 | z42 pal/ 计划 | Status |
|---|---|---|---|
| Thread / stack | thread.cpp | `pal/thread.rs` | Done |
| File I/O | file.cpp / path.cpp | `pal/fs.rs` | Done |
| Signal / exceptions | signal.cpp | `pal/signal.rs`（仅 OS 原语；z42 reporter 留 signal_handler.rs） | Done |
| Memory / pages | virtual.cpp | `pal/mem.rs` | Done |
| Process / environ | process.cpp / environ.cpp | `pal/process.rs`、`pal/system.rs` | Done |
| Sockets | —（.NET 放在 System.Native 的 pal_networking） | `pal/net.rs` | Done |
| Clock / time | time.cpp | `pal/time.rs` | Done |
| Entropy | random.cpp | `pal/random.rs` | Done |
| Dynamic libraries | loader.cpp | `pal/dynlib.rs` | Done |

z42 不抄 CoreCLR 的 PAL 内部分层（CoreCLR 还有 `pal/src/<arch>/` per-arch
ASM）—— Cranelift / Rust 已经兜底 arch 差异。z42 PAL 只关心 **OS 差异**。

## z42 现状

`src/runtime/src/pal/`:

```
pal/
├── mod.rs               — 模块入口 + 子模块 re-exports
├── README.md            — 快速 orientation
├── sys/                 — 编译期后端选择；unix/、windows/、wasm/、fallback/ 和 common/
├── system.rs            — hostname / os_version / os_name、进程环境（pid / cwd / env / temp）、OS_CAPS
├── fs.rs                — Vfs trait / default_vfs / glob_match；make_executable / symlink / sync_before_rename / PATH_LIST_SEP
├── vfs/                 — Vfs 的实现：local.rs（LocalFs，std::fs）、memory.rs（MemoryFs，path → bytes）
├── process.rs           — 退出码解码、进程树 kill（进程组 / job object）、PATH 查找
├── random.rs            — OS CSPRNG
├── dynlib.rs            — 动态库文件名、打开、取符号（native 扩展、压缩库、REPL 编辑器共用）
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
字符串。**所有平台 cfg 切分都在 `pal/sys/` 内部** —— `corelib/system.rs`
零 cfg。

## 不变量（必须遵守）

1. **OS-neutral surface**：pub fn signatures 不带 OS 类型（不暴露
   `libc::c_char` / `winapi::HANDLE` 等）
2. **每个 concern 一个接口**：`pal/<concern>.rs` 不含操作系统选择；`sys/mod.rs` 选择后端，后端实现集中在 `sys/`。`pal/sys/` 外现存的平台 cfg 与
   `libc::` 是欠账，由 GREEN 的 `layers` stage 按组件计数、只减不增（见[测试门禁](../devinfra/test-gate.md)）
3. **graceful degrade**：未实现的平台返回 `None` / `""` / `Err` 而非 panic
4. **错误 OS-neutral**：`Option<T>` / `Result<T, E>` 抽象，不暴露 errno
5. **每个 pal 子模块必须有 unit tests**：smoke test 覆盖 unix 路径 + non-unix
   sentinel return

## 编译期后端边界

`sys/mod.rs` 按目标选择 Unix、Windows、wasm32；其他目标进入 fallback，保持已有的降级返回值。
Unix 后端包含 Apple、Linux/Android 等子分支。共享的宿主实现和不支持操作的哨兵放在 common，
只在所选后端需要时编译。模块选择不产生运行期虚调用；每 VM 的 VFS 句柄保持原有抽象。

平台 cfg 和 libc 的生产代码只能位于 `pal/sys/`。分层门同时检查 PAL 上层，且按完整目录前缀判断，
相似名称的目录不能取得豁免。feature 裁剪和测试代码维持各自原有规则。

## 各 concern 与待办

### `pal/fs.rs` + `pal/vfs/` — 文件系统

**每台 VM 一个 `Vfs` 句柄。** 路径型文件操作（`File.*` / `Directory.*` / `Path.Glob` 的 builtin）、包加载
（`metadata/loader`、`namespace_index`、惰性加载器）与 `app::run` 的启动探测都走 VM 持有的
`Arc<dyn Vfs>`，没有进程级的「当前后端」开关：

```rust
// pal/fs.rs
pub trait Vfs: Send + Sync {
    fn read_to_string / read / write / append / exists / is_dir / remove_file / remove_dir
       / copy / rename / create_dir_all / modified_ms / file_len / read_dir / glob / write_atomic;
    fn local_path<'a>(&self, path: &'a str) -> Option<&'a Path> { None }  // 能力：是否真文件系统
}
pub fn default_vfs() -> Arc<dyn Vfs>;     // HAS_FS ? LocalFs : 新的空 MemoryFs
pub fn glob_match(pattern, text) -> bool; // Vfs::glob 的 `*` / `?` 语言

// pal/vfs/
pub struct LocalFs;                       // std::fs；write_atomic = tmp + sync_before_rename + rename
pub struct MemoryFs;                      // 实例自有 path → bytes；mount(path, bytes)；目录由前缀推出
```

- **谁给句柄**：创建 VM 的一方——`VmContext::with_vfs` / `with_module_and_vfs`、`app::RunOpts::vfs`、
  嵌入 API 的 `HostConfig::vfs`（Tier 2 经 `host::install_vfs` 装入）。不给就是 `default_vfs()`。
  `__run_goldens_isolated` 起的子 VM 沿用父 VM 的句柄。wasm 宿主（`workload/wasm/platform`）持有一个
  页级 `MemoryFs`，`mountFile` / `mountAsset` 写进去，再交给它创建的每台 VM。
- **隔离**：两台 VM 拿两个 `MemoryFs` 就互不可见（`vm_context_tests` 的
  `two_vms_with_separate_memory_fs_do_not_see_each_others_files`）；共享同一个 `Arc` 才共享文件。
- **能力而非类型判断**：需要「真文件系统」性质的调用方问 `local_path`——.NET 的
  FileNotFound / DirectoryNotFound 区分要看父目录是否存在（虚拟文件系统没有真目录，一律按存在算）；
  `namespace_index` 按 (size, mtime) 记忆 NSPC 只在本地文件系统上做（`MemoryFs` 的 mtime 恒为 0）。
- **不走 VFS 的**：FileStream 句柄、临时文件 / 目录、硬链接 / 符号链接、权限位、`Stdio.ToFile`——它们是
  OS 操作，直接用标准库的文件 API / 下面的 OS 调用。

VFS 之外的 OS 调用承接 `corelib/fs.rs` 的 2 个 `#[cfg(unix)]` 块（`make_executable` / `symlink`）：

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

### `pal/dynlib.rs` — 动态库

```rust
// pal/dynlib.rs
pub fn file_name(stem: &str) -> String;                      // lib<stem>.dylib / .so、<stem>.dll
pub struct Library;                                          // feature native-interop；drop 即卸载
impl Library {
    pub unsafe fn open(path: &Path) -> io::Result<Self>;     // 错误信息带路径
    pub unsafe fn symbol<T: Copy>(&self, name: &str) -> io::Result<T>;
}
```

运行时只在这里 dlopen：用户 native 扩展（`interop/loader`）、`libz42_compression`（`interop/ext`）、REPL 编辑器
`libz42_repl`（`corelib/repl_native`）。

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

- **Windows system queries / symlink** — 主机名、系统版本查询与符号链接仍保留降级行为；
  进程 job、线程栈边界和 socket 等已有实现不受此限制。
- **wasm32-wasi** — 目前 wasm32 全 stub，wasi 有 fs / network syscall 可
  实装；视用户需求引入。
- **PAL trait 化** — 目前 PAL 是 free functions + cfg dispatch。若未来需要
  runtime-injectable PAL（测试 mock / 仿真平台），可加 `trait Pal` + Vtable，
  保持 cfg-based 实现是默认。
