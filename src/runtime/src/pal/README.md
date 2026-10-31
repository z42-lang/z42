# pal — Platform Abstraction Layer

## 职责

集中处理 `#[cfg(target_os)]` / `#[cfg(unix)]` / `#[cfg(windows)]` 分支，让
runtime 其余模块**零 cfg 调用** OS 服务。每个 concern 一个文件，公开 surface
返回 OS-neutral 类型。

完整设计见 [`docs/internals/src/runtime/pal.md`](../../../../docs/internals/src/runtime/pal.md)。

## 核心文件

| 文件 | 职责 |
|------|------|
| `system.rs` | `hostname()` / `os_version()` / `os_name()`、进程环境（`pid` / `current_dir` / `set_env` / `temp_dir`）、`OS_CAPS` |
| `fs.rs` | `Vfs` trait（每台 VM 一个句柄，路径型文件操作全走它）/ `default_vfs()` / `glob_match()`；VFS 之外的 OS 调用 `make_executable()` / `symlink()` / `sync_before_rename()` / `is_name_too_long()` / `PATH_LIST_SEP` / `HAS_FS` |
| `vfs/` | `Vfs` 的实现：`LocalFs`（标准库 fs）、`MemoryFs`（实例自有的 `path → bytes`，`mount()` 挂文件）|
| `process.rs` | `exit_code()`、`own_process_group()`、`ProcessTree`（超时杀整棵进程树）、`find_executable()` |
| `random.rs` | `fill()` —— OS CSPRNG |
| `dynlib.rs` | `file_name()`、`Library::open()` / `symbol()` —— 运行时唯一的 dlopen 入口 |
| `mem.rs` | 页大小、GC chunk 页的 `decommit` / `recommit` |
| `thread.rs` | 栈边界、`current_token()`、`run_with_stack()` |
| `time.rs` | `HAS_CLOCK`、`monotonic_us()` / `monotonic_ns()`、`wall_unix_ms()` / `wall_unix_ns()` |
| `net.rs` | socket 的 OS 部分：`winsock_code()`（errno → Winsock 码）/ `timed_out_error()` / `wait_readable()`（可中断 accept 等待）/ `set_tcp_keepalive()`（`net` feature）|
| `signal.rs` | fatal-signal 注册 + `sigsafe` async-signal-safe write + `signal_name` + reset/reraise；非 POSIX 平台为空操作（z42 崩溃 reporter 在 `signal_handler.rs`，调本模块）|

测试：各文件配套 `*_tests.rs`。

## 入口点

`pal::system::hostname()` — `Option<String>`，None 在 Windows / WASM
`pal::system::os_version()` — `String`，空字符串表示 syscall 失败
`pal::fs::make_executable(path)` — `io::Result<()>`，unix 加 u+x g+x o+x；非 unix no-op
`pal::fs::symlink(src, dst)` — `io::Result<()>`，unix 建符号链接；非 unix 报 `Unsupported`
`pal::net::winsock_code()` — `Option<i32>`，OS 错误对应的 Winsock 码（= .NET `SocketError`）；无对应为 None
`pal::net::wait_readable()` — `io::Result<bool>`，unix `poll` / windows `WSAPoll`；其他目标 `Unsupported`
`pal::fs::default_vfs()` — `Arc<dyn Vfs>`，有 OS 文件系统时是 `LocalFs`，否则一个新的空 `MemoryFs`
`MemoryFs::mount` — `(path, bytes)`，往这个内存 VFS 挂一个文件（宿主在创建 VM 前调）
`pal::signal::register_fatal_handlers(handler)` — 注册 5 fatal 信号；非 POSIX 平台返回 `false`
`pal::signal::sigsafe::write_str(fd, bytes)` — async-signal-safe 写
`run_with_stack`（`pal::thread`）— 在指定栈大小的线程上跑闭包；不能起线程时就地跑
`pal::time::monotonic_us()` — 单调微秒；无时钟时为计数

## 不变量（必须遵守）

1. **OS-neutral surface**：pub fn signatures 不带 OS 类型（不 `libc::c_char` /
   `winapi::HANDLE` 等）
2. **每个 concern 一个文件**：`#[cfg(...)]` 切分在内部，consumer 零 cfg
3. **graceful degrade**：未实现的平台返回 None / 空字符串而非 panic
4. **错误也 OS-neutral**：用 `Option<T>` / `Result<T, E>` 抽象，不暴露 errno

## 如何测试验证

```bash
(cd src/runtime && cargo test --lib pal)
```

## 依赖关系

依赖 `libc` (unix-only)；`net.rs` 的 `set_tcp_keepalive` 用 `socket2`（`net` feature）。其他 cargo feature 见
`docs/internals/src/runtime/pal.md` "platform feature gates" 节。
