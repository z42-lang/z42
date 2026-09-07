# test-runner 自举迁移路径

> 状态：**✅ 已落地**（retire-test-runner，2026-06-30）。本文跟踪的"Rust runner → z42
> runner"迁移已完成：runner 现为用 z42 写的 `Std.Test.Runner`（z42.test 库），由
> `z42.builder.zpkg`（z42b）承载、经 `z42vm` 运行；Rust `src/toolchain/test-runner/`
> 已删除。当前架构见 [testing.md](testing.md) 顶部说明。下文保留为迁移决策的历史记录。

z42 的长期目标是自举（compiler 用 z42 重写）。test-runner 是 toolchain 中相对独立的一块 —— 没有编译器那么深的 IR / TypeChecker 依赖，但确实需要一些 runtime introspection 能力。本文回答："runner 全 z42 化什么时候/怎么做？"

---

## 当前状态（已部分 z42 化）

| 组件 | 实现语言 | 位置 |
|---|---|---|
| 测试 attribute（`[Test]` / `[Skip]` / `[Benchmark]` / `[Setup]` / `[Teardown]` / `[ShouldThrow<E>]`） | z42 注解 | 编译期 attribute binder（C#） |
| Assert 库（`Std.Assert.*`） | **z42** | [src/libraries/z42.test/src/Assert.z42](../../src/libraries/z42.test/src/Assert.z42) |
| TestIO（`captureStdout` / `captureStderr` / `captureBoth`） | **z42** + native shim | [src/libraries/z42.test/src/TestIO.z42](../../src/libraries/z42.test/src/TestIO.z42) |
| Bencher（warmup + 统计） | **z42** | [src/libraries/z42.test/src/Bencher.z42](../../src/libraries/z42.test/src/Bencher.z42) |
| Discovery（TIDX 读取） | Rust | `src/runtime/src/metadata/test_index.rs` |
| Runner driver（dispatch + 隔离） | Rust | `src/toolchain/test-runner/` |
| Output formatters（pretty / TAP / JSON） | Rust | `src/toolchain/test-runner/src/format/` |

**结论：** "测试用户 API"层已经 100% z42；Rust 持有的是"调度器"那一层。

---

## 自举的核心阻塞：reflection / dynamic invoke

z42 当前**不能从脚本里调用一个名字直到运行时才知道的函数**。Runner 必须：

```
for entry in tidx_entries:
    invoke_by_name(entry.method_name, args=[])  # ← z42 当前没有这个能力
```

可选实现路径：

| 方案 | 描述 | 代价 |
|---|---|---|
| **A: 显式 reflection API** | `Std.Reflection.Invoke(funcName: string, args: object[])` | 需新 stdlib + 新 VM builtin（`__invoke_by_name`）+ 跨 zbc 函数表查找；类似 .NET `MethodInfo.Invoke` |
| **B: 编译期 codegen 入口表** | 编译器为每个 [Test] 自动生成一个 wrapper：`__test_dispatch(idx: int) -> void { switch idx { case 0: test_a(); case 1: test_b(); ... } }`；runner 调 dispatch(idx) | 不需 reflection，但每次新增 [Test] 重编整个 zbc；test-only artifact 多一个 dispatcher 函数 |
| **C: 函数指针表** | 编译期把所有 [Test] 函数装进 `Func[] tests`，runner 遍历 `tests[i]()` | 同 B，外加要求 z42 支持函数指针数组（已有 delegate 类型） |

A 最通用但最重；B/C 是"够用就好"的轻量方案。

短期看**B 或 C 更现实** —— 都是纯编译期改动，不引入新 VM 概念。但 A 对 reflection 的需求会随其他场景增加（serializer / DI container / mock framework 等），**长期 A 更值得做一次**。

---

## 分阶段迁移路径

按"哪些先 z42 化、哪些晚"排序：

### Stage 0（已完成）— 用户 API z42 化

z42.test 库本身：Assert / TestIO / Bencher / 注解。✅ 现状。

### Stage 1（短期可做，无前置）— Output formatters 迁 z42

`pretty` / `TAP` / `JSON` 三种 formatter 是纯字符串拼装，z42 完全够用。

- 输入：Rust runner 把 `TestResult[]` 序列化为简单 JSON 喂给 z42 formatter
- 输出：z42 字符串，Rust 直 print
- **前置：** 无；只需 `Std.Json.Parse` / `Std.Json.Stringify`（简化版即可，TestResult 结构很扁）

收益小（formatter 代码占 runner 总量 < 30%），但是个**练手 + 验证 z42 库扩展**的好机会。

### Stage 2（中期）— Discovery 迁 z42

让 z42 直读 zbc 字节、解析 TIDX section。

- 新 stdlib：`Std.Zbc.LoadTestIndex(bytes: byte[]): TestEntry[]`
- VM 端只需暴露 TIDX 解析的 builtin（已有 Rust 实现，包一层即可）
- **前置：** byte 数组 + 简单 binary read 能力（z42 的 `Span<byte>` / `BinaryReader` 类型 — 当前 stdlib 缺，但小工程量）

迁移后 runner 的"前半段"（discovery）是 z42；后半段（dispatch + isolate）仍 Rust。

### Stage 3（中后期）— Dispatch 迁 z42（关键里程碑）

需要前述 A/B/C 之一。建议路径：

1. 先做 B（codegen dispatcher）—— 一周内可落地，不引入 reflection
2. runner 主体迁 z42：load → discover → for each entry → call dispatcher
3. Rust 端只剩"启动 VM + 加载 runner.zbc + 加载 test.zbc + invoke runner.Main"的 ~50 行 hosting glue

完成此 stage 后，**runner 是 z42 程序**。Rust 部分缩到 hosting glue + VM 本身。

### Stage 4（长期，可选）— 把 dispatch 升级为 reflection (A)

当 z42 reflection / dynamic-invoke 落地（无论是否为 runner 驱动），把 codegen dispatcher 替换为 reflection 调用。runner 代码更短、更通用，能支持 plugin-style 测试发现。

---

## 跨平台架构的简化

在 Stage 3 之后，每平台的 binding 接口收窄到**一个**：

```
现在 (Phase 1-5)：
  Platform → run_zbc(test.zbc) → Rust runner 内部调度

Stage 3 之后：
  Platform → run_zbc(runner.zbc, with_args=[test.zbc]) → z42 runner 内部调度
```

各平台不再为 runner 维护独立绑定；只暴露"跑一段 z42 zbc"的最小接口。这与我们刚做的 [cross-platform-testing.md](cross-platform-testing.md) 的 library-first 思路天然配合。

---

## Tradeoff 一览

| 维度 | Rust runner（现状） | z42 runner（Stage 3+） |
|---|---|---|
| 自举对齐 | 不是 z42 自举的一环 | ✅ 是自举里程碑之一 |
| 平台绑定面 | 每平台一个 runner-specific binding | ✅ 收窄为单一 VM 入口 |
| 用户可扩展（自定义 runner / BDD / property-based） | 改 Rust + 重编 binary | ✅ 用户写 z42 即可 |
| 性能（host） | 快 | ⚠️ z42 interp 跑 z42 interp 慢 5-10x（200-500ms / load） |
| 启动时延 | 低 | ⚠️ 多一层 zbc load |
| 调试复杂度 | 一层 Rust 栈 | ⚠️ 两层 z42 调用栈，工具支持要跟上 |
| Bootstrap 风险 | 独立于 stdlib，stdlib 全坏 runner 仍能跑 | ⚠️ runner 依赖 stdlib，循环依赖风险 |

性能差异对 CI 可接受（test 总数 < 1000，整体增量 < 5min），但**交互式 `./xtask test changed` / IDE 集成**会感受到延迟。Stage 4 reflection 不会改善这个，只能靠 z42 整体性能优化（JIT 路径覆盖 / AOT cache）。

---

## 何时启动哪个 Stage

| Stage | 触发条件 | 预估工作量 |
|---|---|---|
| **Stage 1**（formatters） | `Std.Json` 库就绪 + 跨平台测试 Phase 5 完成 | 1-2 周 |
| **Stage 2**（discovery） | byte / BinaryReader 在 z42.io 落地 | 2-3 周（含 BinaryReader 设计） |
| **Stage 3**（dispatch via codegen B） | z42 编译器自举进入 60%（runner 是适合首批迁移的小 toolchain） | 4-6 周（含 codegen B + runner 重写 + 测试） |
| **Stage 4**（reflection A） | z42 reflection spec 独立落地（与 runner 解耦） | 视 reflection 范围，独立估 |

**不建议**为了 runner 独立做 reflection — 这是**逆向**驱动，会让 reflection 设计被一个用例带偏。等 reflection 因更广泛的需求（serializer / DI / etc.）启动时顺势接入。

---

## 当前周期内的明确决策

- **Phase 1-5（跨平台测试落地，2026-05-09 起）期内**：runner 保 Rust，**不动**
- z42.test 库的进一步丰富（property-based / parameterized / matchers）继续在 z42 写 —— 不阻塞
- Stage 1 的 `Std.Json` 库可以提前规划，但与本路径解耦
- 跨平台 binding 当前直接绑 Rust runner library 即可，不用为"未来 z42 化"做特殊适配 —— 真到 Stage 3 时 binding 接口自然收窄，**不必现在过度设计**

> 核心原则：**不要因为可能 z42 化就阻塞当前 Rust runner 演进。** 该加 feature 加 feature；迁移时机到了再迁。
