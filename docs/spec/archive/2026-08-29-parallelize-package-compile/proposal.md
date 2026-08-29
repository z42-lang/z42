# Proposal: 包编译文件级并行 + z42c 多线程编译框架

## Why

z42c 编译一个包（成员）时，符号收集（`SymbolCollector.CollectAll`，全包一次）之后是一个**严格串行的 per-file 循环**（[IrDump.z42:155-172](../../../../src/compiler/z42c.semantics/src/IrDump.z42#L155)）：逐文件 typecheck（`TypeChecker.Infer`）+ codegen（`IrGen.Generate` → 独立 `IrModule`）。这段是**近乎 embarrassingly parallel**——每文件产独立 IR、写独立输出槽、共享输入在循环前冻结只读——却被串行化。类似地，源文本读取 + 每文件 SHA-256（[Main.z42:145-161](../../../../src/compiler/z42c.driver/src/Main.z42#L145)，解释执行的 SHA-256 曾是增量差价主项）与「读 cache/IR → 写 dist」的组装 I/O 也是 per-file 独立、可并行。

z42 runtime 的多线程地基（`z42.threading` 的 Thread/Mutex/RwLock/Channel、线程安全共享 GC 堆 + safepoint STW、per-thread 执行状态）**已完整落地并有测试**，但 z42c 从未把它当 workload。本变更用「编译器并发编译自己的文件」dogfood 并完善这套支持，同时抽出一个**可复用的 z42c 并行框架**。

**范围抉择（已与 User 敲定）**：先做**文件级并行（策略①，低风险、易并行）**，不做成员级（层）并行。诚实记录取舍：真正的墙钟大头是 **DepScan（~60%，每成员 ~850ms，在 per-file 循环之外）**，文件级并行打的是 codegen + 源哈希 + I/O（那 ~40%），受 Amdahl 限制上限约 1.3–1.5×；成员级并行（并发 DepScan）留作后续 phase（见 Out of Scope）。

## What Changes

1. **多线程编译框架**（z42c 内部，建于既有 `z42.threading`）：`ParallelFor(n, jobs, IParallelBody)` 工作池——`jobs` 个 worker 线程从共享游标取 `[0,n)` 索引、各写独立输出槽（确定性、顺序无关）；`jobs<=1` 退化串行内联（逃生舱）。回调用**接口派发**（`IParallelBody.Run(int)`），非跨 zpkg delegate（[[z42c-no-cross-pkg-delegates]]）；框架内部才用 `Thread.Start(Action)` 包裹。
2. **并行段共享可变状态根治（前置重构）**：并行前必须**冻结**所有 `SymbolTable` 变更，段内对共享态严格只读。
   - `SymbolTable.CurrentAliases`（per-file 覆写）→ `WithAliases` 浅拷贝视图（共享其余 7 个只读字段引用 + 自带 aliases），消除共享可变。
   - `ConstraintChecker.Resolve` 写 `symbols.ClassConstraints`（[ConstraintChecker.z42:31](../../../../src/compiler/z42c.semantics/src/ConstraintChecker.z42#L31)）→ 审计：若在 per-file `Infer` 内，则 hoist 到 CollectAll 前置（约束是声明级、包全局）；若已在前置，无需改。
3. **并行化三处 per-file 段**：
   - per-file codegen 循环（`IrDump.BuildPackageCus`）。
   - 源读取 + SHA-256（`Main.z42:145-161`）。
   - cache/IR→dist 组装 I/O（indexed dist 写 / packed 前的 per-file 读）。
4. **`--jobs N`** 控制并发度（默认 `OperatingSystem.ProcessorCount`；`--jobs 1` 串行）。
5. **确定性保证**：并行产物与串行**逐字节一致**（parallel==serial + 自举 gen1==gen2 不动点）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.pipeline/src/ParallelFor.z42` | NEW | 并行框架：`IParallelBody` 接口 + `ParallelFor` 工作池（建于 z42.threading） |
| `src/compiler/z42c.pipeline/z42c.pipeline.z42.toml` | MODIFY | 加依赖 `z42.threading` |
| `src/compiler/z42c.driver/z42c.driver.z42.toml` | MODIFY | 加依赖 `z42.threading`（源读取/组装并行在 driver） |
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | MODIFY | `WithAliases` 浅拷贝视图；`ResolveType` 读本视图 aliases |
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | 用 per-file 视图替代 `symbols.CurrentAliases=` 覆写 |
| `src/compiler/z42c.semantics/src/ConstraintChecker.z42` | MODIFY | 若 Resolve 在 per-file 内 → 约束解析 hoist 到 CollectAll（否则不改） |
| `src/compiler/z42c.semantics/src/IrDump.z42` | MODIFY | per-file 循环改 `ParallelFor` fan-out |
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | 源读取+SHA / 组装 I/O 改 `ParallelFor`；`--jobs` 解析 + 传导 |
| `scripts/build/xtask_compiler.z42` 相关 | 只读参考 | 冷启动种子供给（z42.threading 自依赖，见 design 决策 3） |
| `src/compiler/z42c.pipeline/tests/parallel_for/` | NEW | ParallelFor 单测（确定性、jobs=1 等价、异常聚合） |
| `src/compiler/z42c.driver/tests/parallel_compile/` | NEW | 并行编译 golden：多文件包，`--jobs>1` 产物与 `--jobs 1` 逐字节一致 |
| `docs/book/src/compiler/` 编译管线/自举机制页 | MODIFY | 框架 + 确定性保证 + `--jobs` + 冻结铁律（知识上浮） |
| `src/compiler/z42c.pipeline/README.md` / `z42c.driver/README.md` / `z42c.semantics/README.md` | MODIFY | 功能索引：ParallelFor / --jobs / WithAliases |

**只读引用**：`src/libraries/z42.threading/`（Thread/Channel API）、`src/libraries/z42.core/src/OperatingSystem.z42`（ProcessorCount）、`src/runtime/src/gc/safepoint.rs`（STW-under-load 理解）、`.claude/rules/bootstrap-seed.md`（轴④自依赖纪律）。

## Out of Scope

- **成员级（层）并行**（并发 DepScan，打真正的瓶颈）——留后续 phase；本变更只做文件级。
- **重开 workspace 增量编译**（当前 `noIncremental=true`）——正交，与 todo #1 一起做。
- **DepScan 本身提速 / GC 并发标记默认化**——各自独立评估。
- **成员内 typecheck 与 codegen 拆两遍并行**——本变更保持 per-file `Infer`+`Generate` 交织，只把整个 `_compileCu` 作为并行单元。

## Open Questions

- [ ] z42c→z42.threading 新依赖的**冷启动种子**：z42.threading 是否已在上一 nightly？冷启动是否需 `_ensureBootstrapSelfDepLibs` 预建？（design 决策 3，Phase 0 gate）
- [ ] `ConstraintChecker.Resolve` 究竟在 per-file `Infer` 内还是 CollectAll 前置？（Phase 1 审计确认）
- [ ] `--jobs` 默认全用满核，还是留 1 核给 GC collector？（design 决策 5）
- [ ] 是否拆两个 PR：PR1=框架+状态冻结重构（串行验证），PR2=打开并行？（design 决策 6）
