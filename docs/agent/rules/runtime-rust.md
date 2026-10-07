---
paths:
  - "src/runtime/**/*.rs"
---

# Rust VM 开发规范

## 错误处理

- 所有可能失败的函数返回 `anyhow::Result<T>`
- 内部 VM 错误（非用户错误）用 `bail!("...")` 或 `anyhow::anyhow!(...)`
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
1. `metadata/bytecode/instruction.rs` — 枚举定义
2. `interp/exec_*.rs`（`exec_instr` 分派）— match 分支（不允许有 `_` 通配兜底）
3. `docs/internals/src/formats/ir.md` — 指令文档

## Value 类型

- `Value` 枚举是运行时动态类型，所有算术操作前必须匹配类型一致性
- 类型不匹配时 `bail!` 而不是静默转换

### 原生层不得在根集之外持有 `Value`

**GC 只看得见帧寄存器、static 字段、几个 arena 和 pinned roots。** 把 `Value` 放进任何其它 Rust 侧容器
（`Vec` / `HashMap` / mpsc 队列 / `parking_lot` 锁 / 线程闭包里的局部变量……），它就**没有根**——
只要那一刻没有别的 z42 引用，下一次回收就会收掉它，之后读到 `Null` 或别的对象。典型场景：

- `Thread.Start` 捕获的环境从 spawn 到进入 worker 帧之间只在 Rust 局部变量里；
- `Mutex` / `RwLock` / `Channel` 的值存在 Rust 侧容器里。

**首选：让值成为 z42 对象的字段**，原生层只提供机制（参照 `corelib/monitor.rs`）——追踪、写屏障、
随拥有者回收全都免费。**确实只能暂存**（跨线程移交这类短窗口）时，用 `pin_root` + RAII 守卫 unpin
（参照 `corelib/threading.rs` 的 `SpawnedEnvRoot`），并想清楚「谁拥有它、何时释放」，否则就是泄漏。
机制与反例见 [sync-primitives.md](../../internals/src/runtime/sync-primitives.md)。

### 阻塞的 native 调用必须 park

**线程卡在系统调用里就到不了字节码 safepoint；GC 要等「全世界停下」⇒ 一条没 park 的阻塞调用拖死整个进程的 GC。**
判据是「可能长时间不返回」，不是「通常很快」：网络读写 / connect / TLS 握手 / DNS（getaddrinfo 可以卡满解析超时）/
子进程管道读写（管道满就阻塞）/ `join` / 条件变量等待。三条，缺一不可：

1. **包 `NativeParkGuard`**，只包阻塞的那几行。
2. **park 区间内不许分配**：parked 线程造的对象不是任何根，并发回收会收掉它（debug 构建由
   `debug_assert_not_native_parked` 当场炸）。错误先收成 Rust `String`，出 park 再造结果元组。
   同理**不许跑 z42 代码**：collector 会无锁读 parked 线程的帧栈，park 期间压 / 弹帧就是数据竞争
   （debug 构建由 `debug_assert_frame_change_not_parked` 炸）；确需回调 z42 先 `NativeUnparkGuard`。
3. **不许攥着共享锁阻塞**：别的线程排在这把锁上时是**不 park** 的 —— 只 park 阻塞者本身，GC 照样等排队者。
   做法：锁下只取出/克隆句柄（`Arc`），放锁，再 park 着做 I/O（参照 `corelib/process.rs` 的 `ProcessSlot`、`corelib/network/tcp.rs` 的取出-放回）。

新增会阻塞的 builtin 时，照「阻塞线程 `parked_count == 1` → 解除阻塞 → 回到 0」写单测（参照 `process_tests.rs` 末尾、
`monitor_tests.rs`），并做一次阴性对照。

这条不只防死锁，还是**注册协议的前提**：新 `VmContext` 在 `Marking` 期
不注册、等停顿结束，醒来后可能赢得 collector 角色并等所有已注册线程 park —— 若有线程卡在没 park 的调用里就永久死锁
（loom 模型 B′，`tests/gc_registration_race_loom.rs`）。测试里主线程 `join` 一个会触发 GC 的 worker 时同理。

### 堆引用写入必须走带 SATB 屏障的原语

**任何把引用写进堆对象 / 数组的代码，一律走 `ScriptObject::set_field_value` / `set_ref_slot`、
`ArrayObj::set_boxed` / `write_struct_elem` / `set_struct_ref` / `copy_elems_from` / `copy_elems_within`。** 这些原语在覆盖前把旧值交给 SATB 删除屏障
（`gc::satb::record_overwrite`）；绕过它们 = major 标记进行中可能漏标一个仍被使用的对象（从未扫描的对象里读出
引用放进寄存器、再清掉字段，该对象就会被扫掉）。

- **批量写（`clone_from_slice` / `copy_from_slice` / `copy_within` 作用于 `Value` 切片）同样是覆盖**：先对被覆盖区间 `record_overwrite_all`（参照 `copy_elems_from`）。
- **对象字段只经单元方法读写**（`metadata/types/object_fields.rs`：`field_value` / `try_set_field_value` / `visit_refs`，引擎侧经 `objops`）：基元是同宽 relaxed 原子、引用是 8 B 自描述字（release / acquire，标记期 swap），直接对 `bytes_mut()` 写会绕过内存序、种类编码与屏障。
- `refs_mut_raw()` 只给两种场景：**刚分配出来的对象**（旧值全是 `Null`，没有可漏的）和 **GC 自己**（给死对象断边，
  记录死对象只会把悬空句柄塞进标记队列）。新增调用点时在旁边写清是哪一种。
- 新增一种「在堆里存引用」的布局（新的 backing / 内联引用形态）时，读旧值 + `record_overwrite` 必须随写入原语一起加，
  并照 `gc/arc_heap_tests/incremental.rs` 的形状补一对「开屏障存活 / 关屏障被扫」的测试。
- **任何不经强引用把已有堆值交给 mutator 的路径**（弱 / 软引用读取、堆遍历）一律过 `ArcMagrGC::admit_resurrected`：
  标记期染色（快照时只剩弱引用的对象会被交还给寄存器），**增量 major 清扫期拒绝未标记的**（它是待清扫的死对象，
  子对象可能已被回收）。新增这类出口时照 `a_doomed_object_is_not_handed_out_while_the_sweep_has_not_reached_it` 补测试。
- GC 自己在增量周期里**追踪**已有对象（如 minor 的脏卡播种）时同理：清扫期跳过未标记条目（`doomed_unless_marked`）。

机制见 [gc-incremental-major.md](../../internals/src/runtime/gc-incremental-major.md)。

### wasm 上不能取时钟

**`wasm32-unknown-unknown` 没有 `std::time`：`Instant::now()` / `SystemTime::now()` 直接 panic
「time not implemented on this platform」，整个 VM trap。** 编译照过、native 测试全绿，只有
nightly `test-wasm-browser` 能照出来。典型场景：time builtins、GC `now_us`、`Sampler::disabled()` 构造时取 t0（→ 每次 `loadZbc` 都 trap）。

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

- `.zbc` / `.zpkg` 由 `metadata/zbc_reader` 手写解码（规格见 internals 的 formats 部分）；
  `Module` / `Function` / `Instruction` 等 IR 类型不 derive serde——它们只从 zbc 解码而来，没有文本往返

## 资源加载顺序

`std::fs::read_dir` / `HashMap` 迭代 + `or_insert` first-wins 等不确定性来源的处理规则见 [common-pitfalls.md §1](common-pitfalls.md#1-资源加载顺序必须显式排序)。该规则跨语言适用（Rust / z42 / bash 都涉及），统一在 common-pitfalls.md 沉淀。
