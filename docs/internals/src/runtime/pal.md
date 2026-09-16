# Platform Abstraction Layer (PAL)

> review.md Part 1 P2 — single home for every `#[cfg(target_os)]` /
> `#[cfg(unix)]` / `#[cfg(windows)]` split in the runtime. Phase 1
> (2026-06-03, add-pal-system-phase1) shipped the module scaffold + first
> migrated concern (`system`); this document codifies the long-form design
> + Phase 2-N migration plan.

## 设计目标

让 runtime 其余模块**零 cfg 调用** OS 服务。OS-specific 实现集中在
`src/runtime/src/pal/<concern>.rs`，公开 surface 返回 OS-neutral 类型
(`Option<String>` / `Result<T, E>`)。

## CoreCLR 对照

CoreCLR `src/coreclr/pal/` 含 ~50 个文件抽象：

| Concern | CoreCLR pal/ 文件 | z42 pal/ 计划 | Status |
|---|---|---|---|
| Thread / mutex / TLS | `thread.cpp` / `mutex.cpp` | `pal/thread.rs` (Phase 4) | Pending |
| File I/O | `file.cpp` / `path.cpp` | `pal/fs.rs` (Phase 2) | **Phase 2 done** |
| Signal / exceptions | `signal.cpp` | `pal/signal.rs` (Phase 3，仅 OS 原语；z42 reporter 留 signal_handler.rs) | **Phase 3 done** |
| Memory / mmap | `virtual.cpp` | `pal/mem.rs` (Phase 5) | Pending |
| Process / environ | `process.cpp` / `environ.cpp` | `pal/system.rs` 已起步 | Phase 1 done |
| Clock / time | `time.cpp` | `pal/clock.rs` (post-MVP) | Pending |

z42 不抄 CoreCLR 的 PAL 内部分层（CoreCLR 还有 `pal/src/<arch>/` per-arch
ASM）—— Cranelift / Rust 已经兜底 arch 差异。z42 PAL 只关心 **OS 差异**。

## z42 Phase 1 现状（2026-06-03）

`src/runtime/src/pal/`:

```
pal/
├── mod.rs               — 模块入口 + 子模块 re-exports
├── README.md            — 快速 orientation
├── system.rs            — Phase 1：hostname / os_version
└── system_tests.rs      — 单元测试
```

`pal::system` Phase 1 surface：

```rust
pub fn hostname() -> Option<String>;
pub fn os_version() -> String;
```

`#[cfg(unix)]` 走 libc::gethostname / libc::uname；`#[cfg(target_arch =
"wasm32")]` 返回 None / "wasm"；其他 (Windows-without-impl) 返回 None / 空
字符串。**所有 cfg 切分都在 `pal/system.rs` 内部** —— `corelib/system.rs`
零 cfg。

## 不变量（必须遵守）

1. **OS-neutral surface**：pub fn signatures 不带 OS 类型（不暴露
   `libc::c_char` / `winapi::HANDLE` 等）
2. **每个 concern 一个文件**：`#[cfg(...)]` 切分在文件内部，consumer 零 cfg
3. **graceful degrade**：未实现的平台返回 `None` / `""` / `Err` 而非 panic
4. **错误 OS-neutral**：`Option<T>` / `Result<T, E>` 抽象，不暴露 errno
5. **每个 pal 子模块必须有 unit tests**：smoke test 覆盖 unix 路径 + non-unix
   sentinel return

## Phase 2-N Migration Plan

### ~~Phase 2: `pal/fs.rs` — 文件系统~~ — **✅ 已落地 2026-06-11 (`add-pal-fs`)**

迁移 `corelib/fs.rs` 的 2 个 `#[cfg(unix)]` 块（`make_executable` / `symlink`）：

```rust
// pal/fs.rs
pub fn make_executable(path: &str) -> Result<()>;  // unix: mode|0o111；非 unix no-op
pub fn symlink(src: &str, dst: &str) -> Result<()>; // unix symlink；非 unix bail
```

`corelib/fs.rs::builtin_file_make_executable` / `builtin_file_symlink` 现 call
`crate::pal::fs::*`，零 cfg（行为保持，cargo 759+pal 2 单测 + z42.io 45/45 e2e）。

> 原设计列的 `read_permissions` / `set_permissions` 是 make_executable 的 building
> block——当前无独立 consumer，按 YAGNI 折进 `make_executable`（read+modify+set 一体），
> 单独 split 留待有 consumer 时（不做 speculative API）。

### ~~Phase 3: `pal/signal.rs` — POSIX signal 原语~~ — **✅ 已落地 2026-06-11 (`add-pal-signal`)**

> **设计修正（2026-06-11，User 裁决）**：原计划「`signal_handler.rs` **整文件**迁移」
> 与 PAL「OS-neutral surface」不变量冲突——该文件混了 OS 原语**和** z42 崩溃 reporter
> （`write_call_stacks` 走 `VM_CORES`/`vm_contexts`，是 runtime 内省，非 OS 代码）。整文件
> 搬会把 VM 内部知识塞进 `pal/`。改为**只抽 OS 原语**，z42 崩溃逻辑留 `signal_handler.rs`。

抽到 `pal/signal.rs`（`#![cfg(unix)]`，async-signal-safe）：

```rust
// pal/signal.rs
pub fn register_fatal_handlers(handler: extern "C" fn(i32)); // 5 fatal 信号注册
pub fn signal_name(sig: i32) -> &'static [u8];
pub fn reset_default_and_reraise(sig: i32);                  // SIG_DFL + raise
pub mod sigsafe { pub fn write_str / write_dec_u32 / write_hex_u64 }
```

`signal_handler.rs` 保留 z42 崩溃 reporter（`install` / `handler` / `write_call_stacks`
走 VM_CORES），改 call `crate::pal::signal::*`（公开 surface / async-signal-safe 约束不变；
cargo 759 + pal::signal 9 单测 + install idempotent + e2e 信号崩溃路径）。Windows VEH
（Phase 3.1）走 `pal::signal` 同接口不同 impl，仍延后（无 Windows CI runner）。

### Phase 4: `pal/thread.rs` — 多线程基础（**consumer-gated**）

当前 `thread/` 是 stub。Phase 4 引入 PAL 抽象 thread spawn / TLS /
join 作为 multi-thread runtime 的底座（review.md add-multithreading-foundation
spec 进行中，可对接）。**不 speculative 提前做**——它是为多线程 runtime 服务的新
抽象（非现有平台代码迁移），无 consumer 前空 API 没意义；随多线程 runtime 一起落地。

```rust
// pal/thread.rs (Phase 4)
pub fn spawn<F>(f: F) -> ThreadHandle where F: FnOnce() + Send;
pub fn current_id() -> ThreadId;
pub fn yield_now();
```

### Phase 5: `pal/mem.rs` — 页对齐分配 / mmap（**consumer-gated**）

GC bump allocator（review.md C6）需要页对齐大块虚拟内存。先用 std::alloc
顶住，Phase 5 切到 mmap / VirtualAlloc 抽象。**不 speculative 提前做**——
它是为 bump allocator 服务的新抽象（非现有平台代码迁移），随 GC bump allocator
（C6，目前未实现）一起落地。

```rust
// pal/mem.rs (Phase 5)
pub fn alloc_pages(n: usize) -> PageBlock;
pub fn protect(block: &PageBlock, prot: Protection);
pub fn free_pages(block: PageBlock);
```

## Platform feature gates

Cargo.toml 的 `wasm` / `ios` / `android` preset feature 与 PAL 互补：

| Feature | PAL 行为 |
|---|---|
| 默认（无 preset） | 完整 unix / windows 实现 |
| `wasm` | 所有 PAL fn 走 `cfg(target_arch = "wasm32")` 分支 |
| `ios` / `android` | 走 unix 分支（mobile 仍是 POSIX）；个别 syscall 受 sandbox 约束 |

PAL **不** 是 Cargo feature；它是 cfg-based static dispatch。Feature gate
的角色是关掉某些 module（如 `jit`），PAL 始终在。

## Deferred / Future Work

- **Windows real impl** — Phase 1-N 各 concern 的 Windows 分支都 stub。等
  Windows CI runner 投产后实现（review.md backlog 项）。
- **wasm32-wasi** — 目前 wasm32 全 stub，wasi 有 fs / network syscall 可
  实装；视用户需求引入。
- **PAL trait 化** — 目前 PAL 是 free functions + cfg dispatch。若未来需要
  runtime-injectable PAL（测试 mock / 仿真平台），可加 `trait Pal` + Vtable，
  保持 cfg-based 实现是默认。
