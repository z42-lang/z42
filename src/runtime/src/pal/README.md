# pal — Platform Abstraction Layer

## 职责

集中处理 `#[cfg(target_os)]` / `#[cfg(unix)]` / `#[cfg(windows)]` 分支，让
runtime 其余模块**零 cfg 调用** OS 服务。每个 concern 一个文件，公开 surface
返回 OS-neutral 类型。

完整设计见 [`docs/internals/src/runtime/pal.md`](../../../../docs/internals/src/runtime/pal.md)。

## 核心文件

| 文件 | 职责 |
|------|------|
| `system.rs` | `hostname()` / `os_version()` |
| `fs.rs` | `make_executable()` / `symlink()` |
| `signal.rs` (unix) | fatal-signal 注册 + `sigsafe` async-signal-safe write + `signal_name` + reset/reraise（z42 崩溃 reporter 在 `signal_handler.rs`，调本模块）|

测试：各文件配套 `*_tests.rs`。

## 入口点

`pal::system::hostname()` — `Option<String>`，None 在 Windows / WASM
`pal::system::os_version()` — `String`，空字符串表示 syscall 失败
`pal::fs::make_executable(path)` — `Result<()>`，unix 加 u+x g+x o+x；非 unix no-op
`pal::fs::symlink(src, dst)` — `Result<()>`，unix 建符号链接；非 unix bail
`pal::signal::register_fatal_handlers(handler)` — 注册 5 fatal 信号（unix）
`pal::signal::sigsafe::write_str(fd, bytes)` — async-signal-safe 写（unix）

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

依赖 `libc` (unix-only)。其他 cargo feature 见
`docs/internals/src/runtime/pal.md` "platform feature gates" 节。

## 待办
- `thread.rs`（随多线程 runtime 落地，按 consumer 驱动）/ `mem.rs`（随 GC bump allocator 落地）
