---
paths:
  - "src/runtime/**/*.rs"
---

# Rust VM 开发规范

## 错误处理

- 所有可能失败的函数返回 `anyhow::Result<T>`
- 内部 VM 错误（非用户错误）用 `bail!("...")` 或 `anyhow::anyhow!(...)`
- 领域错误类型（如 `BytecodeError`）用 `thiserror::Error` 定义
- **禁止** `unwrap()` / `expect()` 在非测试代码中出现；测试代码中允许使用

## 测试文件组织

**单元测试必须放到独立文件，不得内联在实现文件末尾。**

规则：
- 每个实现模块 `foo.rs` 的测试放在同级 `foo_tests.rs` 中
- 在 `foo.rs` 末尾用条件编译引用：`#[cfg(test)] mod foo_tests;`
- 集成测试放在 crate 级别 `src/runtime/tests/` 目录下
- 测试文件命名：`<module>_tests.rs`（单元）或 `test_<feature>.rs`（集成）

```rust
// foo.rs（实现文件，末尾只有一行引用）
#[cfg(test)]
mod foo_tests;

// foo_tests.rs（测试文件）
use super::*;

#[test]
fn test_something() { ... }
```

**目的：** 减少阅读实现文件时的 token 消耗，实现与测试逻辑分离。

## 指令集扩展

每次新增 `Instruction` variant，必须同时更新：
1. `bytecode.rs` — 枚举定义
2. `interp.rs` — `exec_instr` match 分支（不允许有 `_` 通配兜底）
3. `docs/design/runtime/ir.md` — 指令文档

## Value 类型

- `Value` 枚举是运行时动态类型，所有算术操作前必须匹配类型一致性
- 类型不匹配时 `bail!` 而不是静默转换

### 原生层不得在根集之外持有 `Value`（2026-09-14）

**GC 只看得见帧寄存器、static 字段、几个 arena 和 pinned roots。** 把 `Value` 放进任何其它 Rust 侧容器
（`Vec` / `HashMap` / mpsc 队列 / `parking_lot` 锁 / 线程闭包里的局部变量……），它就**没有根**——
只要那一刻没有别的 z42 引用，下一次回收就会收掉它，之后读到 `Null` 或别的对象。已出过两次：

- #617：`Thread.Start` 捕获的环境从 spawn 到进入 worker 帧之间只在 Rust 局部变量里；
- store-sync-values-in-heap：`Mutex` / `RwLock` / `Channel` 的值存在 Rust 侧容器里。

**首选：让值成为 z42 对象的字段**，原生层只提供机制（参照 `corelib/monitor.rs`）——追踪、写屏障、
随拥有者回收全都免费。**确实只能暂存**（跨线程移交这类短窗口）时，用 `pin_root` + RAII 守卫 unpin
（参照 `threading.rs` 的 `SpawnedEnvRoot`），并想清楚「谁拥有它、何时释放」，否则就是泄漏。
机制与反例见 [sync-primitives.md](../../docs/book/src/runtime/sync-primitives.md)。

### 阻塞的 native 调用必须 park（2026-09-14）

**线程卡在系统调用里就到不了字节码 safepoint；GC 要等「全世界停下」⇒ 一条没 park 的阻塞调用拖死整个进程的 GC。**
判据是「可能长时间不返回」，不是「通常很快」：网络读写 / connect / TLS 握手 / DNS（getaddrinfo 可以卡满解析超时）/
子进程管道读写（管道满就阻塞）/ `join` / 条件变量等待。三条，缺一不可：

1. **包 `NativeParkGuard`**，只包阻塞的那几行。
2. **park 区间内不许分配**：parked 线程造的对象不是任何根，并发回收会收掉它（#641；debug 构建由
   `debug_assert_not_native_parked` 当场炸）。错误先收成 Rust `String`，出 park 再造结果元组。
3. **不许攥着共享锁阻塞**：别的线程排在这把锁上时是**不 park** 的 —— 只 park 阻塞者本身，GC 照样等排队者。
   做法：锁下只取出/克隆句柄（`Arc`），放锁，再 park 着做 I/O（参照 `corelib/process.rs` 的 `ProcessSlot`、`network/tcp.rs` 的取出-放回）。

新增会阻塞的 builtin 时，照「阻塞线程 `parked_count == 1` → 解除阻塞 → 回到 0」写单测（参照 `process_tests.rs` 末尾、
`monitor_tests.rs`），并做一次阴性对照。

### wasm 上不能取时钟（2026-09-14，第三次回归）

**`wasm32-unknown-unknown` 没有 `std::time`：`Instant::now()` / `SystemTime::now()` 直接 panic
「time not implemented on this platform」，整个 VM trap。** 编译照过、native 测试全绿，只有
nightly `test-wasm-browser` 能照出来。已出过三次：#164（time builtins）、#165（GC `now_us`）、
fix-wasm-std-time（`Sampler::disabled()` 构造时取了 t0 → 每次 `loadZbc` 都 trap，nightly 红了三周）。

- **关着的探针不取时钟**：计时起点放进 `Option<Instant>`，开关打开才 `Some(Instant::now())`
  （参照 `gc/phase_timer.rs`、`gc/sampler.rs`）。
- **始终要跑的计时**（每次 GC、time builtin）：`#[cfg(target_arch = "wasm32")]` 给替代实现
  （参照 `gc/arc_heap/observe.rs` 的单调计数、`corelib/bench.rs`）。
- 只在 native 可达的路径（JIT、子进程、信号处理）不受限。

## 执行模式

- `ExecMode` 决定函数级别的分发路径
- 模块级默认模式 → `Vm::default_mode`；函数级注解优先
- JIT/AOT 后端在实现完成前**必须**返回 `bail!("... not yet implemented")`，不允许部分实现

## 序列化

- `Module`、`Function`、`Instruction` 等持久化类型必须 `#[derive(Serialize, Deserialize)]`
- 二进制格式使用 `bincode`；文本调试格式用 `serde_json`（可选依赖）

## 资源加载顺序

`std::fs::read_dir` / `HashMap` 迭代 + `or_insert` first-wins 等不确定性来源的处理规则见 [common-pitfalls.md §1](common-pitfalls.md#1-资源加载顺序必须显式排序2026-05-17-强化)。该规则跨语言适用（C# / Rust / bash 都涉及），统一在 common-pitfalls.md 沉淀。
