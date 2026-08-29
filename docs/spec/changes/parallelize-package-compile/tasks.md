# Tasks: 包编译文件级并行 + z42c 多线程编译框架

> 状态：🟡 进行中（User 已确认，2026-08-29）| 类型：feat（vm/build 行为，规范先行）

## 进度概览
- [ ] Phase 0：框架 + z42.threading 依赖/种子接线
- [ ] Phase 1：并行段共享可变状态根治（串行验证，零漂移）
- [ ] Phase 2：打开三处并行 + --jobs
- [ ] Phase 3：确定性/不动点/冷启动验证 + 文档

> **User 裁决（2026-08-29）**：① 一次性做完（单 PR）；② 量测对照 jobs；③ threading 现有原语够用（未加新 API）。
>
> **⚠️ 实测结论（2026-08-29）——并行默认关闭、opt-in**：文件级并行**代码完成且 byte-identical 验证通过**
> （串行 == --jobs 1 == 默认并行，自举 gen1==gen2 逐字节一致），但**实测净负**：workspace build 墙钟
> jobs=1→35.8s / 2→35.6s / 4→38.3s / 8→53.9s / 24→69s，**越多越慢**。根因=当前**共享-STW-堆 GC** 的
> **每-alloc 全局锁**（region bump + inner ×4-5，无 TLAB）+ STW 频率随线程数放大；`Z42_GC_MODE=concurrent`
> 实验几乎无效（证伪 STW-pause 是主因）。→ **默认串行，`--jobs N` opt-in**（休眠），待 GC 线程模型改进
> （per-thread 分配 / TLAB，见 design「实测与 GC 瓶颈」）能 scale 再翻默认。本 PR 落地的是**框架 + 根因
> 冻结重构（byte-identical，纯收益）+ 并行接线（休眠）**，为 GC 改进备好可测量的消费者。

## Phase 0：框架 + 依赖/种子接线
- [ ] 0.1 **审计确认**：`ConstraintChecker.Resolve`（写 `ClassConstraints`）在 per-file `Infer` 内还是 CollectAll 前置？`Z42Type`/`Z42ClassType` codegen 期有无惰性 memo 回填？→ 决定 Phase 1 范围（design 决策 4b/Impl Notes 残留）
- [ ] 0.2 **种子 gate**：核验 z42.threading 已在上一 nightly libs（`xtask test bootstrap` + nightly SDK `libs/z42.threading.zpkg`）；确认其依赖闭包仅 leaf（z42.core+builtin，无新环）
- [ ] 0.3 `z42c.pipeline` / `z42c.driver` manifest 加依赖 `z42.threading`
- [ ] 0.4 冷启动预建：若需，仿 `_ensureBootstrapSelfDepLibs` 让 z42.threading 冷启动先建（`scripts/build/xtask_compiler.z42`）
- [ ] 0.5 `ParallelFor.z42`（新）：`interface IParallelBody { void Run(int i); }` + `ParallelFor(n, jobs, body)` 工作池（jobs 线程×共享游标×独立槽；jobs<=1 内联；异常聚合）
- [ ] 0.6 `ParallelFor` 单测（确定性 / jobs=1 等价 / 异常 / n=0,1 边界）

## Phase 1：共享可变状态根治（串行验证，零漂移）
- [ ] 1.1 `SymbolTable.WithAliases`（浅拷贝视图，共享 7 只读字段 + 自带 aliases）+ 私有 `_share` ctor
- [ ] 1.2 `TypeChecker` / `CuCompile._compileCu`：起始建 per-file 视图 `local = symbols.WithAliases(BuildAliases(cu))`，替代 `symbols.CurrentAliases=` 覆写；下游全用 `local`
- [ ] 1.3 若 0.1 判定 `ClassConstraints` 写在 per-file 内 → hoist 约束解析到 CollectAll 后一次性冻结；否则记录「已前置，无需改」
- [ ] 1.4 **串行零漂移验证**：改完（并行**尚未打开**）跑 `xtask test compiler` 自举不动点 + golden，确认产物**零字节漂移**（纯根因重构不改行为）

## Phase 2：打开并行 + --jobs
- [ ] 2.1 `--jobs N` 解析（CLI）+ 传导到 `_build`/`BuildPackageCus`；默认 `OperatingSystem.ProcessorCount`
- [ ] 2.2 ★② `IrDump.BuildPackageCus` per-file 循环 → `CompileCuTask : IParallelBody` + `ParallelFor`
- [ ] 2.3 ★① `Main.z42:145-161` 源读取+SHA-256 → `ParallelFor`
- [ ] 2.4 ★③ 组装 I/O（indexed dist 写 / packed 前 per-file 读）→ `ParallelFor`
- [ ] 2.5 并行 golden（多文件含 alias+where，`--jobs 1` vs `--jobs 8` 逐字节一致）

## Phase 3：验证 + 文档
- [ ] 3.1 `cargo build` z42vm + `xtask test`（完整 GREEN gate）
- [ ] 3.2 自举不动点 gen1==gen2（并行路径）
- [ ] 3.3 冷启动本地验证（下载上一 nightly 种子跑 cold `build compiler`）
- [ ] 3.4 墙钟量测：`--jobs 1` vs `--jobs N` 编 stdlib/compiler（确认加速 + 零漂移）
- [ ] 3.5 spec scenarios 逐条覆盖确认
- [ ] 3.6 文档：book 编译管线/自举页（框架+确定性+--jobs+冻结铁律）+ 三个 README 功能索引

## 备注 / 已知风险
- **自举轴④**（z42c 运行期新依赖 z42.threading）：冷启动本地必验，别只验 warm（教训 [[stdlib-interop-and-repl-split-program]]）。
- **Amdahl 上限**：文件级只打 codegen+SHA+I/O（~40%），DepScan（~60%）仍串行 → 整体上限 ~1.3–1.5×。成员级（打 DepScan）留后续。
- **GC-under-load**：并发分配触发 STW，可能封顶加速；先默认全核，实测再调。
- **确定性是硬验收**：任何字节漂移 = 失败；索引槽写 + 段前冻结是保证。
