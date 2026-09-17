# 调试与运行时诊断

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`src/runtime/src/startup.rs`
> （`--info` / panic hook）、`src/runtime/src/signal_handler.rs`、`src/runtime/src/pal/signal.rs`、
> `src/runtime/src/app.rs`（`--stats`）、`src/runtime/src/observer.rs`、
> `src/runtime/src/config/`、`src/runtime/src/metadata/build_id.rs`、`scripts/xtask_profile.z42`
>
> 旋钮登记表与五层优先级见[运行时设置的实现](../runtime/runtime-settings.md)；
> 采样 profiler / 计数器面的**机制**见[诊断与性能分析](../runtime/diagnostics.md)。
> 这页是**照着敲**的那一面。

出了问题先抓什么、日志怎么开、崩了怎么拿栈、性能怎么量。

## 1. 先贴 `--info`

```bash
z42vm --info
```

一行一个 `key: value`，依次是版本 / `target` / `arch` / `build profile` / 编进去的
`features` / 实际可用的 `exec modes` / 两个配置文件层（`Z42_CONFIG` 用户配置、
`Z42_APP_CONFIG` 应用配置）的路径，最后是一整块 **runtime 旋钮快照**——用的是 VM 真正在跑的那份
`Resolution`，和 `--show-config` 同一个渲染器（渲染器自己重读一遍环境变量会让优先级链有两份
实现、必然漂移，所以它只渲染、不查询）。**提 bug 就贴这个**；跨机器对比构建配置也靠它。

同族的三个自省命令，都是打完即退：

```bash
z42vm --list-knobs [--all] [--json]    # 有哪些旋钮：类型 / 可设置层 / 本 build 可用性 / 默认值
z42vm --show-config [--all] [--json]   # 旋钮现在是什么值、来自哪一层、某层为什么没生效
z42vm --info
```

`--all` 连不推荐 / 内部旋钮一起列。改本次运行的旋钮用 `--set key=value`（可重复，
优先级最高）；`--strict-config` 把来自环境变量 / 配置文件的配置问题从警告升级为致命错误
（CI 用它把配置漂移变成硬失败；命令行 `--set` 的问题一律致命，不受它影响）。

## 2. 日志：`Z42_LOG`

`tracing-subscriber` 的 directive 语法，`=` 定 level，逗号分隔多条规则。默认 `z42=warn`；
`--verbose` 等价于 `--set log=z42=info`；`Z42_LOG` 覆盖前两者。

```bash
Z42_LOG=z42::jit=debug,z42::gc=trace,z42=warn ./z42vm script.zbc
Z42_LOG=z42=debug ./z42vm script.zbc
Z42_LOG=z42=warn ./z42vm --verbose script.zbc      # 即便 --verbose 也安静
```

target 就是各个 mod 自己的路径：`z42` / `z42::interp` / `z42::jit` / `z42::gc` /
`z42::native::*` / `z42::metadata::*` …

## 3. 崩溃抓盘：`Z42_CRASH_DIR`

VM 内部 panic（`unwrap` / 越界 / `debug_assert!` 失败）**和** OS 信号默认都把诊断打到 stderr
然后 abort。设了 `Z42_CRASH_DIR` 会**额外**把报告落到 `<dir>/z42vm-crash-<ts_ns>.txt`：

```bash
mkdir -p /var/log/z42
Z42_CRASH_DIR=/var/log/z42 RUST_BACKTRACE=1 ./z42vm script.zbc
```

- **Rust panic 报告**：z42vm 版本 / `target` / `arch` / build profile / panic 位置 /
  payload / Rust backtrace（详细程度由 `RUST_BACKTRACE` 控制）。
- **OS signal 报告**：信号名 / build banner / **所有线程的 z42 调用栈**（一行一帧，
  `#<idx>  <func_name> at <file>:<line>:<col>`）。写完后把 handler 重置成 `SIG_DFL` 再
  `raise()`，让 kernel 走默认 abort + coredump（`ulimit -c unlimited` 仍生效）。

捕获五个信号：

| 信号 | 典型触发 |
|---|---|
| `SIGSEGV` | 坏指针 —— JIT bug、native FFI 写坏内存 |
| `SIGABRT` | `libc::abort()` —— native 模块内部断言失败 / glibc OOM |
| `SIGFPE` | 整数除零 —— z42 解释器已挡，native / JIT 路径的漏网 case |
| `SIGILL` | 非法指令 —— JIT code 被写坏 |
| `SIGBUS` | 对齐错误 / mmap 越界 —— ARM64 上偶发 |

**锁争用时降级不死锁**：报告要 `try_lock` 拿 VM 核心注册表和每个线程的 `call_stack`。
信号触发时若另一线程持锁（比如 GC mark 阶段），handler 写
`<call stack lock contended>` 占位符——不死锁、不丢报告，进程照常 abort。

信号捕获目前只在 POSIX（macOS / Linux）；Windows build 编译得过但没有信号捕获。

## 4. 运行计数：`--stats`

```bash
z42vm script.zbc --stats          # 人读的文本块（等价 --stats=text）
z42vm script.zbc --stats=json     # 单行 JSON，供工具抓取
```

⚠️ 可选值**必须写成 `--stats=json`**（`require_equals`）。不带等号时 clap 会把紧跟其后的
位置参数当成它的值——`z42vm --stats app.zpkg` 会报 `invalid value 'app.zpkg'`。

输出的是一份 `ProfileSnapshot`：7 个 counter（builtin / native call、JIT 编译数与耗时、
异常 throw/catch…）+ 堆派生的 allocations + GC 分代 + 并发探针。程序正常退出后打到 stderr。
脚本侧读同一组计数用 `Std.Diagnostics.RuntimeStats.Counters()`，见
[参考手册 · diagnostics](../../../reference/src/stdlib/diagnostics.md)。

`--info` 是 boot 期的构建信息，`--stats` 是运行期计数，两个可以同时给。

## 5. `xtask profile`：一条命令四个维度

```bash
xtask profile <script.z42> [--cpu|--heap|--threads|--e2e|--all] [--mode interp|jit]
```

不给维度旗标就四个全跑并写 `report.md`。

| 维度 | 产物 | 依赖工具（缺则跳过 + 提示，不让整次 profile 失败） |
|---|---|---|
| `--cpu` | samply 的 native 火焰图（Rust / JIT machine 栈）+ **z42 级**采样火焰图（`Main;foo;bar` folded → inferno SVG）+ perfetto 采样 trace | `samply`、`inferno-flamegraph`、perfetto UI |
| `--heap` | dhat 堆分析 `dhat-heap.json` + counter 摘要 | 无外部依赖（现建一个开 `dhat-heap` feature 的一次性 VM） |
| `--threads` | safepoint park 直方图 + 用户锁争用 + counter 摘要 | 无（争用那半边现建一个开 `profile-contention` 的 VM） |
| `--e2e` | hyperfine wall-clock + peak RSS + counter 摘要 | `hyperfine` |

z42 级 CPU 采样也能直接经 env 用，不必走 xtask：

```bash
Z42_SAMPLE_HZ=4000 Z42_SAMPLE_OUT=z42-samples.folded Z42_TRACE_OUT=z42-trace.json \
  z42vm script.zbc <entry> --mode jit
inferno-flamegraph z42-samples.folded > flame.svg
# z42-trace.json → https://ui.perfetto.dev 导入
```

| env | 作用 | 默认 |
|---|---|---|
| `Z42_SAMPLE_HZ` | 采样频率（Hz，≥1 才开）。未设 = 关，且是**零成本**：不 spawn 后台线程、热路径不受影响 | unset |
| `Z42_SAMPLE_OUT` | folded stacks 输出路径（inferno 格式） | `z42-samples.folded` |
| `Z42_TRACE_OUT` | perfetto 采样 trace 路径；设了才记时间线（省内存） | unset = 不写 |

采样复用的是 GC 已有的协作式 safepoint 轮询，不引信号处理器、不 ptrace——为什么这么选见
[诊断与性能分析](../runtime/diagnostics.md)。

## 6. 事件流：`RuntimeObserver`（嵌入宿主用）

VM 用 push-based 事件流通知**非 GC** 的运行时活动；GC 事件留在独立的 `GcObserver`。
宿主经 `VmContext::add_runtime_observer(Arc<dyn RuntimeObserver>)` 注册。

`RuntimeEvent` 的变体：`ModuleLoaded`（eager 加载与 lazy zpkg 解析都发）、
`JitModuleCompiled`（模块名 + 函数数 + 编译微秒）、`ExceptionThrown` / `ExceptionCaught`、
`NativeCallEntered`、`Custom`。

```rust
use std::sync::Arc;
use z42::observer::{RuntimeObserver, RuntimeEvent};

#[derive(Debug)]
struct JsonLineExporter;

impl RuntimeObserver for JsonLineExporter {
    fn on_event(&self, evt: &RuntimeEvent) { eprintln!("{evt:?}"); }
}

ctx.add_runtime_observer(Arc::new(JsonLineExporter));
```

回调的三条约束：**Send + Sync**（宿主可能跨线程转发到 async runtime / 监控管线）；
**不许 panic**（fire 循环不 catch unwind，panic 会 abort 整个进程）；
**快速返回**（重活丢 channel 给 worker 线程，别阻塞热路径）。注册表是
`Mutex<Vec<Arc<dyn RuntimeObserver>>>`，fire 时先取快照再放锁，所以在回调里重入
`add_runtime_observer` 不会死锁。

## 7. 源码级符号

z42c 默认产出 debug 符号信息：

- **debug build 的 `.zbc`**：`DBUG` section 内嵌 `LineTable` + `LocalVarTable`；
- **release build 的 `.zbc` / `.zpkg` + `.zsym` sidecar**：split-debug-symbols。

VM 运行时自动探测同目录的 `.zsym` 并按 **build_id 配对**合并，trace 里就带上源码
`file:line:col` + 局部变量名。build_id 是 16 字节内容标签，编译器同时写进主文件的 `BLID`
section 和 sidecar；**运行时从不重算它**，配对就是两个存储值的相等比较。因此它不是安全边界，
writer 刻意用快的非加密哈希（MurmurHash3 x86_128）而不是 BLAKE3——z42c 是解释执行的，
BLAKE3 要贵约 15 倍。（别和 indexed zpkg 里散装 `.zbc` 的 `zbc_hash` 搞混，那个**会**被
运行时重算，用的是 BLAKE3-128，是真的跨语言契约。）

`./xtask build stdlib` 的 release 路径默认 strip 并产出 `.zsym`。

拿到一份已 strip 的崩溃 trace（`at <fn> +0x<off>` 形式的帧）可以离线还原：

```bash
z42d symbolicate <trace> -s <file.zsym | 一个装 .zsym 的目录>   # -s 可重复，目录递归搜
```

## 8. 附加调试器

调 Rust 侧（VM 本体、或想看编译器执行本身时附到跑 `z42c.driver.zpkg` 的那个 z42vm 进程）：

```bash
lldb -- ./artifacts/build/runtime/debug/z42vm <file.zbc>
gdb --args ./artifacts/build/runtime/debug/z42vm <file.zbc>
```

rust-analyzer + VS Code 的 `launch.json` 照常 `cargo run` / `cargo test`。

z42 **源码级**的断点 / 单步（`z42d dbg`）目前是 scaffold，子命令都还是 planned；
`z42d` 上真正能用的是 `symbolicate`。

看编译中间产物用 `z42c --dump-*`，见[开发环境与工具链自举 §5](dev-setup.md#5-z42c-的编译阶段转储)。
