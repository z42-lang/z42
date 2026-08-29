# Design: 包编译文件级并行 + z42c 多线程编译框架

## Architecture

### 编译管线里的并行边界

一个包的编译（`z42c.driver/_build` → `PackageCompile.Compile` → `IrDump.BuildPackageCus`）分三段：

```
[串行 setup]                     [★并行段：per-file，独立]           [串行 assemble]
源发现/读取 ─┐
             ├ SHA-256(per-file) ★①  ┐
DepScan ─────┤                        │
CollectAll ──┤ (符号表全包冻结)        ├ per-file: Infer+Generate ★② ┐
             │                        │   → 独立 IrModule[i]         ├ 组装 zpkg ─┐
             └────────────────────────┘                             │  (顺序)    ├ 落盘
                                          cache/IR→dist 写 ★③ ───────┘            │
                                                                                  └ 诊断门
```

三个 ★ 段是 per-file 独立、可并行；其余串行。**铁律：并行段前，所有跨文件共享的可变状态必须冻结；段内只读（除 per-file-local 数据）。**

- ★① 源读取 + SHA-256（`Main.z42:145-161`）：per-file 读文本 + 算 hash，写独立 `texts[i]`/`srcHashes[i]`。SHA-256 解释执行 CPU-heavy → 并行有实益。
- ★② per-file typecheck+codegen（`IrDump.z42:155-172` 的 `_compileCu`）：每文件 fresh `TypeChecker`/`IrGen`/`DiagnosticBag`，产独立 `IrModule` 写 `r[i]`。**主并行段**。
- ★③ 组装 I/O：indexed 模式 per-file 写 dist `.zbc`（`IndexedDist.z42`）/ packed 前 per-file 读 —— 各写各路径。

### 并行框架分层

```
z42c.pipeline/ParallelFor.z42   ← 框架（本变更新增）
  ├ interface IParallelBody { void Run(int i); }     ← 跨 zpkg 安全的回调契约（接口派发，非 delegate）
  └ ParallelFor(int n, int jobs, IParallelBody body) ← 工作池：jobs 线程 × 共享游标 × 独立槽写
        └ 内部用 z42.threading.Thread.Start(Action)  ← Action 仅在本模块内，不跨 zpkg
        └ 内部用 z42.threading.Mutex / 原子游标      ← 取 index
消费方（z42c.semantics/IrDump、z42c.driver/Main）实现 IParallelBody 的小任务类，字段�is捕获上下文。
```

## Decisions

### Decision 1：框架建于 z42c 内部，用既有 z42.threading（不加新 stdlib API）

**问题**：并行原语放哪？
**选项**：A — 给 `z42.threading` 加 `Parallel.For` 新 stdlib API；B — 在 z42c 内部写 `ParallelFor`，只用既有 `Thread`/`Mutex`/`Channel`。
**决定**：**B**。理由：① A 是新 stdlib API，z42c 若立即用它会踩自举两-nightly 纪律（种子 z42c 编不了用了未发布 API 的源，[bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 轴②）；B 只用**已发布**的 Thread/Mutex（见决策 3 的种子核验），无此约束。② 框架是 z42c 编译期私有工具，不属通用 stdlib 面。将来若通用化再上浮 `z42.threading.Parallel`（那时走两-nightly）。

### Decision 2：回调用接口派发，不用跨 zpkg delegate

**问题**：`ParallelFor` 的 body 怎么传？
**决定**：`interface IParallelBody { void Run(int i); }`，消费方定义小任务类实现之。**不**用 `Action<int>` 跨 zpkg 传——[[z42c-no-cross-pkg-delegates]]：命名 delegate 跨 zpkg 丢 FQ 名塌成结构类型 → 消费方 E0443。框架**内部**才用 `Thread.Start(Action)`（Action 是本模块局部 lambda，包裹 `body.Run(i)`，不跨 zpkg）。任务类把捕获的上下文（cus/symbols/输出数组等）作字段，`Run(i)` 处理第 i 项、写 `out[i]`。

### Decision 3：z42c→z42.threading 新依赖的自举种子处理（轴④）

**问题**：z42c 新增运行期依赖 z42.threading（并行编译时加载它跑 Thread）。冷启动种子须供给。
**事实**：z42.threading 已是 stdlib workspace 成员（`z42.workspace.toml:46`），但 z42c 各子包**当前不依赖它**——这是新跨包运行期自依赖（[bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 轴④，同 `converge-z42c-ir-metadata` 把 z42.ir 拉进 z42c 运行期的情形）。
**决定**：
1. **Phase 0 gate**：核验 z42.threading 已在**上一个已发布 nightly** 的 libs 里（`xtask test bootstrap` + 检查 nightly SDK `libs/z42.threading.zpkg`）。若在 → 种子可用。
2. **冷启动预建**：若冷启动 flat dist 尚无 z42.threading（种子只带旧闭包），仿 `_ensureBootstrapSelfDepLibs`（`scripts/build/xtask_compiler.z42`）在建 z42c 前用种子 driver 先把当前源的 z42.threading 单独编进 build-libs。**本地必验冷启动路径**（下载上一 nightly 作种子跑一遍 cold `build compiler`），别只验 warm（轴④教训）。
3. z42.threading 依赖闭包须仅 leaf（依赖 z42.core prelude + runtime builtin），不引入新环——Phase 0 确认。

### Decision 4：确定性 —— 索引槽写 + 段前冻结

**问题**：并行如何保证自举字节不动点（gen1==gen2）与 parallel==serial 逐字节一致？
**决定**：
- **输出按索引写独立槽**（`r[i]`/`texts[i]`/dist 各自路径），完成顺序不影响结果——产物只由「输入 + 索引」决定，与线程调度无关。
- **段前冻结**：★② 前 `SymbolTable` 全部数据字段（Classes/Functions/…）已在 `CollectAll` 建好、段内只读；唯二 per-file 覆写点 `CurrentAliases`（→ 决策见下）与 `ClassConstraints`（→ 审计 hoist）消除后，段内对共享态零写。
- **诊断**已 per-file 隔离（各 `DiagnosticBag`）再顺序聚合（`IrDump.z42:174`），无序依赖。
- **验证**：新 golden `--jobs 1` vs `--jobs 8` 逐字节 diff + 现有自举 gen1==gen2 不动点（本身就是 workspace 全量重建两遍比对）。

#### Decision 4a：`CurrentAliases` 根治 = `WithAliases` 浅拷贝视图

`SymbolTable` 8 个字段，段内只 `CurrentAliases`（per-CU 别名）会变（写 `TypeChecker.z42:276`，读 `ResolveTypeP` `SymbolTable.z42:103`）。根治：

```z42
// 浅拷贝：共享其余 7 个只读字段引用，仅换 CurrentAliases。段内每文件持自己的视图，零共享可变。
public SymbolTable WithAliases(StrMap aliases) {
    SymbolTable t = SymbolTable._share(this);   // 私有 ctor，7 字段引用直传（不复制内容）
    t.CurrentAliases = aliases;
    return t;
}
```

`_compileCu` 起始 `SymbolTable local = symbols.WithAliases(SymbolTable.BuildAliases(cu))`，之后 typecheck+codegen 全用 `local`。共享数据只读、别名 per-file 隔离。（`CollectAll` 阶段的 3 个 `CurrentAliases` 写在并行段**之前**，不受影响。）

#### Decision 4b：`ClassConstraints` 写 —— 审计后 hoist 或确认前置

`ConstraintChecker.Resolve` 写 `symbols.ClassConstraints`（`ConstraintChecker.z42:31`）。Phase 1 审计其是否在 per-file `Infer` 内被调：
- 若**在前置**（声明期/CollectAll）→ 不在并行段，无需改。
- 若**在 per-file 内** → hoist：约束是声明级、包全局，应在 `CollectAll` 后一次性 Resolve 全包约束，冻结进 `SymbolTable`，段内只 `Check`（只读）。

### Decision 5：`--jobs` 语义与默认

- `--jobs N`：worker 线程数。`N<=1` → 串行内联（逃生舱：格式-bump 窗口 / 确定性审计 / 调试）。缺省 → `OperatingSystem.ProcessorCount`（z42.core，种子必有）。
- **GC-under-load**：并行大量分配 → 频繁 STW safepoint，GC 在 safepoint 串行化，封顶加速比。先默认全核；若实测 GC 成瓶颈，再评估默认留 1 核 / opt-in `ConcurrentMarkSweep`（不在本变更）。
- 传导：driver `_build`/`BuildPackageCus` 接 `jobs` 参数；workspace 各成员用同一 `jobs`（成员级仍串行，文件级并行在每成员内）。

### Decision 6：分两个 PR（建议）

| PR | 内容 | 验证 |
|----|------|------|
| **PR1 基座** | 框架 `ParallelFor` + z42.threading 依赖/种子接线 + 状态冻结根治（WithAliases + ClassConstraints hoist）。**先不打开并行**（`--jobs` 默认 1 或消费点仍串行）。 | 纯重构：串行产物**零字节漂移** + 自举不动点 + 冷启动种子 |
| **PR2 打开并行** | ★①②③ 三处 fan-out 到 ParallelFor + `--jobs` 默认核数。 | parallel==serial 逐字节 golden + 不动点 + 墙钟量测 |

理由：PR1 是「有风险的根因重构 + 自举种子」，独立验证零漂移最安全；PR2 才引入并发行为。若 User 要一次做完也可合并，但分两 PR 更符合「一次一逻辑单元」+ 降低自举风险。

## Implementation Notes

- **ParallelFor 结构**：`jobs` 个 `Thread`，共享一个 `Mutex` 保护的 `int` 游标（或 z42.threading 原子）；worker 循环 `{ i = nextIndex(); if i>=n break; body.Run(i) }`；主线程 `Join` 全部。异常：worker 内 `body.Run` 抛 → 记录到 per-worker 槽，Join 后主线程聚合重抛/汇报（不吞）。`jobs<=1` 直接 `for i: body.Run(i)` 内联（不起线程，零开销、便于调试）。
- **任务类样板**（IrDump ★②）：`class CompileCuTask : IParallelBody { CompilationUnit[] cus; ...; IrModule[] outR; void Run(int i){ outR[i] = CuCompile._compileCu(cus[i], ...); } }`。
- **线程数上限**：`min(jobs, n)`——n 个文件不必起超过 n 个线程。
- **确定性再校验点**（Decision 4 残留）：审 `Z42Type`/`Z42ClassType` 在 codegen 期有无惰性 memo 回填（并行前须确认这些类型对象只读）。Phase 1 一并审。

## Testing Strategy

- **单元**：`ParallelFor` 测确定性（结果与串行同）、`jobs=1` 等价、异常聚合、n=0/1 边界。
- **Golden**：多文件包（≥8 文件，含 type-alias + where 约束触发 CurrentAliases/ClassConstraints）`--jobs 1` vs `--jobs 8` 产物 `.zpkg` 逐字节一致。
- **自举不动点**：`xtask test compiler` 的 gen1==gen2（`--workspace` 全量两遍，本身覆盖并行路径）。
- **冷启动**：本地下载上一 nightly 作种子跑 cold `build compiler`（轴④必验）。
- **VM 验证**：`xtask test`（完整 GREEN gate）。
- **墙钟量测**：`--jobs 1` vs `--jobs N` 编 stdlib/compiler 的墙钟。

## 实测与 GC 瓶颈（2026-08-29，决定「默认串行、opt-in」）

**正确性 ✅**：本机（24 核，nightly post-shrink 种子）编 compiler workspace，**串行 == `--jobs 1` == 默认并行 逐字节一致**（3 成员，忽略 16B BLID），自举不动点 gen1==gen2 成立。别名视图 + 约束 hoist 正确消除共享可变态；GC-under-并行-load 无正确性问题。

**性能 ❌ 净负**（workspace build 墙钟，各取多次最快）：

| jobs | 1 | 2 | 3 | 4 | 6 | 8 | 24 |
|---|---|---|---|---|---|---|---|
| 墙钟 | 35.8s | 35.6s | 37.2s | 38.3s | 46.7s | 53.9s | 69s |

**越多线程越慢**，jobs=2 才勉强打平。诊断实验 `Z42_GC_MODE=concurrent`（并发标记，减 STW pause）几乎无效（jobs=8: 53.9→52.4s）→ **证伪「STW pause 是主因」**。

**根因**：z42 GC = **单一共享 STW 堆 + 全局 Mutex region 分配器**，**每次 `new` 抢 2 把进程锁**（`gc/arc_heap.rs:309` region bump + `:286` inner，inner 一次分配锁 4-5 次），**无 TLAB / per-thread 分配**；mark/sweep 单线程；GC 触发频率随线程数线性放大 → STW ~N×。设计注释直言假设「1-2 mutators」（`arc_heap.rs:326`）。**对标 .NET**：Roslyn 能并行靠 Server GC 的 per-core heap + Background GC；z42 相反 = 瓶颈根源。

**决定**：`ParallelConfig` **默认串行**、`--jobs N` opt-in（休眠）。本 change 落地的净收益是**框架 + 根因冻结重构（byte-identical）+ 并行接线**——为后续 **GC 线程模型改进**（per-thread 分配 / TLAB，让并行分配去掉全局锁而 scale）备好可测量的消费者。GC 改进排序：A. codegen 临时对象走 per-thread `stack_arena`（z42c 扩逃逸分析，不碰 runtime）；B. inner stats 原子化（去 inner 锁，部分）；C. TLAB / per-thread region（`gc/region.rs`，根治）；D. 并发/并行 mark-sweep（减 STW，次要）。**一句话**：并行分配还在抢全局锁前，并发 GC 也救不了 scale——最小根治是 per-thread 分配。
