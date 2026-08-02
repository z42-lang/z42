# Proposal: REPL 首次符号求值提速——按符号惰性 reconcile（T2）

> 状态：🟡 DRAFT | 创建：2026-08-01 | 类型：perf（**编译器名字解析核心**改动，正确性风险高 → 规范先行 +
> 自举字节不动点门禁 + 分阶段）
> 前置：B+T1（`repl-scan-nsindex-cache` / #91）已落地——`1+1` 秒回车 1.72s→0.40s。
> 本 change 收尾 B+T1 明确留下的**权衡**：首次**符号**求值（`Console.WriteLine`）仍 ~1.7s。

## 背景：B+T1 留下的权衡

B+T1 让 `1+1` 类**纯表达式**零包加载（0.40s），但首次引用 Std 符号（Console/List/Math）时走前台 E0401
回退，一次性 reconcile prelude+usings 闭包 ≈ **1.7s**。这个成本 B+T1 没碰（它只让 `1+1` 不触发它），
根因是 **reconcile 是「整包」粒度**：

- 用 `Console` → 编译器需要 `Console` 的类型元数据（方法/字段/基类）。
- `Console` 的基类链走到 `Std.Object`（z42.core）→ `LazyReconWorld.EnsureIdx(z42.core)` **读整个 z42.core
  的 TYPE/SIGS**（不是只读 `Object`）。
- 且 `using Std.IO;` 使 Std.IO「active」→ `ImportedSymbolLoader.Load` **构建 Std.IO 的全部类**（不是只
  `Console`）。

所以「用一个 `Console`」拖动了整个 Std.IO + z42.core 闭包。这是**整包/整-active-集**假设,不是缓存能解的
（缓存只是提前干、B 已证；后台干又踩 VM 的 GC 坑、T1 已证）——**唯一的解是「别为一个 Console 干整包的活」**。

## 目标：按符号（per-type）惰性 reconcile

用 `Console` 时**只 materialize `Console` 一个类型（+它的基类链闭包，实测 max=3）**,不 reconcile 整个
Std.IO,也不读整个 z42.core:

| 场景 | B+T1 后 | T2 目标 |
|------|---------|---------|
| `1+1` | 0.40s | 0.40s（不回归） |
| 首次 `Console.WriteLine` | 1.7s | **~0.5s** |
| 首次 `List<int>` | 1.7s | **~0.5s** |

且**不需后台线程**（避开 T1 踩的 GC 安全点坑）——纯前台按需,但只干「一个类型」的活,故快。

## 硬约束：stdlib zpkg 保持 packed 单文件、最小化（User 2026-08-01）

**不改 stdlib 为 indexed 输出**（indexed = 散装 zbc 多文件，破坏发布简化）。zpkg 永远 packed 单文件。
为快速解析**可迭代 zpkg 内部结构**，但**必须最小化、不膨胀**。→ 借鉴 .NET assembly / rustc rmeta：
**单文件 + 内部偏移索引**，非拆多文件。z42 packed zpkg 的 **MODS 目录已是「按命名空间」的内部索引**
（每模块各段体长度前缀 → 可按长度跳过、只解目标命名空间），故**按命名空间读零格式改动**。

## 接缝（调研结论，见 design.md）

- **干净的底层**：`TsigReconcile._rebuildClass` **已经是按单类型的**（基类链遇祖先 FQ → `LazyReconWorld.
  EnsureFq` 只路由到声明该 ns 的包；闭包 max=3 avg=1）。加一个 `ReconcileOne(z, fq, world)` 入口很直接。
- **需补的中层（零格式改动）**：`ReadModuleTypes/Sigs` 按**整包**读全部模块 → 加 `ReadOneModuleTypes(z, ns)`
  经 MODS 目录长度跳过、只读一个命名空间模块（`Object`→只读 `Std` 模块，不碰 z42.core 其余 3 模块）。
- **难的上层（高风险）**：`ImportedSymbolLoader.Load` + `SymbolCollector._mergeImports` 把**整个 active 集**
  eager 展平进 `SymbolTable.Classes`,typecheck 只做短名 `StrMap` 查。按类型要:① 给 `SymbolTable` 加**惰性
  miss 回调**、把 loader+scan 线程进 SymbolTable;② 解决三个**不按类型分解**的整包问题——arity-mangle 预扫
  （要看全部同名类才能定 `List` vs `List<T>` 的键）、first-wins 顺序、跨包 impl 合并 + 接口先于类的顺序。

## 风险与门禁

- **高 miscompile 风险**：改的是编译器名字解析核心。任何按类型解析与整包解析产出不一致 → 类型元数据错
  → 静默 miscompile。
- **铁门禁：自举字节不动点**（gen1==gen2 byte-identical）——z42c 自身用整包解析编译,若按类型改动扰动任何
  产物即字节漂移,门禁当场抓出。**这是本 change 最强的安全网**。
- REPL 求值正确性 + 首次符号 eval 延迟实测 + `xtask test` 全绿。

## 分阶段（降风险；每阶段独立可验）

1. **Phase 0 度量**：确认首次 `Console` 的 ~1.7s 中,`ReadModuleTypes(z42.core)` 整包读 + Std.IO 整包 Load
   各占多少 → 定按类型能省多少（若整包读不是大头则重新评估）。
2. **Phase 1（z42.ir，低风险）**：`ReconcileOne(z, fq, world)` + 按类型 TYPE/SIGS 读。验证:单独 reconcile
   `Console` 只读 `Console`+`Object`,不读整个 z42.core。**不改上层** → 整包路径不变、自举不动点天然守住。
3. **Phase 2（compiler 核心，高风险）**：`SymbolTable` 惰性 miss 回调 + arity-mangle/first-wins/impl/接口
   顺序的按类型解。**最需慎重**,自举不动点逐类型对齐。
4. **Phase 3**：接入 REPL 首次符号 eval 路径（E0401 回退改为「只 reconcile 缺失的那个类型」）。

## 非目标

- 不改非 REPL 的整包编译路径（build/test 仍走整包 `ScanDirs` + 整包 Load,自举不动点守住）——除非 Phase 2
  的惰性 SymbolTable 能证明对整包路径 byte-identical,否则**只在 REPL 路径启用惰性解析**。
- 不做「解释 AST 快路」(T3,跳过 zpkg 生成+load 的 compile floor)——独立方向。

## 待 User 裁决的关键设计点（见 design.md）

1. **惰性只在 REPL 启用,还是全局**？全局风险大（整包路径也变）但收益广;REPL-only 安全但要维护两条路径。
2. **arity-mangle 预扫**怎么按类型化——预计算每包的同名-arity 冲突集（从 ns 索引扩展?）还是别的。
3. Phase 2 若风险过高,是否接受**中间形态**：per-namespace 惰性 Load（只 Load 被引用 ns 的包全部类,不 Load
   其他 usings）——比整-active-集省,但不到 per-type。
