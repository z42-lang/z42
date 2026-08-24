# 调试工作流

> **状态**：📋 部分可用。完整 DAP 集成在 SemVer **0.8.7** 启用。

## VM 运行时 ops 钩子（2026-05-25 起）

z42vm 内置 3 个 ops/devex 设施，无需重编译 / 重启即可控制日志、查看版本、抓崩溃。

### `--info` —— 打印构建信息

```bash
$ z42vm --info
z42vm 0.1.0
target: macos
arch: aarch64
build profile: release
features: jit, native-interop
exec modes: interp, jit
Z42_LIBS: (unset; falls back to artifacts/build/libraries/dist/release)
Z42_PATH: (unset)
Z42_LOG: (unset; defaults to z42=warn / z42=info under --verbose)
libs dir: /Users/.../artifacts/build/libraries/dist/release
```

bug 报告 + CI 预检 / 跨机器对比 build profile 用。

### `Z42_LOG` —— per-module 日志过滤

`tracing-subscriber` directive 语法 — 用 `=` 控制 level，逗号分隔多条规则。
默认 `z42=warn`；`--verbose` 是 `z42=info`；`Z42_LOG` 覆盖前两者。

```bash
# 看 JIT 编译细节 + GC 全部 trace，其他模块只看 warn 以上
Z42_LOG=z42::jit=debug,z42::gc=trace,z42=warn ./z42vm script.zbc

# 单一全局 level
Z42_LOG=z42=debug ./z42vm script.zbc

# 只看 warn 以上（即便 --verbose 也安静）
Z42_LOG=z42=warn ./z42vm --verbose script.zbc
```

可用 target：`z42` / `z42::interp` / `z42::jit` / `z42::gc` / `z42::native::*` /
`z42::metadata::*` 等（每个 mod 自己的路径）。完整 directive 语法见
<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html>。

### `Z42_CRASH_DIR` —— 内部 panic + OS signal 抓盘

VM 内部 panic（`unwrap` / index OOB / `debug_assert!` failure 等）**和** OS 信号
（SIGSEGV / SIGABRT / SIGFPE / SIGILL / SIGBUS）默认都打印诊断到 stderr 后 abort。
设了 `Z42_CRASH_DIR` 后**额外**把报告落到 `<dir>/z42vm-crash-<ts_ns>.txt`：

```bash
mkdir -p /var/log/z42
Z42_CRASH_DIR=/var/log/z42 RUST_BACKTRACE=1 ./z42vm script.zbc

# Rust panic 崩溃：
# [panic hook] crash report written to /var/log/z42/z42vm-crash-1716659200000000000.txt

# OS 信号崩溃（JIT 出 bug / native FFI 写坏内存等）：
# [signal hook] crash report appended to Z42_CRASH_DIR fd
```

**Rust panic 报告内容**（Phase 1, 12cf7ef8 / 2026-05-25）：
z42vm 版本 / 平台 / build profile / panic 位置 / payload / Rust backtrace
（`RUST_BACKTRACE=1` 控制详细程度）。

**OS signal 报告内容**（Phase 2, add-os-signal-handler / 2026-05-26）：
信号名 / build banner / **所有 thread 的 z42 call stack**（一行一帧：
`#<idx>  <func_name> at <file>:<line>:<col>`）。报告写完后 `signal()` 重置到
`SIG_DFL` + `raise()`，让 kernel 走默认 abort + coredump（`ulimit -c unlimited` 仍生效）。

捕获的 5 个信号：

| 信号 | 典型触发 |
|---|---|
| **SIGSEGV** | 访问坏指针 — JIT bug、native FFI 写坏内存 |
| **SIGABRT** | `libc::abort()` 调用 — native module 内部断言失败 / glibc OOM |
| **SIGFPE** | 整数除零 — z42 interp 已挡，native / JIT 路径漏网 case |
| **SIGILL** | 非法指令 — corrupt JIT code |
| **SIGBUS** | 对齐错误 / mmap 越界 — 偶发 ARM64 |

**Lock contended 降级**：报告依赖 `try_lock` 拿 VM_CORES 注册表 + 每个 thread 的
`call_stack`。如果信号触发时另一线程持锁（GC mark 阶段等），handler 写
`<call stack lock contended>` placeholder，**不死锁，不丢报告**，进程仍正常 abort。

**Windows 暂未支持**：现仅 POSIX (macOS / Linux)。Windows VEH (Vectored Exception
Handler) 是 Phase 2.1 后续 spec — Windows build 编译过但无信号捕获。

### `--print-stats-on-exit` —— 运行时计数器快照

VM 退出时打印 `RuntimeCounters` 快照到 stderr —— 观察 JIT 编译数 / builtin call
次数 / 异常 throw/catch 次数 / native call 次数等。Phase 1 已接通 `builtin_calls`
计数器；其他字段（jit / native / exception）框架已就绪，Phase 2 各自小 refactor 接增量点。

```bash
$ z42vm script.zbc --print-stats-on-exit
... script output ...
--- z42vm runtime counters ---
builtin_calls:        12345
native_calls:         0
jit_methods_compiled: 0
jit_compile_us_total: 0
exceptions_thrown:    0
exceptions_caught:    0
---
```

CLI flag 与 `--info` 不同：`--info` 是 boot-time build 信息；`--print-stats-on-exit`
是 runtime 计数。两个 flag 可同时设。

> **已扩展（脚本性能分析 P1a/P1c，2026-08）**：native / jit / exception 计数均已接通增量点；
> 快照升级为 `ProfileSnapshot`（+ allocations + GC 分代 + 并发探针），`--stats-format json` 出单行
> JSON；脚本层 `Std.Diagnostics.RuntimeStats.Counters()` 暴露同一组计数。整体用法见上「脚本性能分析
> （`xtask profile`）」节。定时打印 / Prometheus / OTLP exporter 见 docs/review.md Part 4 D6。

### `RuntimeObserver` —— push-based 事件流（embedding API）

VM 用 push-based 事件流通知**非-GC**运行时活动（模块加载、未来 JIT 编译 / 异常 throw/catch / native 调用）。
GC 事件保留在独立的 `GcObserver`（不破坏既有 embedder）。

Embedder 通过 `VmContext::add_runtime_observer(Arc<dyn RuntimeObserver>)` 注册：

```rust
use std::sync::Arc;
use z42::observer::{RuntimeObserver, RuntimeEvent};

#[derive(Debug)]
struct JsonLineExporter;

impl RuntimeObserver for JsonLineExporter {
    fn on_event(&self, evt: &RuntimeEvent) {
        match evt {
            RuntimeEvent::ModuleLoaded { name, byte_size } => {
                eprintln!(r#"{{"kind":"ModuleLoaded","name":{name:?},"byte_size":{byte_size:?}}}"#);
            }
            RuntimeEvent::Custom { source, message } => {
                eprintln!(r#"{{"kind":"Custom","source":{source:?},"message":{message:?}}}"#);
            }
        }
    }
}

ctx.add_runtime_observer(Arc::new(JsonLineExporter));
```

Phase 1 emit site：`main.rs` 每个 boot-time `load_artifact` 成功后 replay-emit 一条 `ModuleLoaded`
（含文件名 + 字节数）。Phase 2 wire 其他 emit 点（JIT / exception / native / lazy loader）。

Observer 回调约束：**Send + Sync**（embedder 可能跨 thread 转发到 async runtime / 监控管线）；
**禁止 panic**（VM fire-loop 不 catch unwind，panic 会 abort 整个进程）；
**快速返回**（重活走 channel 给 worker thread，不要阻塞 hot path）。

## 脚本性能分析（`xtask profile`）

一条命令把外部 profiler / 运行时探针挂到一次脚本运行上，按四个维度出分析产物。机制原理
（两层成本模型 / safepoint 采样 / perfetto 采样型 trace）见
[`docs/book/src/runtime/diagnostics.md`](../book/src/runtime/diagnostics.md)（知识库机制页）；
program-plan 与决策见归档 `docs/spec/archive/2026-08-24-plan-script-profiling/`。

```bash
# 编译脚本 → 按维度分析（无维度 flag = 全跑，写 report.md）
xtask profile <script.z42> [--cpu|--heap|--threads|--e2e|--all] [--mode interp|jit]
```

| 维度 | 产物 | 依赖工具（缺则跳过 + 安装提示，不让 profile 失败）|
|------|------|------|
| `--cpu` | ① **samply** native 火焰图（Rust/JIT machine 栈）② **z42-level** 采样火焰图（`Main;foo;bar` folded → inferno SVG）+ **perfetto 采样 trace**（`z42-trace.json`）| `samply` / `inferno-flamegraph`（`cargo install`）/ perfetto UI |
| `--heap` | dhat 堆分析 `dhat-heap.json` + counter 摘要 | dhat（内置 feature，隔离 target-dir 现建）→ dh_view.html |
| `--threads` | safepoint park 直方图 + 用户锁争用（`profile-contention` feature VM）+ counter 摘要 | — |
| `--e2e` | hyperfine wall-clock + peak-RSS + counter 摘要 | `hyperfine` |

**z42-level CPU 采样**（`--cpu` 的第二层，safepoint 采样 profiler）直接经 env 控制，不必走 xtask：

```bash
# 采样开：按 Hz 采 z42 调用栈 → folded（火焰图）；设 Z42_TRACE_OUT 再产 perfetto 采样 trace
Z42_SAMPLE_HZ=4000 Z42_SAMPLE_OUT=z42-samples.folded Z42_TRACE_OUT=z42-trace.json \
  z42vm script.zbc <entry> --mode jit
inferno-flamegraph z42-samples.folded > flame.svg   # 渲火焰图
# z42-trace.json → 打开 https://ui.perfetto.dev import（采样火焰图 over time）
```

| env knob | 作用 | 默认 |
|----------|------|------|
| `Z42_SAMPLE_HZ` | 采样频率（Hz，≥1 开启）；未设 = 关（**零成本**：无后台线程、热路径不受影响）| unset |
| `Z42_SAMPLE_OUT` | folded stacks 输出路径（inferno 格式）| `z42-samples.folded` |
| `Z42_TRACE_OUT` | chrome/perfetto 采样 trace 路径；设了才记时间线（省内存）| unset = 不写 |

**counter 快照**（`--print-stats-on-exit`）现由 `ProfileSnapshot` 输出，`--stats-format json`
出单行 JSON 供 `xtask profile` 抓取；脚本层经 `Std.Diagnostics.RuntimeStats.Counters()` 读同一组
计数（7 counter + allocations + GC 分代）。

## 当前可用

### 编译器（z42 自举）

z42c 是自举编译器（源码 `src/compiler/`，编译为 `z42c.driver.zpkg`），经 z42vm 跑：

```bash
z42c <args>                              # 需 stdlib 时前缀 Z42_LIBS="$PWD/.z42/libs"
```

要调试编译器执行本身，按 [VM（Rust）](#vmrust) 那样 lldb / gdb 附加跑 zpkg 的 z42vm 进程。

### VM（Rust）

```bash
# lldb（macOS / Linux）
lldb -- ./artifacts/build/runtime/debug/z42vm <file.zbc>

# gdb
gdb --args ./artifacts/build/runtime/debug/z42vm <file.zbc>

# rust-analyzer + VS Code launch.json：照常 cargo run / cargo test
```

### z42 用户代码

z42c 默认产出 debug-symbol 信息：

- **`.zbc`（debug build）**：DBUG section 内嵌 `LineTable` + `LocalVarTable`
- **`.zbc`（release build）+ `.zsym` sidecar**：split-debug-symbols；运行时按 BLAKE3-128 build_id 自动配对加载

VM trace 已自动显示 `<file>:<line>:<col>` + 局部变量名（DBUG section 加载到时）。详见 [`docs/design/runtime/vm-architecture.md`](../design/runtime/vm-architecture.md) "Sidecar 调试符号加载" 段。

```bash
# 产出 sidecar
./xtask build stdlib   # release 默认 strip + 产出 .zsym
```

## 0.8.7 之后（DAP debugger）

`z42-dap` 适配层启用后支持：

- VS Code / JetBrains 经 DAP 协议调试 z42 源码
- Step / breakpoint / locals / watch
- JIT / AOT 模式下的 step（详见 Q14 待裁决）

详见 [`docs/roadmap.md`](../roadmap.md) §0.8.x charter。

## 当前 dump 工具

```bash
# zbc 反汇编为 zasm 文本
z42c disasm <file.zbc>

# AST dump
z42c <file.z42> --dump-ast

# Build ID
z42c build-id <file.zbc | file.zpkg | file.zsym>
```
